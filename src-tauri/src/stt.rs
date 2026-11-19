//! Speech-to-text. Whisper models run on whisper.cpp (via whisper-rs), Parakeet and Moonshine models
//! on sherpa-onnx. The model stays loaded while the app is running so dictation starts instantly;
//! it is swapped when you pick another model.

use crate::audio::WHISPER_RATE;
use crate::catalog::{SttEngine, SttModel};
use anyhow::{anyhow, Result};
use sherpa_onnx::{
    OfflineMoonshineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const LONGEST_STRETCH: usize = 20 * WHISPER_RATE as usize;
const CUT_SEARCH: usize = 5 * WHISPER_RATE as usize;
const CUT_STEP: usize = WHISPER_RATE as usize / 10;

enum Engine {
    Whisper(WhisperContext),
    Parakeet(OfflineRecognizer),
    Moonshine(OfflineRecognizer),
}

#[derive(Default)]
pub struct Stt {
    loaded: Mutex<Option<(String, Engine)>>,
}

impl Stt {
    pub fn unload(&self) {
        *self.loaded.lock().unwrap() = None;
    }

    /// `audio` must be 16 kHz mono f32. `language` is "auto" or a code like "en"; Whisper uses
    /// it, Parakeet works out the language itself.
    pub fn transcribe(
        &self,
        model: &SttModel,
        models_dir: &Path,
        audio: &[f32],
        language: &str,
    ) -> Result<String> {
        let mut guard = self.loaded.lock().unwrap();
        if guard.as_ref().map(|(id, _)| id.as_str()) != Some(model.id) {
            *guard = None;
            let dir = model.dir(models_dir);
            let engine = match model.engine {
                SttEngine::Whisper => {
                    Engine::Whisper(load_whisper(&dir.join(model.files[0].name))?)
                }
                SttEngine::Parakeet => Engine::Parakeet(load_parakeet(&dir)?),
                SttEngine::Moonshine => Engine::Moonshine(load_moonshine(&dir)?),
            };
            *guard = Some((model.id.to_string(), engine));
        }
        let (_, engine) = guard.as_ref().unwrap();
        let text = match engine {
            Engine::Whisper(ctx) => run_whisper(ctx, audio, language)?,
            Engine::Parakeet(recognizer) => run_sherpa(recognizer, audio)?,
            Engine::Moonshine(recognizer) => run_moonshine(recognizer, audio)?,
        };
        Ok(strip_artifacts(&text))
    }
}

fn threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8) as i32
}

/// Whether this processor has what whisper.cpp is built for: AVX2 and the instructions that come
/// with it on x86, as nearly every PC since 2015 has. Without them Whisper would crash the app the
/// moment it ran, so it isn't loaded.
#[cfg(target_arch = "x86_64")]
fn whisper_runs_here() -> bool {
    is_x86_feature_detected!("avx2")
        && is_x86_feature_detected!("fma")
        && is_x86_feature_detected!("f16c")
        && is_x86_feature_detected!("bmi2")
}

#[cfg(not(target_arch = "x86_64"))]
fn whisper_runs_here() -> bool {
    true
}

fn load_whisper(model_path: &Path) -> Result<WhisperContext> {
    if !whisper_runs_here() {
        return Err(anyhow!(
            "Whisper models need a newer processor than this PC has. Pick Parakeet on the Models page."
        ));
    }
    let path = model_path
        .to_str()
        .ok_or_else(|| anyhow!("model path is not valid UTF-8"))?;
    WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .map_err(|e| anyhow!("Could not load speech model: {e}"))
}

fn run_whisper(ctx: &WhisperContext, audio: &[f32], language: &str) -> Result<String> {
    let mut state = ctx.create_state().map_err(|e| anyhow!("{e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads());
    params.set_translate(false);
    params.set_no_context(true);
    params.set_suppress_blank(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    let lang = if language == "auto" {
        None
    } else {
        Some(language)
    };
    params.set_language(lang);

    // Whisper needs at least 1 s of audio; pad short clips with silence.
    let mut padded;
    let audio = if audio.len() < 16_000 + 1_600 {
        padded = audio.to_vec();
        padded.resize(16_000 + 1_600, 0.0);
        &padded[..]
    } else {
        audio
    };

    state
        .full(params, audio)
        .map_err(|e| anyhow!("Transcription failed: {e}"))?;
    let mut text = String::new();
    for seg in state.as_iter() {
        if let Ok(s) = seg.to_str_lossy() {
            text.push_str(&s);
        }
    }
    Ok(text)
}

fn load_parakeet(dir: &Path) -> Result<OfflineRecognizer> {
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: path("encoder.int8.onnx"),
        decoder: path("decoder.int8.onnx"),
        joiner: path("joiner.int8.onnx"),
    };
    config.model_config.tokens = path("tokens.txt");
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = threads();
    OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("Could not load speech model"))
}

fn load_moonshine(dir: &Path) -> Result<OfflineRecognizer> {
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.moonshine = OfflineMoonshineModelConfig {
        encoder: path("encoder_model.ort"),
        merged_decoder: path("decoder_model_merged.ort"),
        ..Default::default()
    };
    config.model_config.tokens = path("tokens.txt");
    config.model_config.num_threads = threads();
    OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("Could not load speech model"))
}

fn run_sherpa(recognizer: &OfflineRecognizer, audio: &[f32]) -> Result<String> {
    let stream = recognizer.create_stream();
    stream.accept_waveform(WHISPER_RATE as i32, audio);
    recognizer.decode(&stream);
    stream
        .get_result()
        .map(|r| r.text)
        .ok_or_else(|| anyhow!("Transcription failed"))
}

/// Given a long recording in one go, Moonshine ends up repeating itself, so it hears short stretches.
fn run_moonshine(recognizer: &OfflineRecognizer, audio: &[f32]) -> Result<String> {
    let heard = stretches(audio)
        .into_iter()
        .map(|stretch| run_sherpa(recognizer, stretch))
        .collect::<Result<Vec<_>>>()?;
    Ok(heard.join(" "))
}

/// Splits audio at the quietest moment near the end of each 20 seconds, so that no word is cut in two.
fn stretches(audio: &[f32]) -> Vec<&[f32]> {
    let loudness =
        |at: usize, rest: &[f32]| -> f32 { rest[at..at + CUT_STEP].iter().map(|s| s.abs()).sum() };
    let mut pieces = Vec::new();
    let mut rest = audio;
    while rest.len() > LONGEST_STRETCH {
        let quietest = (LONGEST_STRETCH - CUT_SEARCH..LONGEST_STRETCH)
            .step_by(CUT_STEP)
            .min_by(|a, b| loudness(*a, rest).total_cmp(&loudness(*b, rest)))
            .unwrap_or(LONGEST_STRETCH - CUT_STEP);
        let (piece, after) = rest.split_at(quietest + CUT_STEP / 2);
        pieces.push(piece);
        rest = after;
    }
    pieces.push(rest);
    pieces
}

/// Whisper sometimes emits markers like "[BLANK_AUDIO]" or "(music)" for silence.
pub fn strip_artifacts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{stretches, strip_artifacts, LONGEST_STRETCH};
    use crate::audio::WHISPER_RATE;

    #[test]
    fn removes_markers() {
        assert_eq!(strip_artifacts(" [BLANK_AUDIO] "), "");
        assert_eq!(strip_artifacts(" Hello (laughs) world. "), "Hello world.");
    }

    #[test]
    fn long_recordings_are_cut_in_a_quiet_moment() {
        let second = WHISPER_RATE as usize;
        let mut audio = vec![0.5; 50 * second];
        audio[17 * second..17 * second + second / 2].fill(0.0);
        let pieces = stretches(&audio);
        assert!(pieces.iter().all(|piece| piece.len() <= LONGEST_STRETCH));
        assert_eq!(
            pieces.iter().map(|piece| piece.len()).sum::<usize>(),
            audio.len()
        );
        assert!(pieces[0].len().abs_diff(17 * second) < second);
    }

    #[test]
    fn short_recordings_stay_whole() {
        let audio = vec![0.5; LONGEST_STRETCH];
        assert_eq!(stretches(&audio).len(), 1);
    }

    /// Built for the build machine's processor, Whisper crashed every PC without AVX-512.
    #[test]
    fn whisper_is_built_for_any_pc_not_the_build_machine() {
        let config = include_str!("../.cargo/config.toml");
        let off = |line: &str| line.split('#').next().unwrap_or_default().replace(' ', "");
        assert!(config.lines().any(|line| off(line) == "GGML_NATIVE=\"OFF\""));
    }
}

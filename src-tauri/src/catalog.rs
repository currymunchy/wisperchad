//! The list of models shown in the app, with details and minimum requirements.
//!
//! Sizes and requirements are approximate and meant as guidance. Pull requests that
//! add models or correct numbers are welcome.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Requirements {
    /// System RAM needed to run it at all (GB).
    pub min_ram_gb: f32,
    /// GPU memory for comfortable, fast use (GB). 0 = runs fine on CPU.
    pub recommended_vram_gb: f32,
    /// Disk space for the download (MB).
    pub disk_mb: u32,
    /// Free-form note, e.g. "CPU is fine" or "needs a GPU to feel instant".
    pub note: &'static str,
}

/// The engine that runs a speech model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SttEngine {
    Whisper,
    Parakeet,
    Moonshine,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub languages: &'static str,
    /// 1 (slow) .. 5 (instant)
    pub speed: u8,
    /// 1 (rough) .. 5 (excellent)
    pub quality: u8,
    pub recommended: bool,
    pub engine: SttEngine,
    /// Subfolder of the models directory for multi-file models; empty for single-file ones.
    pub folder: &'static str,
    pub files: Vec<ModelFile>,
    pub requirements: Requirements,
}

impl SttModel {
    /// The folder holding this model's files.
    pub fn dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.folder)
    }

    pub fn is_installed(&self, models_dir: &Path) -> bool {
        let dir = self.dir(models_dir);
        self.files.iter().all(|f| dir.join(f.name).exists())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmModel {
    /// Also its Ollama tag, e.g. "qwen3:4b".
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub speed: u8,
    pub quality: u8,
    pub recommended: bool,
    /// Model has a "thinking" mode that we switch off for speed.
    pub thinking: bool,
    pub requirements: Requirements,
    /// Subfolder of the models directory; empty for models that only run through Ollama.
    pub folder: &'static str,
    /// What the built-in engine runs: the model, then the file that lets it read images, if any.
    pub files: Vec<ModelFile>,
}

impl LlmModel {
    pub fn dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.folder)
    }

    pub fn is_installed(&self, models_dir: &Path) -> bool {
        let dir = self.dir(models_dir);
        !self.files.is_empty() && self.files.iter().all(|f| dir.join(f.name).exists())
    }

    /// Whether the built-in engine can show it a screenshot.
    pub fn reads_images(&self) -> bool {
        self.files.len() > 1
    }
}

fn gguf(repo: &str, file_name: &'static str) -> ModelFile {
    ModelFile {
        name: file_name,
        url: format!("https://huggingface.co/{repo}/resolve/main/{file_name}"),
    }
}

macro_rules! whisper {
    ($id:expr, $name:expr, $file:expr, $desc:expr, $langs:expr, $speed:expr, $q:expr, $rec:expr,
     $ram:expr, $vram:expr, $disk:expr, $note:expr) => {
        SttModel {
            id: $id,
            name: $name,
            description: $desc,
            languages: $langs,
            speed: $speed,
            quality: $q,
            recommended: $rec,
            engine: SttEngine::Whisper,
            folder: "",
            files: vec![ModelFile {
                name: $file,
                url: concat!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
                    $file
                )
                .to_string(),
            }],
            requirements: Requirements {
                min_ram_gb: $ram,
                recommended_vram_gb: $vram,
                disk_mb: $disk,
                note: $note,
            },
        }
    };
}

/// Parakeet models are four files from sherpa-onnx's quantised exports. The encoder is nearly all
/// of the download, so it comes last.
fn parakeet(
    id: &'static str,
    name: &'static str,
    repo: &'static str,
    description: &'static str,
    languages: &'static str,
    recommended: bool,
    disk_mb: u32,
) -> SttModel {
    let file = |file_name: &'static str| ModelFile {
        name: file_name,
        url: format!("https://huggingface.co/{repo}/resolve/main/{file_name}"),
    };
    SttModel {
        id,
        name,
        description,
        languages,
        speed: 5,
        quality: 4,
        recommended,
        engine: SttEngine::Parakeet,
        folder: id,
        files: vec![
            file("tokens.txt"),
            file("decoder.int8.onnx"),
            file("joiner.int8.onnx"),
            file("encoder.int8.onnx"),
        ],
        requirements: Requirements {
            min_ram_gb: 2.0,
            recommended_vram_gb: 0.0,
            disk_mb,
            note: "Runs on the CPU, no GPU needed. Spoken language is detected automatically.",
        },
    }
}

/// Three files from sherpa-onnx's export, with the largest last.
fn moonshine() -> SttModel {
    let repo = "csukuangfj2/sherpa-onnx-moonshine-base-en-quantized-2026-02-27";
    let file = |file_name: &'static str| ModelFile {
        name: file_name,
        url: format!("https://huggingface.co/{repo}/resolve/main/{file_name}"),
    };
    SttModel {
        id: "moonshine-base-en",
        name: "Moonshine Base (English)",
        description: "The quickest here, and a small download. As good as Parakeet on everyday speech, but it stumbles if you talk very fast.",
        languages: "English",
        speed: 5,
        quality: 4,
        recommended: false,
        engine: SttEngine::Moonshine,
        folder: "moonshine-base-en",
        files: vec![
            file("tokens.txt"),
            file("encoder_model.ort"),
            file("decoder_model_merged.ort"),
        ],
        requirements: Requirements {
            min_ram_gb: 1.0,
            recommended_vram_gb: 0.0,
            disk_mb: 135,
            note: "Runs on the CPU, no GPU needed.",
        },
    }
}

/// The models on the main Models page. Everything else is on the More models tab, to keep the
/// first choice simple for people who don't know one model from another.
pub const FEATURED: [&str; 4] = [
    "parakeet-tdt-0.6b-v2",
    "whisper-small.en",
    "qwen3.5:4b",
    "qwen3.5:2b",
];

pub fn stt_models() -> Vec<SttModel> {
    vec![
        parakeet(
            "parakeet-tdt-0.6b-v2",
            "Parakeet TDT 0.6B v2 (English)",
            "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
            "NVIDIA's fast, accurate English model. Best pick if you only speak English.",
            "English",
            true,
            661,
        ),
        parakeet(
            "parakeet-tdt-0.6b-v3",
            "Parakeet TDT 0.6B v3 (European languages)",
            "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            "Same speed as v2, covering 25 European languages. No Chinese, Japanese or Korean.",
            "25 European languages",
            false,
            670,
        ),
        moonshine(),
        whisper!(
            "whisper-tiny.en",
            "Whisper Tiny (English)",
            "ggml-tiny.en.bin",
            "Smallest and fastest. Fine for short notes, makes more mistakes.",
            "English",
            5,
            2,
            false,
            1.0,
            0.0,
            75,
            "Runs on almost anything, CPU is fine."
        ),
        whisper!(
            "whisper-base.en",
            "Whisper Base (English)",
            "ggml-base.en.bin",
            "Good default for laptops without a GPU. Fast and decent.",
            "English",
            5,
            3,
            false,
            1.0,
            0.0,
            142,
            "CPU is fine."
        ),
        whisper!(
            "whisper-small.en",
            "Whisper Small (English)",
            "ggml-small.en.bin",
            "Noticeably more accurate than Base, still quick on a modern CPU.",
            "English",
            4,
            4,
            false,
            2.0,
            0.0,
            466,
            "CPU is OK, a GPU makes it instant."
        ),
        whisper!(
            "whisper-small.en-q5",
            "Whisper Small Q5 (English)",
            "ggml-small.en-q5_1.bin",
            "As accurate as Whisper Small at under half the download.",
            "English",
            4,
            4,
            false,
            1.0,
            0.0,
            190,
            "CPU is OK, a GPU makes it instant."
        ),
        whisper!(
            "whisper-small",
            "Whisper Small (Multilingual)",
            "ggml-small.bin",
            "For languages other than English. Understands about 100 languages and mixed speech.",
            "~100 languages",
            4,
            4,
            false,
            2.0,
            0.0,
            466,
            "CPU is OK, a GPU makes it instant."
        ),
        whisper!(
            "whisper-medium",
            "Whisper Medium (Multilingual)",
            "ggml-medium.bin",
            "High accuracy, but slow without a GPU. Most people should pick Turbo instead.",
            "~100 languages",
            2,
            4,
            false,
            4.0,
            2.0,
            1500,
            "Needs a GPU to feel responsive."
        ),
        whisper!(
            "whisper-large-v3-turbo-q5",
            "Whisper Large v3 Turbo (Q5)",
            "ggml-large-v3-turbo-q5_0.bin",
            "Near top accuracy at a third of the size. Best pick if you have an NVIDIA GPU.",
            "~100 languages",
            3,
            5,
            false,
            3.0,
            2.0,
            574,
            "Use the GPU installer for instant results."
        ),
        whisper!(
            "whisper-large-v3-turbo",
            "Whisper Large v3 Turbo",
            "ggml-large-v3-turbo.bin",
            "Full-precision Turbo. Slightly better than Q5, uses more memory.",
            "~100 languages",
            3,
            5,
            false,
            4.0,
            3.0,
            1620,
            "Use the GPU installer for instant results."
        ),
    ]
}

pub fn llm_models() -> Vec<LlmModel> {
    let req = |ram, vram, disk, note| Requirements {
        min_ram_gb: ram,
        recommended_vram_gb: vram,
        disk_mb: disk,
        note,
    };
    vec![
        LlmModel {
            id: "qwen3:1.7b",
            name: "Qwen3 1.7B",
            description: "Very light, but too small for cleanup: it often leaves filler words in. Basic replies only.",
            speed: 5,
            quality: 1,
            recommended: false,
            thinking: true,
            requirements: req(4.0, 2.0, 1100, "CPU is OK, GPU makes it instant. Not tested with the built-in engine yet."),
            folder: "llm/qwen3-1.7b",
            files: vec![gguf("unsloth/Qwen3-1.7B-GGUF", "Qwen3-1.7B-Q4_K_M.gguf")],
        },
        LlmModel {
            id: "qwen3.5:2b",
            name: "Qwen3.5 2B",
            description: "Quick on any computer, writes well and reads screenshots. It can misread a picture now and then, so check the draft.",
            speed: 5,
            quality: 3,
            recommended: true,
            thinking: true,
            requirements: req(4.0, 3.0, 1950, "Measured at 2.6 GB on an RTX 3070."),
            folder: "llm/qwen3.5-2b",
            files: vec![
                gguf("unsloth/Qwen3.5-2B-GGUF", "Qwen3.5-2B-Q4_K_M.gguf"),
                gguf("unsloth/Qwen3.5-2B-GGUF", "mmproj-F16.gguf"),
            ],
        },
        LlmModel {
            id: "qwen3.5:4b",
            name: "Qwen3.5 4B",
            description: "A little more careful than the 2B and better at pictures, but slower. Best with a graphics card.",
            speed: 4,
            quality: 5,
            recommended: false,
            thinking: true,
            requirements: req(6.0, 4.5, 3400, "Measured on an RTX 3070: 4.2 GB built in, 3.2 GB through Ollama."),
            folder: "llm/qwen3.5-4b",
            files: vec![
                gguf("unsloth/Qwen3.5-4B-GGUF", "Qwen3.5-4B-Q4_K_M.gguf"),
                gguf("unsloth/Qwen3.5-4B-GGUF", "mmproj-F16.gguf"),
            ],
        },
        LlmModel {
            id: "qwen3.5:9b",
            name: "Qwen3.5 9B",
            description: "The largest writing model here, for computers with a powerful graphics card. Can read screenshots.",
            speed: 4,
            quality: 5,
            recommended: false,
            thinking: true,
            requirements: req(12.0, 10.0, 6600, "Should need about 7.5 GB of graphics memory, so it won't fit an 8 GB card with everything else running. Tested on a CPU, not yet on a graphics card."),
            folder: "llm/qwen3.5-9b",
            files: vec![
                gguf("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q4_K_M.gguf"),
                gguf("unsloth/Qwen3.5-9B-GGUF", "mmproj-F16.gguf"),
            ],
        },
        LlmModel {
            id: "gemma4:e2b",
            name: "Gemma 4 E2B",
            description: "A very quick writer, matching Qwen3.5 4B on clean-up and replies. Slower at reading screenshots, and it can misread charts, so check the draft.",
            speed: 5,
            quality: 4,
            recommended: false,
            thinking: true,
            requirements: req(6.0, 5.0, 4100, "Measured at 5.4 GB on a CPU. Not tested on a graphics card yet."),
            folder: "llm/gemma4-e2b",
            files: vec![
                gguf("unsloth/gemma-4-E2B-it-GGUF", "gemma-4-E2B-it-Q4_K_M.gguf"),
                gguf("unsloth/gemma-4-E2B-it-GGUF", "mmproj-F16.gguf"),
            ],
        },
        LlmModel {
            id: "gemma4:e4b",
            name: "Gemma 4 E4B",
            description: "Quick, and matches Qwen3.5 4B on clean-up and replies. Slower at reading screenshots, and it can misread charts, so check the draft.",
            speed: 4,
            quality: 4,
            recommended: false,
            thinking: true,
            requirements: req(8.0, 6.5, 6000, "Measured at 7 GB on a CPU. Not tested on a graphics card yet."),
            folder: "llm/gemma4-e4b",
            files: vec![
                gguf("unsloth/gemma-4-E4B-it-GGUF", "gemma-4-E4B-it-Q4_K_M.gguf"),
                gguf("unsloth/gemma-4-E4B-it-GGUF", "mmproj-F16.gguf"),
            ],
        },
        LlmModel {
            id: "qwen3:4b",
            name: "Qwen3 4B",
            description: "Can't skip its reasoning step, so each dictation takes close to a minute. Pick Qwen3.5 4B instead.",
            speed: 1,
            quality: 4,
            recommended: false,
            thinking: true,
            requirements: req(8.0, 3.5, 2500, "Measured at 3 GB on an RTX 3070."),
            folder: "",
            files: vec![],
        },
        LlmModel {
            id: "qwen3:8b",
            name: "Qwen3 8B",
            description: "Fast and accurate, but uses far more memory than Qwen3.5 4B for similar results.",
            speed: 5,
            quality: 5,
            recommended: false,
            thinking: true,
            requirements: req(8.0, 5.5, 5000, "Measured at 5.3 GB on an RTX 3070. Not tested with the built-in engine yet."),
            folder: "llm/qwen3-8b",
            files: vec![gguf("Qwen/Qwen3-8B-GGUF", "Qwen3-8B-Q4_K_M.gguf")],
        },
    ]
}

pub fn find_stt(id: &str) -> Option<SttModel> {
    stt_models().into_iter().find(|m| m.id == id)
}

pub fn find_llm(id: &str) -> Option<LlmModel> {
    llm_models().into_iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_urls_point_at_files() {
        let stt = stt_models();
        let ids: HashSet<_> = stt.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), stt.len());
        for m in &stt {
            assert!(!m.files.is_empty());
            for f in &m.files {
                assert!(f.url.ends_with(f.name));
                assert!(f.url.starts_with("https://huggingface.co/"));
            }
        }
        let llm = llm_models();
        let ids: HashSet<_> = llm.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), llm.len());
    }

    #[test]
    fn multi_file_models_need_every_file_and_their_own_folder() {
        let dir = std::env::temp_dir().join(format!("wisperchad-models-{}", std::process::id()));
        let model = find_stt("parakeet-tdt-0.6b-v2").unwrap();
        assert_eq!(model.engine, SttEngine::Parakeet);
        assert!(!model.folder.is_empty());
        std::fs::create_dir_all(model.dir(&dir)).unwrap();
        let (last, rest) = model.files.split_last().unwrap();
        for f in rest {
            std::fs::write(model.dir(&dir).join(f.name), b"x").unwrap();
        }
        assert!(!model.is_installed(&dir));
        std::fs::write(model.dir(&dir).join(last.name), b"x").unwrap();
        assert!(model.is_installed(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn writing_models_keep_their_files_in_their_own_folders() {
        let llm = llm_models();
        let folders: HashSet<_> = llm
            .iter()
            .map(|m| m.folder)
            .filter(|f| !f.is_empty())
            .collect();
        for m in llm.iter().filter(|m| !m.files.is_empty()) {
            assert!(folders.contains(m.folder), "{} needs a folder", m.id);
            assert!(m.files[0].name.ends_with(".gguf"));
            for f in &m.files {
                assert!(f.url.ends_with(f.name));
                assert!(f.url.starts_with("https://huggingface.co/"));
            }
        }
        assert_eq!(
            folders.len(),
            llm.iter().filter(|m| !m.files.is_empty()).count()
        );
    }

    #[test]
    fn the_recommended_writing_model_needs_nothing_else_installed() {
        let model = llm_models().into_iter().find(|m| m.recommended).unwrap();
        assert!(!model.files.is_empty());
        assert!(model.reads_images());
    }

    #[test]
    fn ollama_only_models_are_never_installed_here() {
        let dir = std::env::temp_dir();
        assert!(!find_llm("qwen3:8b").unwrap().is_installed(&dir));
    }

    #[test]
    fn the_main_page_has_two_models_of_each_kind() {
        let stt = FEATURED.iter().filter(|id| find_stt(id).is_some()).count();
        let llm = FEATURED.iter().filter(|id| find_llm(id).is_some()).count();
        assert_eq!((stt, llm), (2, 2));
        assert!(find_llm("qwen3.5:2b").unwrap().reads_images());
    }

    #[test]
    fn each_list_has_a_recommendation() {
        assert!(stt_models().iter().any(|m| m.recommended));
        assert!(llm_models().iter().any(|m| m.recommended));
    }
}

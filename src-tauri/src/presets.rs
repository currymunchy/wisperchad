//! The ready-made choices in first-run setup: what to install for the languages someone speaks and
//! how strong their computer is.

use crate::catalog;
use crate::hardware::Hardware;
use serde::Serialize;

/// Detected memory comes in a little under what's on the box.
const LEEWAY_GB: f32 = 0.25;

/// Below this a computer can't run a writing model and everything else it's doing.
const WRITING_RAM_GB: f32 = 8.0;

/// What Parakeet v3 understands, by Whisper's language codes. It's far quicker than Whisper without
/// a graphics card, so it's the pick for these.
const PARAKEET_LANGUAGES: [&str; 25] = [
    "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "hr", "hu", "it", "lt", "lv", "mt",
    "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "uk",
];

struct Tier {
    id: &'static str,
    name: &'static str,
    summary: &'static str,
    /// None for typing what was said and nothing more.
    writing_model: Option<&'static str>,
    /// Whether it runs well enough on the processor alone. Balanced on a fast processor with no
    /// graphics card still took too long over a reply, so only the lighter ones do.
    runs_without_gpu: bool,
}

/// Below this much graphics memory, the bigger Whisper isn't worth what it takes.
const BIG_WHISPER_VRAM_GB: f32 = 10.0;

/// Lightest first. There's no preset for Qwen3.5 9B: replies are short, so it wrote little better than
/// the 4B and took twice as long, even on a 48 GB Mac. It's still on the More models page.
const TIERS: [Tier; 3] = [
    Tier {
        id: "transcribe",
        name: "Transcription only",
        summary: "Types what you say, exactly as heard. There's no writing model, so no clean-up and no replies. For computers that can't run one.",
        writing_model: None,
        runs_without_gpu: true,
    },
    Tier {
        id: "light",
        name: "Light",
        summary: "For basic computers, no graphics card needed. Simpler writing, and a reply can take a while.",
        writing_model: Some("qwen3.5:2b"),
        runs_without_gpu: true,
    },
    Tier {
        id: "balanced",
        name: "Balanced",
        summary: "For most computers with a graphics card. Clear writing, ready in moments.",
        writing_model: Some("qwen3.5:4b"),
        runs_without_gpu: false,
    },
];

#[derive(Debug, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub speech_model: &'static str,
    pub writing_model: Option<&'static str>,
    /// Whether this computer should run it well.
    pub fits: bool,
    pub recommended: bool,
}

/// Parakeet v3 has to cover everything spoken, or it's Whisper, the only choice for most of the
/// world's languages, and the bigger one when there's a graphics card with room for it.
fn speech_model(languages: &[String], hw: &Hardware) -> &'static str {
    let big_card = hw
        .gpus
        .iter()
        .any(|gpu| gpu.vram_gb + LEEWAY_GB >= BIG_WHISPER_VRAM_GB);
    let parakeet_covers = |language: &String| PARAKEET_LANGUAGES.contains(&language.as_str());
    if languages == ["en"] {
        "parakeet-tdt-0.6b-v2"
    } else if !languages.is_empty() && languages.iter().all(parakeet_covers) {
        "parakeet-tdt-0.6b-v3"
    } else if big_card && hw.gpu_build {
        "whisper-large-v3-turbo-q5"
    } else {
        "whisper-small"
    }
}

fn fits(tier: &Tier, hw: &Hardware) -> bool {
    let Some(writing_model) = tier.writing_model else {
        return true;
    };
    let Some(model) = catalog::find_llm(writing_model) else {
        return false;
    };
    if hw.ram_gb + LEEWAY_GB < WRITING_RAM_GB {
        return false;
    }
    let vram_gb = hw.gpus.iter().map(|g| g.vram_gb).fold(0.0, f32::max);
    let enough_vram = vram_gb + LEEWAY_GB >= model.requirements.recommended_vram_gb;
    hw.ram_gb + LEEWAY_GB >= model.requirements.min_ram_gb && (enough_vram || tier.runs_without_gpu)
}

/// Every preset, with the strongest one this computer can run marked. Transcription only runs on
/// anything, so there's always one.
pub fn for_computer(languages: &[String], hw: &Hardware) -> Vec<Preset> {
    let recommended = TIERS.iter().rposition(|tier| fits(tier, hw)).unwrap_or(0);
    TIERS
        .iter()
        .enumerate()
        .map(|(i, tier)| Preset {
            id: tier.id,
            name: tier.name,
            summary: tier.summary,
            speech_model: speech_model(languages, hw),
            writing_model: tier.writing_model,
            fits: fits(tier, hw),
            recommended: i == recommended,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::Gpu;

    fn computer(ram_gb: f32, vram_gb: Option<f32>) -> Hardware {
        Hardware {
            ram_gb,
            cpu: "Test".into(),
            cpu_cores: 8,
            gpus: vram_gb
                .map(|vram_gb| Gpu {
                    name: "Test".into(),
                    vram_gb,
                })
                .into_iter()
                .collect(),
            gpu_build: true,
        }
    }

    fn codes(languages: &[&str]) -> Vec<String> {
        languages.iter().map(|code| code.to_string()).collect()
    }

    fn recommended(languages: &[&str], hw: &Hardware) -> Preset {
        for_computer(&codes(languages), hw)
            .into_iter()
            .find(|p| p.recommended)
            .unwrap()
    }

    #[test]
    fn a_16_gb_pc_with_an_rtx_3070_gets_parakeet_and_qwen_4b() {
        let p = recommended(&["en"], &computer(15.9, Some(8.0)));
        assert_eq!(p.id, "balanced");
        assert_eq!(p.speech_model, "parakeet-tdt-0.6b-v2");
        assert_eq!(p.writing_model, Some("qwen3.5:4b"));
    }

    #[test]
    fn stronger_computers_get_stronger_writing_models() {
        let id = |ram_gb, vram_gb| recommended(&["en"], &computer(ram_gb, vram_gb)).id;
        assert_eq!(id(8.0, None), "light");
        assert_eq!(id(32.0, None), "light");
        assert_eq!(id(16.0, Some(4.0)), "light");
        assert_eq!(id(16.0, Some(6.0)), "balanced");
        // There's no preset above Balanced.
        assert_eq!(id(32.0, Some(12.0)), "balanced");
    }

    #[test]
    fn without_a_graphics_card_light_is_the_strongest_recommended() {
        let mut hw = computer(32.0, None);
        hw.cpu_cores = 16;
        assert_eq!(recommended(&["en"], &hw).id, "light");
        let fits: Vec<bool> = for_computer(&codes(&["en"]), &hw)
            .iter()
            .map(|p| p.fits)
            .collect();
        assert_eq!(fits, [true, true, false]);
        assert_eq!(recommended(&["en"], &computer(6.0, None)).id, "transcribe");
    }

    #[test]
    fn a_big_graphics_card_cant_make_up_for_too_little_memory() {
        assert_eq!(
            recommended(&["en"], &computer(8.0, Some(12.0))).id,
            "balanced"
        );
    }

    #[test]
    fn a_computer_too_weak_for_a_writing_model_gets_transcription_only() {
        let p = recommended(&["en"], &computer(6.0, None));
        assert_eq!(p.id, "transcribe");
        assert_eq!(p.writing_model, None);
        assert!(p.fits);
        assert_eq!(p.speech_model, "parakeet-tdt-0.6b-v2");
        let presets = for_computer(&codes(&["en"]), &computer(6.0, None));
        assert!(presets.iter().skip(1).all(|p| !p.fits));
    }

    #[test]
    fn exactly_one_is_recommended_and_a_weak_computer_still_gets_one() {
        for hw in [computer(3.0, None), computer(16.0, Some(8.0))] {
            let presets = for_computer(&codes(&["en"]), &hw);
            assert_eq!(presets.iter().filter(|p| p.recommended).count(), 1);
        }
        assert_eq!(recommended(&["en"], &computer(3.0, None)).id, "transcribe");
    }

    #[test]
    fn presets_too_heavy_for_the_computer_are_marked() {
        let presets = for_computer(&codes(&["en"]), &computer(16.0, Some(4.0)));
        let fits: Vec<bool> = presets.iter().map(|p| p.fits).collect();
        assert_eq!(fits, [true, true, false]);
        assert!(presets
            .iter()
            .all(|p| p.writing_model != Some("qwen3.5:9b")));
    }

    #[test]
    fn the_speech_model_follows_the_language() {
        let hw = computer(16.0, Some(8.0));
        let speech = |languages: &[&str]| recommended(languages, &hw).speech_model;
        assert_eq!(speech(&["en"]), "parakeet-tdt-0.6b-v2");
        assert_eq!(speech(&["es"]), "parakeet-tdt-0.6b-v3");
        assert_eq!(speech(&["zh"]), "whisper-small");
        assert_eq!(speech(&["ms"]), "whisper-small");
        assert_eq!(speech(&["auto"]), "whisper-small");
    }

    #[test]
    fn several_languages_need_a_model_that_covers_them_all() {
        let hw = computer(16.0, Some(8.0));
        let speech = |languages: &[&str]| recommended(languages, &hw).speech_model;
        assert_eq!(speech(&["en", "es"]), "parakeet-tdt-0.6b-v3");
        assert_eq!(speech(&["en", "zh"]), "whisper-small");
        assert_eq!(speech(&["es", "auto"]), "whisper-small");
        assert_eq!(speech(&[]), "whisper-small");
    }

    #[test]
    fn only_a_big_graphics_card_on_a_gpu_build_gets_the_bigger_whisper() {
        let mut hw = computer(32.0, Some(16.0));
        assert_eq!(
            recommended(&["zh"], &hw).speech_model,
            "whisper-large-v3-turbo-q5"
        );
        hw.gpu_build = false;
        assert_eq!(recommended(&["zh"], &hw).speech_model, "whisper-small");
        let small_card = computer(16.0, Some(8.0));
        assert_eq!(
            recommended(&["zh"], &small_card).speech_model,
            "whisper-small"
        );
    }

    #[test]
    fn every_preset_can_be_installed_and_reads_screenshots() {
        for language in ["en", "es", "zh", "auto"] {
            for p in for_computer(&codes(&[language]), &computer(32.0, Some(16.0))) {
                assert!(catalog::find_stt(p.speech_model).is_some());
                if let Some(writing) = p.writing_model {
                    assert!(catalog::find_llm(writing).unwrap().reads_images());
                }
            }
        }
    }
}

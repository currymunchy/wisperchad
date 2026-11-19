//! User settings, persisted as JSON in the app config directory.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Id of the speech-to-text model from the catalog (e.g. "whisper-base.en").
    pub stt_model: Option<String>,
    /// Model used for cleanup and replies, by catalog id or Ollama tag (e.g. "qwen3:4b").
    pub llm_model: Option<String>,
    /// What runs it: "builtin" or "ollama". Empty until the first launch has decided.
    pub llm_engine: String,
    /// Whether the first-run setup is finished. None in settings from before it existed.
    pub setup_done: Option<bool>,
    /// Base URL of the local Ollama server.
    pub ollama_url: String,
    /// Hold this to dictate. Text is typed where your cursor is.
    pub dictate_hotkey: String,
    /// Hold this to write a reply using the text you last copied as context.
    pub reply_hotkey: String,
    /// "hold" = hold the hotkey while talking; "toggle" = press once to start, again to stop.
    pub hotkey_style: String,
    /// The microphone's name. Empty = the system default.
    pub microphone: String,
    /// Mute the speakers while recording, so nothing playing ends up in the dictation.
    pub mute_while_recording: bool,
    /// Press Enter after pasting a dictation. Replies are never submitted.
    pub auto_submit: bool,
    /// "off", "window" = open the window, or "hidden" = stay in the tray.
    pub launch_on_startup: String,
    /// Run dictation through the LLM to remove filler words and fix punctuation.
    pub cleanup_enabled: bool,
    /// Where a reply gets the message from: "clipboard" (what you copied) or "screen" (a
    /// screenshot of the active window).
    pub reply_source: String,
    /// What language a reply comes out in: "message" (the one being replied to), "speech" (the one
    /// the user spoke), or one language's English name. English only for now: see `english_only`.
    pub reply_language: String,
    /// The language dictation is written in, by its English name. Empty = whatever was spoken.
    pub write_language: String,
    /// "professional" | "friendly" | "casual"
    pub tone: String,
    /// Used to sign replies. Empty = no signature.
    pub user_name: String,
    /// Spoken language code for Whisper ("auto", "en", "zh", ...).
    pub language: String,
    /// Minutes the LLM stays loaded in VRAM after the last use. 0 = unload immediately.
    pub keep_alive_minutes: u32,
}

/// Dictate, then reply. On Windows these are modifiers on their own, which no other app can already
/// own: Ctrl+Alt+Space is taken by the Claude desktop app, for one. It also switches keyboard layout
/// on a Mac, so Macs get D for dictate.
const DEFAULT_HOTKEYS: (&str, &str) = if cfg!(windows) {
    ("Ctrl+Win", "Ctrl+Win+Alt")
} else if cfg!(target_os = "macos") {
    ("Ctrl+Alt+D", "Ctrl+Alt+R")
} else {
    ("Ctrl+Alt+Space", "Ctrl+Alt+R")
};

impl Default for Settings {
    fn default() -> Self {
        Self {
            stt_model: None,
            llm_model: None,
            llm_engine: String::new(),
            setup_done: None,
            ollama_url: "http://127.0.0.1:11434".into(),
            dictate_hotkey: DEFAULT_HOTKEYS.0.into(),
            reply_hotkey: DEFAULT_HOTKEYS.1.into(),
            hotkey_style: "hold".into(),
            microphone: String::new(),
            mute_while_recording: true,
            auto_submit: false,
            launch_on_startup: "off".into(),
            cleanup_enabled: true,
            reply_source: "screen".into(),
            reply_language: "English".into(),
            write_language: String::new(),
            tone: "professional".into(),
            user_name: String::new(),
            language: "en".into(),
            keep_alive_minutes: 5,
        }
    }
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("settings.json")
    }

    pub fn load(config_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
            .english_only()
    }

    /// Other languages are switched off for now, whatever an older version saved. Nothing in the
    /// app sets these any more.
    fn english_only(self) -> Self {
        Self {
            language: "en".into(),
            reply_language: "English".into(),
            write_language: String::new(),
            ..self
        }
    }

    pub fn save(&self, config_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        std::fs::write(Self::path(config_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"tone":"casual"}"#).unwrap();
        assert_eq!(s.tone, "casual");
        assert_eq!(s.dictate_hotkey, Settings::default().dictate_hotkey);
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_hotkeys_cant_clash_with_other_apps() {
        let s = Settings::default();
        assert!(crate::chords::parse(&s.dictate_hotkey).is_some());
        assert!(crate::chords::parse(&s.reply_hotkey).is_some());
    }

    #[test]
    fn older_language_choices_become_english() {
        let dir = std::env::temp_dir().join(format!("wisperchad-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            Settings::path(&dir),
            r#"{"language":"zh","reply_language":"message","write_language":"Chinese"}"#,
        )
        .unwrap();
        let s = Settings::load(&dir);
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(s.language, "en");
        assert_eq!(s.reply_language, "English");
        assert_eq!(s.write_language, "");
    }

    #[test]
    fn roundtrip_on_disk() {
        let dir = std::env::temp_dir().join(format!("wisperchad-test-{}", std::process::id()));
        let s = Settings {
            user_name: "Alex".into(),
            ..Default::default()
        };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        let _ = std::fs::remove_dir_all(dir);
    }
}

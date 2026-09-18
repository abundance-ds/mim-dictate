use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub model: String,
    pub languages: Vec<String>,
    pub hotkey: String,
    pub activation_mode: String,
    pub audio_cues: bool,
    pub auto_paste: bool,
    #[serde(default)]
    pub mute_during_recording: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "base".to_string(),
            languages: vec!["de".to_string(), "en".to_string()],
            hotkey: default_hotkey().to_string(),
            activation_mode: "hold".to_string(),
            audio_cues: true,
            auto_paste: true,
            mute_during_recording: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            let settings = Self::default();
            settings.save(path)?;
            return Ok(settings);
        }

        let raw = fs::read_to_string(path)?;
        let mut settings: Self = serde_json::from_str(&raw)?;
        settings.normalize();
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        if !crate::models::is_supported(&self.model) {
            self.model = "base".to_string();
        }
        self.languages.retain(|lang| is_supported_language(lang));
        self.languages.sort();
        self.languages.dedup();
        if !matches!(self.activation_mode.as_str(), "hold" | "toggle") {
            self.activation_mode = "hold".to_string();
        }
        if self.hotkey.trim().is_empty() {
            self.hotkey = default_hotkey().to_string();
        }
    }
}

pub fn default_hotkey() -> &'static str {
    default_hotkey_for_platform()
}

#[cfg(target_os = "macos")]
fn default_hotkey_for_platform() -> &'static str {
    "Option"
}

#[cfg(not(target_os = "macos"))]
fn default_hotkey_for_platform() -> &'static str {
    "F8"
}

pub fn is_supported_language(lang: &str) -> bool {
    matches!(
        lang,
        "auto" | "de" | "en" | "fr" | "es" | "it" | "nl" | "pl" | "pt" | "sv"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_replaces_invalid_model_mode_and_hotkey() {
        let mut settings = Settings {
            model: "large".to_string(),
            languages: vec!["de".to_string()],
            hotkey: "  ".to_string(),
            activation_mode: "press".to_string(),
            audio_cues: true,
            auto_paste: true,
            mute_during_recording: false,
        };

        settings.normalize();

        assert_eq!(settings.model, "base");
        assert_eq!(settings.activation_mode, "hold");
        assert_eq!(settings.hotkey, default_hotkey());
    }

    #[test]
    fn normalize_filters_sorts_and_deduplicates_languages() {
        let mut settings = Settings {
            languages: vec![
                "en".to_string(),
                "xx".to_string(),
                "de".to_string(),
                "en".to_string(),
            ],
            ..Settings::default()
        };

        settings.normalize();

        assert_eq!(settings.languages, vec!["de", "en"]);
    }
}

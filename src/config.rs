use crate::audio::ChannelMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Paramètres persistants (%APPDATA%\ParkinsonAide\config.json).
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub last_file: Option<PathBuf>,
    pub vosk_model_path: Option<PathBuf>,
    /// Nom du micro ; `None` = automatique (XVF3800 si présent).
    pub input_device: Option<String>,
    pub channel_mode: ChannelMode,
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ParkinsonAide")
            .join("config.json")
    }
}

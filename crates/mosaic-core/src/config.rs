use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub theme: Option<String>,
    pub refresh_interval_secs: Option<u64>,
    pub default_market: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;
        Ok(())
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mosaic")
            .join("config.toml")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Some("dark".to_string()),
            refresh_interval_secs: Some(300),
            default_market: Some("in".to_string()),
        }
    }
}

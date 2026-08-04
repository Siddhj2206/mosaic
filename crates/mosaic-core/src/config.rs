//! User preferences from `~/.config/mosaic/config.toml`.
//!
//! v1 keeps this minimal: everything has a default, so the file is optional.
//! Window/panel state lives in the SQLite `key_value_store` instead.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Background sync cadence in minutes (default 360 = 6h).
    pub sync_interval_minutes: Option<u64>,
    /// Subscription poll hour, local time (default 18 = 18:00).
    pub subscription_poll_hour: Option<u8>,
    /// EOD pull hour, local time (default 19 = 19:30).
    pub eod_pull_hour: Option<u8>,
    /// Theme name; only "one-dark" exists in v1.
    pub theme: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sync_interval_minutes: Some(360),
            subscription_poll_hour: Some(18),
            eod_pull_hour: Some(19),
            theme: Some("one-dark".to_string()),
        }
    }
}

impl Config {
    /// `~/.config/mosaic/config.toml`
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mosaic")
            .join("config.toml")
    }

    /// Load from disk, falling back to defaults when absent or invalid.
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Config::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Save to disk (creates parent dirs).
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("serialize failed: {e}")))?;
        std::fs::write(&path, text).map_err(|source| Error::Io { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load_without_file() {
        let cfg = Config::load();
        assert_eq!(cfg.sync_interval_minutes, Some(360));
        assert_eq!(cfg.theme.as_deref(), Some("one-dark"));
    }

    #[test]
    fn toml_roundtrip() {
        let cfg = Config::default();
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.sync_interval_minutes, cfg.sync_interval_minutes);
    }
}

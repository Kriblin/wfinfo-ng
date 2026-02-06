use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    pub game_log_file_path: String,
    pub window_name: String,
    pub hotkey: String,
    pub capture_delay_ms: u64,
    pub prices_file_path: String,
    pub items_file_path: String,
    pub log_level: String,
    pub log_timestamps: bool,
}

impl SettingsState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            game_log_file_path: config
                .game_log_file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            window_name: config.window_name.clone(),
            hotkey: config.hotkey.clone(),
            capture_delay_ms: config.capture_delay_ms,
            prices_file_path: config
                .prices_file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            items_file_path: config
                .items_file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            log_level: config.log_level.clone(),
            log_timestamps: config.log_timestamps,
        }
    }

    pub fn to_config(&self) -> Config {
        Config {
            game_log_file_path: Self::parse_optional_path(&self.game_log_file_path),
            window_name: self.window_name.trim().to_string(),
            hotkey: self.hotkey.trim().to_string(),
            capture_delay_ms: self.capture_delay_ms,
            prices_file_path: Self::parse_optional_path(&self.prices_file_path),
            items_file_path: Self::parse_optional_path(&self.items_file_path),
            log_level: self.log_level.trim().to_string(),
            log_timestamps: self.log_timestamps,
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let config = self.to_config();
        config.save_to_file(path)
    }

    pub fn save_to_default_location(&self) -> Result<PathBuf> {
        let config = self.to_config();
        config.save()?;
        Config::get_config_file_path()
    }

    fn parse_optional_path(value: &str) -> Option<PathBuf> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "wfinfo-settings-test-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn settings_round_trip_preserves_config_values() {
        let config = Config {
            game_log_file_path: Some(PathBuf::from("/tmp/EE.log")),
            window_name: "WarframeTest".to_string(),
            hotkey: "F9".to_string(),
            capture_delay_ms: 2500,
            prices_file_path: Some(PathBuf::from("/tmp/prices.json")),
            items_file_path: Some(PathBuf::from("/tmp/items.json")),
            log_level: "debug".to_string(),
            log_timestamps: true,
        };

        let state = SettingsState::from_config(&config);
        let rebuilt = state.to_config();

        assert_eq!(rebuilt.game_log_file_path, config.game_log_file_path);
        assert_eq!(rebuilt.window_name, config.window_name);
        assert_eq!(rebuilt.hotkey, config.hotkey);
        assert_eq!(rebuilt.capture_delay_ms, config.capture_delay_ms);
        assert_eq!(rebuilt.prices_file_path, config.prices_file_path);
        assert_eq!(rebuilt.items_file_path, config.items_file_path);
        assert_eq!(rebuilt.log_level, config.log_level);
        assert_eq!(rebuilt.log_timestamps, config.log_timestamps);
    }

    #[test]
    fn empty_paths_clear_optional_fields() {
        let mut state = SettingsState::from_config(&Config::default());
        state.game_log_file_path = "   ".to_string();
        state.prices_file_path = "".to_string();
        state.items_file_path = "\n\t".to_string();

        let config = state.to_config();
        assert!(config.game_log_file_path.is_none());
        assert!(config.prices_file_path.is_none());
        assert!(config.items_file_path.is_none());
    }

    #[test]
    fn save_to_file_writes_config_yaml() {
        let config = Config {
            game_log_file_path: Some(PathBuf::from("/tmp/EE.log")),
            window_name: "WarframeTest".to_string(),
            hotkey: "F10".to_string(),
            capture_delay_ms: 1750,
            prices_file_path: None,
            items_file_path: None,
            log_level: "info".to_string(),
            log_timestamps: false,
        };
        let state = SettingsState::from_config(&config);
        let dir = unique_temp_dir();
        let path = dir.join("config.yaml");

        state.save_to_file(&path).expect("save failed");

        let loaded = Config::load_from_file(&path).expect("load failed");
        assert_eq!(loaded.window_name, config.window_name);
        assert_eq!(loaded.hotkey, config.hotkey);
        assert_eq!(loaded.capture_delay_ms, config.capture_delay_ms);
        assert_eq!(loaded.log_level, config.log_level);
        assert_eq!(loaded.log_timestamps, config.log_timestamps);
        assert_eq!(loaded.game_log_file_path, config.game_log_file_path);
    }
}

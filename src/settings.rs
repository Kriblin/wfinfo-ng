use std::path::{Path, PathBuf};

use anyhow::Result;
use eframe::egui;

use crate::{
    config::{CaptureMode, Config},
    utils::refresh_database_files,
};

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
    pub save_screenshots: bool,
    pub capture_mode: CaptureMode,
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
            save_screenshots: config.save_screenshots,
            capture_mode: config.capture_mode.clone(),
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
            save_screenshots: self.save_screenshots,
            capture_mode: self.capture_mode.clone(),
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

pub struct SettingsApp {
    state: SettingsState,
    status: Option<String>,
    config_path_hint: Option<String>,
}

impl SettingsApp {
    pub fn new() -> Self {
        let config_path_hint = Config::get_config_file_path()
            .ok()
            .map(|path| path.display().to_string());
        let config = Config::load().unwrap_or_else(|err| {
            eprintln!("Failed to load config: {err}");
            Config::default()
        });
        Self {
            state: SettingsState::from_config(&config),
            status: None,
            config_path_hint,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("WFinfo-ng Settings");
        if let Some(path) = &self.config_path_hint {
            ui.label(format!("Config file: {path}"));
        }
        ui.separator();

        ui.label("Game log file path");
        ui.text_edit_singleline(&mut self.state.game_log_file_path);

        ui.label("Window name");
        ui.text_edit_singleline(&mut self.state.window_name);

        ui.label("Hotkey");
        ui.text_edit_singleline(&mut self.state.hotkey);

        ui.label("Capture delay (ms)");
        ui.add(egui::DragValue::new(&mut self.state.capture_delay_ms).speed(50));

        ui.label("Prices file path");
        ui.text_edit_singleline(&mut self.state.prices_file_path);

        ui.label("Items file path");
        ui.text_edit_singleline(&mut self.state.items_file_path);

        ui.label("Log level");
        ui.text_edit_singleline(&mut self.state.log_level);

        ui.checkbox(&mut self.state.log_timestamps, "Log timestamps");
        ui.checkbox(
            &mut self.state.save_screenshots,
            "Save screenshots to test-images",
        );

        ui.separator();

        ui.label("Capture mode");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.state.capture_mode, CaptureMode::Window, "Window");
            ui.radio_value(
                &mut self.state.capture_mode,
                CaptureMode::Monitor,
                "Monitor",
            );
        });

        ui.separator();
        if ui.button("Save settings").clicked() {
            let config = self.state.to_config();
            match config.validate() {
                Ok(()) => match config.save() {
                    Ok(()) => {
                        let saved_path = Config::get_config_file_path()
                            .ok()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "unknown location".to_string());
                        self.status = Some(format!("Saved to {saved_path}"));
                    }
                    Err(err) => {
                        self.status = Some(format!("Save failed: {err}"));
                    }
                },
                Err(err) => {
                    self.status = Some(format!("Validation failed: {err}"));
                }
            }
        }

        if ui.button("Update item data").clicked() {
            let config = self.state.to_config();
            match refresh_database_files(&config) {
                Ok((prices_path, items_path)) => {
                    self.status = Some(format!(
                        "Updated item data: {}, {}",
                        prices_path.display(),
                        items_path.display()
                    ));
                }
                Err(err) => {
                    self.status = Some(format!("Update failed: {err:#}"));
                }
            }
        }

        if let Some(status) = &self.status {
            ui.label(status);
        }
    }
}

impl Default for SettingsApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ui(ui);
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
            save_screenshots: false,
            capture_mode: CaptureMode::Window,
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
        assert_eq!(rebuilt.capture_mode, config.capture_mode);
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
    fn save_to_file_writes_config_toml() {
        let config = Config {
            game_log_file_path: Some(PathBuf::from("/tmp/EE.log")),
            window_name: "WarframeTest".to_string(),
            hotkey: "F10".to_string(),
            capture_delay_ms: 1750,
            prices_file_path: None,
            items_file_path: None,
            log_level: "info".to_string(),
            log_timestamps: false,
            save_screenshots: false,
            capture_mode: CaptureMode::Window,
        };
        let state = SettingsState::from_config(&config);
        let dir = unique_temp_dir();
        let path = dir.join("config.toml");

        state.save_to_file(&path).expect("save failed");

        let loaded = Config::load_from_file(&path).expect("load failed");
        assert_eq!(loaded.window_name, config.window_name);
        assert_eq!(loaded.hotkey, config.hotkey);
        assert_eq!(loaded.capture_delay_ms, config.capture_delay_ms);
        assert_eq!(loaded.log_level, config.log_level);
        assert_eq!(loaded.log_timestamps, config.log_timestamps);
        assert_eq!(loaded.capture_mode, config.capture_mode);
        assert_eq!(loaded.game_log_file_path, config.game_log_file_path);
    }
}

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use anyhow::Result;
use eframe::egui;

use crate::{
    config::{CaptureMode, Config},
    database::cache::{
        CacheRefreshState, CachedFileRefreshResult, DatabaseRefreshResult, database_cache_status,
        refresh_database_files_with_status,
    },
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
    database_refresh_result: Option<DatabaseRefreshResult>,
    database_refresh_receiver:
        Option<mpsc::Receiver<std::result::Result<DatabaseRefreshResult, String>>>,
    database_refresh_in_progress: bool,
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
            database_refresh_result: database_cache_status(&config).ok(),
            database_refresh_receiver: None,
            database_refresh_in_progress: false,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll_database_refresh();

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
        let prices_path_response = ui.text_edit_singleline(&mut self.state.prices_file_path);

        ui.label("Items file path");
        let items_path_response = ui.text_edit_singleline(&mut self.state.items_file_path);

        if prices_path_response.changed() || items_path_response.changed() {
            self.refresh_database_cache_status();
        }

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

        ui.separator();
        ui.heading("Database cache");
        if let Some(result) = &self.database_refresh_result {
            render_cached_file_status(ui, "Prices", &result.prices);
            render_cached_file_status(ui, "Items", &result.items);
        }

        let refresh_clicked = ui
            .add_enabled(
                !self.database_refresh_in_progress,
                egui::Button::new("Refresh database"),
            )
            .clicked();
        if refresh_clicked {
            let config = self.state.to_config();
            let (sender, receiver) = mpsc::channel();
            let ctx = ui.ctx().clone();
            self.database_refresh_receiver = Some(receiver);
            self.database_refresh_in_progress = true;
            self.status = Some("Refreshing database...".to_string());
            thread::spawn(move || {
                let result =
                    refresh_database_files_with_status(&config).map_err(|err| format!("{err:#}"));
                let _ = sender.send(result);
                ctx.request_repaint();
            });
        }

        if self.database_refresh_in_progress {
            ui.label("Refresh in progress");
        }

        if let Some(status) = &self.status {
            ui.label(status);
        }
    }

    fn poll_database_refresh(&mut self) {
        let Some(receiver) = &self.database_refresh_receiver else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(result)) => {
                self.status = Some(database_refresh_summary(&result));
                self.database_refresh_result = Some(result);
                self.database_refresh_receiver = None;
                self.database_refresh_in_progress = false;
            }
            Ok(Err(err)) => {
                self.status = Some(format!("Refresh failed: {err}"));
                self.refresh_database_cache_status();
                self.database_refresh_receiver = None;
                self.database_refresh_in_progress = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = Some("Refresh failed: worker stopped before reporting".to_string());
                self.refresh_database_cache_status();
                self.database_refresh_receiver = None;
                self.database_refresh_in_progress = false;
            }
        }
    }

    fn refresh_database_cache_status(&mut self) {
        self.database_refresh_result = database_cache_status(&self.state.to_config()).ok();
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

fn render_cached_file_status(ui: &mut egui::Ui, label: &str, result: &CachedFileRefreshResult) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(label);
            ui.label(refresh_state_label(&result.state));
            if let Some(status) = result.http_status {
                ui.label(format!("HTTP {status}"));
            }
        });
        ui.label(format!("Path: {}", result.path.display()));
        ui.label(format!(
            "Version: {}",
            result.etag.as_deref().unwrap_or("No stored ETag")
        ));
        ui.label(&result.message);
    });
}

fn refresh_state_label(state: &CacheRefreshState) -> &'static str {
    match state {
        CacheRefreshState::Updated => "Updated",
        CacheRefreshState::NotModified => "Current",
        CacheRefreshState::Skipped => "Not refreshed",
        CacheRefreshState::Failed => "Failed",
    }
}

fn database_refresh_summary(result: &DatabaseRefreshResult) -> String {
    let updated_count = [&result.prices, &result.items]
        .iter()
        .filter(|file| file.state == CacheRefreshState::Updated)
        .count();
    let failed_count = [&result.prices, &result.items]
        .iter()
        .filter(|file| file.state == CacheRefreshState::Failed)
        .count();

    if failed_count > 0 {
        format!("Refresh completed with {failed_count} failed file(s)")
    } else if updated_count > 0 {
        format!("Refresh complete: {updated_count} file(s) updated")
    } else {
        "Refresh complete: database already current".to_string()
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

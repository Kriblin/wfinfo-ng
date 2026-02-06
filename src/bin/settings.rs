use std::error::Error;

use eframe::egui;

use wfinfo::{config::Config, settings::SettingsState};

struct SettingsApp {
    state: SettingsState,
    status: Option<String>,
    config_path_hint: Option<String>,
}

impl SettingsApp {
    fn new() -> Self {
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
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
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

            if let Some(status) = &self.status {
                ui.label(status);
            }
        });
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "WFinfo-ng Settings",
        options,
        Box::new(|_cc| Ok(Box::new(SettingsApp::new()))),
    )
    .map_err(|e| Box::new(e) as Box<dyn Error>)
}

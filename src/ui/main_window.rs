use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use eframe::egui;
use log::debug;

use crate::{
    capture::source::find_window_by_title,
    config::CaptureMode,
    theme::Theme,
    ui::{
        overlay::{OverlayState, Reward, draw_overlay, draw_reward_list},
        settings::SettingsApp,
        state::{
            DetectionState, LastCaptureState, MainUiState, WindowCaptureState, WindowTitleState,
        },
    },
};

pub struct MainApp {
    overlay_state: OverlayState,
    last_capture_state: LastCaptureState,
    reward_receiver: mpsc::Receiver<(Vec<Reward>, Theme, Vec<String>)>,
    ui_state: MainUiState,
    window_title_state: WindowTitleState,
    window_capture_state: WindowCaptureState,
    window_title_input: String,
    capture_mode: CaptureMode,
    overlay_enabled: bool,
    overlay_active: bool,
    settings_open: Arc<AtomicBool>,
    settings_app: Arc<Mutex<SettingsApp>>,
    last_window_check: std::time::Instant,
}

impl MainApp {
    pub fn new(
        reward_receiver: mpsc::Receiver<(Vec<Reward>, Theme, Vec<String>)>,
        detection: DetectionState,
        window_title_state: WindowTitleState,
        window_capture_state: WindowCaptureState,
        capture_mode: CaptureMode,
    ) -> Self {
        let window_title_input = window_title_state.get();
        if capture_mode == CaptureMode::Monitor {
            window_capture_state.set_found("Primary Monitor".to_string());
        }

        Self {
            overlay_state: OverlayState::new(),
            last_capture_state: LastCaptureState::default(),
            reward_receiver,
            ui_state: MainUiState::new(detection),
            window_title_state,
            window_capture_state,
            window_title_input,
            capture_mode,
            overlay_enabled: true,
            overlay_active: false,
            settings_open: Arc::new(AtomicBool::new(false)),
            settings_app: Arc::new(Mutex::new(SettingsApp::new())),
            last_window_check: std::time::Instant::now(),
        }
    }
}

impl eframe::App for MainApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let check_interval = Duration::from_secs(3);
        let elapsed = self.last_window_check.elapsed();
        if self.capture_mode == CaptureMode::Window {
            if elapsed >= check_interval {
                let current_title = self.window_title_state.get();
                find_window_by_title(&current_title, &self.window_capture_state);
                self.last_window_check = std::time::Instant::now();
                ctx.request_repaint_after(check_interval);
            } else {
                ctx.request_repaint_after(check_interval - elapsed);
            }
        }

        let did_update = if let Ok((rewards, _, _)) = self.reward_receiver.try_recv() {
            self.overlay_state.rewards = rewards;
            self.overlay_state.last_update = Some(std::time::Instant::now());
            true
        } else {
            false
        };

        if did_update {
            self.overlay_active = true;
            ctx.request_repaint();
            debug!(
                "Main UI received update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
                self.overlay_state.rewards.len(),
                self.overlay_state.rewards,
                self.overlay_state.last_update.map(|time| time.elapsed()),
            );
        }

        if self.overlay_active
            && let Some(last_update) = self.overlay_state.last_update
        {
            let timeout = Duration::from_secs(10);
            if last_update.elapsed() >= timeout {
                self.overlay_state.rewards.clear();
                self.overlay_active = false;
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(timeout - last_update.elapsed());
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            ui.heading("WFinfo-ng");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    self.ui_state.open_settings();
                    self.settings_open.store(true, Ordering::SeqCst);
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.strong(self.ui_state.detection.status_label());
            ui.separator();
            ui.label("Capture:");
            ui.strong(self.window_capture_state.status_label());
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Window:");
            let response = ui.text_edit_singleline(&mut self.window_title_input);
            if response.changed() && self.capture_mode == CaptureMode::Window {
                debug!("Window title changed to {}", self.window_title_input);
                let new_title = self.window_title_input.trim().to_string();
                self.window_title_state.set(new_title.clone());
                self.window_capture_state.set_not_found(new_title);
            } else if response.changed() {
                let new_title = self.window_title_input.trim().to_string();
                self.window_title_state.set(new_title);
            }

            if self.capture_mode == CaptureMode::Window && ui.button("Refresh").clicked() {
                let current_title = self.window_title_state.get();
                find_window_by_title(&current_title, &self.window_capture_state);
            } else if self.capture_mode == CaptureMode::Monitor && ui.button("Refresh").clicked() {
                self.window_capture_state
                    .set_found("Primary Monitor".to_string());
            }
        });

        if let Some(error) = &self.ui_state.last_error {
            ui.colored_label(egui::Color32::RED, error);
        }

        if self.settings_open.load(Ordering::SeqCst) {
            let settings_open = self.settings_open.clone();
            let settings_app = self.settings_app.clone();
            let settings_builder = egui::ViewportBuilder::default()
                .with_title("WFinfo-ng Settings")
                .with_inner_size([600.0, 500.0]);

            ui.ctx().show_viewport_deferred(
                egui::ViewportId::from_hash_of("wfinfo-settings"),
                settings_builder,
                move |viewport_ui, _class| {
                    if viewport_ui
                        .ctx()
                        .input(|input| input.viewport().close_requested())
                    {
                        settings_open.store(false, Ordering::SeqCst);
                        viewport_ui.ctx().request_repaint();
                        return;
                    }

                    egui::CentralPanel::default().show(viewport_ui, |ui| {
                        match settings_app.lock() {
                            Ok(mut settings_app) => settings_app.ui(ui),
                            Err(_) => {
                                ui.colored_label(
                                    egui::Color32::RED,
                                    "Settings UI state is unavailable",
                                );
                            }
                        }
                    });
                },
            );
        }

        if self.overlay_active {
            let overlay_snapshot = self.overlay_state.clone();
            let overlay_builder = egui::ViewportBuilder::default()
                .with_title("WFinfo-ng Overlay")
                .with_always_on_top()
                .with_decorations(false)
                .with_transparent(true)
                .with_mouse_passthrough(true);

            ui.ctx().show_viewport_deferred(
                egui::ViewportId::from_hash_of("wfinfo-overlay"),
                overlay_builder,
                move |viewport_ui, _class| {
                    egui::CentralPanel::default().show(viewport_ui, |ui| {
                        draw_overlay(ui, &overlay_snapshot);
                    });
                },
            );
        }
    }
}

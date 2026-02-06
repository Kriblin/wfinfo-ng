use std::error::Error;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::thread::sleep;
use std::time::Duration;
use std::{fs::File, thread};
use std::{path::PathBuf, sync::mpsc};

use clap::Parser;
use eframe::egui;
use env_logger::{Builder, Env};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use image::DynamicImage;
use log::{debug, error, info, warn};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use xcap::Window;

use wfinfo::{
    config::{Args, Config},
    database::{Database, Item},
    main_ui::{DetectionState, MainUiState, SettingsLauncher, WindowTitleState},
    ocr::{OCR, normalize_string, reward_image_to_reward_names},
    overlay::{OverlayState, Reward, draw_overlay},
    utils::fetch_prices_and_items,
};

fn run_detection(capturer: &Window, db: &Database) -> Result<Vec<Reward>, wfinfo::error::Error> {
    let frame = capturer
        .capture_image()
        .map_err(|e| wfinfo::error::OcrError::CaptureError(e.to_string()))?;
    info!("Captured");
    let image = DynamicImage::ImageRgba8(frame);
    info!("Converted");
    let text = reward_image_to_reward_names(image, None)?;
    let text: Vec<String> = text.iter().map(|s| normalize_string(s)).collect();
    debug!("{:#?}", text);

    let items: Vec<Option<&Item>> = text.iter().map(|s| db.find_item(s, None)).collect();

    let best = items
        .iter()
        .map(|item| {
            item.map(|item| {
                item.platinum
                    .max(item.ducats as f32 / 10.0 + item.platinum / 100.0)
            })
            .unwrap_or(0.0)
        })
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|best| best.0);

    let mut rewards = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(item) = item {
            info!(
                "{}\n\t{}\t{}\t{}",
                item.drop_name,
                item.platinum,
                item.ducats as f32 / 10.0,
                if Some(index) == best { "<----" } else { "" }
            );
            rewards.push(Reward {
                name: item.drop_name.clone(),
                platinum: item.platinum,
                ducats: item.ducats,
                is_best: Some(index) == best,
            });
        } else {
            warn!("Unknown item\n\tUnknown");
        }
    }

    Ok(rewards)
}

fn log_watcher(path: PathBuf, event_sender: mpsc::Sender<()>, capture_delay_ms: u64) {
    debug!("Path: {}", path.display());
    let mut position = match File::open(&path) {
        Ok(mut file) => match file.seek(SeekFrom::End(0)) {
            Ok(pos) => pos,
            Err(err) => {
                error!("Failed to seek to end of file {}: {}", path.display(), err);
                return;
            }
        },
        Err(err) => {
            error!("Failed to open file {}: {}", path.display(), err);
            return;
        }
    };

    thread::spawn(move || {
        debug!("Position: {}", position);

        let (tx, rx) = mpsc::channel();
        let watcher_config =
            notify::Config::default().with_poll_interval(Duration::from_millis(100));
        let mut watcher = match RecommendedWatcher::new(tx, watcher_config) {
            Ok(watcher) => watcher,
            Err(err) => {
                error!("Failed to create file watcher: {}", err);
                return;
            }
        };

        if let Err(err) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            error!("Failed to watch file {}: {}", path.display(), err);
            return;
        }

        loop {
            match rx.recv() {
                Ok(event) => {
                    if event.unwrap().kind.is_modify() {
                        let mut f = match File::open(&path) {
                            Ok(file) => file,
                            Err(err) => {
                                error!("Failed to open file {}: {}", path.display(), err);
                                continue;
                            }
                        };

                        if let Err(err) = f.seek(SeekFrom::Start(position)) {
                            error!(
                                "Failed to seek to position {} in file {}: {}",
                                position,
                                path.display(),
                                err
                            );
                            continue;
                        }

                        let mut reward_screen_detected = false;

                        let reader = BufReader::new(f.by_ref());
                        for line in reader.lines() {
                            let line = match line {
                                Ok(line) => line,
                                Err(err) => {
                                    error!("Error reading line: {}", err);
                                    continue;
                                }
                            };
                            // debug!("> {:?}", line);
                            if line.contains("Pause countdown done")
                                || line.contains("Got rewards")
                                || line
                                    .contains("Created /Lotus/Interface/ProjectionRewardChoice.swf")
                            {
                                reward_screen_detected = true;
                            }
                        }

                        if reward_screen_detected {
                            info!("Detected, waiting for {} ms...", capture_delay_ms);
                            sleep(Duration::from_millis(capture_delay_ms));
                            if let Err(err) = event_sender.send(()) {
                                error!("Failed to send event: {}", err);
                            }
                        }

                        position = match f.metadata() {
                            Ok(metadata) => metadata.len(),
                            Err(err) => {
                                error!("Failed to get file metadata: {}", err);
                                continue;
                            }
                        };
                        debug!("Log position: {}", position);
                    }
                }
                Err(err) => {
                    error!("Error: {:?}", err);
                }
            }
        }
    });
}

fn hotkey_watcher(hotkey: HotKey, event_sender: mpsc::Sender<()>) {
    debug!("Watching hotkey: {hotkey:?}");
    thread::spawn(move || {
        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => manager,
            Err(err) => {
                error!("Failed to create hotkey manager: {}", err);
                return;
            }
        };

        if let Err(err) = manager.register(hotkey) {
            error!("Failed to register hotkey {:?}: {}", hotkey, err);
            return;
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
            debug!("Hotkey event: {:?}", event);
            if event.state == HotKeyState::Pressed {
                if let Err(err) = event_sender.send(()) {
                    error!("Failed to send event: {}", err);
                }
            }
        }
    });
}

struct ProcessSettingsLauncher {
    settings_executable: PathBuf,
}

impl ProcessSettingsLauncher {
    fn new() -> Self {
        let settings_executable = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("settings")))
            .unwrap_or_else(|| PathBuf::from("settings"));
        Self {
            settings_executable,
        }
    }
}

impl SettingsLauncher for ProcessSettingsLauncher {
    fn open_settings(&self) -> Result<(), String> {

        let settings_executable = self.settings_executable.clone();

        std::thread::spawn(move || {
            std::process::Command::new(settings_executable)
                .spawn()
                .expect("Failed to start settings process")
                .wait()
                .expect("Failed to start settings process");
        });
        Ok(())
    }
}

struct MainApp<L: SettingsLauncher> {
    overlay_state: OverlayState,
    reward_receiver: mpsc::Receiver<Vec<Reward>>,
    ui_state: MainUiState<L>,
    window_title_state: WindowTitleState,
    window_title_input: String,
    overlay_active: bool,
}

impl<L: SettingsLauncher> MainApp<L> {
    fn new(
        reward_receiver: mpsc::Receiver<Vec<Reward>>,
        detection: DetectionState,
        launcher: L,
        window_title_state: WindowTitleState,
    ) -> Self {
        let window_title_input = window_title_state.get();
        Self {
            overlay_state: OverlayState::new(),
            reward_receiver,
            ui_state: MainUiState::new(detection, launcher),
            window_title_state,
            window_title_input,
            overlay_active: false,
        }
    }
}

impl<L: SettingsLauncher> eframe::App for MainApp<L> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let did_update = self.overlay_state.try_receive(&self.reward_receiver);
        self.overlay_state
            .clear_if_timed_out(Duration::from_secs(10));

        if did_update {
            self.overlay_active = true;
            debug!(
                "Main UI received update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
                self.overlay_state.rewards.len(),
                self.overlay_state.rewards,
                self.overlay_state.last_update.map(|t| t.elapsed()),
            );
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("WFinfo-ng");
            ui.horizontal(|ui| {
                ui.label("Detection status:");
                ui.strong(self.ui_state.detection.status_label());
            });

            ui.separator();
            ui.label("Window title");
            let response = ui.text_edit_singleline(&mut self.window_title_input);
            if response.changed() {
                debug!("Window title changed to {}", self.window_title_input);
                self.window_title_state
                    .set(self.window_title_input.trim().to_string());
            }

            if let Some(error) = &self.ui_state.last_error {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                if ui.button("Settings").clicked() {
                    let _ = self.ui_state.open_settings();
                }
            });
        });

        if self.overlay_active {
            let overlay_snapshot = self.overlay_state.clone();
            let overlay_builder = egui::ViewportBuilder::default()
                .with_title("WFinfo-ng Overlay")
                .with_always_on_top()
                .with_decorations(false)
                .with_transparent(true)
                .with_mouse_passthrough(true);

            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("wfinfo-overlay"),
                overlay_builder,
                move |overlay_ctx, _class| {
                    draw_overlay(overlay_ctx, &overlay_snapshot);
                },
            );
        }
    }
}

#[allow(dead_code)]
fn benchmark() -> Result<(), Box<dyn Error>> {
    for _ in 0..10 {
        let image = image::open("input3.png").map_err(|e| Box::<dyn Error>::from(e))?;
        println!("Converted");
        let text = reward_image_to_reward_names(image, None)?;
        println!("got names");
        let text = text.iter().map(|s| normalize_string(s));
        println!("{:#?}", text);
    }

    // Clean up OCR resources
    if let Ok(mut guard) = OCR.lock() {
        if let Some(ocr) = guard.take() {
            drop(ocr);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command-line arguments
    let args = Args::parse();

    // Load configuration from file
    let config_path = match &args.config_file {
        Some(path) => path.clone(),
        None => Config::get_config_file_path()?,
    };

    let mut config = Config::load_from_file(&config_path).unwrap_or_else(|err| {
        eprintln!("Error loading config file: {}", err);
        eprintln!("Using default configuration");
        Config::default()
    });

    // Update configuration with command-line arguments
    config.update_from_args(&args);

    // Validate configuration
    if let Err(err) = config.validate() {
        eprintln!("Configuration error: {}", err);
        return Err(err.into());
    }

    // Set up logging
    let env = Env::default()
        .filter_or("WFINFO_LOG", &config.log_level)
        .write_style_or("WFINFO_STYLE", "always");
    let mut builder = Builder::from_env(env);

    if config.log_timestamps {
        builder.format_timestamp_secs();
    } else {
        builder.format_timestamp(None);
    }

    builder
        .format_level(false)
        .format_module_path(false)
        .format_target(false)
        .init();

    let windows = Window::all()?;
    if let Some(warframe_window) = windows
        .iter()
        .find(|x| x.title().ok().as_ref() == Some(&config.window_name))
    {
        debug!(
            "Capture source resolution: {:?}x{:?}",
            warframe_window.width().unwrap_or(0),
            warframe_window.height().unwrap_or(0)
        );
    } else {
        warn!(
            "Warframe window with title '{}' not found. Update the title in the main UI to continue.",
            config.window_name
        );
    }

    // Use configured file paths if provided, otherwise download the data
    let (prices_path, items_path) =
        if config.prices_file_path.is_some() && config.items_file_path.is_some() {
            info!("Using configured database file paths");
            (
                config.prices_file_path.as_ref().map(|p| p.clone()),
                config.items_file_path.as_ref().map(|p| p.clone()),
            )
        } else {
            info!("Downloading database files");
            let (prices, items) = fetch_prices_and_items()?;
            (Some(prices), Some(items))
        };

    let db = Database::load_from_file(
        prices_path.as_ref().map(|p| p.as_path()),
        items_path.as_ref().map(|p| p.as_path()),
    )
    .map_err(|e| Box::<dyn Error>::from(e))?;

    info!("Loaded database");

    let (event_sender, event_receiver) = mpsc::channel();
    let (reward_sender, reward_receiver) = mpsc::channel();
    let detection_state = DetectionState::new();

    if let Some(log_path) = &config.game_log_file_path {
        log_watcher(
            log_path.clone(),
            event_sender.clone(),
            config.capture_delay_ms,
        );
    } else {
        warn!("No game log file path specified, automatic detection disabled");
    }

    hotkey_watcher(config.hotkey.parse()?, event_sender);

    let window_title_state = WindowTitleState::new(config.window_name.clone());
    let detection_state_thread = detection_state.clone();
    let window_title_thread = window_title_state.clone();
    thread::spawn(move || {
        while let Ok(()) = event_receiver.recv() {
            info!("Capturing");
            detection_state_thread.set_running(true);
            let windows = match Window::all() {
                Ok(windows) => windows,
                Err(e) => {
                    error!("Failed to get windows: {}", e);
                    detection_state_thread.set_running(false);
                    continue;
                }
            };
            let current_title = window_title_thread.get();
            let Some(warframe_window) = windows
                .iter()
                .find(|x| x.title().ok().as_ref() == Some(&current_title))
            else {
                error!("Warframe window not found during detection");
                detection_state_thread.set_running(false);
                continue;
            };

            match run_detection(warframe_window, &db) {
                Ok(rewards) => {
                    if let Err(e) = reward_sender.send(rewards) {
                        error!("Failed to send rewards to UI: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error during detection: {}", e);
                }
            }
            detection_state_thread.set_running(false);
        }
    });

    let options = eframe::NativeOptions::default();
    let detection_state_ui = detection_state.clone();
    let window_title_ui = window_title_state.clone();

    eframe::run_native(
        "WFinfo-ng",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(MainApp::new(
                reward_receiver,
                detection_state_ui,
                ProcessSettingsLauncher::new(),
                window_title_ui,
            )))
        }),
    )
    .map_err(|e| Box::new(e) as Box<dyn Error>)?;

    // Clean up OCR resources
    if let Ok(mut guard) = OCR.lock() {
        if let Some(ocr) = guard.take() {
            drop(ocr);
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use image::ImageReader;
    use indexmap::IndexMap;
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::fs::read_to_string;
    use tesseract::Tesseract;
    use wfinfo::ocr::detect_theme;
    use wfinfo::ocr::extract_parts;
    use wfinfo::testing::Label;

    use super::*;

    #[test]
    fn single_image() -> Result<(), Box<dyn Error>> {
        let image = ImageReader::open(format!("test-images/{}.png", 1))
            .map_err(|e| format!("Failed to open image: {}", e))?
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let text = reward_image_to_reward_names(image, None)?;
        let text = text.iter().map(|s| normalize_string(s));
        println!("{:#?}", text);

        let db = Database::load_from_file(None, None)?;
        let items: Vec<_> = text.map(|s| db.find_item(&s, None)).collect();
        println!("{:#?}", items);

        assert_eq!(
            items[0].ok_or("Didn't find item 0")?.drop_name,
            "Octavia Prime Systems Blueprint"
        );
        assert_eq!(
            items[1].ok_or("Didn't find item 1")?.drop_name,
            "Octavia Prime Blueprint"
        );
        assert_eq!(
            items[2].ok_or("Didn't find item 2")?.drop_name,
            "Tenora Prime Blueprint"
        );
        assert_eq!(
            items[3].ok_or("Didn't find item 3")?.drop_name,
            "Harrow Prime Systems Blueprint"
        );

        Ok(())
    }

    // #[test]
    #[allow(dead_code)]
    fn wfi_images_exact() -> Result<(), Box<dyn Error>> {
        let labels: IndexMap<String, Label> = serde_json::from_str(
            &read_to_string("WFI test images/labels.json")
                .map_err(|e| format!("Failed to read labels file: {}", e))?,
        )?;

        for (filename, label) in labels {
            let image = ImageReader::open("WFI test images/".to_string() + &filename)
                .map_err(|e| format!("Failed to open image {}: {}", filename, e))?
                .decode()
                .map_err(|e| format!("Failed to decode image {}: {}", filename, e))?;

            let text = reward_image_to_reward_names(image, None)?;
            let text: Vec<_> = text.iter().map(|s| normalize_string(s)).collect();
            println!("{:#?}", text);

            let db = Database::load_from_file(None, None)?;
            let items: Vec<_> = text.iter().map(|s| db.find_item(s, None)).collect();
            println!("{:#?}", items);
            println!("{}", filename);

            let item_names = items
                .iter()
                .map(|item| item.map(|item| item.drop_name.clone()));

            for (result, expectation) in item_names.zip(label.items) {
                if expectation.is_empty() {
                    assert_eq!(result, None)
                } else {
                    assert_eq!(result, Some(expectation))
                }
            }
        }

        Ok(())
    }

    #[test]
    fn wfi_images_99_percent() -> Result<(), Box<dyn Error>> {
        // Skip this dataset-heavy test when images are not present in the repo
        let has_images = std::fs::read_dir("WFI test images")
            .ok()
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .any(|p| matches!(p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()), Some(ext) if ext == "png" || ext == "jpg" || ext == "jpeg"))
            })
            .unwrap_or(false);
        if !has_images {
            eprintln!("Skipping wfi_images_99_percent: no images found in 'WFI test images/'");
            return Ok(());
        }

        let labels: BTreeMap<String, Label> = serde_json::from_str(
            &read_to_string("WFI test images/labels.json")
                .map_err(|e| format!("Failed to read labels file: {}", e))?,
        )?;

        let total = labels.len();
        let success_count: usize = labels
            .into_par_iter()
            .map(
                |(filename, label)| -> Result<usize, Box<dyn Error + Send + Sync>> {
                    let image = ImageReader::open("WFI test images/".to_string() + &filename)
                        .map_err(|e| format!("Failed to open image {}: {}", filename, e))?
                        .decode()
                        .map_err(|e| format!("Failed to decode image {}: {}", filename, e))?;

                    let text = match reward_image_to_reward_names(image, None) {
                        Ok(text) => text,
                        Err(e) => {
                            println!("Error processing image {}: {}", filename, e);
                            return Ok(0);
                        }
                    };

                    let text: Vec<_> = text.iter().map(|s| normalize_string(s)).collect();
                    println!("{:#?}", text);

                    let db = match Database::load_from_file(None, None) {
                        Ok(db) => db,
                        Err(e) => {
                            println!("Error loading database for image {}: {}", filename, e);
                            return Ok(0);
                        }
                    };

                    let items: Vec<_> = text.iter().map(|s| db.find_item(s, None)).collect();
                    println!("{:#?}", items);
                    println!("{}", filename);

                    let item_names = items
                        .iter()
                        .map(|item| item.map(|item| item.drop_name.clone()));

                    if item_names.zip(label.items).all(|(result, expectation)| {
                        expectation == result.unwrap_or_else(|| "".to_string())
                    }) {
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                },
            )
            .filter_map(Result::ok)
            .sum();

        let success_rate = success_count as f32 / total as f32;
        assert!(success_rate > 0.95, "Success rate: {success_rate}");

        Ok(())
    }

    #[test]
    #[allow(dead_code)]
    fn images() -> Result<(), Box<dyn Error>> {
        let tests = [1];
        for i in tests {
            let image_path = format!("test-images/{}.png", i);
            let image = ImageReader::open(&image_path)
                .map_err(|e| format!("Failed to open image {}: {}", image_path, e))?
                .decode()
                .map_err(|e| format!("Failed to decode image {}: {}", image_path, e))?;

            let theme = detect_theme(&image)
                .map_err(|e| format!("Failed to detect theme for image {}: {}", image_path, e))?;
            println!("Theme: {:?}", theme);

            let parts = extract_parts(&image, theme);

            let mut ocr = Tesseract::new(None, Some("eng"))
                .map_err(|e| format!("Could not initialize Tesseract: {}", e))?;

            for part in parts {
                let buffer = part
                    .as_flat_samples_u8()
                    .ok_or("Failed to get flat samples")?;

                ocr = ocr
                    .set_frame(
                        buffer.samples,
                        part.width() as i32,
                        part.height() as i32,
                        3,
                        3 * part.width() as i32,
                    )
                    .map_err(|e| format!("Failed to set image: {}", e))?;

                let text = ocr
                    .get_text()
                    .map_err(|e| format!("Failed to get text: {}", e))?;
                println!("{}", text);
            }
            println!("=================");
        }

        Ok(())
    }
}

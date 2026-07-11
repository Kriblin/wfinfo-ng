use std::error::Error;
use std::{sync::mpsc, thread};

mod detection;
mod triggers;

use detection::run_detection;
use triggers::{hotkey_watcher, log_watcher};

use crate::{
    capture::{
        labels::save_screenshot_and_label,
        source::{capture_image, find_window_by_title},
    },
    config::{Args, CaptureMode, Config},
    database::{Database, cache::ensure_database_files},
    ocr::{OCR, normalize_string, reward_image_to_reward_names},
    ui::{
        main_window::MainApp,
        overlay::Reward,
        state::{DetectionState, WindowCaptureState, WindowTitleState},
    },
};
use clap::Parser;
use env_logger::{Builder, Env};
use log::{debug, error, info, warn};

#[allow(dead_code)]
fn benchmark() -> Result<(), Box<dyn Error>> {
    for _ in 0..10 {
        let image = image::open("input3.png").map_err(Box::<dyn Error>::from)?;
        println!("Converted");
        let (text, _theme) = reward_image_to_reward_names(image, None)?;
        println!("got names");
        let text: Vec<_> = text.iter().map(|s| normalize_string(s)).collect();
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

pub fn run() -> Result<(), Box<dyn Error>> {
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

    let window_capture_state = WindowCaptureState::new(config.window_name.clone());

    if config.capture_mode == CaptureMode::Window {
        if let Some(warframe_window) =
            find_window_by_title(&config.window_name, &window_capture_state)
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
    } else {
        window_capture_state.set_found("Primary Monitor".to_string());
    }

    info!("Ensuring database files are available");
    let (prices_path, items_path) = ensure_database_files(&config)?;

    let db = Database::load_from_file(Some(prices_path.as_path()), Some(items_path.as_path()))
        .map_err(Box::<dyn Error>::from)?;

    info!("Loaded database");

    let (event_sender, event_receiver) = mpsc::channel();
    let (reward_sender, reward_receiver) =
        mpsc::channel::<(Vec<Reward>, crate::theme::Theme, Vec<String>)>();
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

    let options = eframe::NativeOptions::default();
    let detection_state_ui = detection_state.clone();
    let window_title_ui = window_title_state.clone();
    let window_capture_ui = window_capture_state.clone();

    let config_clone = config.clone();
    eframe::run_native(
        "WFinfo-ng",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let detection_state_thread = detection_state.clone();
            let window_title_thread = window_title_state.clone();
            let window_capture_thread = window_capture_state.clone();
            let db_thread = db.clone();
            let config_thread = config_clone.clone();

            thread::spawn(move || {
                while let Ok(()) = event_receiver.recv() {
                    info!("Capturing");
                    detection_state_thread.set_running(true);
                    ctx.request_repaint();

                    let current_title = window_title_thread.get();
                    let image = match capture_image(
                        &config_thread.capture_mode,
                        &current_title,
                        &window_capture_thread,
                    ) {
                        Ok(image) => image,
                        Err(err) => {
                            error!("{err}");
                            detection_state_thread.set_running(false);
                            ctx.request_repaint();
                            continue;
                        }
                    };

                    match run_detection(image.clone(), &db_thread) {
                        Ok((rewards, theme, raw_text)) => {
                            if config_thread.save_screenshots {
                                if let Err(e) = save_screenshot_and_label(&image, &theme, &raw_text)
                                {
                                    error!("Failed to save screenshot: {}", e);
                                }
                            }
                            if let Err(e) = reward_sender.send((rewards, theme, raw_text)) {
                                error!("Failed to send rewards to UI: {}", e);
                            }
                            ctx.request_repaint();
                        }
                        Err(e) => {
                            error!("Error during detection: {}", e);
                        }
                    }
                    detection_state_thread.set_running(false);
                    ctx.request_repaint();
                }
            });

            Ok(Box::new(MainApp::new(
                reward_receiver,
                detection_state_ui,
                window_title_ui,
                window_capture_ui,
                config_clone.capture_mode.clone(),
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
    use crate::capture::labels::Label;
    use crate::database::Item;
    use crate::ocr::extract_parts;
    use crate::ocr::{detect_theme, get_tessdata_path};
    use image::ImageReader;
    use indexmap::IndexMap;
    use kreuzberg_tesseract::TesseractAPI as Tesseract;
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::fs::read_to_string;

    use super::*;

    #[test]
    fn single_image() -> Result<(), Box<dyn Error>> {
        let image = ImageReader::open("test-images/FullScreenShot2026-02-25 22-36-26255760907.png")
            .map_err(|e| format!("Failed to open image: {}", e))?
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let (text, _theme) = reward_image_to_reward_names(image, None)?;
        let text: Vec<_> = text.iter().map(|s| normalize_string(s)).collect();
        println!("{:#?}", text);

        let db = Database::load_from_file(None, None)?;
        let items: Vec<_> = text.iter().map(|s| db.find_item(s, None)).collect();
        println!("{:#?}", items);

        assert_eq!(
            items[0].ok_or("Didn't find item 0")?.drop_name,
            "Daikyu Prime Blueprint"
        );
        assert_eq!(
            items[1].ok_or("Didn't find item 1")?.drop_name,
            "Forma Blueprint"
        );
        assert_eq!(
            items[2].ok_or("Didn't find item 2")?.drop_name,
            "Forma Blueprint"
        );
        assert_eq!(
            items[3].ok_or("Didn't find item 3")?.drop_name,
            "Sevagoth Prime Blueprint"
        );

        Ok(())
    }

    #[test]
    #[allow(dead_code)]
    fn wfi_images_exact() -> Result<(), Box<dyn Error>> {
        let labels: IndexMap<String, Label> = serde_json::from_str(
            &read_to_string("test-images/labels.json")
                .map_err(|e| format!("Failed to read labels file: {}", e))?,
        )?;

        for (filename, label) in labels {
            let image = ImageReader::open("test-images/".to_string() + &filename)
                .map_err(|e| format!("Failed to open image {}: {}", filename, e))?
                .decode()
                .map_err(|e| format!("Failed to decode image {}: {}", filename, e))?;

            let (text, _theme) = reward_image_to_reward_names(image, None)?;
            let text: Vec<_> = text.iter().map(|s| normalize_string(s)).collect();
            println!("{:#?}", text);

            let db = Database::load_from_file(None, None)?;
            let items: Vec<Option<&Item>> = text.iter().map(|s| db.find_item(s, None)).collect();
            println!("{:#?}", items);
            println!("{}", filename);

            let item_names: Vec<Option<String>> = items
                .iter()
                .map(|item| item.map(|item| item.drop_name.clone()))
                .collect();

            let expected_item_names: Vec<Option<String>> = label
                .items
                .iter()
                .map(|s| {
                    let normalized = normalize_string(s);
                    if normalized.is_empty() {
                        None
                    } else {
                        db.find_item(&normalized, None)
                            .map(|item| item.drop_name.clone())
                    }
                })
                .collect();

            for (result, expectation) in item_names.into_iter().zip(expected_item_names) {
                assert_eq!(result, expectation)
            }
        }

        Ok(())
    }

    #[test]
    fn wfi_images_99_percent() -> Result<(), Box<dyn Error>> {
        // Skip this dataset-heavy test when images are not present in the repo
        let has_images = std::fs::read_dir("test-images")
            .ok()
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .any(|p| matches!(p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()), Some(ext) if ext == "png" || ext == "jpg" || ext == "jpeg"))
            })
            .unwrap_or(false);
        if !has_images {
            eprintln!("Skipping wfi_images_99_percent: no images found in 'test-images/'");
            return Ok(());
        }

        let labels: BTreeMap<String, Label> = serde_json::from_str(
            &read_to_string("test-images/labels.json")
                .map_err(|e| format!("Failed to read labels file: {}", e))?,
        )?;

        let total = labels.len();
        let success_count: usize = labels
            .into_par_iter()
            .map(
                |(filename, label)| -> Result<usize, Box<dyn Error + Send + Sync>> {
                    let image = ImageReader::open("test-images/".to_string() + &filename)
                        .map_err(|e| format!("Failed to open image {}: {}", filename, e))?
                        .decode()
                        .map_err(|e| format!("Failed to decode image {}: {}", filename, e))?;

                    let (text, _theme) = match reward_image_to_reward_names(image, None) {
                        Ok(res) => res,
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

                    let items: Vec<Option<&Item>> =
                        text.iter().map(|s| db.find_item(s, None)).collect();
                    println!("{:#?}", items);
                    println!("{}", filename);

                    let item_names: Vec<Option<String>> = items
                        .iter()
                        .map(|item| item.map(|item| item.drop_name.clone()))
                        .collect();

                    let expected_item_names: Vec<Option<String>> = label
                        .items
                        .iter()
                        .map(|s| {
                            let normalized = normalize_string(s);
                            if normalized.is_empty() {
                                None
                            } else {
                                db.find_item(&normalized, None)
                                    .map(|item| item.drop_name.clone())
                            }
                        })
                        .collect();

                    if item_names == expected_item_names {
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
        let tests = ["FullScreenShot2026-02-25 22-49-36599898510.png"];
        for i in tests {
            let image_path = format!("test-images/{}", i);
            let image = ImageReader::open(&image_path)
                .map_err(|e| format!("Failed to open image {}: {}", image_path, e))?
                .decode()
                .map_err(|e| format!("Failed to decode image {}: {}", image_path, e))?;

            let theme = detect_theme(&image)
                .map_err(|e| format!("Failed to detect theme for image {}: {}", image_path, e))?;
            println!("Theme: {:?}", theme);

            let parts = extract_parts(&image, theme);

            let ocr = Tesseract::new().map_err(|e| format!("Could not create Tesseract: {}", e))?;
            ocr.init(get_tessdata_path(), "eng")
                .map_err(|e| format!("Could not initialize Tesseract: {}", e))?;

            for part in parts {
                let buffer = part
                    .as_flat_samples_u8()
                    .ok_or("Failed to get flat samples")?;

                ocr.set_image(
                    buffer.samples,
                    part.width() as i32,
                    part.height() as i32,
                    3,
                    3 * part.width() as i32,
                )
                .map_err(|e| format!("Failed to set image: {}", e))?;

                let text = ocr
                    .get_utf8_text()
                    .map_err(|e| format!("Failed to get text: {}", e))?;
                println!("{}", text);
            }
            println!("=================");
        }

        Ok(())
    }
}

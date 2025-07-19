use std::{fs::write, path::PathBuf};

use image::ImageReader;
use indexmap::IndexMap;
use wfinfo::{
    database::Database,
    ocr::{detect_theme, normalize_string, reward_image_to_reward_names},
    testing::Label,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut labels = IndexMap::new();

    for argument in std::env::args().skip(1) {
        let filepath = PathBuf::from(argument);
        println!("Processing file: {:?}", filepath);

        // Open and decode the image
        let image = match ImageReader::open(&filepath) {
            Ok(reader) => match reader.decode() {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("Error decoding image {:?}: {}", filepath, e);
                    continue; // Skip this file and continue with the next one
                }
            },
            Err(e) => {
                eprintln!("Error opening image {:?}: {}", filepath, e);
                continue; // Skip this file and continue with the next one
            }
        };

        // Process the image with OCR
        let detections = match reward_image_to_reward_names(image.clone(), None) {
            Ok(detections) => detections,
            Err(e) => {
                eprintln!("Error processing image {:?} with OCR: {}", filepath, e);
                continue; // Skip this file and continue with the next one
            }
        };
        println!("{:#?}", detections);

        let text: Vec<_> = detections.iter().map(|s| normalize_string(s)).collect();
        println!("{:#?}", text);

        // Load the database
        let db = match Database::load_from_file(None, None) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Error loading database: {}", e);
                continue; // Skip this file and continue with the next one
            }
        };

        let items: Vec<_> = text.iter().map(|s| db.find_item(s, None)).collect();
        for item in items.iter() {
            if let Some(item) = item {
                println!("{}: {}\n", item.name, item.platinum);
            } else {
                println!("Unknown item\n");
            }
        }

        let item_names = items
            .iter()
            .map(|item| {
                item.map(|item| item.name.clone())
                    .unwrap_or_else(|| "ERROR".to_string())
            })
            .collect();

        // Detect the theme
        let theme = match detect_theme(&image) {
            Ok(theme) => theme,
            Err(e) => {
                eprintln!("Error detecting theme for {:?}: {}", filepath, e);
                continue; // Skip this file and continue with the next one
            }
        };

        // Get the filename
        let filename = match filepath.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                eprintln!("Error getting filename from {:?}", filepath);
                continue; // Skip this file and continue with the next one
            }
        };

        labels.insert(
            filename,
            Label {
                theme,
                items: item_names,
            },
        );

        println!("{:?}", filepath);
    }

    // Save the labels to a JSON file
    let labels_json = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Error serializing labels to JSON: {}", e))?;

    write("labels.json", labels_json)
        .map_err(|e| format!("Error writing labels to file: {}", e))?;

    Ok(())
}

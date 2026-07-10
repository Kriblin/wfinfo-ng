use std::{fs::write, path::PathBuf};

use image::ImageReader;
use indexmap::IndexMap;
use wfinfo::{
    config::Config,
    database::Database,
    ocr::{normalize_string, reward_image_to_reward_names},
    testing::Label,
    utils::ensure_database_files,
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
        let (detections, theme) = match reward_image_to_reward_names(image.clone(), None) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Error processing image {:?} with OCR: {}", filepath, e);
                continue; // Skip this file and continue with the next one
            }
        };
        println!("{:#?}", detections);

        let text: Vec<_> = detections.iter().map(|s| normalize_string(s)).collect();
        println!("{:#?}", text);

        let (prices_path, items_path) = match ensure_database_files(&Config::default()) {
            Ok(paths) => paths,
            Err(e) => {
                eprintln!("Error updating database files: {}", e);
                continue;
            }
        };

        // Load the database
        let db = match Database::load_from_file(Some(&prices_path), Some(&items_path)) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Error loading database: {}", e);
                continue; // Skip this file and continue with the next one
            }
        };

        let items: Vec<_> = text.iter().map(|s| db.find_item(s, None)).collect();
        for item in items.iter() {
            if let Some(item) = item {
                println!("{}: {}\n", item.drop_name, item.platinum);
            } else {
                println!("Unknown item\n");
            }
        }

        let item_names = items
            .iter()
            .map(|item| {
                item.map(|item| item.drop_name.clone())
                    .unwrap_or_else(|| "ERROR".to_string())
            })
            .collect();

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

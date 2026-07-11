use std::{error::Error, fs, path::PathBuf};

use image::DynamicImage;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Serialize, Deserialize)]
pub struct Label {
    pub theme: Theme,
    pub items: Vec<String>,
}

pub fn save_screenshot_and_label(
    image: &DynamicImage,
    theme: &Theme,
    items: &[String],
) -> Result<(), Box<dyn Error>> {
    let test_images_dir = PathBuf::from("test-images");
    if !test_images_dir.exists() {
        fs::create_dir_all(&test_images_dir)?;
    }

    let timestamp = chrono::Local::now()
        .format("%Y-%m-%d %H-%M-%S%f")
        .to_string();
    let filename = format!("FullScreenShot{}.png", timestamp);
    let image_path = test_images_dir.join(&filename);

    image.save(&image_path)?;

    let labels_path = test_images_dir.join("labels.json");
    let mut labels: IndexMap<String, Label> = if labels_path.exists() {
        let content = fs::read_to_string(&labels_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        IndexMap::new()
    };

    labels.insert(
        filename,
        Label {
            theme: theme.clone(),
            items: items.to_vec(),
        },
    );

    let content = serde_json::to_string_pretty(&labels)?;
    fs::write(labels_path, content)?;

    Ok(())
}

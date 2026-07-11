use image::DynamicImage;
use log::debug;

use crate::{
    error::{OcrError, Result},
    theme::Theme,
};

use super::{OCR, detect_theme, extract_parts, image_to_string};

pub fn normalize_string(string: &str) -> String {
    string.replace(|c: char| !c.is_ascii_alphabetic(), "")
}

pub fn reward_image_to_reward_names(
    image: DynamicImage,
    theme: Option<Theme>,
) -> Result<(Vec<String>, Theme)> {
    let theme = match theme {
        Some(theme) => theme,
        None => detect_theme(&image)?,
    };

    let parts = extract_parts(&image, theme.clone());
    debug!("Extracted {} part images", parts.len());

    let mut results = Vec::new();
    let mut ocr_lock = OCR
        .lock()
        .map_err(|e| OcrError::ProcessingError(format!("Failed to lock OCR mutex: {}", e)))?;

    for part_image in parts {
        results.push(image_to_string(&mut ocr_lock, &part_image)?);
    }

    Ok((results, theme))
}

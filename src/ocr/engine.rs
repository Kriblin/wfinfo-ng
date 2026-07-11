use std::{env, path::Path, sync::Mutex};

use image::DynamicImage;
use kreuzberg_tesseract::TesseractAPI as Tesseract;
use lazy_static::lazy_static;
use log::debug;

use crate::error::{OcrError, Result};

lazy_static! {
    pub static ref OCR: Mutex<Option<Tesseract>> = {
        let ocr = Tesseract::new().expect("Failed to create Tesseract instance");
        let datapath = get_tessdata_path();
        debug!("Initializing Tesseract with datapath: {}", datapath);
        ocr.init(&datapath, "eng")
            .expect("Could not initialize Tesseract");
        Mutex::new(Some(ocr))
    };
}

pub fn get_tessdata_path() -> String {
    if let Ok(prefix) = env::var("TESSDATA_PREFIX") {
        return prefix;
    }

    let local_path = Path::new("tessdata");
    if local_path.exists() && local_path.is_dir() {
        if local_path.join("eng.traineddata").exists() {
            return local_path.to_string_lossy().to_string();
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_tessdata = exe_dir.join("tessdata");
            if exe_tessdata.exists() && exe_tessdata.is_dir() {
                if exe_tessdata.join("eng.traineddata").exists() {
                    return exe_tessdata.to_string_lossy().to_string();
                }
            }
        }
    }

    "/usr/share/tessdata".to_string()
}

pub fn image_to_string(tesseract: &mut Option<Tesseract>, image: &DynamicImage) -> Result<String> {
    let ocr = tesseract
        .as_ref()
        .ok_or_else(|| OcrError::InitializationError("Tesseract instance is None".to_string()))?;

    let buffer = image.as_flat_samples_u8().ok_or_else(|| {
        OcrError::ImageProcessingError("Failed to convert image to flat samples".to_string())
    })?;

    ocr.set_image(
        buffer.samples,
        image.width() as i32,
        image.height() as i32,
        3,
        3 * image.width() as i32,
    )
    .map_err(|e| OcrError::ProcessingError(format!("Failed to set image: {}", e)))?;

    ocr.get_utf8_text()
        .map_err(|e| OcrError::ProcessingError(format!("Failed to get text: {}", e)).into())
}

use std::io;
use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("OCR error: {0}")]
    Ocr(#[from] OcrError),

    #[error("Theme error: {0}")]
    Theme(#[from] ThemeError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Database file not found: {0:?}")]
    FileNotFound(PathBuf, Option<String>),

    #[error("Invalid database format: {0}")]
    InvalidFormat(String),

    #[error("Item not found in database: {0}")]
    ItemNotFound(String),

    #[error("Failed to load database: {0}")]
    LoadError(String),

    #[error("Database error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("Failed to initialize OCR engine: {0}")]
    InitializationError(String),

    #[error("OCR processing error: {0}")]
    ProcessingError(String),

    #[error("Failed to capture image: {0}")]
    CaptureError(String),

    #[error("Image processing error: {0}")]
    ImageProcessingError(String),

    #[error("OCR error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Theme detection error: {0}")]
    DetectionError(String),

    #[error("Unsupported theme: {0}")]
    UnsupportedTheme(String),

    #[error("Theme error: {0}")]
    Other(String),
}
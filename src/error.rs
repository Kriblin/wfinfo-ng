//! Error types for the WFinfo-ng project.
//!
//! This module defines custom error types for different modules in the project,
//! allowing for proper error propagation and context-rich error messages.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::result;

/// A specialized Result type for WFinfo-ng operations.
pub type Result<T> = result::Result<T, Error>;

/// The main error type for WFinfo-ng.
#[derive(Debug)]
pub enum Error {
    /// Errors related to database operations.
    Database(DatabaseError),
    /// Errors related to OCR operations.
    Ocr(OcrError),
    /// Errors related to theme detection.
    Theme(ThemeError),
    /// Errors related to file operations.
    Io(io::Error),
    /// Errors related to JSON parsing.
    Json(serde_json::Error),
    /// Other errors that don't fit into the above categories.
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Database(err) => write!(f, "Database error: {}", err),
            Error::Ocr(err) => write!(f, "OCR error: {}", err),
            Error::Theme(err) => write!(f, "Theme error: {}", err),
            Error::Io(err) => write!(f, "I/O error: {}", err),
            Error::Json(err) => write!(f, "JSON error: {}", err),
            Error::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            Error::Json(err) => Some(err),
            _ => None,
        }
    }
}

// Implement conversions from specific error types to the main Error type
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

impl From<DatabaseError> for Error {
    fn from(err: DatabaseError) -> Self {
        Error::Database(err)
    }
}

impl From<OcrError> for Error {
    fn from(err: OcrError) -> Self {
        Error::Ocr(err)
    }
}

impl From<ThemeError> for Error {
    fn from(err: ThemeError) -> Self {
        Error::Theme(err)
    }
}

/// Errors specific to database operations.
#[derive(Debug)]
pub enum DatabaseError {
    /// The database file could not be found.
    FileNotFound(PathBuf, Option<String>),
    /// The database file is invalid or corrupted.
    InvalidFormat(String),
    /// An item could not be found in the database.
    ItemNotFound(String),
    /// The database could not be loaded.
    LoadError(String),
    /// Other database-related errors.
    Other(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::FileNotFound(path, Some(msg)) => write!(f, "Database file not found: {:?}. Msg: {:?}.", path, msg),
            DatabaseError::FileNotFound(path, None) => write!(f, "Database file not found: {:?}.", path),
            DatabaseError::InvalidFormat(msg) => write!(f, "Invalid database format: {}", msg),
            DatabaseError::ItemNotFound(name) => write!(f, "Item not found in database: {}", name),
            DatabaseError::LoadError(msg) => write!(f, "Failed to load database: {}", msg),
            DatabaseError::Other(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

/// Errors specific to OCR operations.
#[derive(Debug)]
pub enum OcrError {
    /// The OCR engine could not be initialized.
    InitializationError(String),
    /// The OCR engine failed to process an image.
    ProcessingError(String),
    /// The image could not be captured.
    CaptureError(String),
    /// The image could not be processed.
    ImageProcessingError(String),
    /// Other OCR-related errors.
    Other(String),
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OcrError::InitializationError(msg) => write!(f, "Failed to initialize OCR engine: {}", msg),
            OcrError::ProcessingError(msg) => write!(f, "OCR processing error: {}", msg),
            OcrError::CaptureError(msg) => write!(f, "Failed to capture image: {}", msg),
            OcrError::ImageProcessingError(msg) => write!(f, "Image processing error: {}", msg),
            OcrError::Other(msg) => write!(f, "OCR error: {}", msg),
        }
    }
}

impl std::error::Error for OcrError {}

/// Errors specific to theme detection.
#[derive(Debug)]
pub enum ThemeError {
    /// The theme could not be detected.
    DetectionError(String),
    /// The theme is not supported.
    UnsupportedTheme(String),
    /// Other theme-related errors.
    Other(String),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::DetectionError(msg) => write!(f, "Theme detection error: {}", msg),
            ThemeError::UnsupportedTheme(theme) => write!(f, "Unsupported theme: {}", theme),
            ThemeError::Other(msg) => write!(f, "Theme error: {}", msg),
        }
    }
}

impl std::error::Error for ThemeError {}
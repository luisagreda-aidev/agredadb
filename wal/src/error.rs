//! WAL error types

use std::fmt;

/// WAL operation errors
#[derive(Debug)]
pub enum WalError {
    /// IO error
    IoError(std::io::Error),
    /// Serialization error
    SerializationError(serde_json::Error),
    /// Corruption detected
    CorruptionError(String),
    /// Invalid entry
    InvalidEntry(String),
}

impl fmt::Display for WalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::SerializationError(e) => write!(f, "Serialization error: {}", e),
            Self::CorruptionError(msg) => write!(f, "Corruption detected: {}", msg),
            Self::InvalidEntry(msg) => write!(f, "Invalid entry: {}", msg),
        }
    }
}

impl std::error::Error for WalError {}

impl From<std::io::Error> for WalError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<serde_json::Error> for WalError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError(e)
    }
}

/// Result type for WAL operations
pub type Result<T> = std::result::Result<T, WalError>;

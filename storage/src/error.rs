//! Error types for tiered storage

use std::fmt;
use std::io;

/// Tiered storage errors
#[derive(Debug)]
pub enum TieredError {
    /// IO error
    IoError(io::Error),
    /// Tier full
    TierFull(String),
    /// Key not found
    KeyNotFound(String),
    /// Invalid configuration
    InvalidConfig(String),
    /// Migration error
    MigrationError(String),
}

impl fmt::Display for TieredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::TierFull(tier) => write!(f, "Tier full: {}", tier),
            Self::KeyNotFound(key) => write!(f, "Key not found: {}", key),
            Self::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
            Self::MigrationError(msg) => write!(f, "Migration error: {}", msg),
        }
    }
}

impl std::error::Error for TieredError {}

impl From<io::Error> for TieredError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Result type for tiered storage operations
pub type Result<T> = std::result::Result<T, TieredError>;

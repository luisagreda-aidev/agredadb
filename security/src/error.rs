//! Security error types

use std::fmt;

/// Security errors
#[derive(Debug)]
pub enum SecurityError {
    /// Authentication failed
    AuthenticationFailed(String),
    /// Authorization failed
    AuthorizationFailed(String),
    /// Invalid credentials
    InvalidCredentials,
    /// User not found
    UserNotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Invalid token
    InvalidToken(String),
    /// Encryption error
    EncryptionError(String),
    /// Namespace error
    NamespaceError(String),
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationFailed(msg) => write!(f, "Authentication failed: {}", msg),
            Self::AuthorizationFailed(msg) => write!(f, "Authorization failed: {}", msg),
            Self::InvalidCredentials => write!(f, "Invalid credentials"),
            Self::UserNotFound(user) => write!(f, "User not found: {}", user),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::InvalidToken(msg) => write!(f, "Invalid token: {}", msg),
            Self::EncryptionError(msg) => write!(f, "Encryption error: {}", msg),
            Self::NamespaceError(msg) => write!(f, "Namespace error: {}", msg),
        }
    }
}

impl std::error::Error for SecurityError {}

/// Result type for security operations
pub type Result<T> = std::result::Result<T, SecurityError>;

//! Top-level application error type shared across modules.

use std::io::Error;

use {
    oo7::Error as KeyringError, qobuz_api::errors::QobuzApiError, serde_json::Error as SerdeError,
    thiserror::Error,
};

/// Application-level error type.
#[derive(Debug, Error)]
pub enum AppError {
    /// API library error.
    #[error("API error: {0}")]
    Api(#[from] QobuzApiError),
    /// Keyring access error.
    #[error("Keyring error: {0}")]
    Keyring(Box<KeyringError>),
    /// Settings file I/O error.
    #[error("Settings I/O error: {0}")]
    Settings(#[from] Error),
    /// Settings JSON parse error.
    #[error("Settings parse error: {0}")]
    SettingsParse(#[from] SerdeError),
    /// Download-specific error.
    #[error("Download error: {0}")]
    Download(String),
    /// Operation requires authentication.
    #[error("Not authenticated")]
    NotAuthenticated,
}

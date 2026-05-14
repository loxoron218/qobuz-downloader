//! Application error types.

use std::io::Error;

use {
    oo7::Error as Oo7Error, qobuz_api::errors::QobuzApiError, serde_json::Error as SerdeError,
    thiserror::Error,
};

/// Application-level error type.
#[derive(Error, Debug)]
pub enum AppError {
    /// API library error.
    #[error("API error: {0}")]
    Api(#[from] QobuzApiError),
    /// Keyring access error.
    #[error("Keyring error: {0}")]
    Keyring(#[from] Oo7Error),
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

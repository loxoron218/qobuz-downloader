//! Application state management and top-level error type.

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    io::Error,
    sync::Arc,
};

use {
    oo7::Error as Oo7Error,
    parking_lot::Mutex,
    qobuz_api::{api::service::QobuzApiService, errors::QobuzApiError},
    serde_json::Error as SerdeError,
    thiserror::Error,
    tracing::info,
};

use crate::{
    auth::session::AuthState,
    cover_art::cache::CoverArtCache,
    preferences::settings::{AppSettings, load_settings},
};

/// Application-level error type.
#[derive(Error, Debug)]
pub enum AppError {
    /// API library error.
    #[error("API error: {0}")]
    Api(#[from] QobuzApiError),
    /// Keyring access error.
    #[error("Keyring error: {0}")]
    Keyring(Box<Oo7Error>),
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

/// Central application state shared across modules.
#[derive(Clone)]
pub struct AppState {
    /// Shared API client, accessed from background threads.
    pub api_service: Arc<Mutex<QobuzApiService>>,
    /// User preferences.
    pub settings: Arc<Mutex<AppSettings>>,
    /// Current authentication status.
    pub auth_state: Arc<Mutex<AuthState>>,
    /// In-memory cover art texture cache.
    pub cover_art_cache: CoverArtCache,
}

impl AppState {
    /// Creates a new `AppState` with the given API service and loaded settings.
    pub fn new(api_service: QobuzApiService) -> Self {
        let settings = load_settings();
        info!(
            download_directory = %settings.download_directory.display(),
            default_quality = %settings.default_quality,
            "AppState initialized",
        );
        Self {
            api_service: Arc::new(Mutex::new(api_service)),
            settings: Arc::new(Mutex::new(settings)),
            auth_state: Arc::new(Mutex::new(AuthState::default())),
            cover_art_cache: CoverArtCache::new(),
        }
    }
}

impl Debug for AppState {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

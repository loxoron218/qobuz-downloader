//! Application state management.

use std::sync::Arc;

use {parking_lot::Mutex, qobuz_api::api::service::QobuzApiService, tracing::info};

use crate::{
    auth::session::AuthState,
    cover_art::cache::CoverArtCache,
    preferences::settings::{AppSettings, load_settings},
};

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

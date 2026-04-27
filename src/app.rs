//! Application state management.

use std::sync::Arc;

use {parking_lot::Mutex, qobuz_api_rust_refactor::api::service::QobuzApiService};

use crate::{
    auth::session::AuthState,
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
}

impl AppState {
    /// Creates a new `AppState` with the given API service and loaded settings.
    pub fn new(api_service: QobuzApiService) -> Self {
        Self {
            api_service: Arc::new(Mutex::new(api_service)),
            settings: Arc::new(Mutex::new(load_settings())),
            auth_state: Arc::new(Mutex::new(AuthState::default())),
        }
    }
}

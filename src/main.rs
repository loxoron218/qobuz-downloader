//! Qobuz Download GUI application.

mod app;
mod auth;
mod browse;
mod cover_art;
mod download;
mod errors;
mod preferences;
mod search;
mod window;

use std::process::exit;

use {
    libadwaita::{
        Application,
        gio::{
            ApplicationFlags,
            prelude::{ApplicationExt, ApplicationExtManual},
        },
        gtk::prelude::GtkWindowExt,
    },
    qobuz_api_rust_refactor::api::service::QobuzApiService,
    tracing::error,
};

use crate::app::AppState;

fn main() {
    let app = Application::new(Some("com.qobuz.downloader"), ApplicationFlags::default());

    app.connect_activate(|app| {
        let api_service = QobuzApiService::new().unwrap_or_else(|e| {
            error!(error = %e, "Failed to initialize API service");
            exit(1);
        });
        let state = AppState::new(api_service);
        let window = window::build_window(app, &state);
        window.present();
    });

    app.run();
}

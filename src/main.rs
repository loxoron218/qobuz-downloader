//! Qobuz Download GUI application.

pub mod app;
pub mod auth;
pub mod browse;
pub mod cover_art;
pub mod dashboard;
pub mod download;
pub mod errors;
pub mod instrument;
pub mod preferences;
pub mod search;
pub mod types;
pub mod ui;
pub mod window;

use std::{hint::black_box, process::exit};

use {
    libadwaita::{
        Application,
        gio::ApplicationFlags,
        glib::ExitCode,
        prelude::{ApplicationExt, ApplicationExtManual, GtkWindowExt},
    },
    qobuz_api::api::service::QobuzApiService,
    tracing::{Level, error},
    tracing_subscriber::fmt,
};

use crate::{app::AppState, window::frame::build_window};

fn main() -> ExitCode {
    fmt().with_max_level(Level::INFO).init();

    let app = Application::new(Some("com.qobuz.downloader"), ApplicationFlags::default());

    let activate_handler = app.connect_activate(|app| {
        let api_service = QobuzApiService::new().unwrap_or_else(|e| {
            error!(error = %e, "Failed to initialize API service");
            exit(1);
        });
        let state = AppState::new(api_service);
        let window = build_window(app, &state);
        window.present();
    });
    for _ in [black_box(activate_handler)] {}

    app.run()
}

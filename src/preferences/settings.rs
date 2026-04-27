//! Application settings persistence.

use std::{
    fs::{create_dir_all, read_to_string, write},
    io::Result,
    path::PathBuf,
};

use {
    libadwaita::glib::{UserDirectory::Music, user_config_dir, user_special_dir},
    serde::{Deserialize, Serialize},
    serde_json::{from_str, to_string_pretty},
};

use crate::download::progress::Quality;

/// Persistent user preferences.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppSettings {
    /// Download output location.
    pub download_directory: PathBuf,
    /// Default audio quality.
    pub default_quality: Quality,
    /// Saved window width.
    pub window_width: i32,
    /// Saved window height.
    pub window_height: i32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_directory: default_download_dir(),
            default_quality: Quality::default(),
            window_width: 800,
            window_height: 600,
        }
    }
}

/// Returns the settings file path following XDG conventions.
pub fn settings_path() -> PathBuf {
    user_config_dir()
        .join("qobuz-downloader-rs")
        .join("settings.json")
}

/// Loads settings from disk, returning defaults if file is missing.
pub fn load_settings() -> AppSettings {
    let path = settings_path();
    read_to_string(&path).map_or_else(
        |_| AppSettings::default(),
        |content| from_str(&content).unwrap_or_default(),
    )
}

/// Saves settings to disk, creating parent directories as needed.
///
/// # Errors
///
/// Returns an error if directory creation, serialization, or file writing fails.
pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let content = to_string_pretty(settings)?;
    write(path, content)
}

/// Returns the default download directory.
fn default_download_dir() -> PathBuf {
    user_special_dir(Music).unwrap_or_else(|| PathBuf::from("~/Music"))
}

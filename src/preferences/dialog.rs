//! Preferences dialog UI.

use std::{path::PathBuf, sync::Arc};

use {
    libadwaita::{
        ActionRow, ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage,
        gio::{Cancellable, File, prelude::FileExt},
        glib::{Error, WeakRef, prelude::IsA},
        gtk::{Align::Center, Button, FileDialog, StringList, Window},
        prelude::{
            ActionRowExt, AdwDialogExt, ButtonExt, ComboRowExt, ObjectExt, PreferencesDialogExt,
            PreferencesGroupExt, PreferencesPageExt,
        },
    },
    parking_lot::Mutex,
    tracing::warn,
};

use crate::{
    app::AppState,
    auth::{keyring::delete, session::AuthState::Unauthenticated},
    download::progress::Quality,
    preferences::settings::save_settings,
};

/// Maps a `Quality` value to the `ComboRow` selected index.
fn quality_to_index(quality: Quality) -> u32 {
    match quality {
        Quality::Mp3_320 => 0,
        Quality::Flac16_44 => 1,
        Quality::Flac24_96 => 2,
        Quality::Flac24_192 => 3,
    }
}

/// Maps a `ComboRow` selected index to a `Quality` value.
fn index_to_quality(index: u32) -> Quality {
    match index {
        0 => Quality::Mp3_320,
        2 => Quality::Flac24_96,
        3 => Quality::Flac24_192,
        _ => Quality::Flac16_44,
    }
}

/// Handles the result of a folder selection dialog.
fn on_folder_selected(
    selected_dir: &Arc<Mutex<PathBuf>>,
    dir_row: &ActionRow,
    result: Result<File, Error>,
) {
    let Ok(file) = result else { return };
    let Some(path) = file.path() else { return };
    selected_dir.lock().clone_from(&path);
    dir_row.set_subtitle(&path.display().to_string());
}

/// Connects the browse button to open a folder selection dialog.
fn connect_folder_picker(
    window: &impl IsA<Window>,
    selected_dir: &Arc<Mutex<PathBuf>>,
    directory_row: &ActionRow,
    browse_button: &Button,
) {
    let window = window.clone();
    let selected_dir = Arc::clone(selected_dir);
    let dir_row = directory_row.clone();
    browse_button.connect_clicked(move |_| {
        let file_dialog = FileDialog::new();
        file_dialog.set_title("Select Download Directory");

        let dir_path = selected_dir.lock().to_path_buf();
        if let Some(parent) = dir_path.parent() {
            file_dialog.set_initial_folder(Some(&File::for_path(parent)));
        }

        let dir_row = dir_row.clone();
        let selected_dir = Arc::clone(&selected_dir);
        file_dialog.select_folder(Some(&window), Option::<&Cancellable>::None, move |result| {
            on_folder_selected(&selected_dir, &dir_row, result);
        });
    });
}

/// Builds and returns a `PreferencesDialog`.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `on_logout` - Callback invoked when the user clicks the Logout button
/// * `window` - Parent window for presenting the file chooser dialog
pub fn build(
    state: &AppState,
    on_logout: Box<dyn Fn() + 'static>,
    window: &impl IsA<Window>,
) -> PreferencesDialog {
    let dialog = PreferencesDialog::new();

    let download_page = PreferencesPage::builder()
        .title("Download Settings")
        .icon_name("folder-download-symbolic")
        .build();

    let download_group = PreferencesGroup::builder().title("Download").build();

    let dir_path = {
        let settings = state.settings.lock();
        settings.download_directory.clone()
    };

    let selected_dir = Arc::new(Mutex::new(dir_path));

    let directory_row = ActionRow::builder()
        .title("Download Directory")
        .subtitle(selected_dir.lock().display().to_string())
        .build();

    let browse_button = Button::builder()
        .label("Browse")
        .css_classes(["flat"])
        .build();
    directory_row.add_suffix(&browse_button);

    connect_folder_picker(window, &selected_dir, &directory_row, &browse_button);

    let quality_model = StringList::new(&[
        "MP3 320kbps",
        "FLAC 16-bit / 44.1kHz",
        "FLAC 24-bit / 96kHz",
        "FLAC 24-bit / 192kHz",
    ]);

    let default_quality = {
        let settings = state.settings.lock();
        settings.default_quality
    };

    let quality_row = ComboRow::builder()
        .title("Default Audio Quality")
        .subtitle("Quality for downloads from search results")
        .model(&quality_model)
        .selected(quality_to_index(default_quality))
        .build();

    download_group.add(&directory_row);
    download_group.add(&quality_row);

    let credential_group = PreferencesGroup::builder().title("Account").build();

    let logout_button = Button::builder()
        .label("Logout")
        .css_classes(["destructive-action", "flat"])
        .halign(Center)
        .build();

    let logout_row = ActionRow::builder().title("Sign Out").build();
    logout_row.add_suffix(&logout_button);

    credential_group.add(&logout_row);

    download_page.add(&download_group);
    download_page.add(&credential_group);

    dialog.add(&download_page);

    {
        let selected_dir = Arc::clone(&selected_dir);
        let quality_row = quality_row;
        let state = state.clone();

        dialog.connect_closed(move |_| {
            save_preferences_from_dialog(&state, &selected_dir, &quality_row);
        });
    }

    {
        let dialog_weak = dialog.downgrade();
        let state = state.clone();
        logout_button.connect_clicked(move |_| {
            perform_logout(&state, &dialog_weak, &on_logout);
        });
    }

    dialog
}

/// Saves preferences from the dialog widgets to settings.
fn save_preferences_from_dialog(
    state: &AppState,
    selected_dir: &Arc<Mutex<PathBuf>>,
    quality_row: &ComboRow,
) {
    let mut settings = state.settings.lock();
    let dir = selected_dir.lock();
    if !dir.as_os_str().is_empty() {
        settings.download_directory.clone_from(&dir);
    }
    drop(dir);
    settings.default_quality = index_to_quality(quality_row.selected());
    if let Err(err) = save_settings(&settings) {
        warn!(error = %err, "Failed to save preferences");
    }
    drop(settings);
}

/// Performs logout: deletes keyring, resets auth, closes dialog, invokes callback.
fn perform_logout(
    state: &AppState,
    dialog_weak: &WeakRef<PreferencesDialog>,
    on_logout: &dyn Fn(),
) {
    if let Err(err) = delete() {
        warn!(error = %err, "Failed to delete keyring credentials");
    }
    {
        let mut auth_state = state.auth_state.lock();
        *auth_state = Unauthenticated;
    }
    if let Some(dialog) = dialog_weak.upgrade() {
        dialog.close();
    }
    on_logout();
}

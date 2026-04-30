//! Preferences dialog UI.

use std::path::PathBuf;

use {
    libadwaita::{
        ActionRow, ComboRow, EntryRow, PreferencesDialog, PreferencesGroup, PreferencesPage,
        glib::WeakRef,
        gtk::{Align::Center, Button, StringList},
        prelude::{
            ActionRowExt, AdwDialogExt, ButtonExt, ComboRowExt, EditableExt, ObjectExt,
            PreferencesDialogExt, PreferencesGroupExt, PreferencesPageExt,
        },
    },
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

/// Builds and returns a `PreferencesDialog`.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `on_logout` - Callback invoked when the user clicks the Logout button
pub fn build(state: &AppState, on_logout: Box<dyn Fn() + 'static>) -> PreferencesDialog {
    let dialog = PreferencesDialog::new();

    let download_page = PreferencesPage::builder()
        .title("Download Settings")
        .icon_name("folder-download-symbolic")
        .build();

    let download_group = PreferencesGroup::builder().title("Download").build();

    let dir_path = {
        let settings = state.settings.lock();
        settings.download_directory.display().to_string()
    };

    let directory_row = EntryRow::builder()
        .title("Download Directory")
        .text(&dir_path)
        .build();

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
        .css_classes(["destructive-action"])
        .halign(Center)
        .build();

    let logout_row = ActionRow::builder().title("Sign Out").build();
    logout_row.add_suffix(&logout_button);

    credential_group.add(&logout_row);

    download_page.add(&download_group);
    download_page.add(&credential_group);

    dialog.add(&download_page);

    {
        let directory_row = directory_row;
        let quality_row = quality_row;
        let state = state.clone();

        dialog.connect_closed(move |_| {
            save_preferences_from_dialog(&state, &directory_row, &quality_row);
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
    directory_row: &EntryRow,
    quality_row: &ComboRow,
) {
    let mut settings = state.settings.lock();
    let new_dir = directory_row.text().to_string();
    if !new_dir.trim().is_empty() {
        settings.download_directory = PathBuf::from(&new_dir);
    }
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

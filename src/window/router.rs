//! Auth and browse event dispatch for the main window.

use std::{collections::HashMap, sync::Arc};

use {
    async_channel::{Receiver, Sender},
    libadwaita::{
        HeaderBar, NavigationPage, NavigationView, ToolbarView, gio::spawn_blocking,
        glib::MainContext, prelude::WidgetExt,
    },
    tracing::{error, info, warn},
};

use crate::{
    app::AppState,
    auth::{
        login_form::{
            LoginMethod::{EmailPassword, Token},
            LoginWidgets, current_method,
        },
        session::{
            AuthEvent::{self, Authenticated, AuthenticationFailed},
            AuthState::{Authenticated as StateAuthenticated, Unauthenticated},
            perform_keyring_login,
        },
    },
    browse::{
        BrowseEvent::{self, AlbumMeta, AlbumTracks, Artist, Error, Playlist},
        album_profile::{AlbumDetailWidgets, build_meta, populate_tracks},
        artist_spotlight, playlist_collection,
    },
    download::progress::DownloadCommand,
};

/// Sets up the auth event receiver to update the UI on auth state changes.
pub(super) fn setup_auth_receiver(
    state: &AppState,
    toolbar: &ToolbarView,
    nav_view: &NavigationView,
    login_widgets: &LoginWidgets,
    login_header: &HeaderBar,
    receiver: Receiver<AuthEvent>,
) {
    let toolbar = toolbar.clone();
    let nav_view = nav_view.clone();
    let state = state.clone();
    let login_widgets = login_widgets.clone();
    let login_header = login_header.clone();

    drop(MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_auth_event(
                &event,
                &state,
                &toolbar,
                &nav_view,
                &login_widgets,
                &login_header,
            );
        }
    }));
}

/// Handles a single auth event by updating state and UI.
fn handle_auth_event(
    event: &AuthEvent,
    state: &AppState,
    toolbar: &ToolbarView,
    nav_view: &NavigationView,
    login_widgets: &LoginWidgets,
    login_header: &HeaderBar,
) {
    match event {
        Authenticated { user_id } => {
            info!(user_id, "Authentication successful");
            {
                let mut auth_state = state.auth_state.lock();
                *auth_state = StateAuthenticated {
                    user_id: user_id.clone(),
                };
            }

            toolbar.remove(login_header);
            toolbar.set_content(Some(nav_view));
        }
        AuthenticationFailed { error: err_msg } => {
            error!(error = %err_msg, "Authentication failed");
            {
                let mut auth_state = state.auth_state.lock();
                *auth_state = Unauthenticated;
            }
            if !err_msg.is_empty() {
                login_widgets.error_label.set_text(err_msg);
                login_widgets.error_label.set_visible(true);
            }
            reset_login_sensitivity(login_widgets);
        }
    }
}

/// Resets login field sensitivity based on the currently selected login method.
fn reset_login_sensitivity(login_widgets: &LoginWidgets) {
    login_widgets.submit_button.set_sensitive(true);
    match current_method(login_widgets) {
        EmailPassword => {
            login_widgets.email_row.set_sensitive(true);
            login_widgets.password_row.set_sensitive(true);
        }
        Token => {
            login_widgets.user_id_row.set_sensitive(true);
            login_widgets.auth_token_row.set_sensitive(true);
        }
    }
}

/// Attempts automatic login using stored keyring credentials on startup.
pub(super) fn attempt_keyring_login(state: &AppState, sender: &Sender<AuthEvent>) {
    let api_service = Arc::clone(&state.api_service);
    let sender = sender.clone();
    drop(spawn_blocking(move || {
        let result = perform_keyring_login(&api_service);
        let event = match result {
            Ok(user_id) => {
                info!(user_id, "Keyring auto-login successful");
                Authenticated { user_id }
            }
            Err(err) => {
                info!(error = %err, "No stored credentials or keyring login failed");
                AuthenticationFailed {
                    error: String::new(),
                }
            }
        };
        if let Err(err) = sender.send_blocking(event) {
            warn!(error = %err, "Failed to send auth event, receiver likely dropped");
        }
    }));
}

/// Sets up the browse event receiver to show detail views on album navigation.
pub(super) fn setup_browse_receiver(
    state: &AppState,
    nav_view: &NavigationView,
    browse_sender: Sender<BrowseEvent>,
    receiver: Receiver<BrowseEvent>,
    cmd_sender: Sender<DownloadCommand>,
) {
    let nav_view = nav_view.clone();
    let state = state.clone();
    let cmd_sender = cmd_sender;
    let mut pending_albums = HashMap::<String, AlbumDetailWidgets>::new();

    drop(MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_browse_event(
                event,
                &state,
                &cmd_sender,
                &browse_sender,
                &nav_view,
                &mut pending_albums,
            );
        }
    }));
}

/// Handles a browse event by pushing the appropriate detail view or logging errors.
fn handle_browse_event(
    event: BrowseEvent,
    state: &AppState,
    cmd_sender: &Sender<DownloadCommand>,
    browse_sender: &Sender<BrowseEvent>,
    nav_view: &NavigationView,
    pending_albums: &mut HashMap<String, AlbumDetailWidgets>,
) {
    match event {
        AlbumMeta { album } => {
            let album_id = album.id.clone().unwrap_or_default();
            let widgets = build_meta(&album);
            drop(pending_albums.insert(album_id, widgets.clone()));
            let page = NavigationPage::new(&widgets.root, "Album");
            nav_view.push(&page);
        }
        AlbumTracks { album, tracks } => {
            let album_id = album.id.clone().unwrap_or_default();
            let Some(widgets) = pending_albums.remove(&album_id) else {
                error!(album_id = %album_id, "Received tracks for unknown album page");
                return;
            };
            populate_tracks(
                &widgets,
                &album,
                &tracks,
                Arc::clone(&state.settings),
                cmd_sender.clone(),
            );
        }
        Playlist { playlist } => {
            let root = playlist_collection::build(
                &playlist,
                Arc::clone(&state.settings),
                cmd_sender.clone(),
            );
            let page = NavigationPage::new(&root, "Playlist");
            nav_view.push(&page);
        }
        Artist { artist, albums } => {
            let root = artist_spotlight::build(
                &artist,
                &albums,
                Arc::clone(&state.settings),
                cmd_sender.clone(),
                &state.api_service,
                browse_sender,
            );
            let page = NavigationPage::new(&root, "Artist");
            nav_view.push(&page);
        }
        Error { context, error } => {
            error!(%context, %error, "Browse error");
        }
    }
}

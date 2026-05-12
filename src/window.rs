//! Main application window.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application, ApplicationWindow, NavigationPage, NavigationView, ToolbarView,
        gio::spawn_blocking,
        glib::{MainContext, Propagation::Proceed},
        gtk::{Box as GtkBox, Button},
        prelude::{AdwApplicationWindowExt, AdwDialogExt, ButtonExt, GtkWindowExt, WidgetExt},
    },
    tracing::{error, info, warn},
};

use crate::{
    app::AppState,
    auth::{
        login_view::{
            LoginMethod::{EmailPassword, Token},
            LoginWidgets, build, current_method,
        },
        session::{
            AuthEvent::{self, Authenticated, AuthenticationFailed},
            AuthState::{Authenticated as StateAuthenticated, Authenticating, Unauthenticated},
            perform_keyring_login,
        },
    },
    browse::{
        BrowseEvent::{self, AlbumMeta, AlbumTracks, Artist, Error, Playlist},
        album_view, artist_view, playlist_view,
    },
    dashboard,
    download::{manager::DownloadManager, progress::DownloadCommand},
    preferences::{
        dialog,
        settings::{AppSettings, save_settings},
    },
    search::view::build as build_view,
};

/// Saves settings and logs any error.
fn log_save_settings_error(settings: &AppSettings) {
    if let Err(err) = save_settings(settings) {
        error!(error = %err, "Failed to save settings");
    }
}

/// Builds and returns the main application window.
pub fn build_window(app: &Application, state: &AppState) -> ApplicationWindow {
    let window = ApplicationWindow::new(app);

    let width;
    let height;
    {
        let settings = state.settings.lock();
        width = settings.window_width;
        height = settings.window_height;
    }
    window.set_default_size(width, height);

    let (auth_sender, auth_receiver) = unbounded::<AuthEvent>();
    let (browse_sender, browse_receiver) = unbounded::<BrowseEvent>();

    let download_manager = DownloadManager::new(Arc::clone(&state.api_service));

    let nav_view = NavigationView::new();

    let dashboard_widgets = dashboard::build(
        state,
        download_manager.cmd_sender(),
        download_manager.evt_receiver(),
        &download_manager.tasks_handle(),
    );
    let dashboard_page = NavigationPage::new(&dashboard_widgets.root, "Dashboard");

    nav_view.add(&dashboard_page);

    let search_widgets = build_view(state, download_manager.cmd_sender(), browse_sender.clone());
    search_widgets.setup_esc_navigation(&nav_view);
    let search_page = NavigationPage::new(&search_widgets.root, "Search");

    let login_widgets = build(state, auth_sender.clone());

    let toolbar = ToolbarView::new();
    toolbar.set_content(Some(&login_widgets.root));

    {
        let search_button = Button::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();

        let settings_button = Button::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Settings")
            .build();

        dashboard_widgets.header.pack_end(&settings_button);
        dashboard_widgets.header.pack_end(&search_button);

        let nav = nav_view.clone();
        let sp = search_page;
        search_button.connect_clicked(move |_| {
            nav.push(&sp);
        });

        let state_for_dialog = state.clone();
        let window_for_dialog = window.clone();
        let toolbar_for_logout = toolbar.clone();
        let login_root_for_logout = login_widgets.root.clone();
        settings_button.connect_clicked(move |_| {
            let on_logout = make_logout_callback(&toolbar_for_logout, &login_root_for_logout);
            let dialog = dialog::build(&state_for_dialog, on_logout, &window_for_dialog);
            dialog.present(Some(&window_for_dialog));
        });
    }

    {
        let mut auth_state = state.auth_state.lock();
        *auth_state = Authenticating;
    }

    setup_auth_receiver(state, &toolbar, &nav_view, &login_widgets, auth_receiver);
    setup_browse_receiver(
        state,
        &nav_view,
        browse_sender,
        browse_receiver,
        download_manager.cmd_sender(),
    );

    download_manager.start_worker();

    attempt_keyring_login(state, &auth_sender);

    {
        let state_close = state.clone();
        window.connect_close_request(move |win| {
            let mut settings = state_close.settings.lock();
            settings.window_width = win.width();
            settings.window_height = win.height();
            drop(settings);
            let settings = state_close.settings.lock();
            log_save_settings_error(&settings);
            Proceed
        });
    }

    window.set_content(Some(&toolbar));

    window
}

/// Sets up the auth event receiver to update the UI on auth state changes.
fn setup_auth_receiver(
    state: &AppState,
    toolbar: &ToolbarView,
    nav_view: &NavigationView,
    login_widgets: &LoginWidgets,
    receiver: Receiver<AuthEvent>,
) {
    let toolbar = toolbar.clone();
    let nav_view = nav_view.clone();
    let state = state.clone();
    let login_widgets = login_widgets.clone();

    MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_auth_event(&event, &state, &toolbar, &nav_view, &login_widgets);
        }
    });
}

/// Handles a single auth event by updating state and UI.
fn handle_auth_event(
    event: &AuthEvent,
    state: &AppState,
    toolbar: &ToolbarView,
    nav_view: &NavigationView,
    login_widgets: &LoginWidgets,
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
fn attempt_keyring_login(state: &AppState, sender: &Sender<AuthEvent>) {
    let api_service = Arc::clone(&state.api_service);
    let sender = sender.clone();
    spawn_blocking(move || {
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
    });
}

/// Sets up the browse event receiver to show detail views on album navigation.
fn setup_browse_receiver(
    state: &AppState,
    nav_view: &NavigationView,
    browse_sender: Sender<BrowseEvent>,
    receiver: Receiver<BrowseEvent>,
    cmd_sender: Sender<DownloadCommand>,
) {
    let nav_view = nav_view.clone();
    let state = state.clone();
    let cmd_sender = cmd_sender;
    let pending_albums = RefCell::new(HashMap::<String, album_view::AlbumDetailWidgets>::new());

    MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_browse_event(
                event,
                &state,
                &cmd_sender,
                &browse_sender,
                &nav_view,
                &pending_albums,
            );
        }
    });
}

/// Handles a browse event by pushing the appropriate detail view or logging errors.
fn handle_browse_event(
    event: BrowseEvent,
    state: &AppState,
    cmd_sender: &Sender<DownloadCommand>,
    browse_sender: &Sender<BrowseEvent>,
    nav_view: &NavigationView,
    pending_albums: &RefCell<HashMap<String, album_view::AlbumDetailWidgets>>,
) {
    match event {
        AlbumMeta { album } => {
            let album_id = album.id.clone().unwrap_or_default();
            let widgets = album_view::build_meta(&album);
            pending_albums
                .borrow_mut()
                .insert(album_id, widgets.clone());
            let page = NavigationPage::new(&widgets.root, "Album");
            nav_view.push(&page);
        }
        AlbumTracks { album, tracks } => {
            let album_id = album.id.clone().unwrap_or_default();
            let Some(widgets) = pending_albums.borrow_mut().remove(&album_id) else {
                error!(album_id = %album_id, "Received tracks for unknown album page");
                return;
            };
            album_view::populate_tracks(
                &widgets,
                &album,
                &tracks,
                Arc::clone(&state.settings),
                cmd_sender.clone(),
            );
        }
        Playlist { playlist } => {
            let widgets =
                playlist_view::build(&playlist, Arc::clone(&state.settings), cmd_sender.clone());
            let page = NavigationPage::new(&widgets.root, "Playlist");
            nav_view.push(&page);
        }
        Artist { artist, albums } => {
            let widgets = artist_view::build(
                &artist,
                &albums,
                Arc::clone(&state.settings),
                cmd_sender.clone(),
                &state.api_service,
                browse_sender,
            );
            let page = NavigationPage::new(&widgets.root, "Artist");
            nav_view.push(&page);
        }
        Error { context, error } => {
            error!(%context, %error, "Browse error");
        }
    }
}

/// Creates a logout callback that switches the toolbar content back to the login view.
fn make_logout_callback(toolbar: &ToolbarView, login_root: &GtkBox) -> Box<dyn Fn() + 'static> {
    let toolbar = toolbar.clone();
    let login_root = login_root.clone();
    Box::new(move || {
        toolbar.set_content(Some(&login_root));
    })
}

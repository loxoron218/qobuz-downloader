//! Main window frame: construction, geometry, shutdown, and logout.

use std::{
    fs::{read_dir, remove_file},
    hint::black_box,
    path::Path,
    sync::Arc,
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationView, ToolbarView,
        gio::prelude::ApplicationExt,
        glib::Propagation::Proceed,
        gtk::{Box as GtkBox, Button, Label},
        prelude::{
            AdwApplicationWindowExt, AdwDialogExt, ButtonExt, GtkWindowExt, NavigationPageExt,
            WidgetExt,
        },
    },
    tracing::{error, info},
};

use crate::{
    app::AppState,
    auth::{
        login_view::build,
        session::{AuthEvent, AuthState::Authenticating},
    },
    browse::BrowseEvent,
    dashboard::construction::build as build_dashboard,
    download::{
        manager::DownloadManager,
        progress::DownloadCommand::{self, Shutdown},
    },
    preferences::{
        dialog,
        settings::{AppSettings, save_settings},
    },
    search::view::scaffold::build as build_view,
    window::dispatch::{attempt_keyring_login, setup_auth_receiver, setup_browse_receiver},
};

/// Saves settings and logs any error.
fn log_save_settings_error(settings: &AppSettings) {
    if let Err(err) = save_settings(settings) {
        error!(error = %err, "Failed to save settings");
    }
}

/// Removes orphaned `.part` files from the download directory on startup.
fn cleanup_orphaned_part_files(download_dir: &Path) {
    let dir = match read_dir(download_dir) {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, path = %download_dir.display(), "Failed to read download dir for part cleanup");
            return;
        }
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "part") {
            continue;
        }
        if let Err(e) = remove_file(&path) {
            error!(error = %e, path = %path.display(), "Failed to remove orphaned part file");
        } else {
            info!(path = %path.display(), "Removed orphaned part file from previous session");
        }
    }
}

/// Handles shutdown: cancels active downloads, closes window to trigger settings save.
fn handle_force_shutdown(state: &AppState, cmd_sender: &Sender<DownloadCommand>) {
    if let Err(err) = cmd_sender.send_blocking(Shutdown) {
        error!(error = %err, "Failed to send shutdown command during force shutdown");
    }

    let dir = state.settings.lock().download_directory.clone();
    if dir.exists() {
        cleanup_orphaned_part_files(&dir);
    }
}

/// Restores saved window dimensions from settings.
fn restore_window_size(window: &ApplicationWindow, state: &AppState) {
    let (width, height) = {
        let settings = state.settings.lock();
        (settings.window_width, settings.window_height)
    };
    window.set_default_size(width, height);
}

/// Connects the window close handler to save geometry settings.
fn wire_close_handler(state: &AppState, window: &ApplicationWindow) {
    let state_close = state.clone();
    let close_request = window.connect_close_request(move |win| {
        let mut settings = state_close.settings.lock();
        settings.window_width = win.width();
        settings.window_height = win.height();
        drop(settings);
        let settings = state_close.settings.lock();
        log_save_settings_error(&settings);
        Proceed
    });
    for _ in [black_box(close_request)] {}
}

/// Builds and returns the main application window.
#[must_use]
pub fn build_window(app: &Application, state: &AppState) -> ApplicationWindow {
    let window = ApplicationWindow::new(app);
    restore_window_size(&window, state);

    let (auth_sender, auth_receiver) = unbounded::<AuthEvent>();
    let (browse_sender, browse_receiver) = unbounded::<BrowseEvent>();

    let download_manager = DownloadManager::new(Arc::clone(&state.api_service));

    let nav_view = NavigationView::new();

    let dashboard_widgets = build_dashboard(
        state,
        download_manager.cmd_sender(),
        download_manager.evt_receiver(),
        &download_manager.tasks_handle(),
        &download_manager.cancel_signals_handle(),
    );
    let dashboard_page = NavigationPage::new(&dashboard_widgets.root, "Dashboard");

    nav_view.add(&dashboard_page);

    let search_widgets = build_view(state, download_manager.cmd_sender(), browse_sender.clone());
    let search_page = NavigationPage::new(&search_widgets.root, "Search");
    search_page.set_can_pop(false);
    search_widgets.setup_esc_navigation(&nav_view);

    let login_widgets = build(state, auth_sender.clone());

    let login_header = HeaderBar::new();
    login_header.set_title_widget(Some(&Label::new(Some("Qobuz Downloader"))));

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&login_header);
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
        let search_entry = search_widgets.search_entry;
        let search_clicked = search_button.connect_clicked(move |_| {
            nav.push(&sp);
            let focused = search_entry.grab_focus();
            info!(focused = black_box(focused), "Search entry focus requested");
        });
        for _ in [black_box(search_clicked)] {}

        let state_for_dialog = state.clone();
        let window_for_dialog = window.clone();
        let toolbar_for_logout = toolbar.clone();
        let login_header_for_logout = login_header.clone();
        let login_root_for_logout = login_widgets.root.clone();
        let settings_clicked = settings_button.connect_clicked(move |_| {
            let on_logout = make_logout_callback(
                &toolbar_for_logout,
                &login_root_for_logout,
                &login_header_for_logout,
            );
            let dialog = dialog::build(&state_for_dialog, on_logout, &window_for_dialog);
            dialog.present(Some(&window_for_dialog));
        });
        for _ in [black_box(settings_clicked)] {}
    }

    {
        let mut auth_state = state.auth_state.lock();
        *auth_state = Authenticating;
    }

    setup_auth_receiver(
        state,
        &toolbar,
        &nav_view,
        &login_widgets,
        &login_header,
        auth_receiver,
    );
    setup_browse_receiver(
        state,
        &nav_view,
        browse_sender,
        browse_receiver,
        download_manager.cmd_sender(),
    );

    cleanup_orphaned_part_files(&state.settings.lock().download_directory);

    {
        let state_sd = state.clone();
        let cmd_sd = download_manager.cmd_sender();
        let shutdown = app.connect_shutdown(move |_| {
            handle_force_shutdown(&state_sd, &cmd_sd);
        });
        for _ in [black_box(shutdown)] {}
    }

    download_manager.start_worker();

    attempt_keyring_login(state, &auth_sender);

    wire_close_handler(state, &window);

    window.set_content(Some(&toolbar));

    window
}

/// Creates a logout callback that switches the toolbar content back to the login view.
fn make_logout_callback(
    toolbar: &ToolbarView,
    login_root: &GtkBox,
    login_header: &HeaderBar,
) -> Box<dyn Fn() + 'static> {
    let toolbar = toolbar.clone();
    let login_root = login_root.clone();
    let login_header = login_header.clone();
    Box::new(move || {
        toolbar.add_top_bar(&login_header);
        toolbar.set_content(Some(&login_root));
    })
}

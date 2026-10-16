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
        glib::Propagation::Proceed,
        gtk::{
            Box as GtkBox, Button, Label, SearchEntry, accessible::Property::Label as PropertyLabel,
        },
        prelude::{
            AccessibleExtManual, AdwApplicationWindowExt, AdwDialogExt, ApplicationExt, ButtonExt,
            GtkWindowExt, WidgetExt,
        },
    },
    tracing::{error, info},
};

use crate::{
    app::AppState,
    auth::{
        login_form::build,
        session::{AuthEvent, AuthState::Authenticating},
    },
    browse::BrowseEvent,
    dashboard::page::build as build_dashboard,
    download::{
        manager::DownloadManager,
        progress::DownloadCommand::{self, Shutdown},
    },
    preferences::{
        editor,
        settings::{AppSettings, save_settings},
    },
    search::view::console::build as build_view,
    window::router::{attempt_keyring_login, setup_auth_receiver, setup_browse_receiver},
};

/// Header button wiring context.
struct HeaderButtonCtx<'a> {
    /// Dashboard header bar holding the buttons.
    dashboard_header: &'a HeaderBar,
    /// Navigation view for pushing the search page.
    nav_view: &'a NavigationView,
    /// Search page pushed on search clicks.
    search_page: &'a NavigationPage,
    /// Search entry focused after navigation.
    search_entry: &'a SearchEntry,
    /// Shared application state for the settings dialog.
    state: &'a AppState,
    /// Parent window for dialogs.
    window: &'a ApplicationWindow,
    /// Toolbar restored on logout.
    toolbar: &'a ToolbarView,
    /// Login header restored on logout.
    login_header: &'a HeaderBar,
    /// Login root widget restored on logout.
    login_root: &'a GtkBox,
}

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
    let window = ApplicationWindow::builder()
        .application(app)
        .css_classes(["main-window"])
        .tooltip_text("Qobuz Downloader")
        .build();
    window.update_property(&[PropertyLabel("Qobuz Downloader main window")]);
    restore_window_size(&window, state);

    let (auth_sender, auth_receiver) = unbounded::<AuthEvent>();
    let (browse_sender, browse_receiver) = unbounded::<BrowseEvent>();

    let download_manager = DownloadManager::new(Arc::clone(&state.api_service));

    let nav_view = NavigationView::builder()
        .css_classes(["main-navigation"])
        .tooltip_text("Main navigation")
        .build();

    let dashboard_widgets = build_dashboard(
        state,
        download_manager.cmd_sender(),
        download_manager.evt_receiver(),
        &download_manager.tasks_handle(),
        &download_manager.cancel_signals_handle(),
    );
    let dashboard_page = NavigationPage::builder()
        .child(&dashboard_widgets.root)
        .tag("Dashboard")
        .css_classes(["dashboard-page"])
        .tooltip_text("Dashboard")
        .build();

    nav_view.add(&dashboard_page);

    let search_widgets = build_view(state, download_manager.cmd_sender(), browse_sender.clone());
    let search_page = NavigationPage::builder()
        .child(&search_widgets.root)
        .tag("Search")
        .can_pop(false)
        .css_classes(["search-page"])
        .tooltip_text("Search")
        .build();
    search_widgets.setup_esc_navigation(&nav_view);

    let login_widgets = build(state, auth_sender.clone());

    let login_title = Label::builder()
        .label("Qobuz Downloader")
        .css_classes(["login-title"])
        .tooltip_text("Qobuz Downloader")
        .build();
    let login_header = HeaderBar::builder()
        .title_widget(&login_title)
        .css_classes(["login-header-bar"])
        .tooltip_text("Login header")
        .build();

    let toolbar = ToolbarView::builder()
        .css_classes(["login-toolbar"])
        .tooltip_text("Login view")
        .build();
    toolbar.add_top_bar(&login_header);
    toolbar.set_content(Some(&login_widgets.root));

    wire_header_buttons(&HeaderButtonCtx {
        dashboard_header: &dashboard_widgets.header,
        nav_view: &nav_view,
        search_page: &search_page,
        search_entry: &search_widgets.search_entry,
        state,
        window: &window,
        toolbar: &toolbar,
        login_header: &login_header,
        login_root: &login_widgets.root,
    });
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

/// Wires dashboard header buttons for search navigation and settings.
///
/// # Arguments
///
/// * `ctx` - Header button wiring context with widgets and state.
fn wire_header_buttons(ctx: &HeaderButtonCtx<'_>) {
    let dashboard_header = ctx.dashboard_header;
    let nav_view = ctx.nav_view;
    let search_page = ctx.search_page;
    let search_entry = ctx.search_entry;
    let state = ctx.state;
    let window = ctx.window;
    let toolbar = ctx.toolbar;
    let login_header = ctx.login_header;
    let login_root = ctx.login_root;
    {
        let search_button = Button::builder()
            .icon_name("system-search-symbolic")
            .css_classes(["header-button", "image-button"])
            .tooltip_text("Search")
            .build();

        let settings_button = Button::builder()
            .icon_name("emblem-system-symbolic")
            .css_classes(["header-button", "image-button"])
            .tooltip_text("Settings")
            .build();

        dashboard_header.pack_end(&settings_button);
        dashboard_header.pack_end(&search_button);

        let nav = nav_view.clone();
        let sp = search_page.clone();
        let search_entry = search_entry.clone();
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
        search_button.update_property(&[PropertyLabel("Search")]);
        let login_root_for_logout = login_root.clone();
        let settings_clicked = settings_button.connect_clicked(move |_| {
            let on_logout = make_logout_callback(
                &toolbar_for_logout,
                &login_root_for_logout,
                &login_header_for_logout,
            );
            let dialog = editor::build(&state_for_dialog, on_logout, &window_for_dialog);
            dialog.present(Some(&window_for_dialog));
        });
        for _ in [black_box(settings_clicked)] {}
    }
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

//! Main application window.

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationView, ToolbarView,
        gio::spawn_blocking,
        glib::{MainContext, Propagation::Proceed, clone},
        gtk::Label,
        prelude::{AdwApplicationWindowExt, GtkWindowExt, WidgetExt},
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
            AuthEvent::{self, Authenticated, AuthenticationFailed, Reauthenticated},
            AuthState::{Authenticated as StateAuthenticated, Authenticating, Unauthenticated},
            perform_keyring_login,
        },
    },
    preferences::settings::save_settings,
    window::AuthEvent::ReauthFailed,
};

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

    let nav_view = NavigationView::new();
    let main_placeholder = Label::new(Some("Qobuz Downloader"));
    let main_page = NavigationPage::new(&main_placeholder, "Qobuz Downloader");
    nav_view.push(&main_page);

    let login_widgets = build(state, auth_sender.clone());

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&HeaderBar::new());
    toolbar.set_content(Some(&login_widgets.root));
    window.set_content(Some(&toolbar));

    {
        let mut auth_state = state.auth_state.lock();
        *auth_state = Authenticating;
    }

    setup_auth_receiver(state, &toolbar, &nav_view, &login_widgets, auth_receiver);

    attempt_keyring_login(state, &auth_sender);

    window.connect_close_request(clone!(
        #[strong]
        state,
        move |win| {
            let mut settings = state.settings.lock();
            settings.window_width = win.width();
            settings.window_height = win.height();
            drop(settings);
            let settings = state.settings.lock();
            if let Err(err) = save_settings(&settings) {
                error!(error = %err, "Failed to save settings");
            }
            Proceed
        }
    ));

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
        Reauthenticated => {
            info!("Re-authentication successful");
        }
        ReauthFailed { error: err_msg } => {
            error!(error = %err_msg, "Re-authentication failed");
            {
                let mut auth_state = state.auth_state.lock();
                *auth_state = Unauthenticated;
            }
            toolbar.set_content(Some(&login_widgets.root));
            login_widgets.error_label.set_text(err_msg);
            login_widgets.error_label.set_visible(true);
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

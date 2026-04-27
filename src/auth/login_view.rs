//! Login view UI for Qobuz authentication.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        EntryRow, PasswordEntryRow,
        gio::spawn_blocking,
        glib::clone,
        gtk::{Align::Center, Box, Button, Label, Orientation::Vertical, ToggleButton},
        prelude::{BoxExt, ButtonExt, EditableExt, ToggleButtonExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::api::service::QobuzApiService,
    tracing::warn,
};

use crate::{
    app::AppState,
    auth::session::{
        AuthEvent::{self, Authenticated, AuthenticationFailed},
        perform_login, perform_token_login,
    },
};

/// Login method selected by the user.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoginMethod {
    /// Email and password authentication.
    EmailPassword,
    /// User ID and auth token authentication.
    Token,
}

/// Widgets from the login view needed for external event handling.
#[derive(Clone)]
pub struct LoginWidgets {
    /// Root content box.
    pub root: Box,
    /// Error label for displaying authentication failures.
    pub error_label: Label,
    /// Submit button for triggering login.
    pub submit_button: Button,
    /// Email entry row.
    pub email_row: EntryRow,
    /// Password entry row.
    pub password_row: PasswordEntryRow,
    /// User ID entry row.
    pub user_id_row: EntryRow,
    /// Auth token entry row.
    pub auth_token_row: PasswordEntryRow,
    /// Email/password login toggle button.
    pub email_toggle: ToggleButton,
    /// Token login toggle button.
    pub token_toggle: ToggleButton,
}

/// Builds the login view UI and returns the root widget with widget references.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `sender` - Channel sender for auth events to the GUI thread
pub fn build(state: &AppState, sender: Sender<AuthEvent>) -> LoginWidgets {
    let content = Box::new(Vertical, 18);
    content.set_margin_top(36);
    content.set_margin_bottom(36);
    content.set_margin_start(36);
    content.set_margin_end(36);
    content.set_valign(Center);

    let title = Label::new(Some("Qobuz Downloader"));
    title.add_css_class("title-2");
    content.append(&title);

    let subtitle = Label::new(Some("Sign in with your Qobuz account"));
    subtitle.add_css_class("dim-label");
    content.append(&subtitle);

    let toggle_box = Box::new(Vertical, 6);
    toggle_box.set_halign(Center);

    let email_toggle = ToggleButton::with_label("Email / Password");
    email_toggle.set_active(true);
    let token_toggle = ToggleButton::with_label("User ID / Token");
    email_toggle.set_group(Some(&token_toggle));

    toggle_box.append(&email_toggle);
    toggle_box.append(&token_toggle);
    content.append(&toggle_box);

    let email_row = EntryRow::builder().title("Email").build();
    email_row.set_hexpand(true);
    content.append(&email_row);

    let password_row = PasswordEntryRow::builder().title("Password").build();
    password_row.set_hexpand(true);
    content.append(&password_row);

    let user_id_row = EntryRow::builder().title("User ID").build();
    user_id_row.set_hexpand(true);
    user_id_row.set_visible(false);
    content.append(&user_id_row);

    let auth_token_row = PasswordEntryRow::builder().title("Auth Token").build();
    auth_token_row.set_hexpand(true);
    auth_token_row.set_visible(false);
    content.append(&auth_token_row);

    connect_toggle_handlers(
        &email_toggle,
        &token_toggle,
        &email_row,
        &password_row,
        &user_id_row,
        &auth_token_row,
    );

    let error_label = Label::new(None);
    error_label.add_css_class("error");
    error_label.set_visible(false);
    content.append(&error_label);

    let submit_button = Button::with_label("Sign In");
    submit_button.add_css_class("suggested-action");
    submit_button.add_css_class("pill");
    submit_button.set_halign(Center);
    content.append(&submit_button);

    let widgets = LoginWidgets {
        root: content,
        error_label,
        submit_button,
        email_row,
        password_row,
        user_id_row,
        auth_token_row,
        email_toggle,
        token_toggle,
    };

    connect_submit_handler(state, sender, &widgets);

    widgets
}

/// Returns which login method is currently selected.
pub fn current_method(widgets: &LoginWidgets) -> LoginMethod {
    if widgets.email_toggle.is_active() {
        LoginMethod::EmailPassword
    } else {
        LoginMethod::Token
    }
}

/// Connects toggle button handlers to show/hide the appropriate credential fields.
fn connect_toggle_handlers(
    email_toggle: &ToggleButton,
    token_toggle: &ToggleButton,
    email_row: &EntryRow,
    password_row: &PasswordEntryRow,
    user_id_row: &EntryRow,
    auth_token_row: &PasswordEntryRow,
) {
    let email_row_c = email_row.clone();
    let password_row_c = password_row.clone();
    let user_id_row_c = user_id_row.clone();
    let auth_token_row_c = auth_token_row.clone();
    email_toggle.connect_toggled(clone!(
        #[strong]
        token_toggle,
        move |btn| {
            let is_email = btn.is_active();
            token_toggle.set_active(!is_email);
            email_row_c.set_visible(is_email);
            password_row_c.set_visible(is_email);
            user_id_row_c.set_visible(!is_email);
            auth_token_row_c.set_visible(!is_email);
        }
    ));

    let email_row_c = email_row.clone();
    let password_row_c = password_row.clone();
    let user_id_row_c = user_id_row.clone();
    let auth_token_row_c = auth_token_row.clone();
    token_toggle.connect_toggled(clone!(
        #[strong]
        email_toggle,
        move |btn| {
            let is_token = btn.is_active();
            email_toggle.set_active(!is_token);
            email_row_c.set_visible(!is_token);
            password_row_c.set_visible(!is_token);
            user_id_row_c.set_visible(is_token);
            auth_token_row_c.set_visible(is_token);
        }
    ));
}

/// Connects the submit button handler for both login modes.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `sender` - Channel sender for auth events
/// * `widgets` - Cloneable references to all login UI widgets
fn connect_submit_handler(state: &AppState, sender: Sender<AuthEvent>, widgets: &LoginWidgets) {
    let api_service = Arc::clone(&state.api_service);
    let LoginWidgets {
        email_row,
        password_row,
        user_id_row,
        auth_token_row,
        error_label,
        submit_button,
        email_toggle,
        ..
    } = widgets.clone();

    submit_button.connect_clicked(clone!(
        #[strong]
        email_row,
        #[strong]
        password_row,
        #[strong]
        user_id_row,
        #[strong]
        auth_token_row,
        #[strong]
        error_label,
        #[strong]
        submit_button,
        #[strong]
        email_toggle,
        move |_| {
            let is_email_mode = email_toggle.is_active();

            if is_email_mode {
                handle_email_login(
                    &email_row,
                    &password_row,
                    &error_label,
                    &submit_button,
                    &sender,
                    &api_service,
                );
            } else {
                handle_token_login(
                    &user_id_row,
                    &auth_token_row,
                    &error_label,
                    &submit_button,
                    &sender,
                    &api_service,
                );
            }
        }
    ));
}

/// Handles email/password login submission.
fn handle_email_login(
    email_row: &EntryRow,
    password_row: &PasswordEntryRow,
    error_label: &Label,
    submit_button: &Button,
    sender: &Sender<AuthEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
) {
    let email = email_row.text().to_string();
    let password = password_row.text().to_string();

    if email.is_empty() || password.is_empty() {
        error_label.set_text("Email and password are required");
        error_label.set_visible(true);
        return;
    }

    error_label.set_visible(false);
    submit_button.set_sensitive(false);
    email_row.set_sensitive(false);
    password_row.set_sensitive(false);

    let sender = sender.clone();
    let api_service = Arc::clone(api_service);
    spawn_blocking(move || {
        let result = perform_login(&api_service, &email, &password);
        let event = match result {
            Ok(user_id) => Authenticated { user_id },
            Err(err) => {
                warn!(error = %err, "Login failed");
                AuthenticationFailed {
                    error: format!("{err}"),
                }
            }
        };
        if let Err(err) = sender.send_blocking(event) {
            warn!(error = %err, "Failed to send auth event, receiver likely dropped");
        }
    });
}

/// Handles user ID/token login submission.
fn handle_token_login(
    user_id_row: &EntryRow,
    auth_token_row: &PasswordEntryRow,
    error_label: &Label,
    submit_button: &Button,
    sender: &Sender<AuthEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
) {
    let user_id = user_id_row.text().to_string();
    let auth_token = auth_token_row.text().to_string();

    if user_id.is_empty() || auth_token.is_empty() {
        error_label.set_text("User ID and auth token are required");
        error_label.set_visible(true);
        return;
    }

    error_label.set_visible(false);
    submit_button.set_sensitive(false);
    user_id_row.set_sensitive(false);
    auth_token_row.set_sensitive(false);

    let sender = sender.clone();
    let api_service = Arc::clone(api_service);
    spawn_blocking(move || {
        let result = perform_token_login(&api_service, &user_id, &auth_token);
        let event = match result {
            Ok(user_id) => Authenticated { user_id },
            Err(err) => {
                warn!(error = %err, "Token login failed");
                AuthenticationFailed {
                    error: format!("{err}"),
                }
            }
        };
        if let Err(err) = sender.send_blocking(event) {
            warn!(error = %err, "Failed to send auth event, receiver likely dropped");
        }
    });
}

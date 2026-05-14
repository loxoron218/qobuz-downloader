//! Login view UI for Qobuz authentication.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        EntryRow, PasswordEntryRow, PreferencesGroup,
        gio::spawn_blocking,
        glib::clone,
        gtk::{
            Align::{Center, Start},
            Box, Button, CheckButton, Label,
            Orientation::Vertical,
            Stack,
        },
        prelude::{BoxExt, ButtonExt, CheckButtonExt, EditableExt, PreferencesGroupExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
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
    /// Email/password login radio button.
    pub email_radio: CheckButton,
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

    let title = Label::new(Some("Qobuz"));
    title.add_css_class("title-1");
    content.append(&title);

    let subtitle = Label::new(Some("Enter your credentials to get started"));
    subtitle.add_css_class("subtitle");
    content.append(&subtitle);

    let spacer = Box::new(Vertical, 0);
    spacer.set_vexpand(true);
    content.append(&spacer);

    let selection_label = Label::new(Some("Authentication Method"));
    selection_label.set_halign(Start);
    selection_label.add_css_class("heading");
    content.append(&selection_label);

    let email_radio = CheckButton::with_label("Email/Username and Password");
    email_radio.set_active(true);
    let token_radio = CheckButton::with_label("User ID and Auth Token");
    email_radio.set_group(Some(&token_radio));

    let radio_box = Box::new(Vertical, 6);
    radio_box.append(&email_radio);
    radio_box.append(&token_radio);
    content.append(&radio_box);

    let email_preferences_group = PreferencesGroup::builder()
        .title("Email/Username and Password")
        .build();

    let email_row = EntryRow::builder()
        .title("Email or Username")
        .show_apply_button(false)
        .build();
    email_preferences_group.add(&email_row);

    let password_row = PasswordEntryRow::builder().title("Password").build();
    email_preferences_group.add(&password_row);

    let email_section = Box::new(Vertical, 16);
    email_section.append(&email_preferences_group);

    let token_preferences_group = PreferencesGroup::builder()
        .title("User ID and Auth Token")
        .build();

    let user_id_row = EntryRow::builder()
        .title("User ID")
        .show_apply_button(false)
        .build();
    token_preferences_group.add(&user_id_row);

    let auth_token_row = PasswordEntryRow::builder().title("Auth Token").build();
    token_preferences_group.add(&auth_token_row);

    let token_section = Box::new(Vertical, 16);
    token_section.append(&token_preferences_group);

    let credential_stack = Stack::new();
    credential_stack.add_named(&email_section, Some("email"));
    credential_stack.add_named(&token_section, Some("token"));
    credential_stack.set_visible_child_name("email");
    content.append(&credential_stack);

    connect_toggle_handlers(&email_radio, &token_radio, &credential_stack);

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
        email_radio,
    };

    connect_submit_handler(state, sender, &widgets);

    widgets
}

/// Returns which login method is currently selected.
pub fn current_method(widgets: &LoginWidgets) -> LoginMethod {
    if widgets.email_radio.is_active() {
        LoginMethod::EmailPassword
    } else {
        LoginMethod::Token
    }
}

/// Connects radio button handlers to switch the credential stack.
fn connect_toggle_handlers(
    email_radio: &CheckButton,
    token_radio: &CheckButton,
    credential_stack: &Stack,
) {
    let credential_stack_c1 = credential_stack.clone();
    let email_radio_c1 = email_radio.clone();
    email_radio.connect_toggled(move |_| {
        if email_radio_c1.is_active() {
            credential_stack_c1.set_visible_child_name("email");
        }
    });

    let credential_stack_c2 = credential_stack.clone();
    let token_radio_c2 = token_radio.clone();
    token_radio.connect_toggled(move |_| {
        if token_radio_c2.is_active() {
            credential_stack_c2.set_visible_child_name("token");
        }
    });
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
        email_radio,
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
        email_radio,
        move |_| {
            let is_email_mode = email_radio.is_active();

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

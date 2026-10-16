//! Login form UI for Qobuz authentication.

use std::{hint::black_box, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        EntryRow, PasswordEntryRow, PreferencesGroup,
        glib::clone,
        gtk::{
            Align::{Center, Start},
            Box, Button, CheckButton, Label,
            Orientation::Vertical,
            Stack,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, CheckButtonExt, PreferencesGroupExt},
    },
};

use crate::{
    app::AppState,
    auth::{
        login_form::LoginMethod::{EmailPassword, Token},
        login_handlers::{handle_email_login, handle_token_login},
        session::AuthEvent,
    },
};

/// Credential input widgets for both login methods.
#[derive(Clone, Debug)]
struct CredentialFields {
    /// Stack switching between email and token sections.
    stack: Stack,
    /// Email entry row.
    email_row: EntryRow,
    /// Password entry row.
    password_row: PasswordEntryRow,
    /// User ID entry row.
    user_id_row: EntryRow,
    /// Auth token entry row.
    auth_token_row: PasswordEntryRow,
}

/// Login method selected by the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginMethod {
    /// Email and password authentication.
    EmailPassword,
    /// User ID and auth token authentication.
    Token,
}

/// Widgets from the login view needed for external event handling.
#[derive(Clone, Debug)]
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
#[must_use]
pub fn build(state: &AppState, sender: Sender<AuthEvent>) -> LoginWidgets {
    let content = Box::builder()
        .orientation(Vertical)
        .spacing(18)
        .margin_top(36)
        .margin_bottom(36)
        .margin_start(36)
        .margin_end(36)
        .valign(Center)
        .css_classes(["login-content"])
        .tooltip_text("Login form")
        .build();
    content.update_property(&[PropertyLabel("Login form")]);

    let title = Label::builder()
        .label("Qobuz")
        .css_classes(["title-1"])
        .tooltip_text("Application name")
        .build();
    content.append(&title);

    let subtitle = Label::builder()
        .label("Enter your credentials to get started")
        .css_classes(["subtitle"])
        .tooltip_text("Login instructions")
        .build();
    content.append(&subtitle);

    let spacer = Box::builder()
        .orientation(Vertical)
        .vexpand(true)
        .css_classes(["login-spacer"])
        .tooltip_text("Spacer")
        .build();
    content.append(&spacer);

    let selection_label = Label::builder()
        .label("Authentication Method")
        .halign(Start)
        .css_classes(["heading"])
        .tooltip_text("Choose an authentication method")
        .build();
    content.append(&selection_label);

    let email_radio = CheckButton::builder()
        .label("Email/Username and Password")
        .active(true)
        .css_classes(["login-radio", "text-button"])
        .tooltip_text("Log in with email and password")
        .build();
    let token_radio = CheckButton::builder()
        .label("User ID and Auth Token")
        .css_classes(["login-radio", "text-button"])
        .tooltip_text("Log in with user ID and auth token")
        .build();
    email_radio.set_group(Some(&token_radio));

    let radio_box = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["login-radio-box"])
        .tooltip_text("Authentication method options")
        .build();
    radio_box.append(&email_radio);
    radio_box.append(&token_radio);
    content.append(&radio_box);

    let credentials = build_credential_stack();
    let email_row = credentials.email_row;
    let password_row = credentials.password_row;
    let user_id_row = credentials.user_id_row;
    let auth_token_row = credentials.auth_token_row;
    let credential_stack = credentials.stack;
    content.append(&credential_stack);

    connect_toggle_handlers(&email_radio, &token_radio, &credential_stack);

    let error_label = Label::builder()
        .css_classes(["error"])
        .tooltip_text("Login error message")
        .visible(false)
        .build();
    content.append(&error_label);

    let submit_button = Button::builder()
        .label("Sign In")
        .halign(Center)
        .css_classes(["suggested-action", "pill", "text-button"])
        .tooltip_text("Sign in to Qobuz")
        .build();
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

/// Builds the credential stack with email and token sections.
///
/// # Returns
///
/// Credential input widgets for both login methods.
fn build_credential_stack() -> CredentialFields {
    let email_preferences_group = PreferencesGroup::builder()
        .title("Email/Username and Password")
        .css_classes(["login-group"])
        .tooltip_text("Email login credentials")
        .build();

    let email_row = EntryRow::builder()
        .title("Email or Username")
        .show_apply_button(false)
        .css_classes(["login-entry"])
        .tooltip_text("Enter your email or username")
        .build();
    email_preferences_group.add(&email_row);

    let password_row = PasswordEntryRow::builder()
        .title("Password")
        .css_classes(["login-entry", "password"])
        .tooltip_text("Enter your password")
        .build();
    email_preferences_group.add(&password_row);

    let email_section = Box::builder()
        .orientation(Vertical)
        .spacing(16)
        .css_classes(["login-section"])
        .tooltip_text("Email login section")
        .build();
    email_section.append(&email_preferences_group);

    let token_preferences_group = PreferencesGroup::builder()
        .title("User ID and Auth Token")
        .css_classes(["login-group"])
        .tooltip_text("Token login credentials")
        .build();

    let user_id_row = EntryRow::builder()
        .title("User ID")
        .show_apply_button(false)
        .css_classes(["login-entry"])
        .tooltip_text("Enter your user ID")
        .build();
    token_preferences_group.add(&user_id_row);

    let auth_token_row = PasswordEntryRow::builder()
        .title("Auth Token")
        .css_classes(["login-entry", "password"])
        .tooltip_text("Enter your auth token")
        .build();
    token_preferences_group.add(&auth_token_row);

    let token_section = Box::builder()
        .orientation(Vertical)
        .spacing(16)
        .css_classes(["login-section"])
        .tooltip_text("Token login section")
        .build();
    token_section.append(&token_preferences_group);

    let credential_stack = Stack::builder()
        .css_classes(["login-stack"])
        .tooltip_text("Credential input")
        .build();
    drop(credential_stack.add_named(&email_section, Some("email")));
    drop(credential_stack.add_named(&token_section, Some("token")));
    credential_stack.set_visible_child_name("email");
    credential_stack.update_property(&[PropertyLabel("Credential input")]);
    CredentialFields {
        stack: credential_stack,
        email_row,
        password_row,
        user_id_row,
        auth_token_row,
    }
}

/// Returns which login method is currently selected.
#[must_use]
pub fn current_method(widgets: &LoginWidgets) -> LoginMethod {
    if widgets.email_radio.is_active() {
        EmailPassword
    } else {
        Token
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
    let email_toggle_handler = email_radio.connect_toggled(move |_| {
        if email_radio_c1.is_active() {
            credential_stack_c1.set_visible_child_name("email");
        }
    });
    for _ in [black_box(email_toggle_handler)] {}

    let credential_stack_c2 = credential_stack.clone();
    let token_radio_c2 = token_radio.clone();
    let token_toggle_handler = token_radio.connect_toggled(move |_| {
        if token_radio_c2.is_active() {
            credential_stack_c2.set_visible_child_name("token");
        }
    });
    for _ in [black_box(token_toggle_handler)] {}
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

    let submit_handler = submit_button.connect_clicked(clone!(
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
    for _ in [black_box(submit_handler)] {}
}

//! Login submission handlers for email and token authentication.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        EntryRow, PasswordEntryRow,
        gio::spawn_blocking,
        gtk::{Button, Label},
        prelude::{EditableExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::api::service_types::QobuzApiService,
    tracing::warn,
};

use crate::auth::session::{
    AuthEvent,
    AuthEvent::{Authenticated, AuthenticationFailed},
    perform_login, perform_token_login,
};

/// Handles email/password login submission.
pub fn handle_email_login(
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
    drop(spawn_blocking(move || {
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
    }));
}

/// Handles user ID/token login submission.
pub fn handle_token_login(
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
    drop(spawn_blocking(move || {
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
    }));
}

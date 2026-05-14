//! Session management for authentication.

use std::sync::Arc;

use {
    keyring::{
        StoredCredentials::{self, EmailPassword, Token},
        load, store,
    },
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
    tracing::info,
};

use crate::{
    auth::keyring,
    errors::AppError::{self, NotAuthenticated},
};

/// Events sent from background auth operations to the GUI thread.
#[derive(Clone, Debug)]
pub enum AuthEvent {
    /// Login succeeded.
    Authenticated {
        /// The authenticated user's ID.
        user_id: String,
    },
    /// Login failed with reason.
    AuthenticationFailed {
        /// Error description.
        error: String,
    },
}

/// Tracks the current authentication status.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AuthState {
    /// No credentials stored or login required.
    #[default]
    Unauthenticated,
    /// Login in progress.
    Authenticating,
    /// Successfully authenticated with user ID.
    Authenticated {
        /// The authenticated user's ID.
        user_id: String,
    },
}

/// Performs login with email and password, storing credentials in the keyring.
///
/// This function is designed to be called from `gio::spawn_blocking`.
///
/// # Arguments
///
/// * `api_service` - Shared API service to authenticate with
/// * `email` - Qobuz account email
/// * `password` - Qobuz account password
///
/// # Returns
///
/// The authenticated user ID on success.
///
/// # Errors
///
/// Returns `AppError` if keyring storage or API login fails.
pub fn perform_login(
    api_service: &Arc<Mutex<QobuzApiService>>,
    email: &str,
    password: &str,
) -> Result<String, AppError> {
    store(&EmailPassword {
        email: email.to_string(),
        password: password.to_string(),
    })?;
    let mut api = api_service.lock();
    api.login(email, password)?;
    let user_id = extract_user_id(&api);
    drop(api);
    info!(user_id, "Login successful");
    Ok(user_id)
}

/// Performs login with user ID and auth token, storing credentials in the keyring.
///
/// This function is designed to be called from `gio::spawn_blocking`.
///
/// # Arguments
///
/// * `api_service` - Shared API service to authenticate with
/// * `user_id` - Qobuz user ID
/// * `auth_token` - User authentication token
///
/// # Returns
///
/// The authenticated user ID on success.
///
/// # Errors
///
/// Returns `AppError` if keyring storage or API login fails.
pub fn perform_token_login(
    api_service: &Arc<Mutex<QobuzApiService>>,
    user_id: &str,
    auth_token: &str,
) -> Result<String, AppError> {
    store(&Token {
        user_id: user_id.to_string(),
        auth_token: auth_token.to_string(),
    })?;
    let mut api = api_service.lock();
    api.login_with_token(user_id, auth_token)?;
    drop(api);
    info!(user_id, "Token login successful");
    Ok(user_id.to_string())
}

/// Attempts silent login using stored keyring credentials.
///
/// This function is designed to be called from `gio::spawn_blocking`.
///
/// # Arguments
///
/// * `api_service` - Shared API service to authenticate with
///
/// # Returns
///
/// The authenticated user ID on success.
///
/// # Errors
///
/// Returns `AppError` if no credentials are stored or login fails.
pub fn perform_keyring_login(
    api_service: &Arc<Mutex<QobuzApiService>>,
) -> Result<String, AppError> {
    let creds = load()?.ok_or(NotAuthenticated)?;
    let user_id = authenticate_with_credentials(api_service, &creds)?;
    info!(user_id, "Keyring login successful");
    Ok(user_id)
}

/// Authenticates the API service using the stored credential type.
///
/// # Returns
///
/// The authenticated user ID on success.
///
/// # Errors
///
/// Returns `AppError` if the API login call fails.
fn authenticate_with_credentials(
    api_service: &Arc<Mutex<QobuzApiService>>,
    creds: &StoredCredentials,
) -> Result<String, AppError> {
    let mut api = api_service.lock();
    match creds {
        EmailPassword { email, password } => {
            api.login(email, password)?;
        }
        Token {
            user_id,
            auth_token,
        } => {
            api.login_with_token(user_id, auth_token)?;
        }
    }
    let user_id = extract_user_id(&api);
    drop(api);
    Ok(user_id)
}

/// Extracts a user ID from the API service.
///
/// Uses the auth token presence as a proxy since the API service does not expose `user_id`
/// directly.
fn extract_user_id(api: &QobuzApiService) -> String {
    if api.require_auth_token().is_ok() {
        "authenticated".to_string()
    } else {
        "unknown".to_string()
    }
}

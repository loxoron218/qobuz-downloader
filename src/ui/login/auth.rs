use qobuz_api_rust::{
    QobuzApiError::{self, AuthenticationError},
    QobuzApiService,
    utils::get_md5_hash,
};

/// Performs asynchronous authentication with the Qobuz API service.
///
/// This function handles the actual authentication process by initializing the Qobuz API
/// service and attempting to log in using the provided credentials. It supports both
/// authentication methods and validates that at least one complete credential set is provided.
///
/// # Arguments
///
/// * `email` - Email address or username (for email/password authentication)
/// * `password` - Plain text password (will be MD5 hashed internally)
/// * `user_id` - User ID (for token-based authentication)
/// * `auth_token` - Authentication token (for token-based authentication)
///
/// # Returns
///
/// * `Ok(QobuzApiService)` - Successfully authenticated API service instance
/// * `Err(QobuzApiError)` - Authentication failure or initialization error
///
/// # Authentication Logic
///
/// The function determines which authentication method to use based on which
/// credentials are non-empty:
/// - If both `email` and `password` are non-empty, uses email/password authentication
/// - If both `user_id` and `auth_token` are non-empty, uses token-based authentication
/// - If neither complete set is provided, returns an [`AuthenticationError`]
///
/// For email/password authentication, the password is automatically MD5 hashed
/// as required by the Qobuz API.
///
/// # Errors
///
/// This function can return various [`QobuzApiError`] variants:
/// - [`AuthenticationError`] - Invalid credentials or incomplete credential sets
/// - [`QobuzApiInitializationError`] - Failed to initialize the API service
/// - Other API-specific errors from the underlying `qobuz-api-rust` crate
pub async fn perform_login(
    email: String,
    password: String,
    user_id: String,
    auth_token: String,
) -> Result<QobuzApiService, QobuzApiError> {
    // Initialize the Qobuz API service
    let mut service = QobuzApiService::new().await?;

    // Determine which authentication method to use
    if !email.is_empty() && !password.is_empty() {
        // Email/Username + Password authentication
        // Password must be MD5 hashed for Qobuz API
        let hashed_password = get_md5_hash(&password);
        service.login(&email, &hashed_password).await?;
    } else if !user_id.is_empty() && !auth_token.is_empty() {
        // User ID + Auth Token authentication
        service.login_with_token(&user_id, &auth_token).await?;
    } else {
        return Err(AuthenticationError {
            message: "Please provide either email/password or user ID/auth token".to_string(),
        });
    }

    Ok(service)
}

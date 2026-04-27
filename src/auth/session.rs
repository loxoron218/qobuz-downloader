//! Session management for authentication.

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
    /// Silent re-authentication succeeded.
    Reauthenticated,
    /// Silent re-authentication failed; user must log in again.
    ReauthFailed {
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
    /// Token expired, attempting re-auth.
    Expired,
}

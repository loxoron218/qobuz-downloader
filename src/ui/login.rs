pub mod auth;
pub mod credentials;
pub mod signals;
pub mod ui;
pub mod window;

use std::{boxed::Box, cell::RefCell, rc::Rc};

use {
    libadwaita::{
        ApplicationWindow, EntryRow, PasswordEntryRow, ToastOverlay,
        gtk::{Button, CheckButton, Stack},
    },
    qobuz_api_rust::QobuzApiService,
};

/// Type alias for a callback function that handles successful login.
///
/// This callback is invoked when authentication succeeds and receives the authenticated
/// [`QobuzApiService`] instance. The callback is wrapped in [`Rc<RefCell<_>>`] to allow
/// shared mutable access across different parts of the UI.
type LoginSuccessCallback = Rc<RefCell<Option<Box<dyn Fn(QobuzApiService)>>>>;

/// A login window for authenticating with the Qobuz service.
///
/// This struct represents the complete login interface that allows users to authenticate
/// using either email/username + password or user ID + auth token credentials.
/// It provides a modal dialog with form validation, loading states, and credential persistence.
///
/// # Authentication Methods
///
/// The login window supports two authentication methods:
/// - **Email/Username + Password**: Standard email or username with password (password is MD5 hashed)
/// - **User ID + Auth Token**: Direct authentication using Qobuz user ID and authentication token
///
/// # Credential Persistence
///
/// Successfully entered credentials are automatically saved to a `.env` file in the current
/// working directory for future sessions. The authentication method preference is also stored.
///
/// # Examples
///
/// ```rust
/// let login_window = LoginWindow::new(&app);
/// login_window.set_on_login_success(|service| {
///     // Handle successful authentication
///     println!("Login successful!");
/// });
/// login_window.present();
/// ```
#[derive(Clone)]
pub struct LoginWindow {
    /// The main application window container.
    pub window: ApplicationWindow,
    /// Overlay for displaying toast notifications.
    pub toast_overlay: ToastOverlay,
    /// Input field for email or username (wrapped in AdwEntryRow).
    pub email_entry_row: EntryRow,
    /// Input field for password (masked).
    pub password_entry: PasswordEntryRow,
    /// Input field for user ID (token-based auth, wrapped in AdwEntryRow).
    pub user_id_entry_row: EntryRow,
    /// Input field for authentication token (masked).
    pub auth_token_entry: PasswordEntryRow,
    /// Primary login action button.
    pub login_button: Button,
    /// Radio button for email/password authentication method.
    pub email_radio: CheckButton,
    /// Radio button for user ID/token authentication method.
    pub token_radio: CheckButton,
    /// Stack widget containing the different credential input sections.
    pub credential_stack: Stack,
    /// Optional authenticated API service instance (stored if no callback is provided).
    pub api_service: Rc<RefCell<Option<QobuzApiService>>>,
    /// Callback function invoked on successful authentication.
    pub on_login_success: LoginSuccessCallback,
}

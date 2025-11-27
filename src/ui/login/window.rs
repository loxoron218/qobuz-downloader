use std::{boxed::Box, cell::RefCell, rc::Rc};

use {
    libadwaita::{
        Application, ApplicationWindow, EntryRow, PasswordEntryRow, ToastOverlay,
        gtk::{Button, CheckButton, Stack, glib::Propagation},
        prelude::{CheckButtonExt, GtkWindowExt},
    },
    qobuz_api_rust::QobuzApiService,
};

use crate::ui::login::LoginWindow;

impl LoginWindow {
    /// Creates a new [`LoginWindow`] instance.
    ///
    /// This constructor initializes all UI components, sets up the widget hierarchy,
    /// loads any existing credentials from the `.env` file, and connects signal handlers.
    ///
    /// # Arguments
    ///
    /// * `app` - The parent [`Application`] instance that owns this window.
    ///
    /// # Returns
    ///
    /// A new [`LoginWindow`] instance ready for presentation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let login_window = LoginWindow::new(&app);
    /// login_window.present();
    /// ```
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Qobuz Login")
            .default_width(400)
            .default_height(500)
            .modal(true)
            .build();

        let toast_overlay = ToastOverlay::new();

        let email_entry_row = EntryRow::builder()
            .title("Email or Username")
            .show_apply_button(false)
            .build();
        let password_entry = PasswordEntryRow::builder().title("Password").build();
        let user_id_entry_row = EntryRow::builder()
            .title("User ID")
            .show_apply_button(false)
            .build();
        let auth_token_entry = PasswordEntryRow::builder().title("Auth Token").build();
        let login_button = Button::builder()
            .label("Login")
            .css_classes(["suggested-action"])
            .halign(libadwaita::gtk::Align::Center)
            .hexpand(true)
            .build();

        // Create radio buttons for credential type selection
        let email_radio = CheckButton::with_label("Email/Username and Password");
        let token_radio = CheckButton::with_label("User ID and Auth Token");

        // Make them mutually exclusive (radio button behavior)
        email_radio.set_group(Some(&token_radio));

        let credential_stack = Stack::new();

        let login_window = Self {
            window,
            toast_overlay,
            email_entry_row,
            password_entry,
            user_id_entry_row,
            auth_token_entry,
            login_button,
            email_radio,
            token_radio,
            credential_stack,
            api_service: Rc::new(RefCell::new(None)),
            on_login_success: Rc::new(RefCell::new(None)),
        };

        login_window.setup_widgets();

        // Load credentials from .env and pre-fill fields if available
        // This must be called after setup_widgets() so that the stack children exist
        login_window.load_credentials_from_env();
        login_window.setup_signals();

        login_window
    }

    /// Presents the login window to the user.
    ///
    /// This method makes the login window visible and brings it to the front
    /// of other windows. It should be called after the window has been fully
    /// configured with any necessary callbacks.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let login_window = LoginWindow::new(&app);
    /// login_window.set_on_login_success(|service| {
    ///     // Handle successful login
    /// });
    /// login_window.present(); // Show the window
    /// ```
    pub fn present(&self) {
        self.window.present();
    }

    /// Connects a callback to handle window close requests.
    ///
    /// This method allows custom logic to be executed when the user attempts
    /// to close the login window (e.g., by clicking the close button or pressing Alt+F4).
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that takes no arguments and returns a [`Propagation`] value.
    ///   Return [`Proceed`] to allow the window to close, or [`Stop`] to prevent closing.
    ///
    /// # Examples
    ///
    /// ```rust
    /// login_window.connect_close_request(|| {
    ///     // Quit the entire application when login window is closed
    ///     app.quit();
    ///     Proceed
    /// });
    /// ```
    pub fn connect_close_request<F: Fn() -> Propagation + 'static>(&self, f: F) {
        self.window.connect_close_request(move |_| f());
    }

    /// Sets a callback function to be invoked on successful authentication.
    ///
    /// This method configures what happens when the user successfully logs in.
    /// The callback receives the authenticated [`QobuzApiService`] instance which
    /// can be used for subsequent API operations.
    ///
    /// If no callback is set, the authenticated service will be stored internally
    /// in the [`api_service`] field for later retrieval.
    ///
    /// # Arguments
    ///
    /// * `callback` - A closure that takes a [`QobuzApiService`] parameter and handles
    ///   the successful authentication result.
    ///
    /// # Examples
    ///
    /// ```rust
    /// login_window.set_on_login_success(|service| {
    ///     println!("Successfully authenticated with Qobuz!");
    ///     // Create main application window with the authenticated service
    ///     let main_window = MainWindow::new(&app, service);
    ///     main_window.present();
    /// });
    /// ```
    pub fn set_on_login_success<F: Fn(QobuzApiService) + 'static>(&self, callback: F) {
        *self.on_login_success.borrow_mut() = Some(Box::new(callback));
    }
}

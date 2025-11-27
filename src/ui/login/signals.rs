use {
    libadwaita::{
        Toast,
        gtk::glib::MainContext,
        prelude::{ButtonExt, CheckButtonExt, EditableExt, WidgetExt},
    },
    qobuz_api_rust::QobuzApiError::{AuthenticationError, QobuzApiInitializationError},
};

use crate::ui::login::{
    LoginWindow, auth::perform_login, credentials::save_user_credentials_to_env,
};

impl LoginWindow {
    /// Sets up signal handlers for UI interactions.
    ///
    /// This private method connects event handlers for:
    /// - Radio button toggles to switch between credential input sections
    /// - Login button clicks to initiate authentication
    ///
    /// The login process includes:
    /// 1. Input validation based on the selected authentication method
    /// 2. Display of loading feedback via toast notifications
    /// 3. Asynchronous authentication using [`perform_login`]
    /// 4. Credential persistence to `.env` file on success
    /// 5. Error handling with user-friendly toast messages
    ///
    /// Signal handlers capture necessary UI state and spawn asynchronous tasks
    /// using GTK's [`MainContext`] to maintain responsiveness during authentication.
    pub(crate) fn setup_signals(&self) {
        let toast_overlay = self.toast_overlay.clone();
        let login_button = self.login_button.clone();
        let email_entry_row = self.email_entry_row.clone();
        let password_entry = self.password_entry.clone();
        let user_id_entry_row = self.user_id_entry_row.clone();
        let auth_token_entry = self.auth_token_entry.clone();
        let api_service = self.api_service.clone();
        let on_login_success = self.on_login_success.clone();
        let email_radio = self.email_radio.clone();
        let token_radio = self.token_radio.clone();

        // Connect radio button signals to update the visible credential section
        let credential_stack_clone1 = self.credential_stack.clone();
        let email_radio_clone1 = self.email_radio.clone();
        email_radio.connect_toggled(move |_| {
            if email_radio_clone1.is_active() {
                credential_stack_clone1.set_visible_child_name("email");
            }
        });

        let credential_stack_clone2 = self.credential_stack.clone();
        let token_radio_clone2 = self.token_radio.clone();
        token_radio.connect_toggled(move |_| {
            if token_radio_clone2.is_active() {
                credential_stack_clone2.set_visible_child_name("token");
            }
        });

        login_button.clone().connect_clicked(move |_| {
            let toast_overlay = toast_overlay.clone();
            let email = email_entry_row.text().to_string();
            let password = password_entry.text().to_string();
            let user_id = user_id_entry_row.text().to_string();
            let auth_token = auth_token_entry.text().to_string();
            let email_radio_active = email_radio.is_active();
            let on_login_success_clone = on_login_success.clone();

            // Validate inputs based on selected authentication method
            if email_radio_active {
                if email.is_empty() {
                    let error_toast = Toast::new("Please enter your email or username");
                    toast_overlay.add_toast(error_toast);
                    return;
                }
                if password.is_empty() {
                    let error_toast = Toast::new("Please enter your password");
                    toast_overlay.add_toast(error_toast);
                    return;
                }

                // Basic email validation if it contains @
                if email.contains('@') && !email.contains('.') {
                    let error_toast = Toast::new("Invalid email format");
                    toast_overlay.add_toast(error_toast);
                    return;
                }
            } else {
                if user_id.is_empty() {
                    let error_toast = Toast::new("Please enter your User ID");
                    toast_overlay.add_toast(error_toast);
                    return;
                }
                if auth_token.is_empty() {
                    let error_toast = Toast::new("Please enter your Auth Token");
                    toast_overlay.add_toast(error_toast);
                    return;
                }

                // Validate that user ID is numeric or alphanumeric
                if !user_id
                    .chars()
                    .all(|c: char| c.is_ascii_alphanumeric() || c == '-')
                {
                    let error_toast = Toast::new("Invalid User ID format");
                    toast_overlay.add_toast(error_toast);
                    return;
                }
            }

            // Show loading feedback
            let loading_toast = Toast::new("Authenticating...");
            loading_toast.set_timeout(0); // Persistent until dismissed
            toast_overlay.add_toast(loading_toast.clone());

            // Disable login button during authentication
            let login_button_clone = login_button.clone();
            login_button_clone.set_sensitive(false);

            // Spawn async login task
            let api_service_clone = api_service.clone();
            let email_clone = email.clone();
            let password_clone = password.clone();
            let user_id_clone = user_id.clone();
            let auth_token_clone = auth_token.clone();
            MainContext::default().spawn_local(async move {
                match perform_login(email, password, user_id, auth_token).await {
                    Ok(service) => {
                        // Save user credentials to .env file
                        if email_radio_active {
                            // Email/Username + Password authentication
                            let _ = save_user_credentials_to_env(
                                Some(&email_clone),
                                None,
                                &qobuz_api_rust::utils::get_md5_hash(&password_clone),
                                "email",
                            );
                        } else {
                            // User ID + Auth Token authentication
                            let _ = save_user_credentials_to_env(
                                None,
                                Some(&user_id_clone),
                                &auth_token_clone,
                                "token",
                            );
                        }

                        // Show success on main thread
                        toast_overlay.dismiss_all();
                        login_button_clone.set_sensitive(true); // Re-enable button
                        let success_toast = Toast::new("Login successful!");
                        toast_overlay.add_toast(success_toast);

                        // Call login success callback if set
                        if let Some(callback) = on_login_success_clone.borrow_mut().take() {
                            callback(service);
                        } else {
                            // If no callback is set, store the service for later retrieval
                            *api_service_clone.borrow_mut() = Some(service);
                        }
                    }

                    Err(e) => {
                        // Show error on main thread with more specific messaging
                        toast_overlay.dismiss_all();
                        login_button_clone.set_sensitive(true); // Re-enable button

                        let error_message = match e {
                            AuthenticationError { .. } => {
                                "Authentication failed: Invalid credentials".to_string()
                            }

                            QobuzApiInitializationError { .. } => {
                                "Failed to initialize API service".to_string()
                            }
                            _ => format!("Login failed: {}", e),
                        };

                        let error_toast = Toast::new(&error_message);
                        toast_overlay.add_toast(error_toast);
                    }
                }
            });
        });
    }
}

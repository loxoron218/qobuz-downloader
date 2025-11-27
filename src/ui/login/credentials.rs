use std::{
    fs::{read_to_string, write},
    path::Path,
};

use {
    libadwaita::gtk::prelude::{CheckButtonExt, EditableExt},
    qobuz_api_rust::QobuzApiError::{self, QobuzApiInitializationError},
};

use crate::ui::login::LoginWindow;

impl LoginWindow {
    /// Loads user authentication credentials from the `.env` file and pre-fills the login form.
    ///
    /// This function reads the `.env` file in the current working directory and looks for
    /// Qobuz-specific environment variables. It supports both authentication methods and
    /// respects the user's saved authentication method preference.
    ///
    /// # Environment Variables
    ///
    /// The following environment variables are recognized:
    /// - `QOBUZ_EMAIL` or `QOBUZ_USERNAME`: Email address or username for authentication
    /// - `QOBUZ_PASSWORD`: MD5-hashed password (stored securely)
    /// - `QOBUZ_USER_ID`: User ID for token-based authentication
    /// - `QOBUZ_USER_AUTH_TOKEN`: Authentication token for token-based authentication
    /// - `QOBUZ_AUTH_METHOD`: Preferred authentication method (`"email"` or `"token"`)
    ///
    /// # Behavior
    ///
    /// 1. If a valid authentication method preference is saved and complete credentials exist,
    ///    those credentials are loaded and the corresponding radio button is selected.
    /// 2. If no preference is saved but complete credentials exist for one method,
    ///    that method is used automatically.
    /// 3. If credentials exist for both methods but no preference is saved,
    ///    token-based authentication is preferred over email/password.
    /// 4. If no complete credential set is available, the form remains empty with
    ///    email/password as the default selection.
    ///
    /// # Error Handling
    ///
    /// The function silently fails if the `.env` file doesn't exist or can't be read,
    /// leaving the login form in its default state.
    pub(crate) fn load_credentials_from_env(&self) {
        // Check if .env file exists
        if !Path::new(".env").exists() {
            return;
        }

        // Read .env file content
        let env_content = match read_to_string(".env") {
            Ok(content) => content,
            Err(_) => return, // Silently fail if we can't read the file
        };

        let mut email_or_username = None;
        let mut password = None;
        let mut user_id = None;
        let mut auth_token = None;

        // Parse .env content line by line
        for line in env_content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue; // Skip empty lines and comments
            }

            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "QOBUZ_EMAIL" | "QOBUZ_USERNAME" => {
                        if !value.is_empty() {
                            email_or_username = Some(value.to_string());
                        }
                    }

                    "QOBUZ_PASSWORD" => {
                        if !value.is_empty() {
                            password = Some(value.to_string());
                        }
                    }

                    "QOBUZ_USER_ID" => {
                        if !value.is_empty() {
                            user_id = Some(value.to_string());
                        }
                    }

                    "QOBUZ_USER_AUTH_TOKEN" => {
                        if !value.is_empty() {
                            auth_token = Some(value.to_string());
                        }
                    }

                    _ => {} // Ignore other environment variables
                }
            }
        }

        // Check for saved authentication method preference
        let mut auth_method = None;
        for line in env_content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=')
                && key == "QOBUZ_AUTH_METHOD"
            {
                auth_method = Some(value.to_string());
                break;
            }
        }

        // Determine which authentication method to use
        let has_email_auth = email_or_username.is_some() && password.is_some();
        let has_token_auth = user_id.is_some() && auth_token.is_some();

        match auth_method.as_deref() {
            Some("token") if has_token_auth => {
                // Use token authentication as preferred
                if let Some(id) = user_id {
                    self.user_id_entry_row.set_text(&id);
                }
                if let Some(token) = auth_token {
                    self.auth_token_entry.set_text(&token);
                }
                self.token_radio.set_active(true);
                self.credential_stack.set_visible_child_name("token");
            }

            Some("email") if has_email_auth => {
                // Use email authentication as preferred
                if let Some(email_or_user) = email_or_username {
                    self.email_entry_row.set_text(&email_or_user);
                }
                if let Some(pwd) = password {
                    self.password_entry.set_text(&pwd);
                }
                self.email_radio.set_active(true);
                self.credential_stack.set_visible_child_name("email");
            }

            _ => {
                // Fallback logic: prefer token if available, otherwise email
                if has_token_auth {
                    if let Some(id) = user_id {
                        self.user_id_entry_row.set_text(&id);
                    }
                    if let Some(token) = auth_token {
                        self.auth_token_entry.set_text(&token);
                    }
                    self.token_radio.set_active(true);
                    self.credential_stack.set_visible_child_name("token");
                } else if has_email_auth {
                    if let Some(email_or_user) = email_or_username {
                        self.email_entry_row.set_text(&email_or_user);
                    }
                    if let Some(pwd) = password {
                        self.password_entry.set_text(&pwd);
                    }
                    self.email_radio.set_active(true);
                    self.credential_stack.set_visible_child_name("email");
                }
                // If neither complete set is available, leave fields empty and use default (email)
            }
        }
    }
}

/// Saves user authentication credentials to the `.env` file.
///
/// This function persists authentication credentials to enable automatic login
/// in future sessions. It intelligently manages the `.env` file by:
/// - Updating existing entries instead of creating duplicates
/// - Automatically detecting whether to use `QOBUZ_EMAIL` or `QOBUZ_USERNAME`
/// - Removing credentials from the unused authentication method
/// - Storing the preferred authentication method
///
/// # Arguments
///
/// * `email_or_username` - Email or username for email/password authentication (exclusive with `user_id`)
/// * `user_id` - User ID for token-based authentication (exclusive with `email_or_username`)
/// * `password_or_token` - MD5-hashed password or auth token
/// * `auth_method` - Authentication method used (`"email"` or `"token"`)
///
/// # Returns
///
/// * `Ok(())` - Credentials successfully saved to `.env` file
/// * `Err(QobuzApiError)` - File I/O error during read/write operations
///
/// # File Format
///
/// The function maintains a clean `.env` file format with the following variables:
/// ```env
/// # For email/password authentication:
/// QOBUZ_EMAIL=user@example.com
/// QOBUZ_PASSWORD=md5_hashed_password
/// QOBUZ_AUTH_METHOD=email
///
/// # OR for token-based authentication:
/// QOBUZ_USER_ID=12345678
/// QOBUZ_USER_AUTH_TOKEN=your_auth_token
/// QOBUZ_AUTH_METHOD=token
/// ```
///
/// # Security Notes
///
/// - Passwords are stored as MD5 hashes (as required by Qobuz API)
/// - The `.env` file should have appropriate file permissions (600) in production
/// - Sensitive credentials are stored in plain text in the `.env` file, which is
///   acceptable for personal applications but should be handled carefully
///
/// # Error Handling
///
/// File system errors (permission denied, disk full, etc.) are wrapped in
/// [`QobuzApiInitializationError`] with descriptive messages.
pub fn save_user_credentials_to_env(
    email_or_username: Option<&str>,
    user_id: Option<&str>,
    password_or_token: &str,
    auth_method: &str,
) -> Result<(), QobuzApiError> {
    // Read existing content or start with empty string
    let env_content = if Path::new(".env").exists() {
        read_to_string(".env").map_err(|e| QobuzApiInitializationError {
            message: format!("Failed to read .env file: {}", e),
        })?
    } else {
        String::new()
    };

    // Parse existing content to avoid duplicating entries
    let mut lines: Vec<String> = env_content.lines().map(|s| s.to_string()).collect();

    // Track which credentials we're setting
    let mut email_found = false;
    let mut username_found = false;
    let mut user_id_found = false;
    let mut password_found = false;
    let mut auth_token_found = false;
    let mut auth_method_found = false;

    // Update existing entries or mark them as found
    for line in &mut lines {
        if line.starts_with("QOBUZ_EMAIL=") {
            if let Some(email) = email_or_username {
                *line = format!("QOBUZ_EMAIL={}", email);
                email_found = true;
            }
        } else if line.starts_with("QOBUZ_USERNAME=") {
            if let Some(username) = email_or_username {
                // We can't determine if it's an email or username, so we'll assume email for now
                // In practice, the user would specify which one they're using
                *line = format!("QOBUZ_USERNAME={}", username);
                username_found = true;
            }
        } else if line.starts_with("QOBUZ_USER_ID=") {
            if let Some(id) = user_id {
                *line = format!("QOBUZ_USER_ID={}", id);
                user_id_found = true;
            }
        } else if line.starts_with("QOBUZ_PASSWORD=") {
            *line = format!("QOBUZ_PASSWORD={}", password_or_token);
            password_found = true;
        } else if line.starts_with("QOBUZ_USER_AUTH_TOKEN=") {
            *line = format!("QOBUZ_USER_AUTH_TOKEN={}", password_or_token);
            auth_token_found = true;
        } else if line.starts_with("QOBUZ_AUTH_METHOD=") {
            *line = format!("QOBUZ_AUTH_METHOD={}", auth_method);
            auth_method_found = true;
        }
    }

    // Add missing entries based on authentication type
    if let Some(email_or_username) = email_or_username {
        // Email/Username + Password authentication
        // Try to detect if it's an email or username
        if email_or_username.contains('@') {
            // It's likely an email
            if !email_found {
                lines.push(format!("QOBUZ_EMAIL={}", email_or_username));
            }
        } else {
            // It's likely a username
            if !username_found {
                lines.push(format!("QOBUZ_USERNAME={}", email_or_username));
            }
        }

        if !password_found {
            lines.push(format!("QOBUZ_PASSWORD={}", password_or_token));
        }

        if !auth_method_found {
            lines.push("QOBUZ_AUTH_METHOD=email".to_string());
        }

        // Remove token-based credentials if they exist
        lines.retain(|line| {
            !line.starts_with("QOBUZ_USER_ID=") && !line.starts_with("QOBUZ_USER_AUTH_TOKEN=")
        });
    } else if let Some(user_id) = user_id {
        // User ID + Auth Token authentication
        if !user_id_found {
            lines.push(format!("QOBUZ_USER_ID={}", user_id));
        }

        if !auth_token_found {
            lines.push(format!("QOBUZ_USER_AUTH_TOKEN={}", password_or_token));
        }

        if !auth_method_found {
            lines.push("QOBUZ_AUTH_METHOD=token".to_string());
        }

        // Remove email/username and password credentials if they exist
        lines.retain(|line| {
            !line.starts_with("QOBUZ_EMAIL=")
                && !line.starts_with("QOBUZ_USERNAME=")
                && !line.starts_with("QOBUZ_PASSWORD=")
        });
    }

    // Write back to .env file
    write(".env", lines.join("\n")).map_err(|e| QobuzApiInitializationError {
        message: format!("Failed to write to .env file: {}", e),
    })?;

    Ok(())
}

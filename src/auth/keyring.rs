//! GNOME Keyring credential storage via oo7.

use {
    oo7::Keyring,
    serde::{Deserialize, Serialize},
    serde_json::{from_slice, to_vec},
    tokio::runtime::Runtime,
};

use crate::errors::AppError::{self, Download};

/// Keyring attribute key-value pairs for application identification.
const KEYRING_ATTRIBUTES: [(&str, &str); 1] = [("application", "qobuz-downloader-rs")];

/// Label for the keyring item storing Qobuz credentials.
const KEYRING_LABEL: &str = "Qobuz Downloader Credentials";

/// Stored credentials retrieved from the keyring.
#[derive(Deserialize, Serialize)]
pub enum StoredCredentials {
    /// Email and password authentication.
    EmailPassword {
        /// Qobuz account email.
        email: String,
        /// Qobuz account password.
        password: String,
    },
    /// User ID and auth token authentication.
    Token {
        /// Qobuz user ID.
        user_id: String,
        /// User authentication token.
        auth_token: String,
    },
}

/// Creates a new Tokio runtime for synchronous keyring operations.
///
/// # Errors
///
/// Returns `AppError::Download` if the runtime cannot be created.
fn create_runtime() -> Result<Runtime, AppError> {
    Runtime::new().map_err(|e| Download(format!("Failed to create async runtime: {e}")))
}

/// Stores credentials in GNOME Keyring, replacing any existing entry.
///
/// # Errors
///
/// Returns `AppError::Keyring` if keyring access fails, or `AppError::SettingsParse` if
/// serialization fails.
pub fn store(creds: &StoredCredentials) -> Result<(), AppError> {
    let rt = create_runtime()?;
    rt.block_on(async {
        let keyring = Keyring::new().await?;
        let secret = to_vec(creds)?;
        keyring
            .create_item(KEYRING_LABEL, &KEYRING_ATTRIBUTES, secret, true)
            .await?;
        Ok(())
    })
}

/// Loads stored credentials from GNOME Keyring.
///
/// # Returns
///
/// `Ok(Some(creds))` if credentials were found, `Ok(None)` if no credentials are stored.
///
/// # Errors
///
/// Returns `AppError::Keyring` if keyring access fails.
pub fn load() -> Result<Option<StoredCredentials>, AppError> {
    let rt = create_runtime()?;
    rt.block_on(async {
        let keyring = Keyring::new().await?;
        let items = keyring.search_items(&KEYRING_ATTRIBUTES).await?;
        let Some(item) = items.into_iter().next() else {
            return Ok(None);
        };
        let secret = item.secret().await?;
        let creds: StoredCredentials = from_slice(&secret)?;
        Ok(Some(creds))
    })
}

/// Deletes stored credentials from GNOME Keyring.
///
/// # Errors
///
/// Returns `AppError::Keyring` if keyring access fails.
pub fn delete() -> Result<(), AppError> {
    let rt = create_runtime()?;
    rt.block_on(async {
        let keyring = Keyring::new().await?;
        keyring.delete(&KEYRING_ATTRIBUTES).await?;
        Ok(())
    })
}

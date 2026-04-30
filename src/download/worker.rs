//! Download worker thread for background download processing.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread::sleep,
    time::Duration,
};

use {
    parking_lot::Mutex,
    qobuz_api_rust_refactor::{api::service::QobuzApiService, errors::QobuzApiError},
    tracing::{error, info, warn},
};

use crate::{
    auth::session::perform_reauth,
    download::progress::Quality,
    errors::AppError::{self, Api},
};

/// Maximum number of download retry attempts.
const MAX_RETRIES: u32 = 3;

/// Base delay in milliseconds for exponential backoff between retries.
const RETRY_BASE_DELAY_MS: u64 = 2000;

/// Resolution after attempting to handle a download error.
enum ErrorResolution {
    /// Continue the retry loop (retries incremented).
    Continue,
    /// Return a fatal error immediately.
    Fatal(AppError),
}

/// Handles a download error, attempting re-auth or retry as appropriate.
///
/// Takes ownership of the error. Returns the resolution and updated retries count.
fn resolve_error(
    err: QobuzApiError,
    api_service: &Arc<Mutex<QobuzApiService>>,
    retries: u32,
) -> (ErrorResolution, u32) {
    if is_authentication_error(&err) {
        return resolve_auth_error(api_service, err, retries);
    }
    if is_retryable_error(&err) && retries < MAX_RETRIES {
        let new_retries = retries + 1;
        let delay = RETRY_BASE_DELAY_MS * 2u64.pow(new_retries - 1);
        warn!(
            retries = new_retries,
            delay_ms = delay,
            error = %err,
            "Retrying download"
        );
        sleep(Duration::from_millis(delay));
        return (ErrorResolution::Continue, new_retries);
    }
    (ErrorResolution::Fatal(Api(err)), retries)
}

/// Handles an authentication error by attempting re-authentication.
fn resolve_auth_error(
    api_service: &Arc<Mutex<QobuzApiService>>,
    err: QobuzApiError,
    retries: u32,
) -> (ErrorResolution, u32) {
    info!("Authentication error detected, attempting re-auth");
    match perform_reauth(api_service) {
        Ok(_) => {
            let new_retries = retries + 1;
            if new_retries > MAX_RETRIES {
                return (ErrorResolution::Fatal(Api(err)), new_retries);
            }
            (ErrorResolution::Continue, new_retries)
        }
        Err(reauth_err) => {
            error!(error = %reauth_err, "Re-authentication failed");
            (ErrorResolution::Fatal(reauth_err), retries)
        }
    }
}

/// Processes a download failure: attempts re-auth or retry, returns fatal error if unrecoverable.
///
/// # Errors
///
/// Returns `AppError` if the download cannot be retried or recovered.
fn handle_download_failure(
    err: QobuzApiError,
    api_service: &Arc<Mutex<QobuzApiService>>,
    retries: &mut u32,
) -> Result<(), AppError> {
    let (resolution, new_retries) = resolve_error(err, api_service, *retries);
    *retries = new_retries;
    match resolution {
        ErrorResolution::Continue => Ok(()),
        ErrorResolution::Fatal(e) => Err(e),
    }
}

/// Downloads a track with retry logic and re-authentication support.
///
/// # Errors
///
/// Returns `AppError::Api` if all retry attempts fail or if re-authentication fails.
pub fn download_track_with_retry(
    api_service: &Arc<Mutex<QobuzApiService>>,
    track_id: i32,
    format_id: i32,
    output_dir: &Path,
) -> Result<PathBuf, AppError> {
    let mut retries = 0;
    loop {
        let result = {
            let mut api = api_service.lock();
            api.download_track(track_id, format_id, output_dir, None)
        };

        match result {
            Ok(path) => return Ok(path),
            Err(err) => handle_download_failure(err, api_service, &mut retries)?,
        }
    }
}

/// Checks if an error is an authentication error requiring re-auth.
fn is_authentication_error(err: &QobuzApiError) -> bool {
    let msg = format!("{err}");
    msg.contains("Authentication") || msg.contains("auth")
}

/// Checks if an error is retryable (network timeout, 5xx, 429).
fn is_retryable_error(err: &QobuzApiError) -> bool {
    let msg = format!("{err}");
    msg.contains("timeout")
        || msg.contains('5')
        || msg.contains("429")
        || msg.contains("Rate limit")
}

/// Checks if a file already exists at the expected output path.
pub fn file_exists(output_dir: &Path, track_id: i32, quality: Quality) -> Option<PathBuf> {
    let ext = quality.extension();
    let filename = format!("{track_id}.{ext}");
    let path = output_dir.join(&filename);
    path.exists().then_some(path)
}

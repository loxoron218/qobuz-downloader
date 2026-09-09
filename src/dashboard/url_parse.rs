//! Qobuz URL parsing for dashboard downloads.

use {
    libadwaita::{Toast, ToastOverlay},
    regex::Regex,
    tracing::warn,
};

use crate::dashboard::url_parse::ParsedUrl::{Album, Playlist, Track};

/// Download type parsed from a Qobuz URL or ID.
#[derive(Clone, Debug)]
pub enum ParsedUrl {
    /// Album download.
    Album(String),
    /// Track download.
    Track(String),
    /// Playlist download.
    Playlist(String),
}

/// Parses a Qobuz URL or direct ID into a download type.
///
/// # Arguments
///
/// * `input` - The URL or ID string to parse
///
/// # Returns
///
/// `Some(ParsedUrl)` if the input is valid, `None` otherwise.
fn parse_qobuz_url(input: &str) -> Option<ParsedUrl> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Some(Album(trimmed.to_string()));
    }

    let playlist_re = match Regex::new(
        "https?:\x2F\x2F(?:www\\.|play\\.|open\\.)?qobuz\\.com/(?:[a-z]{2}-[a-z]{2}/)?playlists?/\
         [^/]+/(\\d+)",
    ) {
        Ok(re) => re,
        Err(e) => {
            warn!(error = %e, "Failed to compile playlist regex");
            return None;
        }
    };

    let album_play_re =
        match Regex::new("https?:\x2F\x2F(?:play\\.|open\\.)?qobuz\\.com/album/([a-zA-Z0-9-]+)") {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile album play regex");
                return None;
            }
        };
    let track_play_re =
        match Regex::new("https?:\x2F\x2F(?:play\\.|open\\.)?qobuz\\.com/track/([a-zA-Z0-9-]+)") {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile track play regex");
                return None;
            }
        };

    let old_album_re = match Regex::new(
        "https?:\x2F\x2F(?:www\\.)?qobuz\\.com/(?:[a-z]{2}-[a-z]{2}/)?album/[^/]+/(\\d+)",
    ) {
        Ok(re) => re,
        Err(e) => {
            warn!(error = %e, "Failed to compile old album regex");
            return None;
        }
    };
    let old_track_re = match Regex::new(
        "https?:\x2F\x2F(?:www\\.)?qobuz\\.com/(?:[a-z]{2}-[a-z]{2}/)?track/[^/]+/(\\d+)",
    ) {
        Ok(re) => re,
        Err(e) => {
            warn!(error = %e, "Failed to compile old track regex");
            return None;
        }
    };

    if let Some(caps) = album_play_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(Album(id.as_str().to_string()));
    }
    if let Some(caps) = track_play_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(Track(id.as_str().to_string()));
    }
    if let Some(caps) = playlist_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(Playlist(id.as_str().to_string()));
    }
    if let Some(caps) = old_album_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(Album(id.as_str().to_string()));
    }
    if let Some(caps) = old_track_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(Track(id.as_str().to_string()));
    }

    None
}

/// Validates and parses the input text, showing an error toast if invalid.
///
/// # Returns
///
/// `Some(ParsedUrl)` if the input is valid, `None` if validation or parsing fails.
#[must_use]
pub fn try_parse_download_url(text: &str, toast_overlay: &ToastOverlay) -> Option<ParsedUrl> {
    if text.trim().is_empty() {
        let toast = Toast::new("Please enter a Qobuz URL or ID");
        toast.set_timeout(3);
        toast_overlay.add_toast(toast);
        return None;
    }
    let parsed = parse_qobuz_url(text);
    if parsed.is_none() {
        let toast = Toast::new("Invalid Qobuz URL or ID format");
        toast.set_timeout(3);
        toast_overlay.add_toast(toast);
    }
    parsed
}

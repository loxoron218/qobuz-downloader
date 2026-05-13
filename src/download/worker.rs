//! Download worker thread for background download processing.

use std::path::{Path, PathBuf};

use qobuz_api_rust_refactor::sanitize::sanitize_filename;

use crate::download::progress::Quality;

/// Computes the album output directory using "Artist/Album Title" folder naming.
///
/// # Arguments
///
/// * `base_dir` - Base download directory from settings
/// * `artist` - Artist name
/// * `album_title` - Album title
/// * `_quality` - Audio quality (used for extension context)
///
/// # Returns
///
/// The album output directory path: `{base_dir}/Artist/Album Title/`
pub fn album_output_dir(
    base_dir: &Path,
    artist: &str,
    album_title: &str,
    _quality: Quality,
) -> PathBuf {
    let safe_artist = sanitize_filename(artist);
    let safe_album = sanitize_filename(album_title);
    base_dir.join(&safe_artist).join(&safe_album)
}

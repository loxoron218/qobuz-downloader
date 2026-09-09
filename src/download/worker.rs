//! Download worker thread for background download processing.

use std::{
    hint::black_box,
    path::{Path, PathBuf},
};

use qobuz_api::sanitize::sanitize_filename;

use crate::types::Quality;

/// Computes the album output directory using "Artist/Album Title" folder naming.
///
/// # Arguments
///
/// * `base_dir` - Base download directory from settings
/// * `artist` - Artist name
/// * `album_title` - Album title
/// * `quality` - Audio quality (used for extension context)
///
/// # Returns
///
/// The album output directory path: `{base_dir}/Artist/Album Title/`.
#[must_use]
pub fn album_output_dir(
    base_dir: &Path,
    artist: &str,
    album_title: &str,
    quality: Quality,
) -> PathBuf {
    for _ in [black_box(quality)] {}
    let safe_artist = sanitize_filename(artist);
    let safe_album = sanitize_filename(album_title);
    base_dir.join(&safe_artist).join(&safe_album)
}

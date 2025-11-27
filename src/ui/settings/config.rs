use std::{
    fs::{read_to_string, write},
    io::Error,
    path::Path,
};

use qobuz_api_rust::metadata::MetadataConfig;

/// Saves a single environment variable to the `.env` configuration file.
///
/// This function handles both updating existing environment variables and adding
/// new ones to the `.env` file. If the file doesn't exist, it will be created.
/// The function preserves all existing content in the file while only modifying
/// or adding the specified key-value pair.
///
/// # Arguments
///
/// * `key` - The environment variable name (e.g., "QOBUZ_DOWNLOAD_PATH")
/// * `value` - The value to assign to the environment variable
///
/// # Returns
///
/// Returns `Ok(())` on successful write operation, or an `io::Error` if:
/// - The file cannot be read (permissions, non-existent parent directories)
/// - The file cannot be written (permissions, disk full)
/// - Any other I/O error occurs during the operation
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::save_env_var;
///
/// // Save download path setting
/// if let Err(e) = save_env_var("QOBUZ_DOWNLOAD_PATH", "/home/user/music") {
///     eprintln!("Failed to save setting: {}", e);
/// }
/// ```
pub fn save_env_var(key: &str, value: &str) -> Result<(), Error> {
    let env_content = if Path::new(".env").exists() {
        read_to_string(".env")?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = env_content.lines().map(|s| s.to_string()).collect();
    let mut key_found = false;

    // Update existing entry
    for line in &mut lines {
        if line.starts_with(&format!("{}=", key)) {
            *line = format!("{}={}", key, value);
            key_found = true;
        }
    }

    // Add entry if not found
    if !key_found {
        lines.push(format!("{}={}", key, value));
    }

    write(".env", lines.join("\n"))?;
    Ok(())
}

/// Loads the configured download path from the `.env` file.
///
/// Retrieves the `QOBUZ_DOWNLOAD_PATH` environment variable from the `.env`
/// configuration file. If the variable is not set or the file doesn't exist,
/// returns the default download directory "downloads".
///
/// # Returns
///
/// Returns `Ok(String)` containing the download path, or an `io::Error` if
/// the `.env` file exists but cannot be read due to I/O issues.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::load_download_path;
///
/// match load_download_path() {
///     Ok(path) => println!("Download path: {}", path),
///     Err(e) => eprintln!("Error loading download path: {}", e),
/// }
/// ```
pub fn load_download_path() -> Result<String, Error> {
    load_env_var("QOBUZ_DOWNLOAD_PATH", "downloads")
}

/// Saves the download path to the `.env` configuration file.
///
/// Persists the specified download path by setting the `QOBUZ_DOWNLOAD_PATH`
/// environment variable in the `.env` file. This setting will be loaded
/// automatically on subsequent application launches.
///
/// # Arguments
///
/// * `download_path` - The absolute or relative path where downloads should be saved
///
/// # Returns
///
/// Returns `Ok(())` on successful save, or an `io::Error` if the file cannot
/// be written.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::save_download_path;
///
/// if let Err(e) = save_download_path("/home/user/music/qobuz") {
///     eprintln!("Failed to save download path: {}", e);
/// }
/// ```
pub fn save_download_path(download_path: &str) -> Result<(), Error> {
    save_env_var("QOBUZ_DOWNLOAD_PATH", download_path)
}

/// Loads the preferred audio format from the `.env` file.
///
/// Retrieves the `QOBUZ_PREFERRED_FORMAT` environment variable which determines
/// the audio quality/format preference for downloads. If not configured, defaults
/// to format ID "6" (typically representing high-quality lossless audio).
///
/// # Returns
///
/// Returns `Ok(String)` containing the format ID, or an `io::Error` if the
/// `.env` file exists but cannot be read.
///
/// # Format IDs
///
/// Common Qobuz format IDs include:
/// - "5": MP3 320kbps
/// - "6": FLAC Lossless (16-bit/44.1kHz)
/// - "7": FLAC Hi-Res (24-bit, varies by track availability)
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::load_preferred_format;
///
/// match load_preferred_format() {
///     Ok(format_id) => println!("Preferred format: {}", format_id),
///     Err(e) => eprintln!("Error loading format preference: {}", e),
/// }
/// ```
pub fn load_preferred_format() -> Result<String, Error> {
    load_env_var("QOBUZ_PREFERRED_FORMAT", "6")
}

/// Loads the preferred search scope from the `.env` file.
///
/// Retrieves the `QOBUZ_SEARCH_SCOPE` environment variable which determines
/// the default search scope (All, Albums, or Tracks) for the search page.
/// If not configured, defaults to "0" (All).
///
/// # Returns
///
/// Returns `Ok(String)` containing the search scope index as a string, or an `io::Error` if the
/// `.env` file exists but cannot be read.
///
/// # Search Scope Values
///
/// - "0": All (search both albums and tracks)
/// - "1": Albums only
/// - "2": Tracks only
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::load_search_scope;
///
/// match load_search_scope() {
///     Ok(scope) => println!("Search scope: {}", scope),
///     Err(e) => eprintln!("Error loading search scope: {}", e),
/// }
/// ```
pub fn load_search_scope() -> Result<String, Error> {
    load_env_var("QOBUZ_SEARCH_SCOPE", "0")
}

/// Saves the preferred search scope to the `.env` configuration file.
///
/// Persists the search scope preference by setting the `QOBUZ_SEARCH_SCOPE`
/// environment variable. This determines the default search scope when the
/// application starts.
///
/// # Arguments
///
/// * `scope_index` - The search scope index as a string ("0", "1", or "2")
///
/// # Returns
///
/// Returns `Ok(())` on successful save, or an `io::Error` if the file cannot
/// be written.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::save_search_scope;
///
/// // Set preference to Albums only
/// if let Err(e) = save_search_scope("1") {
///     eprintln!("Failed to save search scope: {}", e);
/// }
/// ```
pub fn save_search_scope(scope_index: &str) -> Result<(), Error> {
    save_env_var("QOBUZ_SEARCH_SCOPE", scope_index)
}

/// Saves the preferred audio format to the `.env` configuration file.
///
/// Persists the audio format preference by setting the `QOBUZ_PREFERRED_FORMAT`
/// environment variable. This determines the quality of audio files downloaded
/// from Qobuz.
///
/// # Arguments
///
/// * `format_id` - The Qobuz format identifier (e.g., "5", "6", or "7")
///
/// # Returns
///
/// Returns `Ok(())` on successful save, or an `io::Error` if the file cannot
/// be written.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::save_preferred_format;
///
/// // Set preference to FLAC Lossless
/// if let Err(e) = save_preferred_format("6") {
///     eprintln!("Failed to save format preference: {}", e);
/// }
/// ```
pub fn save_preferred_format(format_id: &str) -> Result<(), Error> {
    save_env_var("QOBUZ_PREFERRED_FORMAT", format_id)
}

/// Loads an environment variable from the `.env` file with a fallback default.
///
/// Reads the `.env` configuration file and searches for the specified environment
/// variable key. The function properly handles:
/// - Non-existent `.env` files (returns default)
/// - Empty lines and comments (lines starting with `#`)
/// - Whitespace trimming around keys and values
/// - Empty values (treated as not found, returns default)
///
/// # Arguments
///
/// * `key` - The environment variable name to search for
/// * `default` - The fallback value to return if the key is not found or invalid
///
/// # Returns
///
/// Returns `Ok(String)` containing either the found value or the default value.
/// Returns an `io::Error` only if the `.env` file exists but cannot be read.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::load_env_var;
///
/// // Load with default
/// let path = load_env_var("QOBUZ_DOWNLOAD_PATH", "downloads").unwrap();
/// println!("Using download path: {}", path);
/// ```
fn load_env_var(key: &str, default: &str) -> Result<String, Error> {
    if !Path::new(".env").exists() {
        return Ok(default.to_string());
    }

    let env_content = read_to_string(".env")?;
    for line in env_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((k, v)) = line.split_once('=')
            && k == key
            && !v.is_empty()
        {
            return Ok(v.to_string());
        }
    }

    Ok(default.to_string())
}

/// Loads the complete metadata configuration from the `.env` file.
///
/// Constructs a [`MetadataConfig`] struct by reading individual boolean settings
/// from the `.env` configuration file. Each metadata field (track title, artist,
/// album, etc.) can be individually enabled or disabled through corresponding
/// environment variables.
///
/// All metadata fields default to `true` (enabled) except for the comment field,
/// which defaults to `false` (disabled) to avoid potential privacy concerns with
/// user-generated comments.
///
/// # Environment Variables
///
/// The following environment variables control metadata inclusion:
/// - `QOBUZ_TAG_ALBUM_ARTIST`: Album artist name
/// - `QOBUZ_TAG_ARTIST`: Track artist name
/// - `QOBUZ_TAG_TRACK_TITLE`: Track title
/// - `QOBUZ_TAG_TRACK_NUMBER`: Track number within album
/// - `QOBUZ_TAG_TRACK_TOTAL`: Total tracks in album
/// - `QOBUZ_TAG_DISC_NUMBER`: Disc number for multi-disc releases
/// - `QOBUZ_TAG_DISC_TOTAL`: Total discs in release
/// - `QOBUZ_TAG_ALBUM`: Album title
/// - `QOBUZ_TAG_EXPLICIT`: Explicit content advisory
/// - `QOBUZ_TAG_UPC`: Universal Product Code
/// - `QOBUZ_TAG_ISRC`: International Standard Recording Code
/// - `QOBUZ_TAG_COPYRIGHT`: Copyright information
/// - `QOBUZ_TAG_COMPOSER`: Composer credits
/// - `QOBUZ_TAG_GENRE`: Music genre
/// - `QOBUZ_TAG_RELEASE_YEAR`: Release year
/// - `QOBUZ_TAG_RELEASE_DATE`: Full release date
/// - `QOBUZ_TAG_COMMENT`: User/album comments (disabled by default)
/// - `QOBUZ_TAG_COVER_ART`: Album cover artwork
/// - `QOBUZ_TAG_LABEL`: Record label
/// - `QOBUZ_TAG_PRODUCER`: Producer credits
/// - `QOBUZ_TAG_INVOLVED_PEOPLE`: Additional personnel credits
/// - `QOBUZ_TAG_URL`: Qobuz track/album URL
/// - `QOBUZ_TAG_MEDIA_TYPE`: Media format type
///
/// # Returns
///
/// Returns a fully populated [`MetadataConfig`] struct with all boolean fields
/// set according to the `.env` configuration or their respective defaults.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::settings::config::load_metadata_config;
///
/// let config = load_metadata_config();
/// println!("Album artist tagging enabled: {}", config.album_artist);
/// println!("Comment tagging enabled: {}", config.comment);
/// ```
pub fn load_metadata_config() -> MetadataConfig {
    let load_bool = |key: &str, default: bool| -> bool {
        load_env_var(key, &default.to_string())
            .unwrap_or(default.to_string())
            .parse()
            .unwrap_or(default)
    };

    MetadataConfig {
        album_artist: load_bool("QOBUZ_TAG_ALBUM_ARTIST", true),
        artist: load_bool("QOBUZ_TAG_ARTIST", true),
        track_title: load_bool("QOBUZ_TAG_TRACK_TITLE", true),
        track_number: load_bool("QOBUZ_TAG_TRACK_NUMBER", true),
        track_total: load_bool("QOBUZ_TAG_TRACK_TOTAL", true),
        disc_number: load_bool("QOBUZ_TAG_DISC_NUMBER", true),
        disc_total: load_bool("QOBUZ_TAG_DISC_TOTAL", true),
        album: load_bool("QOBUZ_TAG_ALBUM", true),
        explicit: load_bool("QOBUZ_TAG_EXPLICIT", true),
        upc: load_bool("QOBUZ_TAG_UPC", true),
        isrc: load_bool("QOBUZ_TAG_ISRC", true),
        copyright: load_bool("QOBUZ_TAG_COPYRIGHT", true),
        composer: load_bool("QOBUZ_TAG_COMPOSER", true),
        genre: load_bool("QOBUZ_TAG_GENRE", true),
        release_year: load_bool("QOBUZ_TAG_RELEASE_YEAR", true),
        release_date: load_bool("QOBUZ_TAG_RELEASE_DATE", true),
        comment: load_bool("QOBUZ_TAG_COMMENT", false),
        cover_art: load_bool("QOBUZ_TAG_COVER_ART", true),
        label: load_bool("QOBUZ_TAG_LABEL", true),
        producer: load_bool("QOBUZ_TAG_PRODUCER", true),
        involved_people: load_bool("QOBUZ_TAG_INVOLVED_PEOPLE", true),
        url: load_bool("QOBUZ_TAG_URL", true),
        media_type: load_bool("QOBUZ_TAG_MEDIA_TYPE", true),
    }
}

# Data Model: Qobuz Download GUI

**Branch**: `003-qobuz-download-gui` | **Date**: 2026-04-26

## Overview

This document defines the application's internal data types. The application does not define a database schema; it wraps the `qobuz-api` library types and adds GUI-specific state.

## Application State

### `AppState`

Central application state shared across modules.

| Field | Type | Purpose |
|-------|------|---------|
| `api_service` | `Arc<parking_lot::Mutex<QobuzApiService>>` | Shared API client, accessed from background threads |
| `settings` | `Arc<parking_lot::Mutex<AppSettings>>` | User preferences |
| `auth_state` | `AuthState` | Current authentication status |
| `download_manager` | `DownloadManager` | Download queue and worker handle |

### `AuthState`

Tracks the current authentication status.

| Variant | Data | Purpose |
|---------|------|---------|
| `Unauthenticated` | - | No credentials stored or login required |
| `Authenticating` | - | Login in progress |
| `Authenticated` | `user_id: String` | Successfully authenticated |
| `Expired` | - | Token expired, attempting re-auth |

State transitions:
```
Unauthenticated → Authenticating → Authenticated
                                       ↓ (token expires)
                                    Expired → Authenticating → Authenticated
                                                          ↓ (re-auth fails)
                                                    Unauthenticated
```

## Download Domain

### `DownloadTaskId`

Unique identifier for a download task.

- Type: `uuid::Uuid` (newtype wrapper not needed - use directly)

### `DownloadTask`

Represents a single download operation.

| Field | Type | Purpose |
|-------|------|---------|
| `id` | `Uuid` | Unique task identifier |
| `item` | `DownloadItem` | What to download |
| `quality` | `Quality` | Audio quality level |
| `output_dir` | `PathBuf` | Destination directory |
| `status` | `DownloadStatus` | Current state |
| `progress` | `DownloadProgress` | Byte-level progress |
| `created_at` | `chrono::DateTime<Local>` | Task creation time |
| `completed_at` | `Option<chrono::DateTime<Local>>` | Completion time |

### `DownloadItem`

What is being downloaded (discriminated union).

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Track` | `track: Track` | Single track download |
| `Album` | `album: Album` | Full album download |
| `Playlist` | `playlist: Playlist` | Playlist download |

### `DownloadStatus`

State machine for download lifecycle.

| Variant | Purpose |
|---------|---------|
| `Queued` | Waiting for a download slot |
| `Active` | Currently downloading |
| `Completed` | Successfully finished |
| `Failed` | Errored out |
| `Cancelled` | User cancelled |
| `Skipped` | File already exists |

State transitions:
```
Queued → Active → Completed
              ↓         ↓
           Failed    Failed
              ↓
           Queued (retry)
       ↓
    Cancelled (from any non-terminal state)
```

Validation: Terminal states (`Completed`, `Failed`, `Cancelled`, `Skipped`) cannot transition further.

### `DownloadProgress`

Byte-level download progress.

| Field | Type | Purpose |
|-------|------|---------|
| `bytes_downloaded` | `u64` | Bytes received so far |
| `total_bytes` | `Option<u64>` | Total file size (may be unknown) |

Derived: `percentage: Option<f64>` = `bytes_downloaded / total_bytes * 100.0`

### `Quality`

Audio quality selection (wraps API library constants).

| Variant | API Value | Display Label | File Extension |
|---------|-----------|---------------|----------------|
| `Mp3_320` | `5` | "MP3 320kbps" | `.mp3` |
| `Flac16_44` | `6` | "FLAC 16-bit / 44.1kHz" | `.flac` |
| `Flac24_96` | `7` | "FLAC 24-bit / 96kHz" | `.flac` |
| `Flac24_192` | `27` | "FLAC 24-bit / 192kHz" | `.flac` |

Implements: `Display`, `From<Quality> for i32`, `TryFrom<i32> for Quality`

**Location note**: Defined in `src/download/progress.rs` alongside download types, but consumed by `search`, `browse`, and `preferences` modules. A shared `src/types.rs` location was considered; the current placement was chosen because `Quality` is conceptually a download property and co-location with `DownloadProgress` keeps related types together. Downstream modules import via `crate::download::progress::Quality`.

## Settings Domain

### `AppSettings`

Persistent user preferences.

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `download_directory` | `PathBuf` | `$XDG_MUSIC_DIR` or `~/Music` | Download output location |
| `default_quality` | `Quality` | `Flac16_44` | Default audio quality |
| `window_width` | `i32` | `800` | Saved window width |
| `window_height` | `i32` | `600` | Saved window height |

Persistence: `$XDG_CONFIG_HOME/qobuz-downloader-rs/settings.json` via `serde_json`

### `StoredCredentials`

Keyring-stored credentials (not serialized to disk, only in keyring).

| Field | Type | Purpose |
|-------|------|---------|
| `email` | `String` | Qobuz account email |
| `password` | `String` | Qobuz account password |

Storage: GNOME Keyring via `oo7::Keyring`, attributes: `[("application", "qobuz-downloader-rs")]`

## Error Types

### `AppError`

Application-level error type (thiserror).

| Variant | Source | Purpose |
|---------|--------|---------|
| `Api` | `QobuzApiError` | API library error |
| `Keyring` | `oo7::Error` | Keyring access error |
| `Settings` | `std::io::Error` | Settings file I/O error |
| `SettingsParse` | `serde_json::Error` | Settings JSON parse error |
| `Download` | `DownloadError` | Download-specific errors (see below) |
| `NotAuthenticated` | - | Operation requires authentication |

### `DownloadError`

Structured download error type.

| Variant | Purpose |
|---------|---------|
| `NetworkTimeout` | Connection timed out |
| `ServerError(u16)` | Server returned 5xx status |
| `RateLimited` | API rate limit (429) hit after retries exhausted |
| `DiskFull` | Insufficient disk space |
| `NotAvailable(&str)` | Track DRM-restricted or geo-blocked with reason |
| `SubscriptionQualityMismatch` | Quality not available on subscription tier |
| `Io(String)` | Other I/O errors |

### `DownloadCommand`

Commands sent to the download worker.

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Enqueue` | `task: DownloadTask` | Add download to queue |
| `Cancel` | `id: Uuid` | Cancel active or queued download |
| `Shutdown` | - | Stop the download worker |

### `DownloadEvent`

Events sent from download worker to GUI.

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Started` | `id: Uuid` | Download began |
| `Progress` | `id: Uuid`, `bytes: u64`, `total: Option<u64>` | Progress update |
| `Completed` | `id: Uuid`, `path: PathBuf` | Download finished |
| `Failed` | `id: Uuid`, `error: String` | Download failed |
| `Skipped` | `id: Uuid`, `reason: String` | Skipped (file exists) |
| `ReauthRequired` | - | Authentication expired, re-auth needed |

## Module-Local Event Types

These types are defined per-module and not shared across the full data model:

| Type | Module | Definition Location |
|------|--------|-------------------|
| `SearchEvent` | Search | `src/search/controller.rs` — T015 |
| `BrowseEvent` | Browse | `src/browse/mod.rs` — T030 |

## External Types (from qobuz-api)

These types are used directly from the API library without modification:

| Type | Module | Usage in GUI |
|------|--------|-------------|
| `QobuzApiService` | `api::service` | Central API client |
| `SearchResult` | `models::search` | Search results display |
| `Album` | `models::album` | Album detail view |
| `Track` | `models::track` | Track display, download |
| `Artist` | `models::artist` | Artist detail view |
| `Playlist` | `models::playlist` | Playlist detail view |
| `Image` | `models::album` | Cover art URLs |
| `MetadataConfig` | `metadata::config` | Metadata embedding settings |
| `QobuzApiError` | `errors` | Error propagation |

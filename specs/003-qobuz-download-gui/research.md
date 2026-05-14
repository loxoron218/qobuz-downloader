# Research: Qobuz Download GUI

**Branch**: `003-qobuz-download-gui` | **Date**: 2026-04-26

## R1: Blocking API Library + GUI Thread Integration

**Decision**: Use `gio::spawn_blocking` to offload all `qobuz-api` calls to a thread pool, with `glib::MainContext::channel` for returning results to the GUI thread.

**Rationale**: The `qobuz-api` library exposes blocking methods (internally uses `Runtime::block_on` via `delegate!` macros). GTK requires the main thread to remain free for event processing. `gio::spawn_blocking` is the idiomatic GTK-Rs pattern documented in the gtk4-rs book. Results are sent back via `glib::MainContext::channel<PRIORITY, Msg>` which delivers messages on the main loop, enabling safe UI updates.

**Alternatives considered**:
- **tokio::spawn_blocking**: Works but requires managing a separate tokio runtime alongside the GLib main loop. Adds complexity without benefit since GLib already provides thread pool management.
- **std::thread::spawn + glib::idle_add**: Lower-level approach. `gio::spawn_blocking` is the recommended higher-level abstraction with proper thread pool reuse.
- **Relm4 framework**: Higher-level Elm-style abstraction over gtk4-rs. Rejected because AGENTS.md mandates programmatic widgets directly, and adding a framework layer increases dependency surface without clear benefit for this application's complexity.

## R2: Credential Storage via GNOME Keyring

**Decision**: Use `oo7` crate (pure Rust, async, Secret Service API compatible).

**Rationale**: The spec requires GNOME Keyring via libsecret (Secret Service API). `oo7` is a modern pure-Rust implementation that supports both the DBus-based Secret Service (GNOME Keyring) and a file-based backend for sandboxed environments. It is async-native and works with tokio. The API is straightforward: `Keyring::new()`, `create_item()`, `search_items()`, `delete()`.

**Alternatives considered**:
- **libsecret FFI bindings**: C FFI bindings are available but require system libraries and are less idiomatic. `oo7` provides equivalent functionality without C dependencies.
- **keyring crate**: Cross-platform but wraps C libraries on Linux. `oo7` is pure Rust and more actively maintained for Linux-specific use.

**Integration pattern**:
1. On first launch, prompt for credentials via login form
2. After successful auth, store email + password in keyring via `oo7`
3. On subsequent launches, retrieve credentials from keyring and auto-authenticate
4. On token expiry, re-authenticate silently using stored credentials
5. On logout, delete credentials from keyring

## R3: Download Manager Architecture

**Decision**: Dedicated download worker thread with `async_channel` command channel and semaphore-based concurrency limit of 3.

**Rationale**: Downloads are long-running I/O operations that must not block the GUI. A single download worker thread processes a command queue, allowing centralized management of the 3-slot concurrency limit, cancellation, and progress reporting. Using `async_channel::bounded` provides backpressure when the queue is full.

**Architecture**:
```
GUI Thread                    Download Worker Thread
─────────                    ──────────────────────
DownloadView ──[cmd_tx]──→ DownloadManager
              ←[progress_rx]──  │
                                ├── Semaphore(3 permits)
                                ├── spawns gio::spawn_blocking per download
                                └── reports progress via channel
```

**Commands** (sent via `async_channel`):
- `Enqueue { task: DownloadTask }` - Add to queue
- `Cancel { task_id: Uuid }` - Cancel specific download
- `Shutdown` - Graceful shutdown

**Progress messages** (sent via `glib::MainContext::channel`):
- `Started { task_id }` - Download began
- `Progress { task_id, bytes_downloaded, total_bytes }` - Byte-level progress
- `Completed { task_id, path }` - Download finished
- `Failed { task_id, error }` - Download failed
- `Skipped { task_id, reason }` - File already exists

**Alternatives considered**:
- **tokio::spawn per download**: No centralized control, harder to enforce concurrency limit and manage queue ordering.
- **rayon thread pool**: Designed for CPU-bound parallelism, not I/O-bound async work.

## R4: GUI Navigation Architecture

**Decision**: `NavigationView` with `ToolbarView` per page for the main content area, following GNOME HIG.

**Rationale**: The application has a clear navigation hierarchy: Search → Item Detail → Download. `AdwNavigationView` provides push/pop navigation with transitions. Each page uses `ToolbarView` with `HeaderBar` as mandated by AGENTS.md. For the download view, use a separate page accessible via a bottom bar or view switcher.

**Navigation flow**:
```
[Login View] → (on auth) → [Main View]
                            ├── Search Tab (NavigationView)
                            │   ├── Search Results
                            │   └── Album/Track/Playlist Detail (pushed)
                            └── Downloads Tab
                                ├── Active Downloads
                                └── History
```

**Alternatives considered**:
- **Leaflet for responsive**: Useful for mobile/narrow layouts but adds complexity. The app is desktop-focused; can add later if needed.
- **ViewStack with ViewSwitcher**: Simple tab-based navigation. Works for the top-level but doesn't handle the search→detail drill-down naturally.

## R5: Cover Art Handling

**Decision**: In-memory `HashMap<String, gdk::Texture>` cache with async loading via `gio::spawn_blocking`.

**Rationale**: Cover art thumbnails appear in search results and detail views. The `qobuz-api` library provides cover art URLs via `Image` structs. Downloading cover art is network I/O that must be offloaded. A simple in-memory cache avoids redundant downloads. `gdk::Texture::from_bytes()` creates GPU-ready textures from downloaded bytes.

**Cache strategy**:
- LRU or unbounded (cover art is small, typically <100KB per image)
- Key: URL string
- Load on demand when rendering list items or detail views
- Use `gtk::Picture` widget for display

## R6: Settings Persistence

**Decision**: XDG config directory with JSON file via `serde_json`.

**Rationale**: AGENTS.md specifies XDG paths and `serde` + `serde_json`. A simple JSON file in `$XDG_CONFIG_HOME/qobuz-downloader-rs/settings.json` stores preferences. The settings struct is small (download dir, quality, window size) and doesn't warrant a database.

**Settings schema**:
```rust
struct AppSettings {
    download_directory: PathBuf,
    default_quality: Quality,
    window_width: i32,
    window_height: i32,
}
```

**Alternatives considered**:
- **GSettings (GIO)**: Native GNOME settings system. Adds complexity (schema compilation) for minimal benefit in a standalone app.
- **TOML**: Common in Rust but JSON is already a dependency via serde_json.

## R7: Quality Format Mapping

**Decision**: Use `qobuz_api::models::file_url::quality` constants directly, wrapped in a local enum for UI display.

**Rationale**: The API library already defines quality constants (`MP3_320 = 5`, `FLAC_16_44 = 6`, `FLAC_24_96 = 7`, `FLAC_24_192 = 27`). The GUI wraps these in a local enum with `Display` impl for user-facing labels and conversion to `i32` for API calls.

```rust
enum Quality {
    Mp3_320,     // 5
    Flac16_44,   // 6
    Flac24_96,   // 7
    Flac24_192,  // 27
}
```

## R8: Thread Safety for QobuzApiService

**Decision**: Wrap `QobuzApiService` in `Mutex` and access only from `spawn_blocking` closures. Use `Arc<Mutex<QobuzApiService>>` shared between the download worker and the GUI.

**Rationale**: `QobuzApiService` is not `Sync` (it holds a `Box<dyn HttpClient>` and mutable state). All API methods take `&mut self` or `&self`. The download manager and search controller both need access. A `Mutex` ensures exclusive access. Since all API calls run in `spawn_blocking` (off the GUI thread), the lock is never contended from the GUI thread.

**Alternatives considered**:
- **Clone per operation**: Some fields can't be cloned (HTTP client is boxed). Would require restructuring the library.
- **Message-passing only**: Send API commands through a channel to a dedicated API thread. More complex than necessary for a single-user desktop app.

## R9: Auto Re-authentication on Token Expiry

**Decision**: When an API call returns an `AuthenticationError`, attempt re-authentication using stored keyring credentials before reporting failure.

**Rationale**: Spec requires silent re-auth on token expiry. The download worker and search controller catch `QobuzApiError::AuthenticationError`, retrieve credentials from keyring, call `service.login()`, and retry the original operation. If re-auth fails, the error propagates to the UI which shows the login view.

## R10: GTK-Rs Crate Versions

**Decision**: Use latest stable `gtk4` 0.9.x, `adw` 0.7.x, `glib` 0.20.x, `gio` 0.20.x.

**Rationale**: The gtk4-rs ecosystem follows GTK4 releases. These versions support GTK 4.16+ and libadwaita 1.7+ which provide `ToolbarView`, `PreferencesDialog`, `NavigationView`, and `ToastOverlay`. Compatible with GNOME 47+.

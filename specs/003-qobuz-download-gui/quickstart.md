# Quickstart: Qobuz Download GUI

**Branch**: `003-qobuz-download-gui` | **Date**: 2026-04-26

## Prerequisites

- Rust toolchain (2024 edition)
- GTK4 development libraries (`gtk4-devel` on Fedora, `libgtk-4-dev` on Debian/Ubuntu)
- libadwaita development libraries (`libadwaita-devel` on Fedora, `libadwaita-1-dev` on Debian/Ubuntu)
- GNOME Keyring (typically pre-installed on GNOME desktops)
- `qobuz-api` library at `/home/arch/Downloads/github/qobuz-api`

## Build

```bash
# From repository root
cargo build
```

## Run

```bash
cargo run
```

## First Launch Flow

1. Application launches and checks GNOME Keyring for stored credentials
2. If no credentials found, login form is displayed
3. User enters Qobuz email and password
4. On successful authentication, credentials are stored in GNOME Keyring
5. Main interface appears with search bar and download views

## Subsequent Launches

1. Application retrieves credentials from GNOME Keyring
2. Automatic authentication using stored email/password
3. If auth fails, user is prompted to re-enter credentials
4. Main interface appears immediately on success

## Configuration

Settings are stored at `$XDG_CONFIG_HOME/qobuz-downloader-rs/settings.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `download_directory` | `~/Music` | Where downloads are saved |
| `default_quality` | `FLAC 16-bit / 44.1kHz` | Default audio quality |
| `window_width` | `800` | Saved window width |
| `window_height` | `600` | Saved window height |

Preferences are accessible via the Preferences dialog (gear icon in header).

## Download Workflow

1. Type a query in the search bar
2. Results appear grouped by tracks, albums, artists, playlists
3. Click an item to view details
4. Select quality and click download
5. Monitor progress in the Downloads view
6. Files appear in the configured download directory with metadata embedded

## Concurrency

- Maximum 3 concurrent downloads
- Additional downloads are queued and start automatically
- Existing files in the download directory are skipped with a toast notification

## Architecture Diagram

```
┌─────────────────────────────────────────────────┐
│                    GUI Thread                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │  Search   │ │  Browse  │ │ Download View    │ │
│  │  View     │ │  View    │ │ (active+history) │ │
│  └────┬──────┘ └────┬─────┘ └────────┬─────────┘ │
│       │              │                │           │
│       │   glib::MainContext::channel  │           │
│       │              │                │           │
├───────┼──────────────┼────────────────┼───────────┤
│       ▼              ▼                ▼           │
│  gio::spawn_blocking              DownloadCommand │
│       │              │               Channel      │
│       ▼              ▼                │           │
│  ┌──────────────────────────┐   ┌────▼─────────┐ │
│  │    Blocking API Calls    │   │  Download    │ │
│  │  (QobuzApiService)       │   │  Manager     │ │
│  │                          │   │  (3 slots)   │ │
│  └──────────────────────────┘   └──────────────┘ │
│                    Background Threads             │
└───────────────────────────────────────────────────┘
```

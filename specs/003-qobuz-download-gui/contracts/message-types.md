# Message Type Contracts: Inter-Thread Communication

**Branch**: `003-qobuz-download-gui` | **Date**: 2026-04-26

## Overview

This document defines the message contracts for all inter-thread communication in the application. The GUI follows a single-producer / single-consumer pattern between the main (GUI) thread and background workers.

## Channel Architecture

```
┌──────────────┐                    ┌──────────────────┐
│              │  DownloadCommand   │                  │
│  GUI Thread  │ ───[cmd_channel]──→│  Download Worker │
│              │                    │                  │
│              │  ←──[evt_channel]──│                  │
└──────────────┘                    └──────────────────┘

┌──────────────┐                    ┌──────────────────┐
│              │  SearchRequest     │                  │
│  GUI Thread  │ ──────────────────→│  spawn_blocking  │
│              │                    │                  │
│              │  ←──SearchResult───│                  │
└──────────────┘                    └──────────────────┘
```

## Channel 1: Download Command Channel

**Type**: `async_channel::Sender<DownloadCommand>` / `async_channel::Receiver<DownloadCommand>`
**Direction**: GUI → Download Worker
**Capacity**: Bounded(16)

### `DownloadCommand`

```rust
enum DownloadCommand {
    /// Add a download task to the queue.
    /// Worker responds with DownloadEvent::Started when the download begins.
    Enqueue {
        id: Uuid,
        item: DownloadItem,
        quality: Quality,
        output_dir: PathBuf,
    },

    /// Cancel a specific download.
    /// Worker responds with DownloadEvent::Failed with cancellation context.
    Cancel {
        id: Uuid,
    },

    /// Shut down the download worker.
    /// No response. Worker drains gracefully.
    Shutdown,
}
```

**Invariants**:
- `Enqueue.id` MUST be unique across all active and queued tasks
- `Enqueue.output_dir` MUST be an absolute path
- `Cancel` for unknown `id` is silently ignored
- `Shutdown` causes the worker to stop accepting new commands after draining

## Channel 2: Download Event Channel

**Type**: `glib::MainContext::channel<PRIORITY_DEFAULT, DownloadEvent>`
**Direction**: Download Worker → GUI Thread
**Capacity**: Unbounded (glib-managed)

### `DownloadEvent`

```rust
enum DownloadEvent {
    /// Download has started (a slot was available).
    Started {
        id: Uuid,
    },

    /// Progress update for an active download.
    /// GUI should update progress bar and bytes label.
    Progress {
        id: Uuid,
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
    },

    /// Download completed successfully.
    /// File is at the specified path with metadata embedded.
    Completed {
        id: Uuid,
        path: PathBuf,
    },

    /// Download failed permanently (retries exhausted or non-retryable error).
    Failed {
        id: Uuid,
        error: String,
    },

    /// Download skipped because the file already exists.
    /// GUI should show a toast notification.
    Skipped {
        id: Uuid,
        path: PathBuf,
    },

    /// Authentication token expired during download.
    /// GUI should attempt silent re-auth, then retry or show login.
    ReauthRequired,
}
```

**Invariants**:
- Events for a given `id` arrive in order: `Started` → (`Progress`)* → `Completed` | `Failed` | `Skipped`
- `Progress.total_bytes` is `None` when the server does not provide Content-Length
- `ReauthRequired` is sent at most once per auth expiry; GUI handles re-auth globally
- All events are processed on the GLib main loop (GUI thread)

## Channel 3: Search Results

**Type**: `glib::MainContext::channel<PRIORITY_DEFAULT, SearchEvent>`
**Direction**: Background Thread → GUI Thread
**Trigger**: User submits search query

### `SearchEvent`

```rust
enum SearchEvent {
    /// Search completed successfully.
    Results {
        query: String,
        result: SearchResult,
    },

    /// Search failed.
    Error {
        query: String,
        error: String,
    },
}
```

**Invariants**:
- Results for a query may arrive after a newer query has been submitted; GUI must discard stale results by comparing `query`
- `SearchResult` is the type from `qobuz_api_rust_refactor::models::search`

## Channel 4: Browse Results

**Type**: `glib::MainContext::channel<PRIORITY_DEFAULT, BrowseEvent>`
**Direction**: Background Thread → GUI Thread

### `BrowseEvent`

```rust
enum BrowseEvent {
    /// Album detail loaded.
    Album {
        album: Album,
    },

    /// Artist detail loaded.
    Artist {
        artist: Artist,
    },

    /// Playlist detail loaded.
    Playlist {
        playlist: Playlist,
    },

    /// Browse request failed.
    Error {
        context: String,
        error: String,
    },
}
```

**Invariants**:
- Only one browse event is active at a time (user cannot browse two items simultaneously)
- If user navigates back before event arrives, the event is discarded

## Channel 5: Auth Events

**Type**: `glib::MainContext::channel<PRIORITY_DEFAULT, AuthEvent>`
**Direction**: Background Thread → GUI Thread

### `AuthEvent`

```rust
enum AuthEvent {
    /// Login succeeded.
    Authenticated {
        user_id: String,
    },

    /// Login failed with reason.
    AuthenticationFailed {
        error: String,
    },

    /// Silent re-authentication succeeded.
    Reauthenticated,

    /// Silent re-authentication failed; user must log in again.
    ReauthFailed {
        error: String,
    },
}
```

**Invariants**:
- Auth events are mutually exclusive with GUI interaction (login form is modal)
- On `ReauthFailed`, GUI transitions to login view with error message

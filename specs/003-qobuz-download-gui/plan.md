# Implementation Plan: Qobuz Download GUI

**Branch**: `003-qobuz-download-gui` | **Date**: 2026-04-26 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/003-qobuz-download-gui/spec.md`

## Summary

Build a GNOME/Libadwaita desktop application for downloading music from Qobuz. The application wraps the `qobuz-api-rust-refactor` library, providing a GUI for authentication (via GNOME Keyring), catalog search, album/track/playlist browsing, and high-quality audio downloading with progress tracking. The architecture uses `gio::spawn_blocking` to offload blocking API calls from the GUI thread, `async-channel` for inter-thread communication, and a download manager with a 3-slot concurrency limit.

## Technical Context

**Language/Version**: Rust 2024 edition
**Primary Dependencies**: `gtk4`, `adw` (libadwaita), `qobuz-api-rust-refactor` (local path dep), `oo7` (keyring), `async-channel`, `parking_lot`, `serde` + `serde_json`, `tracing` + `tracing-subscriber`, `thiserror`, `anyhow`
**Omitted from AGENTS.md stack**: `dynosaur` (no dynamic trait objects needed), `rayon` (no CPU-parallel workloads), `crossbeam` (using `async-channel` + `parking_lot` instead), `criterion` (no benchmarks in initial scope)
**Storage**: XDG directories for preferences (`serde_json`), GNOME Keyring via `oo7` for credentials
**Testing**: `cargo test`, `tempfile` for test fixtures
**Target Platform**: Linux desktop with GNOME/Libadwaita
**Project Type**: Desktop GUI application
**Performance Goals**: Search results within 3s, UI remains responsive during downloads, progress updates within 1s
**Constraints**: Max 3 concurrent downloads, no download resume, blocking API library calls must not freeze UI, SIGTERM/SIGINT handlers cancel active downloads and clean up partial files
**Scale/Scope**: Single-user desktop app, ~15 UI views/dialogs, local filesystem I/O only

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The project constitution (`constitution.md`) contains only template placeholders and has not been ratified. No concrete gates to evaluate. The AGENTS.md file provides the de facto project standards:

| Principle | Source | Status |
|-----------|--------|--------|
| Group by capability/domain | AGENTS.md | PASS - structure organized by feature (search, download, auth, etc.) |
| No models/handlers/utils structure | AGENTS.md | PASS - no generic organizational folders |
| Only `.rs` files, no UI/XML/BLP | AGENTS.md | PASS - programmatic widgets only |
| `thiserror` for library errors | AGENTS.md | PASS - using `thiserror` for domain error types |
| `anyhow` at top level only | AGENTS.md | PASS - only in binary crate |
| No `let _` or `.ok()` | AGENTS.md | PASS - will propagate errors with context |
| Structured `tracing` | AGENTS.md | PASS - all logging with fields |
| GNOME HIG compliance | AGENTS.md | PASS - ToolbarView, PreferencesDialog, Toast, etc. |
| Max 400 lines per file | AGENTS.md | PASS - will enforce via code review |
| No unsafe code | AGENTS.md | PASS |
| No clippy warnings | AGENTS.md | PASS - will run `cargo clippy --fix` |

## Project Structure

### Documentation (this feature)

```text
specs/003-qobuz-download-gui/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── message-types.md # Inter-thread message contracts
└── tasks.md             # Phase 2 output (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── main.rs                  # Application entry point, GTK/Adwaita init
├── app.rs                   # AdwApplication setup, window lifecycle
├── window.rs                # Main ApplicationWindow with NavigationView
├── auth/
│   ├── mod.rs               # Auth module
│   ├── login_view.rs        # Login form UI
│   ├── keyring.rs           # GNOME Keyring credential storage (oo7)
│   └── session.rs           # Auth state management, re-auth logic
├── search/
│   ├── mod.rs               # Search module
│   ├── view.rs              # Search bar + results UI
│   └── controller.rs        # Search API interaction, result handling
├── browse/
│   ├── mod.rs               # Browse module
│   ├── album_view.rs        # Album detail view
│   ├── artist_view.rs       # Artist detail view
│   └── playlist_view.rs     # Playlist detail view
├── download/
│   ├── mod.rs               # Download module
│   ├── manager.rs           # Download queue, concurrency (3 slots), cancellation
│   ├── worker.rs            # Background download worker (spawn_blocking)
│   ├── view.rs              # Active downloads + history UI
│   └── progress.rs          # Progress tracking state
├── preferences/
│   ├── mod.rs               # Preferences module
│   ├── dialog.rs            # PreferencesDialog (quality, directory, credentials)
│   └── settings.rs          # Settings persistence (XDG, serde_json)
├── cover_art/
│   ├── mod.rs               # Cover art module
│   └── cache.rs             # In-memory cover art cache with async loading
└── errors.rs                # Application error types (thiserror)
```

**Structure Decision**: Capability/domain grouping as mandated by AGENTS.md. Each feature domain (auth, search, browse, download, preferences, cover_art) is a self-contained module with its own view(s), controller logic, and any domain-specific types. Shared types (`errors.rs`) live at the top level.

## Complexity Tracking

> No constitution violations to justify.

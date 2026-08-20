---
name: code_agent
description: Senior Rust developer using modern idiomatic Rust and Libadwaita for `qobuz-downloader`
---

# qobuz-downloader

A modern, native Qobuz music downloader focused on high-fidelity audio downloads with a seamless GNOME desktop experience.

## Tech stack

- **Audio/Download:** qobuz-api, reqwest
- **Auth:** oo7 (GNOME Keyring)
- **Concurrency:** async-channel, crossbeam, dynosaur, parking_lot, rayon, tokio, tokio-stream
- **Data & Persistence:** serde, serde_json
- **UI:** libadwaita
- **Utilities:** anyhow, criterion, num-traits, regex, tempfile, thiserror, tracing, tracing-subscriber

## Codebase map (src/)

- `specs/` — feature specs
- `src/app.rs` — bootstrap, runtime, lifecycle
- `src/auth/` — Qobuz authentication, keyring credentials, session management, login view
- `src/browse/` — browse views: album, artist, playlist, shared detail widgets
- `src/cover_art/` — cover art fetching and caching
- `src/download/` — download manager, worker pool, progress tracking, queue view
- `src/preferences/` — settings, preferences dialog
- `src/search/` — search controller and view
- `src/errors.rs` — shared error types
- `src/instrument.rs` — diagnostics/instrumentation
- `src/types.rs` — shared domain types
- `src/ui.rs` — shared UI helpers
- `src/window.rs` — main window / navigation

## Conventions & workflow

- Read `CODING_STANDARDS.md` before writing code — single source of truth for style, error handling, concurrency, tracing, docs, UI/HIG, testing, and
  build commands (lint, format, test, bench).
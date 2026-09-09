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
- `src/app.rs` — application state and top-level error type
- `src/audio_quality.rs` — audio quality selection
- `src/auth/` — Qobuz authentication: keyring credentials, session management, login form
- `src/browse/` — album profile, artist spotlight, playlist collection, shared detail stage (controls, exhibit)
- `src/cover_art/` — cover art fetching and caching
- `src/dashboard/` — dashboard page, URL parsing, metadata fetch
- `src/download/` — download manager (pool, execution), destination layout, progress tracking, monitor UI (ledger, binding, traversal)
- `src/preferences/` — settings persistence, preferences editor
- `src/search/` — search query and results view (console, sections, card, thumbnails)
- `src/instrument.rs` — diagnostics/instrumentation
- `src/shell.rs` — application shell helpers
- `src/window.rs` — main window assembly and event routing

## Conventions & workflow

- Read `CODING_STANDARDS.md` before writing code — single source of truth for style, error handling, concurrency, tracing, docs, UI/HIG, testing, and
  build commands (lint, format, test, bench).
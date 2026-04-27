# Tasks: Qobuz Download GUI

**Input**: Design documents from `/specs/003-qobuz-download-gui/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/message-types.md, quickstart.md

**Tests**: Unit tests at bottom of each implementation file per AGENTS.md standards. Integration validation via quickstart.md flow (T045).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- Single project: `src/` at repository root
- Local dependency: `qobuz-api-rust-refactor` at `/home/arch/Downloads/github/qobuz-api-rust-refactor`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependency setup

- [X] T001 Add all dependencies to Cargo.toml (gtk4, adw, glib, gio, oo7, async-channel, parking_lot, uuid, chrono, serde, serde_json, tracing, tracing-subscriber, thiserror, anyhow, qobuz-api-rust-refactor path dep)
- [X] T002 Create module file structure with mod declarations: src/app.rs, src/window.rs, src/errors.rs, src/auth/mod.rs, src/auth/keyring.rs, src/auth/session.rs, src/auth/login_view.rs, src/search/mod.rs, src/search/view.rs, src/search/controller.rs, src/browse/mod.rs, src/browse/album_view.rs, src/browse/artist_view.rs, src/browse/playlist_view.rs, src/download/mod.rs, src/download/manager.rs, src/download/worker.rs, src/download/view.rs, src/download/progress.rs, src/preferences/mod.rs, src/preferences/dialog.rs, src/preferences/settings.rs, src/cover_art/mod.rs, src/cover_art/cache.rs; wire mod declarations in src/main.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types, application shell, and shared infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T003 Implement AppError enum (Api, Keyring, Settings, SettingsParse, Download, NotAuthenticated) with thiserror in src/errors.rs
- [X] T004 [P] Implement Quality enum (Mp3_320, Flac16_44, Flac24_96, Flac24_192) with Display, From<Quality> for i32, TryFrom<i32> for Quality in src/download/progress.rs (shared type used across search, browse, and preferences modules)
- [X] T005 [P] Implement AppSettings struct with serde serialization, load/save to $XDG_CONFIG_HOME/qobuz-downloader-rs/settings.json in src/preferences/settings.rs
- [X] T006 Implement AppState struct with Arc<parking_lot::Mutex<QobuzApiService>>, settings, auth_state fields in src/app.rs
- [X] T007 Set up GTK/Adwaita application initialization in src/main.rs: create AdwApplication, connect activate signal, build AppState, run application
- [X] T008 Create main window shell in src/window.rs: AdwApplicationWindow with AdwNavigationView, AdwToolbarView with header bar, placeholder content area

**Checkpoint**: Foundation ready — error types, settings, quality enum, and app shell are in place. User story implementation can now begin in parallel.

---

## Phase 3: User Story 3 - Authenticate with Qobuz (Priority: P1)

**Goal**: Enable user authentication with Qobuz credentials, stored securely in GNOME Keyring for automatic re-login between sessions.

**Independent Test**: Launch app with no stored credentials → login form appears → enter valid Qobuz credentials → authentication succeeds → main interface shown → relaunch app → auto-authentication from keyring → main interface shown directly.

### Implementation for User Story 3

- [ ] T009 [US3] Implement keyring credential store/load/delete using oo7 (attributes: [("application", "qobuz-downloader-rs")]) in src/auth/keyring.rs
- [ ] T010 [P] [US3] Implement AuthState enum (Unauthenticated, Authenticating, Authenticated { user_id: String }, Expired) and AuthEvent enum (Authenticated, AuthenticationFailed, Reauthenticated, ReauthFailed) in src/auth/session.rs
- [ ] T011 [US3] Implement session management (login, logout, re-auth including token expiry handling via stored credentials) with gio::spawn_blocking for API calls and glib::MainContext::channel for AuthEvent in src/auth/session.rs
- [ ] T012 [US3] Implement login view UI in src/auth/login_view.rs: AdwToolbarView with AdwHeaderBar, email EntryRow, password PasswordEntryRow, submit Button with suggested-action CSS, error label, use glib::MainContext::channel to receive AuthEvent
- [ ] T013 [US3] Wire auth module in src/auth/mod.rs: export keyring, session, login_view; connect login_view submit to session.login, connect AuthEvent to window state transitions
- [ ] T014 [US3] Update src/window.rs and src/app.rs to show login view when Unauthenticated/Authenticating, transition to main view on Authenticated, check keyring on startup for auto-login

**Checkpoint**: Authentication is fully functional — users can log in, credentials persist in GNOME Keyring, auto-login works on subsequent launches, re-auth handles token expiry.

---

## Phase 4: User Story 1 - Search and Download a Track (Priority: P1) 🎯 MVP

**Goal**: Allow users to search the Qobuz catalog, view results grouped by type with artwork, and download individual tracks with real-time progress.

**Independent Test**: Type a song name in search bar → results appear within 3s grouped by tracks/albums/artists/playlists with cover art → select a track → choose quality → download begins → progress bar updates in real-time → file appears in output directory with correct metadata tags and cover art.

### Implementation for User Story 1

- [ ] T015 [US1] Implement SearchEvent enum (Results { query, result: SearchResult }, Error { query, error }) in src/search/controller.rs
- [ ] T016 [US1] Implement search controller in src/search/controller.rs: accept query, call QobuzApiService::search via gio::spawn_blocking, send SearchEvent via glib::MainContext::channel, discard stale results by comparing query string
- [ ] T017 [US1] Implement search view UI in src/search/view.rs: AdwToolbarView with search entry in header, GtkListView for results grouped by type (tracks, albums, artists, playlists), show title/artist/duration/thumbnail per item, connect selection to detail navigation
- [ ] T018 [US1] Wire search module in src/search/mod.rs and integrate search view into src/window.rs NavigationView as the default page
- [ ] T019 [P] [US1] Implement DownloadTask, DownloadItem (Track/Album/Playlist), DownloadStatus (Queued/Active/Completed/Failed/Cancelled/Skipped), DownloadProgress { bytes_downloaded, total_bytes } types in src/download/progress.rs
- [ ] T020 [US1] Implement DownloadManager in src/download/manager.rs: async_channel::bounded(16) for DownloadCommand (Enqueue/Cancel/Shutdown), glib::MainContext::channel for DownloadEvent, semaphore with 3 permits for concurrency, task tracking HashMap<Uuid, DownloadTask>
- [ ] T021 [US1] Implement download worker in src/download/worker.rs: process commands from channel, spawn individual downloads via gio::spawn_blocking, report Started/Progress/Completed/Failed/Skipped events, handle file-exists skip with toast notification, retry transient failures (network timeouts, 5xx, 429) with exponential backoff (up to 3 retries, 2s/4s/8s), verify metadata tags and cover art are embedded in downloaded files (FR-005)
- [ ] T022 [US1] Implement download view UI in src/download/view.rs: active downloads list with per-download progress bar, file name label, quality label, status indicator
- [ ] T023 [US1] Wire download module in src/download/mod.rs: connect search result track selection to DownloadCommand::Enqueue, connect DownloadEvent to download view updates, integrate download view into src/window.rs
- [ ] T024 [P] [US1] Implement cover art cache in src/cover_art/cache.rs: HashMap<String, gdk::Texture> with spawn_blocking HTTP fetch, gdk::Texture::from_bytes for GPU-ready textures, on-demand loading for list items and detail views
- [ ] T025 [US1] Wire cover art module in src/cover_art/mod.rs and integrate texture loading into search view result items in src/search/view.rs

**Checkpoint**: Search and single-track download is fully functional — this is the MVP. Users can search, browse results, download a track, and see progress. The application delivers standalone value.

---

## Phase 5: User Story 6 - View and Manage Active Downloads (Priority: P2)

**Goal**: Show active downloads with real-time progress, enable cancellation, and display download history with status.

**Independent Test**: Start 3+ downloads → verify 3 run concurrently and rest are queued → cancel one active download → verify partial file cleanup and slot freed → wait for remaining downloads → verify completed/failed appear in history with file location.

### Implementation for User Story 6

- [ ] T026 [US6] Add cancel button per active download in src/download/view.rs: send DownloadCommand::Cancel on click, update UI to show "Cancelling..." state
- [ ] T027 [US6] Implement download history tracking in src/download/manager.rs: maintain completed tasks list (Completed/Failed/Skipped states), store completed_at timestamp and file path
- [ ] T028 [US6] Add download history section to src/download/view.rs: separate active and completed lists, show status icon (success/failed/skipped), file path, completed time
- [ ] T029 [US6] Wire auto re-authentication on token expiry: download worker (src/download/worker.rs) catches authentication errors from API calls and delegates to session.reauth in src/auth/session.rs; send ReauthRequired event if re-auth fails (FR-008a)

**Checkpoint**: Download management is fully functional — users can monitor, cancel, and review all downloads with full history.

---

## Phase 6: User Story 2 - Download a Full Album (Priority: P2)

**Goal**: Allow users to browse album details with full track listings and download entire albums with organized folder structure.

**Independent Test**: Search for album → select album → detail view shows cover, track list, release info → initiate download → all tracks download concurrently → files saved in "Artist - Album Title/" folder named "TrackNumber - Title.ext".

### Implementation for User Story 2

- [ ] T030 [US2] Implement BrowseEvent enum (Album { album }, Artist { artist }, Playlist { playlist }, Error { context, error }) in src/browse/mod.rs
- [ ] T031 [US2] Implement album detail view in src/browse/album_view.rs: AdwToolbarView with back navigation, album cover (GtkPicture), artist/genre/release date labels, track listing (GtkListView with track number/title/duration), quality selector, download album button
- [ ] T032 [US2] Implement album browsing via gio::spawn_blocking in src/browse/mod.rs: load album details from API, send BrowseEvent::Album via glib channel, push album_view onto NavigationView in src/window.rs
- [ ] T033 [US2] Add batch album download to src/download/manager.rs: enqueue all album tracks as individual DownloadCommand::Enqueue with shared album metadata, report per-track and overall progress
- [ ] T034 [US2] Implement album folder naming ("Artist - Album Title") and track file naming ("TrackNumber - Title.ext") with quality-based extension in src/download/worker.rs

**Checkpoint**: Album browsing and downloading is fully functional — users can view album details and download complete albums with proper organization.

---

## Phase 7: User Story 5 - Configure Application Preferences (Priority: P2)

**Goal**: Allow users to customize download directory, default audio quality, and manage stored credentials via a GNOME HIG-compliant preferences dialog.

**Independent Test**: Open preferences → change download directory → change default quality → verify next download uses new settings → click logout → verify credentials cleared and login view shown.

### Implementation for User Story 5

- [ ] T035 [US5] Implement AdwPreferencesDialog in src/preferences/dialog.rs: AdwPreferencesPage with AdwPreferencesGroup for download settings (directoryFileChooserRow, quality AdwComboRow), credentials group (logout button)
- [ ] T036 [US5] Wire preferences changes to AppSettings: save on change via src/preferences/settings.rs, update AppState settings mutex, reflect quality change in download enqueue defaults
- [ ] T037 [US5] Wire preferences module in src/preferences/mod.rs: add preferences button (gear icon) to window header bar in src/window.rs, connect to preferences dialog, connect logout to auth/keyring credential deletion and login view transition

**Checkpoint**: Preferences are fully functional — users can configure all settings and changes take effect immediately or on next use.

---

## Phase 8: User Story 4 - Browse and Download Playlists (Priority: P3)

**Goal**: Allow users to browse playlist details and download tracks from playlists.

**Independent Test**: Search for public playlist → view playlist detail (name, creator, track count, track list) → initiate download → all tracks download with progress → files saved correctly.

### Implementation for User Story 4

- [ ] T038 [P] [US4] Implement playlist detail view in src/browse/playlist_view.rs: AdwToolbarView with back navigation, playlist name/creator/track count/duration labels, track listing (GtkListView), download playlist button
- [ ] T039 [P] [US4] Implement artist detail view (FR-009a) in src/browse/artist_view.rs: AdwToolbarView with back navigation, artist name/image, album listing from artist catalog
- [ ] T040 [US4] Add playlist and artist browsing via gio::spawn_blocking in src/browse/mod.rs: send BrowseEvent::Playlist/Artist via glib channel, push views onto NavigationView in src/window.rs
- [ ] T041 [US4] Add batch playlist download to src/download/manager.rs: enqueue all playlist tracks as individual downloads, same concurrency and progress reporting as album downloads

**Checkpoint**: Playlist and artist browsing and downloading are fully functional — all content types are now supported.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] T042 Add structured tracing with fields throughout all modules (e.g., error tracing in download worker, search timing, auth state transitions)
- [ ] T043 Handle remaining edge cases across modules: subscription quality mismatch (show error toast with alternative quality), disk space errors (clean up partial files, mark remaining as failed), geo-blocked/DRM content (clear error toast, skip in batch downloads)
- [ ] T044 Ensure GNOME HIG compliance across all views: 6px spacing scale, mnemonic keyboard navigation, accessible labels via accessible_update_property, tooltip text on interactive elements, suggested-action/destructive-action CSS on buttons
- [ ] T045 Run full quickstart.md validation covering all success criteria: first launch flow (SC-006: first-time auth + download within 2 min), subsequent launch flow, search/download workflow (SC-001: search to download within 30s, SC-002: results within 3s, SC-003: metadata and cover art verification, SC-007: progress updates within 1s), album download (SC-004: all tracks present with correct folder structure), UI responsiveness during downloads (SC-005), preferences workflow

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **US3 Auth (Phase 3)**: Depends on Foundational — must complete before US1 (search/download requires auth)
- **US1 Search & Download (Phase 4)**: Depends on US3 completion — this is the MVP
- **US6 Download Management (Phase 5)**: Depends on US1 — enhances download view from US1
- **US2 Album Download (Phase 6)**: Depends on US1 — adds browse views and batch download
- **US5 Preferences (Phase 7)**: Depends on US3 and US1 — configures auth and download behavior
- **US4 Playlists (Phase 8)**: Depends on US1 and US2 — extends browse/download patterns
- **Polish (Phase 9)**: Depends on all desired user stories being complete

### User Story Dependencies

- **US3 (P1)**: Can start after Foundational (Phase 2) — No dependencies on other stories; BLOCKS US1
- **US1 (P1)**: Depends on US3 — requires authentication for API calls
- **US6 (P2)**: Depends on US1 — enhances the download view and manager built in US1
- **US2 (P2)**: Depends on US1 — reuses download manager, adds browse module
- **US5 (P2)**: Depends on US3 and US1 — configures auth and download settings
- **US4 (P3)**: Depends on US1, US2 — reuses browse and download patterns

### Within Each User Story

- Types/enums before logic that uses them
- UI views after controller/logic that drives them
- Module wiring after all components are implemented
- Integration into window.rs last

### Parallel Opportunities

- T004 and T005 (Phase 2): Quality enum and AppSettings — different files, no dependencies
- T010 and T009 (Phase 3): AuthState enum and keyring — different files, no dependencies
- T019 and T024 (Phase 4): Download types and cover art cache — different files, no dependencies
- T019 and T015 (Phase 4): Download types and SearchEvent — different modules
- T038 and T039 (Phase 8): Playlist view and artist view — different files, no dependencies

---

## Parallel Example: User Story 1

```bash
# Launch independent types/modules in parallel:
Task T015: "SearchEvent enum in src/search/controller.rs"
Task T019: "Download types in src/download/progress.rs"
Task T024: "Cover art cache in src/cover_art/cache.rs"

# Then sequential implementation:
Task T016: "Search controller (needs SearchEvent from T015)"
Task T020: "Download manager (needs DownloadTask from T019)"
Task T021: "Download worker (needs DownloadManager from T020)"
```

## Parallel Example: User Story 4

```bash
# Launch both detail views in parallel:
Task T038: "Playlist detail view in src/browse/playlist_view.rs"
Task T039: "Artist detail view in src/browse/artist_view.rs"

# Then wire both into browse module:
Task T040: "Browse module integration (needs T038, T039)"
```

---

## Implementation Strategy

### MVP First (US3 + US1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 3 (Authentication)
4. Complete Phase 4: User Story 1 (Search & Download Track)
5. **STOP and VALIDATE**: Test authentication + search + single track download end-to-end
6. Demo/deploy if ready — this is the MVP

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US3 (Auth) → Test login/keyring/auto-login independently
3. Add US1 (Search + Download Track) → Test search + download end-to-end (**MVP!**)
4. Add US6 (Download Management) → Test cancel, history, re-auth
5. Add US2 (Album Download) → Test album browse + batch download
6. Add US5 (Preferences) → Test settings dialog + credential management
7. Add US4 (Playlists) → Test playlist browse + download
8. Polish → Edge cases, HIG compliance, tracing

### Parallel Team Strategy

With multiple developers after Foundational phase completes:

1. Developer A: US3 (Auth) — must complete first
2. Developer B: Prepare US1 types (T019 download types, T024 cover art) — can start in parallel with US3
3. Once US3 completes:
   - Developer A: US1 search module (T015-T018)
   - Developer B: US1 download module (T020-T023)
4. Then fan out to P2 stories in parallel

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- All UI uses programmatic Libadwaita widgets only (no .ui/.xml/.blp files)
- All blocking API calls use gio::spawn_blocking, never called from GUI thread
- Run `cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt` after each task

# Feature Specification: Qobuz Download GUI

**Feature Branch**: `003-qobuz-download-gui`
**Created**: 2026-04-26
**Status**: Draft
**Input**: User description: "Build an application that allows users to download songs/albums from Qobuz via the API via GUI while using `/home/arch/Downloads/github/qobuz-api-rust-refactor`."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Search and Download a Track (Priority: P1)

A user wants to find a specific song and download it in high quality. The user opens the application, types a song name into the search bar, sees matching results with artwork and metadata, selects the desired track, picks an audio quality, and initiates the download. The download completes and the file appears in the user's chosen download directory with correct metadata tags embedded.

**Why this priority**: This is the core value proposition. Without search-and-download, the application serves no purpose. It is the minimum viable workflow.

**Independent Test**: Can be fully tested by searching for a known track, selecting it, downloading, and verifying the file exists with correct metadata on disk. Delivers immediate standalone value.

**Acceptance Scenarios**:

1. **Given** the user is on the main screen, **When** they type a query and submit, **Then** results are displayed grouped by type (tracks, albums, artists, playlists) with titles, artists, and thumbnails
2. **Given** search results are visible, **When** the user selects a track and chooses a quality level, **Then** a download begins and progress is shown in real-time
3. **Given** a download is in progress, **When** the download completes, **Then** the file is saved to the configured download directory with correct metadata tags and cover art embedded
4. **Given** a download is in progress, **When** a network error occurs, **Then** the download retries automatically and shows a clear error if it ultimately fails

---

### User Story 2 - Download a Full Album (Priority: P2)

A user wants to download an entire album. The user searches for or navigates to an album, sees its track listing, artwork, and quality information, then initiates a full album download. All tracks download concurrently with a progress indicator showing per-track and overall progress. Files are organized into an album folder with proper naming.

**Why this priority**: Album downloading is the second most common use case. It builds on P1 by adding batch operations and folder organization, but a user could still get value from single-track downloads alone.

**Independent Test**: Can be tested by searching for a known album, triggering the full download, and verifying all tracks are present in the download directory with correct folder structure and metadata.

**Acceptance Scenarios**:

1. **Given** the user has found an album in search results, **When** they select it, **Then** a detailed view shows the album cover, artist, genre, release date, track list, and available quality levels
2. **Given** the album detail view is open, **When** the user initiates a download, **Then** all tracks download concurrently with individual and overall progress bars
3. **Given** an album download completes, **Then** files are saved in a folder named "Artist - Album Title" with each track named "TrackNumber - Title.ext"

---

### User Story 3 - Authenticate with Qobuz (Priority: P1)

A first-time user needs to connect their Qobuz account. The user opens the application, is prompted to log in with their Qobuz email and password, and upon successful authentication gains access to search and download. The credentials are securely stored for subsequent sessions.

**Why this priority**: Authentication is a prerequisite for all other functionality. Without it, no search or download operations can occur. It must be in place alongside P1 search-and-download.

**Independent Test**: Can be tested by launching the application with no stored credentials, entering valid Qobuz credentials, and verifying that the application transitions to an authenticated state where search and download are available.

**Acceptance Scenarios**:

1. **Given** no stored credentials exist, **When** the user launches the application, **Then** a login form is presented requesting email and password
2. **Given** the user enters valid credentials, **When** they submit, **Then** authentication succeeds and the main interface becomes available
3. **Given** the user enters invalid credentials, **When** they submit, **Then** a human-readable error message is displayed indicating authentication failure, without echoing the password or revealing which field was incorrect
4. **Given** credentials were previously saved, **When** the user launches the application, **Then** authentication is performed automatically and the user goes directly to the main interface

---

### User Story 4 - Browse and Download Playlists and Artists (Priority: P3)

A user wants to download tracks from a playlist. The user searches for a playlist, sees its tracks and metadata, then downloads selected tracks or the entire playlist.

**Why this priority**: Playlist support extends the application's utility but is less critical than individual tracks and albums.

**Independent Test**: Can be tested by searching for a public playlist, viewing its contents, and downloading tracks from it.

**Acceptance Scenarios**:

1. **Given** search results include playlists, **When** the user selects a playlist, **Then** a detail view shows the playlist name, creator, track count, duration, and track listing
2. **Given** a playlist detail view, **When** the user initiates download, **Then** all tracks download with the same progress feedback as album downloads
3. **Given** search results include artists, **When** the user selects an artist, **Then** a detail view shows the artist name, image, and album catalog listing
4. **Given** an artist detail view, **When** the user selects an album, **Then** the album detail view opens with full track listing and download options

---

### User Story 5 - Configure Application Preferences (Priority: P2)

A user wants to customize the application behavior: change the download directory, select default audio quality, and manage stored credentials. The user opens a preferences dialog, adjusts settings, and the changes take effect immediately or on next use.

**Why this priority**: Configuration is essential for a usable application but can initially use sensible defaults. It becomes important quickly as users want control over quality and storage.

**Independent Test**: Can be tested by changing the download directory and quality, then verifying that subsequent downloads use the new settings.

**Acceptance Scenarios**:

1. **Given** the user opens preferences, **When** they change the download directory, **Then** future downloads save to the new location
2. **Given** the user opens preferences, **When** they select a default quality, **Then** future downloads default to that quality unless overridden
3. **Given** the user opens preferences, **When** they log out, **Then** stored credentials are cleared and the user is returned to the login screen

---

### User Story 6 - View and Manage Active Downloads (Priority: P2)

A user wants to see what is currently downloading, cancel a download, and view recent download history. The application shows active downloads with progress and allows cancellation.

**Why this priority**: Download management is important for user control and feedback, especially for large albums. It enhances the core download experience.

**Independent Test**: Can be tested by starting multiple downloads, cancelling one, and verifying the remaining downloads continue and completed downloads appear in history.

**Acceptance Scenarios**:

1. **Given** multiple downloads are active, **When** the user views the download view, **Then** each download shows its progress, file name, and quality
2. **Given** an active download, **When** the user cancels it, **Then** the download stops, partial files are cleaned up, and the slot is freed
3. **Given** downloads have completed, **When** the user views download history, **Then** completed downloads are listed with status (success/failed) and file location

---

### Edge Cases

- When the user's subscription does not support a requested quality level → Display an error toast indicating the quality is unavailable for their subscription tier, and offer the highest available quality as an alternative
- When the application is closed during a download → Interrupted downloads are not resumed on restart; partial files with `.part` extension are cleaned up on the next application startup and the download must be re-initiated by the user (see FR-012a)
- When disk space runs out during a multi-track album download → Stop the current download, clean up partial files, display an error toast indicating insufficient disk space, and mark remaining queued tracks as failed
- When the Qobuz API is temporarily unavailable or rate-limited → Retry with exponential backoff (up to 3 retries, 2s/4s/8s delays); if all retries fail, mark the download as failed with a clear error message
- When a track is not available for download (DRM-restricted or geo-blocked) → Display an error toast indicating the track is unavailable with the reason, and skip to the next track in batch downloads
- How does the system handle duplicate downloads (same track/album downloaded twice)? → Skip files that already exist in the download directory and display a toast notification to the user (see FR-013b)
- What happens when the user's authentication token expires mid-session? → Automatically re-authenticate silently using stored oo7 credentials; if re-auth fails, prompt user to log in again (see FR-008a)
- When a search query returns no results → Display an empty-state message indicating no results were found for the query, with a suggestion to try a different search term

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The application MUST allow users to search the Qobuz catalog by entering a text query and viewing results categorized by tracks, albums, artists, and playlists (limited to the first page of API results per category, default 25 items per category; pagination beyond the first page is not supported in v1)
- **FR-002**: The application MUST display search results with relevant metadata including title, artist name, album name, duration, quality indicators (Hi-Res badge for FLAC 24-bit), and cover art thumbnails
- **FR-003**: The application MUST allow users to download individual tracks in a user-selected quality level (MP3 320kbps, FLAC 16/44.1, FLAC 24/96, FLAC 24/192). For batch downloads (albums, playlists), all tracks use the same selected quality level.
- **FR-004**: The application MUST allow users to download full albums with all tracks, organized into folders named "Artist - Album Title" with tracks named "TrackNumber - Title.ext" (see US2 Acceptance Scenario #3)
- **FR-005**: The application MUST embed complete metadata tags (title, artist, album, genre, track number, year, ISRC, cover art) into downloaded files
- **FR-006**: The application MUST provide download progress indication with updates within 1 second (SC-007), per-file and overall for batch downloads
- **FR-007**: The application MUST authenticate users against the Qobuz API using email and password credentials
- **FR-008**: The application MUST securely store authentication credentials for automatic re-login between sessions using GNOME Keyring via oo7 (Secret Service API)
- **FR-008a**: When the authentication token expires mid-session, the application MUST automatically re-authenticate silently using stored credentials; if re-authentication fails, the user MUST be prompted to re-enter credentials
- **FR-009**: The application MUST allow users to browse album details including full track listings, release information, and cover art
- **FR-009a**: The application MUST allow users to browse artist details including name, image, and album catalog listing
- **FR-010**: The application MUST allow users to configure the download directory
- **FR-011**: The application MUST allow users to set a default audio quality for downloads
- **FR-012**: The application MUST handle download errors with automatic retry (up to 3 retries with exponential backoff: 2s, 4s, 8s) for transient failures (network timeouts, 5xx responses, rate-limit 429) and clear error messages for permanent failures (4xx responses excluding 429)
- **FR-012a**: Interrupted downloads (app close, network failure) are NOT resumed on restart; partial files are cleaned up and the download must be re-initiated by the user
- **FR-013**: The application MUST allow users to cancel in-progress downloads
- **FR-013a**: The application MUST support a maximum of 3 concurrent downloads; additional downloads are queued and started automatically as slots become available
- **FR-013b**: The application MUST skip downloads for files that already exist in the download directory and display a toast notification informing the user
- **FR-014**: The application MUST allow users to download playlists
- **FR-015**: The application MUST display download history showing completed and failed downloads (in-memory only, cleared on application restart, capped at 100 entries with FIFO eviction)
- **FR-016**: The application MUST use the existing `qobuz-api-rust-refactor` library for all Qobuz API interactions (search, browse, download, authentication)
- **FR-017**: The application MUST provide a graphical user interface following GNOME Human Interface Guidelines
- **FR-018**: The application MUST save and restore window geometry (width, height) between sessions, restoring the previous window dimensions on next launch

### Key Entities

*See [data-model.md](./data-model.md) for formal type definitions.*

- **Search Result**: A collection of tracks, albums, artists, and playlists matching a user query, each with metadata and artwork
- **Download Task**: An active or completed download operation tracking progress, status, file path, quality, and associated track/album metadata
- **User Preferences**: Configurable settings including download directory, default quality, and stored credentials
- **Album View**: A detailed presentation of an album's metadata, cover art, track listing, and available quality options
- **Artist View**: A detailed presentation of an artist's name, image, and album catalog listing
- **Track**: A single audio recording with associated metadata (title, artist, duration, quality capabilities, ISRC)

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can search, select, and begin downloading a track within 30 seconds of launching the application (with stored credentials)
- **SC-002**: Search results appear within 3 seconds of submitting a query
- **SC-003**: All downloaded files contain complete metadata tags and embedded cover art verified by a standard audio player
- **SC-004**: Album downloads complete with all tracks present in the correct folder structure when no transient errors exceed the retry limit
- **SC-005**: The application remains responsive during downloads (UI thread never blocks for more than 50ms), allowing the user to continue searching and queuing additional downloads without perceptible lag
- **SC-006**: First-time users can authenticate and complete their first download within 2 minutes
- **SC-007**: Download progress updates are visible within 1 second of actual progress changes

## Clarifications

### Session 2026-04-26

- Q: How should Qobuz credentials be stored between sessions? → A: GNOME Keyring via oo7 (Secret Service API)
- Q: What is the maximum number of concurrent downloads the application should support? → A: 3 concurrent downloads
- Q: Should downloads that are interrupted (e.g., app closed, network failure) resume on next app launch? → A: No, restart the download (simpler implementation)
- Q: How should the application handle an expired authentication token mid-session? → A: Auto re-authenticate silently using stored credentials
- Q: How should the application handle when a user tries to download a track/album that already exists in the download directory? → A: Skip existing files and show a toast notification

## Assumptions

- Users have an active Qobuz subscription that permits streaming/downloading at their requested quality levels
- Users have stable internet connectivity sufficient for downloading large audio files
- The application runs on a Linux desktop environment with GNOME/Libadwaita available
- The `qobuz-api-rust-refactor` library is available as a local dependency and provides all necessary API functionality (search, browse, download, authentication, metadata embedding)
- Users are comfortable entering their Qobuz credentials in a desktop application
- Downloads are for personal use in compliance with Qobuz terms of service
- The application is a single-user desktop application with no multi-user or server-side requirements
- Audio quality availability depends on the user's Qobuz subscription tier
- The application can access the filesystem to save downloaded files and read/write preferences

# UI/UX Replication Plan

## Goal
Replicate the UI/UX of the original C# Qobuz Downloader in a new Rust application using Libadwaita, adhering to GNOME HIG.

## Original Application Analysis

### 1. Login View (`LoginForm`)
- **Purpose**: Authenticate the user with Qobuz.
- **Inputs**:
    - **Email/Password**: Standard login.
    - **User ID/Auth Token**: Alternative login.
- **Logic**:
    - Passwords are MD5 hashed.
    - Credentials are saved to settings.
    - Validates inputs before attempting login.
- **UI Elements**:
    - Text entries for credentials.
    - Toggle for password visibility.
    - "Login" button.
    - Status label for feedback.

### 2. Main Dashboard (`MainForm`)
- **Purpose**: Central hub for downloading and settings.
- **Layout**:
    - **Top Bar**: Logo, Version, Window controls (minimize, close).
    - **Input Area**: URL text box for downloading specific links.
    - **Download Button**: Triggers the download process.
    - **Quality Selection**: Checkboxes for MP3, FLAC (Low, Mid, High). Mutually exclusive.
    - **Settings Panel**:
        - **Path Selection**: Button to choose download folder.
        - **Metadata Toggles**: Checkboxes for Album Artist, Track Title, Track Number, etc.
        - **Filename Template**: Options for naming downloaded files.
    - **Status/Output**: Text area or log for showing progress and messages.

### 3. Search View (`SearchForm`)
- **Purpose**: Search for albums and tracks on Qobuz.
- **Inputs**: Search query text box, Type selector (Album/Track).
- **Results**:
    - Displayed in a scrollable list.
    - Each result shows: Cover Art, Title, Artist, Release Date, Duration.
    - **Action**: "Download" button for each result.
- **Interaction**: Clicking "Download" populates the Main Dashboard's URL field and starts the download.

## Proposed Rust Implementation (Libadwaita)

### Architecture
- **Framework**: GTK4 + Libadwaita.
- **State Management**: Shared `AppState` struct using `Rc<RefCell<...>>` or `Arc<Mutex<...>>` (depending on threading needs, likely `Rc` for UI thread).
- **Async Runtime**: Tokio for handling API requests and downloads without freezing the UI.

### 1. Login Window (`ui/login.rs`)
- **Widget**: `adw::Window` or `adw::ApplicationWindow`.
- **Content**: `adw::StatusPage` or `adw::Clamp` containing:
    - `adw::PreferencesGroup` for inputs.
    - `adw::EntryRow` for Email/ID.
    - `adw::PasswordEntryRow` for Password/Token.
    - `gtk::Button` with `.suggested-action` style for "Login".
- **Feedback**: `adw::ToastOverlay` for error messages.

### 2. Main Window (`ui/main_window.rs`)
- **Widget**: `adw::ApplicationWindow`.
- **Header**: `adw::HeaderBar`.
- **Content**: `adw::NavigationView` to switch between Dashboard and Search, or a split view.
    - **Dashboard View**:
        - `adw::Clamp` layout.
        - **Download Section**: `adw::EntryRow` for URL + `gtk::Button` ("Download").
        - **Quality Section**: `adw::ComboRow` (cleaner than checkboxes) for Quality selection.
        - **Settings Section**: `adw::ExpanderRow` containing metadata toggles and path selection.
        - **Log/Status**: `gtk::TextView` in a `gtk::ScrolledWindow` for logs.

### 3. Search Window/Page (`ui/search.rs`)
- **Widget**: `adw::Window` (modal) or a page in `adw::NavigationView`.
- **Header**: Search bar (`gtk::SearchEntry`).
- **Content**: `gtk::ListView` or `gtk::GridView` (using Factory).
    - **Item Template**: `adw::ActionRow` with prefix (Cover Art) and suffix (Download Button).
    - **Async Image Loading**: Use `gdk_pixbuf` or similar to load cover art asynchronously.

### 4. Settings (`ui/settings.rs`)
- **Widget**: `adw::PreferencesWindow`.
- **Content**:
    - **General**: Download path (`gtk::FileChooserNative`).
    - **Metadata**: `adw::SwitchRow` for each tag.
    - **Advanced**: Filename templates.

## Step-by-Step Implementation Plan

1.  **Project Setup**: Initialize Rust project, add dependencies (`libadwaita`, `tokio`, `qobuz-api-rust`).
2.  **Login UI**: Implement `LoginWindow` and connect to `QobuzApiService`.
3.  **Main UI Shell**: Create `MainWindow` with navigation.
4.  **Dashboard**: Implement URL input, Quality selector, and basic Settings.
5.  **Download Logic**: Integrate `download_manager` (from API or custom) with UI.
6.  **Search UI**: Implement Search page with results list.
7.  **Refinement**: Polish UI, add error handling, ensure responsiveness.

## Verification
- **Build**: `cargo build` should pass.
- **Login**: Verify successful login with valid credentials.
- **Search**: Verify search returns results and displays them.
- **Download**: Verify clicking download starts the process (mocked if needed initially).

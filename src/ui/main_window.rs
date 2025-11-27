use std::{
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering::Relaxed},
        mpsc::channel,
    },
    thread::spawn,
    time::{Duration, Instant, SystemTime},
};

use {
    libadwaita::{
        Application, ApplicationWindow, Clamp, ComboRow, EntryRow, HeaderBar, NavigationPage,
        NavigationView, PreferencesGroup, StatusPage, Toast, ToastOverlay,
        gtk::{
            Align::{Center, Start},
            Box, Button, Image, Label, ListItem, ListView,
            Orientation::{Horizontal, Vertical},
            ProgressBar, ScrolledWindow, SelectionModel, SignalListItemFactory, SingleSelection,
            Stack, StringList, Widget,
            gdk::Texture,
            gio::{File, ListModel, ListStore},
            glib::{BoxedAnyObject, MainContext, Propagation::Proceed, timeout_future},
            pango::EllipsizeMode::End,
        },
        prelude::{
            AdwApplicationWindowExt, ApplicationExt, BoxExt, ButtonExt, Cast, ComboRowExt,
            EditableExt, FileExt, GtkWindowExt, ListItemExt, ListModelExt, PreferencesGroupExt,
            WidgetExt,
        },
    },
    qobuz_api_rust::{QobuzApiService, utils::sanitize_filename},
    regex::Regex,
    serde::{Deserialize, Serialize},
    tokio::runtime::Runtime,
    tokio_util::sync::CancellationToken,
};

use crate::ui::{
    search::SearchPage,
    settings::{
        SettingsDialog, load_download_path, load_metadata_config, load_preferred_format,
        save_preferred_format,
    },
};

#[derive(Clone)]
pub struct MainWindow {
    /// The main application window container.
    pub window: ApplicationWindow,
    /// Navigation view for switching between dashboard and search pages.
    pub navigation_view: NavigationView,
    /// Toast overlay for displaying transient notifications to the user.
    pub toast_overlay: ToastOverlay,
    /// Entry row widget for user input of Qobuz URLs or direct IDs.
    pub url_entry: EntryRow,
    /// Primary action button that initiates the download process.
    pub download_button: Button,
    /// Quality selection combo box for choosing audio format preferences.
    pub quality_combo: ComboRow,
    /// List view widget that displays the current download queue items.
    pub download_queue_list: ListView,
    /// Stack widget to switch between empty state and download queue.
    pub download_queue_stack: Stack,
    /// List store model that holds the serialized download queue data.
    pub download_queue_model: ListStore,
    /// Button to cancel all pending and active downloads in the queue.
    pub cancel_all_button: Button,
    /// Search page component for browsing and discovering Qobuz content.
    pub search_page: SearchPage,
    /// Optional Qobuz API service instance for authenticated API requests.
    pub api_service: Option<Rc<QobuzApiService>>,
    /// Thread-safe download queue manager for coordinating download operations.
    pub download_queue_manager: Arc<DownloadQueueManager>,
}

/// Represents the type of Qobuz content to download.
///
/// This enum distinguishes between album and track downloads, each containing
/// the respective Qobuz content identifier.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::main_window::DownloadType;
///
/// let album_download = DownloadType::Album("ip8qjy1m6dakc".to_string());
/// let track_download = DownloadType::Track("123456789".to_string());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DownloadType {
    /// An album download identified by its Qobuz album ID.
    Album(String),
    /// A track download identified by its Qobuz track ID.
    Track(String),
}

/// Metadata information for Qobuz content items.
///
/// This struct contains rich metadata that enhances the user interface display
/// for download queue items, providing contextual information about albums and tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadMetadata {
    /// The title of the content (album title or track title).
    pub title: String,
    /// The primary artist or performer name.
    pub artist: String,
    /// The album title (only present for track downloads).
    pub album: Option<String>,
    /// Duration in seconds (only present for track downloads).
    pub duration: Option<i64>,
    /// The release year as a string (e.g., "2023").
    pub release_year: Option<String>,
    /// URL to the cover art image.
    pub cover_url: Option<String>,
    /// Number of tracks in the album (only present for album downloads).
    pub track_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct DownloadRowData {
    pub item: DownloadItem,
    pub texture: Option<Texture>,
}

/// The current status of a download item in the queue.
///
/// This enum tracks the lifecycle of download items from initial queuing through
/// completion, cancellation, or failure states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DownloadStatus {
    /// The item is waiting in the queue to be processed.
    Queued,
    /// The item is currently being downloaded.
    Downloading,
    /// The download completed successfully.
    Completed,
    /// The download was cancelled by the user.
    Cancelled,
    /// The download failed with an associated error message.
    Failed(String),
}

/// A complete download item representing a single download task in the queue.
///
/// Each `DownloadItem` contains all necessary information to process, display,
/// and manage a download operation, including metadata, status, and progress tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadItem {
    /// Unique identifier for the download item, used for cancellation and tracking.
    pub id: String,
    /// The type of content to download (album or track).
    pub download_type: DownloadType,
    /// The Qobuz audio quality format ID (e.g., "5" for MP3, "6" for FLAC Lossless).
    pub format_id: String,
    /// Optional rich metadata for enhanced UI display.
    pub metadata: Option<DownloadMetadata>,
    /// Current status of the download operation.
    pub status: DownloadStatus,
    /// Progress indicator ranging from 0.0 (not started) to 1.0 (complete).
    pub progress: f64,
    /// Timestamp when the item was added to the queue.
    pub created_at: SystemTime,
}

/// Thread-safe manager for the download queue operations.
///
/// This struct provides synchronized access to the download queue, managing
/// item addition, status updates, progress tracking, and cancellation operations.
/// It ensures thread safety through `Arc<Mutex<T>>` wrappers around shared state.
#[derive(Debug, Clone)]
pub struct DownloadQueueManager {
    /// The actual queue of download items, protected by a mutex for thread safety.
    queue: Arc<Mutex<Vec<DownloadItem>>>,
    /// Flag indicating whether the queue processor is currently active.
    is_downloading: Arc<Mutex<bool>>,
    /// Registry of cancellation tokens for active downloads, keyed by download ID.
    cancellation_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(0);

impl DownloadQueueManager {
    /// Creates a new `DownloadQueueManager` instance.
    ///
    /// Initializes an empty queue with all necessary synchronization primitives.
    ///
    /// # Returns
    ///
    /// A new `DownloadQueueManager` ready for use.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            is_downloading: Arc::new(Mutex::new(false)),
            cancellation_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Adds a new download item to the queue.
    ///
    /// Automatically assigns a unique ID to the item before adding it to the queue.
    ///
    /// # Arguments
    ///
    /// * `item` - The download item to add (ID will be overwritten with a unique identifier).
    pub fn add_item(&self, mut item: DownloadItem) {
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Relaxed);
        item.id = id.to_string();
        let mut queue = self.queue.lock().unwrap();
        queue.push(item);
    }

    /// Cancels a specific download item by its ID.
    ///
    /// If the item is currently downloading, its cancellation token is triggered.
    /// The item's status is updated to `Cancelled` regardless of its current state.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item to cancel.
    pub fn cancel_item(&self, item_id: &str) {
        let mut tokens = self.cancellation_tokens.lock().unwrap();
        if let Some(token) = tokens.get(item_id) {
            token.cancel();
            tokens.remove(item_id);
        }

        // Update item status to Cancelled
        let mut queue = self.queue.lock().unwrap();
        if let Some(item) = queue.iter_mut().find(|i| i.id == item_id) {
            item.status = DownloadStatus::Cancelled;
        }
    }

    /// Cancels all active and queued download items.
    ///
    /// Triggers cancellation tokens for all currently downloading items and
    /// updates the status of all queued items to `Cancelled`.
    pub fn cancel_all(&self) {
        let mut tokens = self.cancellation_tokens.lock().unwrap();
        for (_, token) in tokens.drain() {
            token.cancel();
        }

        // Update all queued items to Cancelled
        let mut queue = self.queue.lock().unwrap();
        for item in queue.iter_mut() {
            if matches!(
                item.status,
                DownloadStatus::Queued | DownloadStatus::Downloading
            ) {
                item.status = DownloadStatus::Cancelled;
            }
        }
    }

    /// Clears all items from the download queue.
    ///
    /// Removes all download items from the queue regardless of their current status.
    /// This includes queued, downloading, completed, cancelled, and failed items.
    ///
    /// Note: This method does not cancel active downloads. Use `cancel_all()` first
    /// if you need to stop active downloads before clearing the queue.
    pub fn clear_queue(&self) {
        let mut queue = self.queue.lock().unwrap();
        queue.clear();

        // Also clear any remaining cancellation tokens to prevent memory leaks
        let mut tokens = self.cancellation_tokens.lock().unwrap();
        tokens.clear();
    }

    /// Cancels all downloads and clears the entire queue.
    ///
    /// This method combines the functionality of `cancel_all()` and `clear_queue()`
    /// to provide a complete cleanup operation. It first cancels all active and
    /// queued downloads, then removes all items from the queue entirely.
    ///
    /// This is the recommended method to use when the user wants to completely
    /// reset the download queue (e.g., when clicking "Cancel All" button).
    pub fn cancel_all_and_clear(&self) {
        self.cancel_all();
        self.clear_queue();
    }

    /// Retrieves all download items from the queue.
    ///
    /// Returns a clone of the current queue contents for read-only access.
    ///
    /// # Returns
    ///
    /// A vector containing all download items in the queue.
    pub fn get_items(&self) -> Vec<DownloadItem> {
        let queue = self.queue.lock().unwrap();
        queue.clone()
    }

    /// Updates the progress of a specific download item.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item.
    /// * `progress` - The new progress value (0.0 to 1.0).
    pub fn update_progress(&self, item_id: &str, progress: f64) {
        let mut queue = self.queue.lock().unwrap();
        if let Some(item) = queue.iter_mut().find(|i| i.id == item_id) {
            item.progress = progress;
        }
    }

    /// Updates the status of a specific download item.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item.
    /// * `status` - The new status to assign to the item.
    pub fn update_status(&self, item_id: &str, status: DownloadStatus) {
        let mut queue = self.queue.lock().unwrap();
        if let Some(item) = queue.iter_mut().find(|i| i.id == item_id) {
            item.status = status;
        }
    }

    /// Updates the metadata of a specific download item.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item.
    /// * `metadata` - The new metadata to assign (can be `None`).
    pub fn update_metadata(&self, item_id: &str, metadata: Option<DownloadMetadata>) {
        let mut queue = self.queue.lock().unwrap();
        if let Some(item) = queue.iter_mut().find(|i| i.id == item_id) {
            item.metadata = metadata;
        }
    }

    /// Retrieves the next queued item for processing.
    ///
    /// Finds the first item with `Queued` status, updates its status to `Downloading`,
    /// and returns a clone of the item for processing.
    ///
    /// # Returns
    ///
    /// `Some(DownloadItem)` if a queued item is found, `None` otherwise.
    pub fn get_next_item(&self) -> Option<DownloadItem> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(index) = queue
            .iter()
            .position(|item| matches!(item.status, DownloadStatus::Queued))
        {
            // Update status to Downloading but keep item in queue
            queue[index].status = DownloadStatus::Downloading;
            Some(queue[index].clone())
        } else {
            None
        }
    }

    /// Checks if the queue is effectively empty.
    ///
    /// Returns `true` if there are no items in the queue or if all items
    /// have been processed (i.e., no items have `Queued` status).
    ///
    /// # Returns
    ///
    /// `true` if no queued items remain, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        let queue = self.queue.lock().unwrap();
        queue.is_empty()
            || queue
                .iter()
                .all(|item| !matches!(item.status, DownloadStatus::Queued))
    }

    /// Sets the downloading state flag.
    ///
    /// # Arguments
    ///
    /// * `downloading` - `true` if queue processing is active, `false` otherwise.
    pub fn set_downloading(&self, downloading: bool) {
        *self.is_downloading.lock().unwrap() = downloading;
    }

    /// Checks if the queue processor is currently active.
    ///
    /// # Returns
    ///
    /// `true` if downloading is in progress, `false` otherwise.
    pub fn is_downloading(&self) -> bool {
        *self.is_downloading.lock().unwrap()
    }

    /// Registers a cancellation token for a download item.
    ///
    /// Associates a cancellation token with a download item ID for later cancellation.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item.
    /// * `token` - The cancellation token to register.
    pub fn register_cancellation_token(&self, item_id: String, token: CancellationToken) {
        let mut tokens = self.cancellation_tokens.lock().unwrap();
        tokens.insert(item_id, token);
    }

    /// Unregisters a cancellation token for a download item.
    ///
    /// Removes the cancellation token associated with the given item ID.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The unique identifier of the download item.
    pub fn unregister_cancellation_token(&self, item_id: &str) {
        let mut tokens = self.cancellation_tokens.lock().unwrap();
        tokens.remove(item_id);
    }
}

impl DownloadType {
    /// Parses a Qobuz URL or direct ID and returns the corresponding download type.
    ///
    /// This function handles multiple Qobuz URL formats:
    /// - New format: `https://play.qobuz.com/album/{id}` or `https://play.qobuz.com/track/{id}`
    /// - Old format: `https://qobuz.com/album/.../{id}` or `https://qobuz.com/track/.../{id}`
    /// - Direct IDs: alphanumeric strings (assumed to be album IDs by default)
    ///
    /// # Arguments
    ///
    /// * `url` - The Qobuz URL or direct ID string to parse
    ///
    /// # Returns
    ///
    /// Returns `Some(DownloadType)` if the URL/ID is valid and recognized,
    /// `None` if the input doesn't match any supported Qobuz format.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::main_window::DownloadType;
    ///
    /// // New play.qobuz.com album URL
    /// let album_url = "https://play.qobuz.com/album/ip8qjy1m6dakc";
    /// assert_eq!(DownloadType::from_url(album_url), Some(DownloadType::Album("ip8qjy1m6dakc".to_string())));
    ///
    /// // Direct ID (assumed to be album)
    /// let direct_id = "123456789";
    /// assert_eq!(DownloadType::from_url(direct_id), Some(DownloadType::Album("123456789".to_string())));
    ///
    /// // Invalid URL
    /// let invalid = "https://example.com/not-qobuz";
    /// assert_eq!(DownloadType::from_url(invalid), None);
    /// ```
    fn from_url(url: &str) -> Option<Self> {
        let trimmed_url = url.trim();

        // Handle direct ID input (numeric or alphanumeric)
        // Check if it's a valid ID (alphanumeric, possibly with hyphens)
        if trimmed_url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !trimmed_url.is_empty()
        {
            // For now, we'll assume direct ID input is an album ID
            // This is a reasonable assumption since album IDs are more commonly shared
            return Some(DownloadType::Album(trimmed_url.to_string()));
        }

        // Compile regex patterns for Qobuz URLs
        // New play.qobuz.com format: https://play.qobuz.com/album/ip8qjy1m6dakc
        let play_album_pattern =
            Regex::new(r"https?://play\.qobuz\.com/album/([a-zA-Z0-9-]+)").ok()?;
        let play_track_pattern =
            Regex::new(r"https?://play\.qobuz\.com/track/([a-zA-Z0-9-]+)").ok()?;

        // Old qobuz.com format: https://qobuz.com/album/.../12345
        let old_album_pattern =
            Regex::new(r"https?://(?:www\.)?qobuz\.com/(?:[a-z]{2}-[a-z]{2}/)?album/[^/]+/(\d+)")
                .ok()?;
        let old_track_pattern =
            Regex::new(r"https?://(?:www\.)?qobuz\.com/(?:[a-z]{2}-[a-z]{2}/)?track/[^/]+/(\d+)")
                .ok()?;

        // Try to match new play.qobuz.com album URL
        if let Some(captures) = play_album_pattern.captures(trimmed_url)
            && let Some(id) = captures.get(1)
        {
            return Some(DownloadType::Album(id.as_str().to_string()));
        }

        // Try to match new play.qobuz.com track URL
        if let Some(captures) = play_track_pattern.captures(trimmed_url)
            && let Some(id) = captures.get(1)
        {
            return Some(DownloadType::Track(id.as_str().to_string()));
        }

        // Try to match old qobuz.com album URL
        if let Some(captures) = old_album_pattern.captures(trimmed_url)
            && let Some(id) = captures.get(1)
        {
            return Some(DownloadType::Album(id.as_str().to_string()));
        }

        // Try to match old qobuz.com track URL
        if let Some(captures) = old_track_pattern.captures(trimmed_url)
            && let Some(id) = captures.get(1)
        {
            return Some(DownloadType::Track(id.as_str().to_string()));
        }

        None
    }
}

impl MainWindow {
    /// Creates a new `MainWindow` instance with the complete Qobuz Downloader UI.
    ///
    /// This constructor initializes all UI components, sets up navigation between
    /// dashboard and search pages, configures signal handlers, and establishes
    /// the download queue management system. It creates a fully functional
    /// Libadwaita application window ready for user interaction.
    ///
    /// The method performs the following key setup tasks:
    /// - Creates the main application window with proper dimensions
    /// - Sets up navigation between dashboard and search pages
    /// - Initializes the download queue ListView with custom factory
    /// - Configures quality selection with persisted user preferences
    /// - Establishes signal connections for all interactive elements
    /// - Integrates the Qobuz API service with search functionality
    ///
    /// # Arguments
    ///
    /// * `app` - Reference to the Libadwaita Application instance
    /// * `service` - Initialized QobuzApiService for API interactions
    ///
    /// # Returns
    ///
    /// Returns a fully configured `MainWindow` instance ready for presentation.
    pub fn new(app: &Application, service: QobuzApiService) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Qobuz Downloader")
            .default_width(800)
            .default_height(600)
            .build();

        // Connect to close request to properly quit the application
        let app_clone = app.clone();
        window.connect_close_request(move |_| {
            app_clone.quit();
            Proceed
        });

        let navigation_view = NavigationView::new();
        let toast_overlay = ToastOverlay::new();

        // Create all the widgets we need to reference later
        let url_entry = EntryRow::builder().title("Qobuz URL or ID").build();

        let download_button = Button::builder()
            .label("Download")
            .css_classes(["suggested-action", "pill"])
            .halign(Center)
            .hexpand(true)
            .build();

        let quality_model = StringList::new(&[
            "MP3 320 kbps",
            "FLAC Lossless",
            "FLAC Hi-Res ≤96kHz",
            "FLAC Hi-Res >96kHz",
        ]);

        // Load preferred format from settings
        let preferred_format = load_preferred_format().unwrap_or_else(|_| "6".to_string());
        let selected_index = match preferred_format.as_str() {
            "5" => 0,  // MP3 320 kbps
            "6" => 1,  // FLAC Lossless
            "7" => 2,  // FLAC Hi-Res ≤96kHz
            "27" => 3, // FLAC Hi-Res >96kHz
            _ => 1,    // Default to FLAC Lossless
        };

        let quality_combo = ComboRow::builder()
            .title("Audio Quality")
            .subtitle("Higher quality requires more storage space")
            .model(&quality_model)
            .selected(selected_index as u32)
            .build();

        // Create download queue components
        let download_queue_model = ListStore::new::<BoxedAnyObject>();
        let selection_model =
            SingleSelection::new(Some(download_queue_model.clone().upcast::<ListModel>()));
        let download_queue_list = ListView::new(
            Some(selection_model.clone().upcast::<SelectionModel>()),
            None::<SignalListItemFactory>,
        );

        // Create download queue stack and empty state
        let download_queue_stack = Stack::new();

        let empty_status_page = StatusPage::builder()
            .icon_name("folder-download-symbolic")
            .title("No Active Downloads")
            .description("Your download queue is empty. Search for music to start downloading.")
            .vexpand(true)
            .height_request(350)
            .build();

        let queue_scrolled = ScrolledWindow::builder()
            .child(&download_queue_list)
            .vexpand(true)
            .min_content_height(200)
            .build();

        download_queue_stack.add_named(&empty_status_page, Some("empty"));
        download_queue_stack.add_named(&queue_scrolled, Some("content"));
        download_queue_stack.set_visible_child_name("empty");

        // Create cancel all button
        let cancel_all_button = Button::builder()
            .label("Cancel All")
            .icon_name("process-stop-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Cancel all pending and active downloads")
            .halign(Center)
            .valign(Center)
            .build();

        // Build the dashboard content using these widgets
        let main_content = create_dashboard_page(
            &url_entry,
            &download_button,
            &quality_combo,
            &download_queue_stack,
            &cancel_all_button,
        );

        // Create dashboard page with header bar
        let dashboard_header_bar = HeaderBar::builder()
            .title_widget(&Label::new(Some("Dashboard")))
            .build();

        // Add settings button to header bar (gear icon)
        let settings_button = Button::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Settings")
            .build();

        dashboard_header_bar.pack_end(&settings_button);

        // Add search button to header bar
        let search_button = Button::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();

        dashboard_header_bar.pack_end(&search_button);

        let dashboard_content = Box::new(Vertical, 0);
        dashboard_content.append(&dashboard_header_bar);
        dashboard_content.append(&main_content);

        let dashboard_page = NavigationPage::builder()
            .title("Dashboard")
            .child(&dashboard_content)
            .build();

        navigation_view.add(&dashboard_page);

        // Create search page
        let search_page = SearchPage::new();
        let search_header_bar = HeaderBar::builder()
            .title_widget(&Label::new(Some("Search")))
            .build();

        let search_content = Box::new(Vertical, 0);
        search_content.append(&search_header_bar);
        search_content.append(&search_page.toast_overlay);

        let search_nav_page = NavigationPage::builder()
            .title("Search")
            .child(&search_content)
            .build();

        // Set up signals for navigation
        let window_clone = window.clone();
        settings_button.connect_clicked(move |_| {
            let settings_dialog = SettingsDialog::new();
            settings_dialog.present(&window_clone);
        });

        let navigation_view_clone = navigation_view.clone();
        let search_entry_clone = search_page.search_entry.clone();
        search_button.connect_clicked(move |_| {
            navigation_view_clone.push(&search_nav_page);

            // Focus the search entry after a brief delay to ensure the page is fully rendered
            let search_entry_for_focus = search_entry_clone.clone();
            MainContext::default().spawn_local(async move {
                timeout_future(Duration::from_millis(50)).await;
                search_entry_for_focus.grab_focus();
            });
        });

        // Set up the main window content
        toast_overlay.set_child(Some(&navigation_view));
        window.set_content(Some(&toast_overlay));

        let mut main_window = Self {
            window,
            navigation_view,
            toast_overlay,
            url_entry,
            download_button,
            quality_combo,
            download_queue_list,
            download_queue_stack,
            download_queue_model,
            cancel_all_button,
            search_page,
            api_service: Some(Rc::new(service)),
            download_queue_manager: Arc::new(DownloadQueueManager::new()),
        };

        // Set up search page with API service
        if let Some(ref service) = main_window.api_service {
            main_window.search_page.set_api_service(service.clone());
            main_window.search_page.setup_search_functionality();
            main_window
                .search_page
                .setup_esc_navigation(main_window.navigation_view.clone());

            // Set up download callback
            let main_window_clone = main_window.clone();
            main_window
                .search_page
                .set_on_download_request(move |download_type| {
                    // Navigate back to dashboard
                    main_window_clone.navigation_view.pop();

                    // Start the download
                    main_window_clone.handle_search_download(download_type);
                });

            // Set up add to queue callback
            let main_window_clone2 = main_window.clone();
            main_window
                .search_page
                .set_on_add_to_queue_request(move |download_type| {
                    // Get the selected quality format and save it as preferred
                    let format_id = main_window_clone2.get_and_save_selected_format_id();

                    // Add to queue without navigating away
                    main_window_clone2.add_to_download_queue(download_type, format_id);
                });
        }

        // Set up download queue factory
        setup_download_queue_factory(&main_window.download_queue_list, &main_window);

        // Set up signals after creating the main window instance
        setup_signals(
            &main_window.download_button,
            &main_window,
            &main_window.quality_combo,
        );

        main_window
    }

    /// Presents the main window to the user.
    ///
    /// This method makes the application window visible on screen by calling
    /// GTK's present() method. It should be called after the MainWindow is
    /// fully constructed to display the application interface to the user.
    pub fn present(&self) {
        self.window.present();
    }

    /// Shows a persistent loading toast that can be dismissed programmatically.
    ///
    /// Creates a toast notification with the specified message that remains visible
    /// until explicitly dismissed. This is useful for indicating ongoing operations
    /// like downloads or metadata fetching.
    ///
    /// # Arguments
    ///
    /// * `message` - The text message to display in the toast
    ///
    /// # Returns
    ///
    /// Returns the created `Toast` instance, allowing the caller to dismiss it later
    /// by calling `toast.dismiss()`.
    pub fn show_loading_toast(&self, message: &str) -> Toast {
        let toast = Toast::new(message);
        toast.set_timeout(0); // Persistent until dismissed
        self.toast_overlay.add_toast(toast.clone());
        toast
    }

    /// Maps the selected quality combo box index to the corresponding Qobuz format ID.
    ///
    /// Converts the user's quality selection (0-3) to the actual Qobuz API format IDs:
    /// - 0 → "5" (MP3 320 kbps)
    /// - 1 → "6" (FLAC Lossless 16-bit/44.1kHz)
    /// - 2 → "7" (FLAC Hi-Res 24-bit ≤96kHz)
    /// - 3 → "27" (FLAC Hi-Res 24-bit >96kHz & ≤192kHz)
    ///
    /// # Returns
    ///
    /// Returns the Qobuz format ID as a String, defaulting to "6" (FLAC Lossless)
    /// if an invalid selection index is encountered.
    fn get_selected_format_id(&self) -> String {
        match self.quality_combo.selected() {
            0 => "5".to_string(),  // MP3 320 kbps
            1 => "6".to_string(),  // FLAC Lossless (16-bit/44.1kHz)
            2 => "7".to_string(),  // FLAC Hi-Res 24-bit ≤ 96kHz
            3 => "27".to_string(), // FLAC Hi-Res 24-bit >96kHz & ≤ 192kHz
            _ => "6".to_string(),  // Default to FLAC Lossless
        }
    }

    /// Gets the currently selected format ID and saves it as the user's preferred format.
    ///
    /// Retrieves the selected quality format ID and persists it to the application
    /// settings so it will be remembered across sessions. If saving fails, an error
    /// is logged to stderr but the operation continues.
    ///
    /// # Returns
    ///
    /// Returns the selected format ID as a String.
    fn get_and_save_selected_format_id(&self) -> String {
        let format_id = self.get_selected_format_id();
        if let Err(e) = save_preferred_format(&format_id) {
            eprintln!("Failed to save preferred format: {}", e);
        }
        format_id
    }

    /// Validates user input and initiates the download process.
    ///
    /// This method is called when the user clicks the download button. It validates
    /// that the URL/ID field is not empty, parses the input to determine the download
    /// type (album or track), and initiates the download workflow.
    ///
    /// If validation fails, an error toast is displayed to the user.
    pub fn start_download(&self) {
        let url = self.url_entry.text().to_string();
        if url.trim().is_empty() {
            self.show_error_toast("Please enter a Qobuz URL or ID");
            return;
        }

        // Parse the URL to determine download type
        let download_type = match DownloadType::from_url(&url) {
            Some(dt) => dt,
            None => {
                self.show_error_toast("Invalid Qobuz URL or ID format");
                return;
            }
        };

        self.initiate_download(download_type);
    }

    /// Initiates the download process for a given download type.
    ///
    /// This method handles the complete download initiation workflow:
    /// 1. Validates authentication status and API credentials
    /// 2. Creates a new download item with the specified type and quality
    /// 3. Adds the item to the download queue
    /// 4. Fetches metadata asynchronously for enhanced UI display
    /// 5. Shows user feedback and starts queue processing if needed
    ///
    /// # Arguments
    ///
    /// * `download_type` - The type of content to download (album or track with ID)
    fn initiate_download(&self, download_type: DownloadType) {
        // Get the selected quality format and save it as preferred
        let format_id = self.get_and_save_selected_format_id();

        // Check authentication status
        if self.api_service.is_none() {
            self.show_error_toast("Not authenticated. Please log in first.");
            return;
        }

        // Validate that we have a proper service instance
        if let Some(ref service) = self.api_service {
            // Check if service has valid credentials
            if service.app_id.is_empty() || service.app_secret.is_empty() {
                self.show_error_toast("Invalid API credentials. Please log in again.");
                return;
            }
        }

        // Create a download item without ID (will be assigned in add_item)
        let download_item = DownloadItem {
            id: String::new(), // Will be set by add_item
            download_type: download_type.clone(),
            format_id: format_id.clone(),
            metadata: None,
            status: DownloadStatus::Queued,
            progress: 0.0,
            created_at: SystemTime::now(),
        };

        // Add to queue first (this assigns the ID)
        self.download_queue_manager.add_item(download_item.clone());

        // Get the actual item from the queue to get the correct ID
        let items = self.download_queue_manager.get_items();
        let actual_item = items.iter().find(|item| {
            item.download_type == download_item.download_type
                && item.format_id == download_item.format_id
                && item.created_at == download_item.created_at
        });

        if let Some(actual_item) = actual_item {
            let actual_id = actual_item.id.clone();
            let mut metadata_item = actual_item.clone();

            // Fetch metadata asynchronously for the queued item
            let window_clone = self.clone();
            MainContext::default().spawn_local(async move {
                if let Ok(()) = window_clone
                    .fetch_metadata_for_item(&mut metadata_item)
                    .await
                {
                    // Update the queue with the fetched metadata using correct ID
                    window_clone
                        .download_queue_manager
                        .update_metadata(&actual_id, metadata_item.metadata.clone());
                    window_clone.refresh_download_queue_display();
                }
                // If metadata fetching fails, the item will show "Loading metadata..."
                // which is acceptable fallback behavior
            });
        }

        // Show specific success toast
        let message = match &download_type {
            DownloadType::Album(_) => "Album added to download queue",
            DownloadType::Track(_) => "Track added to download queue",
        };
        self.show_success_toast(message);

        // Refresh display to show the new queued item
        self.refresh_download_queue_display();

        // Start queue processing if not already downloading
        if !self.download_queue_manager.is_downloading() {
            self.start_queue_processing();
        }
    }

    /// Performs the actual download operation in a background thread.
    ///
    /// This method handles the complete download workflow for a single download item:
    /// - Spawns a background thread with its own Tokio runtime
    /// - Authenticates with Qobuz API using environment credentials
    /// - Downloads album tracks or individual tracks based on content type
    /// - Manages file organization with proper artist/album directory structure
    /// - Handles progress updates and cancellation tokens
    /// - Reports results back to the main thread for UI updates
    ///
    /// The method uses channels for inter-thread communication and ensures proper
    /// cleanup of cancellation tokens upon completion.
    ///
    /// # Arguments
    ///
    /// * `download_item` - The fully configured download item containing all necessary
    ///   information for the download operation
    fn perform_background_download_with_item(&self, download_item: DownloadItem) {
        // Clone necessary references for background operation
        let window_clone1 = self.clone();
        let window_clone2 = self.clone();
        let download_item_clone = download_item.clone();
        let item_id1 = download_item.id.clone();
        let item_id2 = download_item.id.clone();
        let metadata_config = load_metadata_config();

        // Create a cancellation token for this download
        let cancellation_token = CancellationToken::new();
        self.download_queue_manager
            .register_cancellation_token(item_id1.clone(), cancellation_token.clone());

        // Create channels for communication between threads
        let (sender, receiver) = channel::<Result<(), String>>();
        let (progress_sender, progress_receiver) = channel::<f64>(); // Use f64 for progress instead of String

        // Spawn a background thread that runs its own Tokio runtime
        spawn(move || {
            // Initialize a Tokio runtime for this thread
            let rt = Runtime::new().expect("Failed to create Tokio runtime");

            let result = rt.block_on(async move {
                // Initialize a new service instance and authenticate using environment variables
                // This ensures the background thread has access to the same authenticated session
                let mut service = QobuzApiService::new().await.map_err(|e| e.to_string())?;

                // Authenticate using environment variables (user credentials should be in .env from login)
                service
                    .authenticate_with_env()
                    .await
                    .map_err(|e| e.to_string())?;

                match download_item_clone.download_type {
                    DownloadType::Album(album_id) => {
                        // First, try to get album details to verify the ID is valid
                        // Use track_ids as it's the only accepted extra parameter
                        match service
                            .get_album(&album_id, None, Some("track_ids"), None, None)
                            .await
                        {
                            Ok(album) => {
                                // Create proper download path with artist/album structure
                                let album_artist_name = if let Some(ref album_artist) = album.artist
                                {
                                    album_artist
                                        .name
                                        .as_ref()
                                        .unwrap_or(&"Unknown Artist".to_string())
                                        .clone()
                                } else {
                                    "Unknown Artist".to_string()
                                };

                                let album_title = album
                                    .title
                                    .as_ref()
                                    .unwrap_or(&"Unknown Album".to_string())
                                    .clone();

                                // Validate that we have valid album information
                                if album_artist_name == "Unknown Artist"
                                    && album_title == "Unknown Album"
                                {
                                    return Err(
                                        "Invalid album information received from API".to_string()
                                    );
                                }

                                let album_artist_dir = sanitize_filename(&album_artist_name);
                                let album_title_dir = sanitize_filename(&album_title);
                                let base_download_path = {
                                    let download_path = load_download_path()
                                        .unwrap_or_else(|_| "downloads".to_string());
                                    format!(
                                        "{}/{}/{}",
                                        download_path, album_artist_dir, album_title_dir
                                    )
                                };

                                // Get track list
                                let tracks = if let Some(ref tracks_data) = album.tracks {
                                    if let Some(ref items) = tracks_data.items {
                                        items.clone()
                                    } else {
                                        return Err("No tracks found in album".to_string());
                                    }
                                } else {
                                    return Err("No tracks data in album".to_string());
                                };

                                let track_count = tracks.len();
                                let _ = progress_sender.send(0.0); // Initial progress

                                // Download each track individually
                                let mut downloaded_count = 0;
                                for (index, track) in tracks.iter().enumerate() {
                                    // Check for cancellation before each track
                                    if cancellation_token.is_cancelled() {
                                        return Err("Download cancelled by user".to_string());
                                    }

                                    let track_id = if let Some(ref id) = track.id {
                                        id.to_string() // Ensure it's a String
                                    } else {
                                        continue; // Skip tracks without ID
                                    };

                                    let track_number =
                                        track.track_number.unwrap_or((index + 1) as i32);
                                    let track_title = track
                                        .title
                                        .as_ref()
                                        .unwrap_or(&format!("Track {}", track_id))
                                        .clone();

                                    // Update progress - each track represents a portion of total progress
                                    let progress = (index as f64) / (track_count as f64);
                                    let _ = progress_sender.send(progress);

                                    // Create track filename
                                    let track_filename =
                                        format!("{:02}. {}", track_number, track_title);
                                    let sanitized_filename = sanitize_filename(&track_filename);
                                    let file_extension =
                                        match download_item_clone.format_id.as_str() {
                                            "5" => "mp3",
                                            "6" | "7" | "27" => "flac",
                                            _ => "flac",
                                        };
                                    let track_download_path = format!(
                                        "{}/{}.{}",
                                        base_download_path, sanitized_filename, file_extension
                                    );

                                    // Download the track
                                    match service
                                        .download_track(
                                            &track_id,
                                            &download_item_clone.format_id,
                                            &track_download_path,
                                            &metadata_config,
                                        )
                                        .await
                                    {
                                        Ok(_) => {
                                            downloaded_count += 1;
                                        }

                                        Err(_) => {
                                            // Continue with next track even if one fails
                                        }
                                    }
                                }

                                let _ = progress_sender.send(1.0);

                                // Log completion summary
                                if downloaded_count == 0 {
                                    return Err(
                                        "No tracks were successfully downloaded".to_string()
                                    );
                                }

                                Ok(())
                            }

                            Err(e) => Err(e.to_string()),
                        }
                    }

                    DownloadType::Track(track_id) => {
                        // First, try to get track details to verify the ID is valid
                        match service.get_track(&track_id, None).await {
                            Ok(track) => {
                                // Create proper download path
                                let file_extension = match download_item_clone.format_id.as_str() {
                                    "5" => "mp3",
                                    "6" | "7" | "27" => "flac",
                                    _ => "flac",
                                };

                                // Try to get album info for proper naming
                                let download_path = if let Some(ref track_album) = track.album {
                                    let album_artist_name =
                                        if let Some(ref album_artist) = track_album.artist {
                                            album_artist
                                                .name
                                                .as_ref()
                                                .unwrap_or(&"Unknown Artist".to_string())
                                                .clone()
                                        } else {
                                            "Unknown Artist".to_string()
                                        };

                                    let album_title = track_album
                                        .title
                                        .as_ref()
                                        .unwrap_or(&"Unknown Album".to_string())
                                        .clone();
                                    let album_artist_dir = sanitize_filename(&album_artist_name);
                                    let album_title_dir = sanitize_filename(&album_title);
                                    let album_dir = {
                                        let download_path = load_download_path()
                                            .unwrap_or_else(|_| "downloads".to_string());
                                        format!(
                                            "{}/{}/{}",
                                            download_path, album_artist_dir, album_title_dir
                                        )
                                    };

                                    let track_number = track.track_number.unwrap_or(0);
                                    let track_title = track
                                        .title
                                        .as_ref()
                                        .unwrap_or(&format!("Track {}", track_id))
                                        .clone();
                                    let track_filename =
                                        format!("{:02}. {}", track_number, track_title);
                                    let sanitized_filename = sanitize_filename(&track_filename);

                                    format!(
                                        "{}/{}.{}",
                                        album_dir, sanitized_filename, file_extension
                                    )
                                } else {
                                    // Fallback to simple naming
                                    let download_path = load_download_path()
                                        .unwrap_or_else(|_| "downloads".to_string());
                                    format!(
                                        "{}/track_{}.{}",
                                        download_path, track_id, file_extension
                                    )
                                };

                                // Check for cancellation before download
                                if cancellation_token.is_cancelled() {
                                    return Err("Download cancelled by user".to_string());
                                }

                                // Send initial progress
                                let _ = progress_sender.send(0.0);

                                match service
                                    .download_track(
                                        &track_id,
                                        &download_item_clone.format_id,
                                        &download_path,
                                        &metadata_config,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        // Send final progress
                                        let _ = progress_sender.send(1.0);
                                        Ok(())
                                    }

                                    Err(e) => {
                                        let track_title = track
                                            .title
                                            .as_ref()
                                            .unwrap_or(&format!("Track {}", track_id))
                                            .clone();
                                        let error_msg = format!(
                                            "Failed to download track {}: {}",
                                            track_title, e
                                        );
                                        Err(error_msg)
                                    }
                                }
                            }

                            Err(e) => Err(e.to_string()),
                        }
                    }
                }
            });

            // Send result back to main thread
            let _ = sender.send(result.map_err(|e| e.to_string()));

            // Unregister the cancellation token
            window_clone1
                .download_queue_manager
                .unregister_cancellation_token(&item_id1);
        });

        // Use MainContext to check for completion and update UI
        MainContext::default().spawn_local(async move {
            let mut last_progress_update = 0.0;
            let mut last_update_time = Instant::now();

            loop {
                // Check for progress updates
                while let Ok(progress) = progress_receiver.try_recv() {
                    // Debounce progress updates to avoid excessive UI refreshes
                    // Only update if progress has changed significantly or enough time has passed
                    let now = Instant::now();
                    if (progress - last_progress_update).abs() > 0.01
                        || now.duration_since(last_update_time).as_millis() > 100
                    {
                        // Update progress through queue manager
                        window_clone2
                            .download_queue_manager
                            .update_progress(&item_id2, progress);
                        window_clone2.refresh_download_queue_display();
                        last_progress_update = progress;
                        last_update_time = now;
                    }
                }

                // Check if we have a result from the background thread
                match receiver.try_recv() {
                    Ok(result) => {
                        // Re-enable the download button
                        window_clone2.download_button.set_sensitive(true);

                        match result {
                            Ok(()) => {
                                window_clone2
                                    .show_success_toast("Download completed successfully!");
                                window_clone2
                                    .download_queue_manager
                                    .update_status(&item_id2, DownloadStatus::Completed);
                            }

                            Err(e) => {
                                let error_message = format!("Download failed: {}", e);
                                window_clone2.show_error_toast(&error_message);
                                window_clone2
                                    .download_queue_manager
                                    .update_status(&item_id2, DownloadStatus::Failed(e));
                            }
                        }

                        // Refresh display to show final status
                        window_clone2.refresh_download_queue_display();

                        // Check if there are more items in the queue
                        window_clone2.download_queue_manager.set_downloading(false);
                        if !window_clone2.download_queue_manager.is_empty() {
                            window_clone2.start_queue_processing();
                        }

                        break; // Exit the loop
                    }

                    Err(_) => {
                        // No result yet, wait a bit and check again
                        timeout_future(Duration::from_millis(100)).await;
                    }
                }
            }
        });
    }

    /// Displays an error toast notification to the user.
    ///
    /// Creates a temporary toast message with the specified error text that
    /// automatically disappears after a short duration. Used for validation
    /// errors, authentication issues, and other user-facing error conditions.
    ///
    /// # Arguments
    ///
    /// * `message` - The error message text to display
    fn show_error_toast(&self, message: &str) {
        let toast = Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    /// Starts processing the download queue if items are available.
    ///
    /// Checks if the queue contains queued items and initiates the download
    /// processing workflow. This includes getting the next queued item,
    /// updating its status to "Downloading", showing user feedback, and
    /// starting the actual download operation in a background thread.
    ///
    /// If the queue is empty or already being processed, this method returns
    /// immediately without taking any action.
    fn start_queue_processing(&self) {
        if self.download_queue_manager.is_empty() {
            return;
        }

        self.download_queue_manager.set_downloading(true);

        // Get the next item from the queue
        if let Some(download_item) = self.download_queue_manager.get_next_item() {
            // Update status to Downloading
            self.download_queue_manager
                .update_status(&download_item.id, DownloadStatus::Downloading);
            self.refresh_download_queue_display();

            // Show loading feedback
            self.show_loading_toast("Processing download queue...");

            // Disable the main download button during queue processing
            self.download_button.set_sensitive(false);

            // Perform the download with the enhanced item
            self.perform_background_download_with_item(download_item);
        } else {
            self.download_queue_manager.set_downloading(false);
        }
    }

    /// Handles download requests originating from the search page.
    ///
    /// This method is called when a user selects a "Download" action from the search
    /// results page. It navigates back to the dashboard and initiates the download
    /// process for the specified content type.
    ///
    /// # Arguments
    ///
    /// * `download_type` - The type of content to download (album or track with ID)
    pub fn handle_search_download(&self, download_type: DownloadType) {
        self.initiate_download(download_type);
    }

    /// Adds a download item to the queue and starts processing if not already active.
    ///
    /// This method is used when adding items to the queue without immediate navigation
    /// (e.g., from the search page's "Add to Queue" action). It creates a download
    /// item, adds it to the queue, fetches metadata asynchronously, and starts queue
    /// processing if no downloads are currently active.
    ///
    /// # Arguments
    ///
    /// * `download_type` - The type of content to download (album or track with ID)
    /// * `format_id` - The Qobuz audio quality format ID (e.g., "5", "6", "7", "27")
    pub fn add_to_download_queue(&self, download_type: DownloadType, format_id: String) {
        let download_item = DownloadItem {
            id: String::new(), // Will be set by add_item
            download_type: download_type.clone(),
            format_id: format_id.clone(),
            metadata: None,
            status: DownloadStatus::Queued,
            progress: 0.0,
            created_at: SystemTime::now(),
        };

        // Add to queue first (this assigns the ID)
        self.download_queue_manager.add_item(download_item.clone());

        // Get the actual item from the queue to get the correct ID
        let items = self.download_queue_manager.get_items();
        let actual_item = items.iter().find(|item| {
            item.download_type == download_item.download_type
                && item.format_id == download_item.format_id
                && item.created_at == download_item.created_at
        });

        if let Some(actual_item) = actual_item {
            let actual_id = actual_item.id.clone();
            let mut metadata_item = actual_item.clone();

            // Fetch metadata asynchronously for the queued item
            let window_clone = self.clone();
            MainContext::default().spawn_local(async move {
                if let Ok(()) = window_clone
                    .fetch_metadata_for_item(&mut metadata_item)
                    .await
                {
                    // Update the queue with the fetched metadata using correct ID
                    window_clone
                        .download_queue_manager
                        .update_metadata(&actual_id, metadata_item.metadata.clone());
                    window_clone.refresh_download_queue_display();
                }
                // If metadata fetching fails, the item will show "Loading metadata..."
                // which is acceptable fallback behavior
            });
        }

        // Show specific success toast
        let message = match &download_type {
            DownloadType::Album(_) => "Album added to download queue",
            DownloadType::Track(_) => "Track added to download queue",
        };
        self.show_success_toast(message);

        // Refresh display to show the new queued item
        self.refresh_download_queue_display();

        // Start queue processing if not already downloading
        if !self.download_queue_manager.is_downloading() {
            self.start_queue_processing();
        }
    }

    /// Displays a success toast notification to the user.
    ///
    /// Creates a temporary toast message with the specified success text that
    /// automatically disappears after a short duration. Used for successful
    /// operations like adding items to the queue or completing downloads.
    ///
    /// # Arguments
    ///
    /// * `message` - The success message text to display
    fn show_success_toast(&self, message: &str) {
        let toast = Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    /// Refreshes the download queue display by updating the underlying model.
    ///
    /// This method synchronizes the UI ListView with the current state of the
    /// download queue. It uses a smart update strategy with BoxedAnyObject to
    /// minimize UI flickering:
    /// 1. Iterates through the current items in the model.
    /// 2. If an item's data has changed, it updates the internal data structure
    ///    inside the BoxedAnyObject WITHOUT replacing the object itself if possible.
    /// 3. If the cover art URL changes, it invalidates the cached texture.
    /// 4. It notifies the model of changes to trigger a redraw only for changed rows.
    pub fn refresh_download_queue_display(&self) {
        let items = self.download_queue_manager.get_items();

        if items.is_empty() {
            self.download_queue_stack.set_visible_child_name("empty");
        } else {
            self.download_queue_stack.set_visible_child_name("content");
        }

        let model_count = self.download_queue_model.n_items();

        // Update existing items or append new ones
        for (i, item) in items.iter().enumerate() {
            let index = i as u32;

            if index < model_count {
                // Check if item needs update by comparing with existing one
                let mut needs_update = true;
                let mut cached_texture = None;

                if let Some(obj) = self.download_queue_model.item(index)
                    && let Ok(boxed) = obj.downcast::<BoxedAnyObject>()
                {
                    let data = boxed.borrow::<DownloadRowData>();
                    if data.item == *item {
                        needs_update = false;
                    } else {
                        // Check if cover URL is same, if so preserve texture
                        let old_url = data
                            .item
                            .metadata
                            .as_ref()
                            .and_then(|m| m.cover_url.clone());
                        let new_url = item.metadata.as_ref().and_then(|m| m.cover_url.clone());

                        if old_url == new_url {
                            cached_texture = data.texture.clone();
                        }
                    }
                }

                if needs_update {
                    // Create a NEW BoxedAnyObject to ensure GTK sees the change.
                    // Splicing the same object pointer (even with updated internal data)
                    // is often treated as a no-op by ListStore.
                    let row_data = DownloadRowData {
                        item: item.clone(),
                        texture: cached_texture,
                    };
                    let boxed = BoxedAnyObject::new(row_data);
                    self.download_queue_model.splice(index, 1, &[boxed]);
                }
            } else {
                // Append new item
                let row_data = DownloadRowData {
                    item: item.clone(),
                    texture: None,
                };
                let boxed = BoxedAnyObject::new(row_data);
                self.download_queue_model.append(&boxed);
            }
        }

        // Remove extra items if the queue shrunk (e.g. cleared)
        let new_count = items.len() as u32;
        if model_count > new_count {
            let empty: [BoxedAnyObject; 0] = [];
            self.download_queue_model
                .splice(new_count, model_count - new_count, &empty);
        }
    }
}

impl MainWindow {
    /// Fetches rich metadata for a download item from the Qobuz API.
    ///
    /// This asynchronous method retrieves detailed metadata from Qobuz for either
    /// albums or tracks, depending on the download item type. The fetched metadata
    /// includes title, artist, album information (for tracks), release year,
    /// cover art URLs, duration (for tracks), and track count (for albums).
    ///
    /// The method handles authentication validation and gracefully manages API
    /// errors by returning descriptive error messages. Successfully fetched
    /// metadata is stored directly in the provided `DownloadItem`.
    ///
    /// # Arguments
    ///
    /// * `item` - A mutable reference to the `DownloadItem` to populate with metadata
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if metadata is successfully fetched and stored,
    /// `Err(String)` with an error message if authentication fails or the API request fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // In an async context
    /// let mut download_item = DownloadItem { /* ... */ };
    /// match window.fetch_metadata_for_item(&mut download_item).await {
    ///     Ok(()) => println!("Metadata fetched successfully"),
    ///     Err(e) => eprintln!("Failed to fetch metadata: {}", e),
    /// }
    /// ```
    async fn fetch_metadata_for_item(&self, item: &mut DownloadItem) -> Result<(), String> {
        if self.api_service.is_none() {
            return Err("Not authenticated".to_string());
        }

        let service = self.api_service.as_ref().unwrap();
        match &item.download_type {
            DownloadType::Album(album_id) => {
                match service
                    .get_album(album_id, None, Some("track_ids"), None, None)
                    .await
                {
                    Ok(album) => {
                        let artist_name = if let Some(ref album_artist) = album.artist {
                            album_artist
                                .name
                                .as_ref()
                                .unwrap_or(&"Unknown Artist".to_string())
                                .clone()
                        } else {
                            "Unknown Artist".to_string()
                        };

                        let title = album
                            .title
                            .as_ref()
                            .unwrap_or(&"Unknown Album".to_string())
                            .clone();
                        let release_year = album.released_at.as_ref().map(|date| {
                            date.to_string().split('-').next().unwrap_or("").to_string()
                        });

                        let cover_url = album.image.as_ref().and_then(|image| {
                            image
                                .large
                                .as_ref()
                                .or(image.medium.as_ref())
                                .or(image.small.as_ref())
                                .cloned()
                        });

                        let track_count = if let Some(ref tracks_data) = album.tracks {
                            tracks_data.items.as_ref().map(|items| items.len())
                        } else {
                            None
                        };

                        item.metadata = Some(DownloadMetadata {
                            title,
                            artist: artist_name,
                            album: None,
                            duration: None,
                            release_year,
                            cover_url,
                            track_count,
                        });
                        Ok(())
                    }

                    Err(e) => Err(e.to_string()),
                }
            }

            DownloadType::Track(track_id) => match service.get_track(track_id, None).await {
                Ok(track) => {
                    let title = track
                        .title
                        .as_ref()
                        .unwrap_or(&format!("Track {}", track_id))
                        .clone();
                    let artist_name = if let Some(ref performers) = track.performers {
                        performers.clone()
                    } else if let Some(ref composer) = track.composer {
                        composer
                            .name
                            .as_ref()
                            .unwrap_or(&"Unknown Artist".to_string())
                            .clone()
                    } else {
                        "Unknown Artist".to_string()
                    };

                    let album_title = if let Some(ref track_album) = track.album {
                        track_album
                            .title
                            .as_ref()
                            .unwrap_or(&"Unknown Album".to_string())
                            .clone()
                    } else {
                        "Unknown Album".to_string()
                    };

                    let duration = track.duration;
                    let release_year = track.album.as_ref().and_then(|album| {
                        album.released_at.as_ref().map(|date| {
                            date.to_string().split('-').next().unwrap_or("").to_string()
                        })
                    });

                    let cover_url = track.album.as_ref().and_then(|album| {
                        album.image.as_ref().and_then(|image| {
                            image
                                .large
                                .as_ref()
                                .or(image.medium.as_ref())
                                .or(image.small.as_ref())
                                .cloned()
                        })
                    });

                    item.metadata = Some(DownloadMetadata {
                        title,
                        artist: artist_name,
                        album: Some(album_title),
                        duration,
                        release_year,
                        cover_url,
                        track_count: None,
                    });
                    Ok(())
                }

                Err(e) => Err(e.to_string()),
            },
        }
    }
}

/// Creates the main dashboard page layout with all UI components.
///
/// This function assembles the primary user interface for the Qobuz Downloader,
/// organizing widgets into logical sections using Libadwaita's PreferencesGroup
/// and Clamp components for proper spacing and visual hierarchy.
///
/// The dashboard includes:
/// - Download section with URL input and download button
/// - Quality selection section with audio format options
/// - Settings section with configuration options
/// - Download queue section with active downloads and cancellation controls
///
/// # Arguments
///
/// * `url_entry` - The EntryRow widget for Qobuz URL/ID input
/// * `download_button` - The primary download action button
/// * `quality_combo` - The ComboRow widget for audio quality selection
/// * `settings_expander` - The ExpanderRow for download settings configuration
/// * `download_queue_list` - The ListView widget displaying active downloads
/// * `cancel_all_button` - The button to cancel all pending downloads
///
/// # Returns
///
/// Returns a GTK Box containing the complete dashboard layout wrapped in a Clamp
/// for responsive sizing constraints.
fn create_dashboard_page(
    url_entry: &EntryRow,
    download_button: &Button,
    quality_combo: &ComboRow,
    download_queue_stack: &Stack,
    cancel_all_button: &Button,
) -> Box {
    // Main container with clamp for proper spacing
    let main_clamp = Clamp::builder().maximum_size(800).build();
    let main_box = Box::new(Vertical, 24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);

    // Download Section
    let download_group = PreferencesGroup::builder()
        .title("Download")
        .description("Enter a Qobuz URL to download")
        .build();

    download_group.add(url_entry);

    // Add download button with proper spacing
    let download_button_container = Box::new(Vertical, 0);
    download_button_container.set_margin_top(8);
    download_button_container.set_margin_bottom(8);
    download_button_container.append(download_button);
    download_group.add(&download_button_container);

    // Quality Section
    let quality_group = PreferencesGroup::builder()
        .title("Quality")
        .description("Select your preferred audio quality")
        .build();

    quality_group.add(quality_combo);

    // Download Queue Section
    let download_queue_group = PreferencesGroup::builder().build();

    // Create a custom header with title, subtitle, and cancel all button
    let download_queue_header_box = Box::new(Horizontal, 12);

    let download_queue_title_label = Label::builder()
        .label("Download Queue")
        .css_classes(["heading"])
        .halign(Start)
        .build();

    let download_queue_subtitle_label = Label::builder()
        .label("Active downloads and queued items")
        .css_classes(["subtitle", "dim-label"])
        .halign(Start)
        .build();

    let download_queue_header_content = Box::new(Vertical, 8);
    download_queue_header_content.append(&download_queue_title_label);
    download_queue_header_content.append(&download_queue_subtitle_label);
    download_queue_header_content.set_hexpand(true);
    download_queue_header_content.set_halign(Start);

    download_queue_header_box.append(&download_queue_header_content);
    download_queue_header_box.append(cancel_all_button);

    // Add the custom header as the first row
    download_queue_group.add(&download_queue_header_box);

    // Add the stack directly
    download_queue_group.add(download_queue_stack);

    // Return the cancel all button so it can be connected later
    // For now, we'll store it in a way that can be accessed
    // We'll handle the connection in the MainWindow constructor

    // Assemble all sections
    main_box.append(&download_group);
    main_box.append(&quality_group);
    main_box.append(&download_queue_group);

    main_clamp.set_child(Some(&main_box));

    // Return the clamp wrapped in a box
    let content_box = Box::new(Vertical, 0);
    content_box.append(&main_clamp);
    content_box
}

/// Sets up signal connections for the main window's interactive elements.
///
/// This function establishes the event handlers that connect user interactions
/// with application logic. It handles three key interactions:
/// 1. Download button clicks - triggers the main download workflow
/// 2. Cancel All button clicks - cancels all active and queued downloads
/// 3. Quality combo selection changes - saves user preferences persistently
///
/// # Arguments
///
/// * `download_button` - The primary download action button to connect
/// * `main_window` - Reference to the MainWindow instance for callback access
/// * `quality_combo` - The quality selection widget to monitor for changes
fn setup_signals(download_button: &Button, main_window: &MainWindow, quality_combo: &ComboRow) {
    let main_window_clone = main_window.clone();
    download_button.connect_clicked(move |_| {
        main_window_clone.start_download();
    });

    // Connect Cancel All button
    let main_window_clone2 = main_window.clone();
    main_window.cancel_all_button.connect_clicked(move |_| {
        main_window_clone2
            .download_queue_manager
            .cancel_all_and_clear();
        main_window_clone2.refresh_download_queue_display();
        main_window_clone2.show_success_toast("All downloads cancelled and queue cleared");
    });

    // Connect quality combo change signal to save preference
    let main_window_clone3 = main_window.clone();
    quality_combo.connect_selected_item_notify(move |_| {
        let _ = main_window_clone3.get_and_save_selected_format_id();
    });
}

/// Configures the ListView factory for rendering download queue items.
///
/// This function sets up the GTK SignalListItemFactory that defines how each
/// download queue item is displayed in the ListView. It creates a custom widget
/// layout for each row containing:
/// - Cover art image (with fallback icon)
/// - Metadata box (title, artist/album, status)
/// - Progress bar (visible during downloads)
/// - Individual cancel button (context-sensitive visibility)
///
/// The factory handles both the initial widget creation (`connect_setup`) and
/// the data binding (`connect_bind`) that updates the UI when download items
/// change state, progress, or metadata.
///
/// # Arguments
///
/// * `queue_list` - The ListView widget that will display download queue items
/// * `main_window` - Reference to the MainWindow instance for accessing queue management
fn setup_download_queue_factory(queue_list: &ListView, main_window: &MainWindow) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(move |_, list_item_obj| {
        let list_item = list_item_obj.downcast_ref::<ListItem>().unwrap();

        // Create widget structure:
        // [Cover Image] [Metadata Box] [Progress/Status] [Action Buttons]
        let main_box = Box::new(Horizontal, 16);
        main_box.set_margin_top(8);
        main_box.set_margin_bottom(8);
        main_box.set_margin_start(16);
        main_box.set_margin_end(16);

        // Cover image - use pixel size for proper scaling
        let cover_image = Image::builder().halign(Start).valign(Center).build();

        // Set pixel size to ensure proper display size (72px fits well with row height)
        cover_image.set_pixel_size(72);

        // Metadata box with title, artist, status
        let metadata_box = Box::new(Vertical, 4);
        metadata_box.set_hexpand(true);
        metadata_box.set_valign(Center);

        let title_label = Label::builder()
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .css_classes(["title-4"])
            .build();

        let subtitle_label = Label::builder()
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .css_classes(["dim-label"])
            .build();

        let status_label = Label::builder()
            .halign(Start)
            .xalign(0.0)
            .css_classes(["caption"])
            .build();

        metadata_box.append(&title_label);
        metadata_box.append(&subtitle_label);
        metadata_box.append(&status_label);

        // Progress bar (hidden when not downloading)
        let progress_bar = ProgressBar::builder().show_text(true).hexpand(true).build();

        // Progress container
        let progress_container = Box::new(Vertical, 4);
        progress_container.set_hexpand(true);
        progress_container.set_valign(Center);
        progress_container.append(&progress_bar);

        // Action buttons (Cancel individual, visible based on status)
        let cancel_button = Button::builder()
            .icon_name("process-stop-symbolic")
            .tooltip_text("Cancel download")
            .css_classes(["flat", "circular"])
            .build();

        let action_container = Box::new(Vertical, 0);
        action_container.set_valign(Center);
        action_container.append(&cancel_button);

        // Assemble all components
        main_box.append(&cover_image);
        main_box.append(&metadata_box);
        main_box.append(&progress_container);
        main_box.append(&action_container);

        // Store references in list item for binding
        list_item.set_child(Some(&main_box.upcast::<Widget>()));
    });

    let main_window_clone = main_window.clone();
    factory.connect_bind(move |_, list_item_obj| {
        let list_item = list_item_obj.downcast_ref::<ListItem>().unwrap();

        // Get DownloadRowData from BoxedAnyObject
        let (item, texture) = match list_item.item() {
            Some(obj) => {
                if let Ok(boxed) = obj.downcast::<BoxedAnyObject>() {
                    let data = boxed.borrow::<DownloadRowData>();
                    (data.item.clone(), data.texture.clone())
                } else {
                    return;
                }
            }

            None => return,
        };

        let download_item = item;

        if let Some(child) = list_item.child() {
            let main_box = child.downcast_ref::<Box>().unwrap();

            // Extract components
            let cover_image = main_box
                .first_child()
                .and_then(|w| w.downcast::<Image>().ok())
                .unwrap();
            let metadata_box = main_box
                .first_child()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<Box>().ok())
                .unwrap();
            let progress_container = main_box
                .first_child()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<Box>().ok())
                .unwrap();
            let action_container = main_box
                .last_child()
                .and_then(|w| w.downcast::<Box>().ok())
                .unwrap();

            let title_label = metadata_box
                .first_child()
                .and_then(|w| w.downcast::<Label>().ok())
                .unwrap();
            let subtitle_label = metadata_box
                .first_child()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<Label>().ok())
                .unwrap();
            let status_label = metadata_box
                .first_child()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<Label>().ok())
                .unwrap();
            let progress_bar = progress_container
                .first_child()
                .and_then(|w| w.downcast::<ProgressBar>().ok())
                .unwrap();
            let cancel_button = action_container
                .first_child()
                .and_then(|w| w.downcast::<Button>().ok())
                .unwrap();

            // Update labels based on metadata
            if let Some(metadata) = &download_item.metadata {
                title_label.set_label(&metadata.title);
                let subtitle = if let Some(album) = &metadata.album {
                    format!("{} • {}", metadata.artist, album)
                } else {
                    metadata.artist.clone()
                };
                subtitle_label.set_label(&subtitle);
            } else {
                title_label.set_label("Loading metadata...");
                subtitle_label.set_label("");
            }

            // Update status and progress
            match &download_item.status {
                DownloadStatus::Queued => {
                    status_label.set_label("Queued");
                    progress_bar.set_visible(false);
                    cancel_button.set_visible(true);
                }

                DownloadStatus::Downloading => {
                    status_label.set_label("Downloading...");
                    progress_bar.set_visible(true);
                    progress_bar.set_fraction(download_item.progress);
                    progress_bar.set_text(Some(&format!("{:.0}%", download_item.progress * 100.0)));
                    cancel_button.set_visible(true);
                }

                DownloadStatus::Completed => {
                    status_label.set_label("Completed");
                    progress_bar.set_visible(false);
                    cancel_button.set_visible(false);
                }

                DownloadStatus::Cancelled => {
                    status_label.set_label("Cancelled");
                    progress_bar.set_visible(false);
                    cancel_button.set_visible(false);
                }

                DownloadStatus::Failed(error) => {
                    status_label.set_label(&format!("Failed: {}", error));
                    progress_bar.set_visible(false);
                    cancel_button.set_visible(false);
                }
            }

            // Load cover image with caching
            // If we have a cached texture, use it immediately (zero blink)
            if let Some(tex) = texture {
                cover_image.set_paintable(Some(&tex));
                cover_image.set_pixel_size(72);
            } else if let Some(metadata) = &download_item.metadata {
                if let Some(cover_url) = &metadata.cover_url {
                    if !cover_url.is_empty() {
                        // No cache, need to load
                        // Set placeholder while loading
                        cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
                        cover_image.set_pixel_size(72);

                        let cover_image_clone = cover_image.clone();
                        let url_clone = cover_url.clone();

                        // Need to access the item wrapper to save the texture
                        // We can't keep the borrow from 'boxed' across the async boundary
                        // But we can get the object again inside the async block if we pass the list_item or something?
                        // Actually, we can clone the BoxedAnyObject
                        let item_obj = list_item
                            .item()
                            .and_then(|o| o.downcast::<BoxedAnyObject>().ok());

                        if let Some(boxed_item) = item_obj {
                            MainContext::default().spawn_local(async move {
                                if let Some(texture) = load_image_from_url(&url_clone).await {
                                    // Update UI
                                    cover_image_clone.set_paintable(Some(&texture));
                                    cover_image_clone.set_pixel_size(72);

                                    // Update Cache
                                    // This writes back to the model so next bind is fast
                                    let mut data = boxed_item.borrow_mut::<DownloadRowData>();
                                    data.texture = Some(texture);
                                } else {
                                    cover_image_clone
                                        .set_icon_name(Some("audio-x-generic-symbolic"));
                                    cover_image_clone.set_pixel_size(72);
                                }
                            });
                        }
                    } else {
                        cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
                        cover_image.set_pixel_size(72);
                    }
                } else {
                    cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
                    cover_image.set_pixel_size(72);
                }
            } else {
                cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
                cover_image.set_pixel_size(72);
            }

            // Connect cancel button
            let main_window_clone2 = main_window_clone.clone();
            let item_id = download_item.id.clone();
            cancel_button.connect_clicked(move |_| {
                main_window_clone2
                    .download_queue_manager
                    .cancel_item(&item_id);

                // Refresh the queue display
                main_window_clone2.refresh_download_queue_display();
            });
        }
    });

    queue_list.set_factory(Some(&factory));
}

/// Asynchronously loads an image from a URL and returns it as a GTK Texture.
///
/// This helper function handles the asynchronous loading of cover art images
/// from Qobuz URLs. It performs basic validation to ensure the URL is non-empty
/// and starts with "http", then uses GTK's built-in file loading capabilities
/// to fetch and convert the image data into a Texture for display.
///
/// # Arguments
///
/// * `url` - The HTTP/HTTPS URL of the image to load
///
/// # Returns
///
/// Returns `Some(Texture)` if the image loads successfully, `None` if the URL
/// is invalid, empty, or if the image loading fails.
///
/// # Examples
///
/// ```rust
/// // In an async context with MainContext
/// MainContext::default().spawn_local(async move {
///     if let Some(texture) = load_image_from_url("https://example.com/cover.jpg").await {
///         cover_image.set_paintable(Some(&texture));
///     }
/// });
/// ```
async fn load_image_from_url(url: &str) -> Option<Texture> {
    if url.is_empty() {
        return None;
    }

    if !url.starts_with("http") {
        return None;
    }

    let file = File::for_uri(url);
    match file.load_bytes_future().await {
        Ok((bytes, _)) => Texture::from_bytes(&bytes).ok(),
        Err(_) => None,
    }
}

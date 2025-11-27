use std::{boxed::Box, cell::RefCell, rc::Rc};

use {
    libadwaita::{
        NavigationView, SplitButton, Toast, ToastOverlay,
        gtk::{
            Align::{Center, End, Start},
            Box as GtkBox, Button, DropDown, EventControllerKey, Image, Label, ListItem, ListView,
            Orientation::{Horizontal, Vertical},
            Popover, ScrolledWindow, SearchEntry, SelectionModel, SignalListItemFactory,
            SingleSelection, StringObject, Widget,
            gdk::{Key, Texture},
            gio::{File, ListModel, ListStore},
            glib::{
                MainContext, Object,
                Propagation::{Proceed, Stop},
            },
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{
            BoxExt, ButtonExt, Cast, EditableExt, FileExt, ListItemExt, ListModelExt, PopoverExt,
            WidgetExt,
        },
    },
    qobuz_api_rust::{
        QobuzApiError::{ApiErrorResponse, AuthenticationError, HttpError},
        QobuzApiService,
        models::{Album, SearchResult, Track},
        utils::timestamp_to_date_and_year,
    },
};

pub use crate::ui::{main_window::DownloadType, settings::load_search_scope};

/// Type alias for download request callback functions.
///
/// This type represents a callback that handles download requests from search results.
/// It uses `Rc<RefCell<...>>` to enable shared ownership and interior mutability,
/// allowing the callback to be stored and modified within the `SearchPage` struct.
///
/// The callback takes a `DownloadType` parameter indicating whether the user wants
/// to download an album or track.
type DownloadCallback = Rc<RefCell<Option<Box<dyn Fn(DownloadType) + 'static>>>>;

/// Type alias for "add to queue" request callback functions.
///
/// This type represents a callback that handles queue requests from search results.
/// It uses `Rc<RefCell<...>>` to enable shared ownership and interior mutability,
/// allowing the callback to be stored and modified within the `SearchPage` struct.
///
/// The callback takes a `DownloadType` parameter indicating whether the user wants
/// to add an album or track to the download queue.
type AddToQueueCallback = Rc<RefCell<Option<Box<dyn Fn(DownloadType) + 'static>>>>;

/// Represents the different search scopes available in the Qobuz search interface.
///
/// This enum defines the types of content that users can search for in the Qobuz catalog.
/// Each variant corresponds to a specific entity type that can be queried through the
/// Qobuz API.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::search::SearchScope;
///
/// // Search for all content types
/// let scope = SearchScope::All;
///
/// // Search only for albums
/// let scope = SearchScope::Albums;
///
/// // Search only for tracks
/// let scope = SearchScope::Tracks;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// Search across all content types (albums and tracks).
    All,
    /// Search only for albums.
    Albums,
    /// Search only for tracks.
    Tracks,
}

impl SearchScope {
    /// Converts the search scope to the corresponding Qobuz API query parameter.
    ///
    /// The Qobuz API uses specific string values to filter search results by entity type.
    /// This method maps each `SearchScope` variant to the appropriate API parameter:
    /// - `SearchScope::All` returns `None`, which tells the API to search all entity types
    /// - `SearchScope::Albums` returns `Some("albums")`
    /// - `SearchScope::Tracks` returns `Some("tracks")`
    ///
    /// # Returns
    ///
    /// Returns `Some(&str)` containing the API parameter for filtered searches,
    /// or `None` for unfiltered searches across all content types.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchScope;
    ///
    /// assert_eq!(SearchScope::All.to_api_query(), None);
    /// assert_eq!(SearchScope::Albums.to_api_query(), Some("albums"));
    /// assert_eq!(SearchScope::Tracks.to_api_query(), Some("tracks"));
    /// ```
    pub fn to_api_query(self) -> Option<&'static str> {
        match self {
            SearchScope::All => None,
            SearchScope::Albums => Some("albums"),
            SearchScope::Tracks => Some("tracks"),
        }
    }
}

/// A complete search interface component for the Qobuz Downloader application.
///
/// The `SearchPage` struct encapsulates all the UI elements and functionality required
/// for searching the Qobuz catalog. It provides a user-friendly interface with:
/// - A search entry field for entering queries
/// - A scope selector dropdown for filtering by content type (all, albums, tracks)
/// - A results list view displaying search results with cover art and metadata
/// - Interactive download and queue buttons for each result
/// - Toast notifications for user feedback
///
/// This component integrates with the Qobuz API service to perform actual searches
/// and provides callback mechanisms for handling download and queue requests.
///
/// # Examples
///
/// ```rust
/// use libadwaita::prelude::*;
/// use qobuz_downloader_rust::ui::search::SearchPage;
///
/// // Create a new search page
/// let mut search_page = SearchPage::new();
///
/// // Set up the Qobuz API service
/// // search_page.set_api_service(api_service);
///
/// // Configure download callback
/// // search_page.set_on_download_request(|download_type| {
/// //     // Handle download request
/// // });
/// ```
#[derive(Clone)]
pub struct SearchPage {
    /// The search entry widget where users input their search queries.
    pub search_entry: SearchEntry,
    /// The dropdown widget for selecting the search scope (all, albums, tracks).
    pub scope_selector: DropDown,
    /// The list view widget that displays search results.
    pub results_list: ListView,
    /// The list store model that holds the search results data.
    pub list_model: ListStore,
    /// The toast overlay for displaying transient notifications to the user.
    pub toast_overlay: ToastOverlay,
    /// The optional Qobuz API service instance for performing authenticated searches.
    pub api_service: Option<Rc<QobuzApiService>>,
    /// Callback function triggered when a user requests to download a search result.
    pub on_download_request: DownloadCallback,
    /// Callback function triggered when a user requests to add a search result to the download queue.
    pub on_add_to_queue_request: AddToQueueCallback,
    /// Loading state flag to prevent concurrent search requests.
    pub is_loading: Rc<RefCell<bool>>,
    /// Reference to the navigation view for handling ESC key navigation.
    pub navigation_view: Option<NavigationView>,
}

impl SearchPage {
    /// Creates a new `SearchPage` instance with all UI components initialized.
    ///
    /// This constructor builds the complete search interface including:
    /// - A header section with search scope selector and search entry
    /// - A scrollable results area with a ListView for displaying search results
    /// - Proper GNOME Human Interface Guidelines (HIG) compliant layout and spacing
    ///
    /// The search page is initially created without an API service or callbacks,
    /// which must be configured separately using `set_api_service()`,
    /// `set_on_download_request()`, and `set_on_add_to_queue_request()`.
    ///
    /// # Returns
    ///
    /// Returns a fully initialized `SearchPage` ready for configuration and display.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchPage;
    ///
    /// let search_page = SearchPage::new();
    /// // Configure API service and callbacks before use
    /// ```
    pub fn new() -> Self {
        let toast_overlay = ToastOverlay::new();
        let main_box = GtkBox::new(Vertical, 0);

        // Create search header with search entry and scope selector
        let scope_selector = DropDown::from_strings(&["All", "Albums", "Tracks"]);

        // Load saved search scope from settings, default to "All" (0) if not found
        let saved_scope = load_search_scope().unwrap_or_else(|_| "0".to_string());
        let selected_index: u32 = saved_scope.parse().unwrap_or(0);
        scope_selector.set_selected(selected_index);

        let search_entry = SearchEntry::builder()
            .placeholder_text("Search for albums or tracks...")
            .hexpand(true)
            .build();

        // Assemble Header Box following GNOME HIG guidelines
        let header_box = GtkBox::new(Horizontal, 8);
        header_box.set_margin_top(16);
        header_box.set_margin_bottom(16);
        header_box.set_margin_start(16);
        header_box.set_margin_end(16);

        // GNOME HIG: Place the filter/selector before the main search entry
        header_box.append(&scope_selector);
        header_box.append(&search_entry);

        // Create results area with ListView
        let list_model = ListStore::new::<StringObject>();
        let selection_model = SingleSelection::new(Some(list_model.clone().upcast::<ListModel>()));
        let results_list = ListView::new(
            Some(selection_model.clone().upcast::<SelectionModel>()),
            None::<SignalListItemFactory>,
        );
        let results_scrolled = ScrolledWindow::builder()
            .child(&results_list)
            .vexpand(true)
            .min_content_height(400)
            .build();

        // Assemble the page
        main_box.append(&header_box);
        main_box.append(&results_scrolled);

        toast_overlay.set_child(Some(&main_box));

        Self {
            search_entry,
            scope_selector,
            results_list,
            list_model,
            toast_overlay,
            api_service: None,
            on_download_request: Rc::new(RefCell::new(None)),
            on_add_to_queue_request: Rc::new(RefCell::new(None)),
            is_loading: Rc::new(RefCell::new(false)),
            navigation_view: None,
        }
    }

    /// Sets the callback function for handling download requests from search results.
    ///
    /// This method configures the action that should be performed when a user clicks
    /// the "Download" button on a search result item. The callback receives a
    /// `DownloadType` enum indicating whether the user wants to download an album or track.
    ///
    /// # Arguments
    ///
    /// * `callback` - A closure that takes a `DownloadType` parameter and handles
    ///   the download request. The closure must have a static lifetime.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchPage;
    /// use qobuz_downloader_rust::ui::main_window::DownloadType;
    ///
    /// let mut search_page = SearchPage::new();
    /// search_page.set_on_download_request(|download_type| {
    ///     match download_type {
    ///         DownloadType::Album(id) => println!("Downloading album: {}", id),
    ///         DownloadType::Track(id) => println!("Downloading track: {}", id),
    ///     }
    /// });
    /// ```
    pub fn set_on_download_request<F: Fn(DownloadType) + 'static>(&mut self, callback: F) {
        *self.on_download_request.borrow_mut() = Some(Box::new(callback));
    }

    /// Sets the callback function for handling "Add to Queue" requests from search results.
    ///
    /// This method configures the action that should be performed when a user selects
    /// the "Add to Queue" option from a search result item's dropdown menu. The callback
    /// receives a `DownloadType` enum indicating whether the user wants to queue an
    /// album or track for later download.
    ///
    /// # Arguments
    ///
    /// * `callback` - A closure that takes a `DownloadType` parameter and handles
    ///   the queue request. The closure must have a static lifetime.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchPage;
    /// use qobuz_downloader_rust::ui::main_window::DownloadType;
    ///
    /// let mut search_page = SearchPage::new();
    /// search_page.set_on_add_to_queue_request(|download_type| {
    ///     match download_type {
    ///         DownloadType::Album(id) => println!("Adding album to queue: {}", id),
    ///         DownloadType::Track(id) => println!("Adding track to queue: {}", id),
    ///     }
    /// });
    /// ```
    pub fn set_on_add_to_queue_request<F: Fn(DownloadType) + 'static>(&mut self, callback: F) {
        *self.on_add_to_queue_request.borrow_mut() = Some(Box::new(callback));
    }

    /// Sets the Qobuz API service instance for performing authenticated searches.
    ///
    /// This method configures the API service that will be used to communicate with
    /// the Qobuz API. The service must be properly authenticated before performing
    /// searches.
    ///
    /// # Arguments
    ///
    /// * `service` - A reference-counted `QobuzApiService` instance that has been
    ///   properly initialized and authenticated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchPage;
    /// use qobuz_api_rust::QobuzApiService;
    /// use std::rc::Rc;
    ///
    /// // Assuming you have an authenticated service instance
    /// // let service = QobuzApiService::new().await?;
    /// // service.authenticate_with_env().await?;
    ///
    /// let mut search_page = SearchPage::new();
    /// // search_page.set_api_service(Rc::new(service));
    /// ```
    pub fn set_api_service(&mut self, service: Rc<QobuzApiService>) {
        self.api_service = Some(service);
    }

    /// Sets up all search-related signal connections and UI functionality.
    ///
    /// This method establishes the event handlers that trigger search operations
    /// in response to user interactions:
    /// - `search_started` signal on the search entry (when user starts typing)
    /// - `activate` signal on the search entry (when user presses Enter)
    /// - `selected-item-notify` signal on the scope selector (when user changes search scope)
    ///
    /// It also initializes the ListView factory for rendering search results with
    /// proper cover art, metadata, and interactive download/queue controls.
    ///
    /// This method should be called after configuring the API service and callbacks
    /// to ensure the search functionality works correctly.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use qobuz_downloader_rust::ui::search::SearchPage;
    ///
    /// let mut search_page = SearchPage::new();
    /// // Configure API service and callbacks first
    /// // search_page.set_api_service(service);
    /// // search_page.set_on_download_request(callback);
    /// // search_page.set_on_add_to_queue_request(callback);
    ///
    /// // Then set up search functionality
    /// search_page.setup_search_functionality();
    /// ```
    pub fn setup_search_functionality(&self) {
        let search_page_clone = self.clone();
        self.search_entry.connect_search_started(move |_| {
            search_page_clone.perform_search();
        });

        let search_page_clone = self.clone();
        self.search_entry.connect_activate(move |_| {
            search_page_clone.perform_search();
        });

        let search_page_clone = self.clone();
        self.scope_selector.connect_selected_item_notify(move |_| {
            // Save the selected search scope to settings
            let selected_index = search_page_clone.scope_selector.selected().to_string();
            if let Err(e) = crate::ui::settings::save_search_scope(&selected_index) {
                eprintln!("Failed to save search scope: {}", e);
            }
            search_page_clone.perform_search();
        });

        // Setup the ListView factory
        self.setup_list_view_factory();
    }

    /// Sets the navigation view reference and configures ESC key navigation.
    ///
    /// This method stores a reference to the navigation view and sets up a key press
    /// event handler on the toast overlay that detects when the ESC key is pressed.
    /// When ESC is pressed, the navigation view is popped to return to the previous page.
    ///
    /// # Arguments
    ///
    /// * `navigation_view` - A reference to the NavigationView containing this search page
    pub fn setup_esc_navigation(&mut self, navigation_view: NavigationView) {
        self.navigation_view = Some(navigation_view.clone());

        // Create a key controller for the toast overlay (root widget of search page)
        let key_controller = EventControllerKey::new();
        let nav_view_clone = navigation_view.clone();

        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Escape {
                // Pop the navigation view to go back to the previous page
                nav_view_clone.pop();

                // Return Propagation::Stop to indicate the event was handled
                Stop
            } else {
                Proceed
            }
        });

        // Add the key controller to the search entry (which has focus)
        self.search_entry.add_controller(key_controller);
    }

    /// Sets up the ListView factory for rendering search results with custom UI components.
    ///
    /// This method configures a `SignalListItemFactory` that defines how each search result
    /// item should be displayed in the ListView. It handles both the initial setup of UI
    /// components and the binding of data to those components.
    ///
    /// The factory creates a custom layout for each list item containing:
    /// - A cover art image (64x64 pixels) with placeholder fallback
    /// - An info box with title and subtitle labels
    /// - A SplitButton with "Download" action and "Add to queue" dropdown option
    ///
    /// The method also establishes signal connections for user interactions:
    /// - Download button clicks trigger the download callback
    /// - Queue button clicks trigger the add-to-queue callback
    /// - Asynchronous image loading for cover art with proper error handling
    ///
    /// Data binding uses a custom string format where each result is encoded as:
    /// - Albums: `"ALBUM:{id}|{title}|{artist}|{year}|{duration}|{image_url}|{bit_depth}|{sampling_rate}|{track_count}"`
    /// - Tracks: `"TRACK:{id}|{title}|{artist}|{album}|{duration}|{image_url}|{bit_depth}|{sampling_rate}"`
    fn setup_list_view_factory(&self) {
        let factory = SignalListItemFactory::new();

        factory.connect_setup(move |_, list_item_obj: &Object| {
            let list_item = list_item_obj.downcast_ref::<ListItem>().unwrap();

            let main_box = GtkBox::new(Horizontal, 16);
            main_box.set_margin_top(12);
            main_box.set_margin_bottom(12);
            main_box.set_margin_start(16);
            main_box.set_margin_end(16);

            // Image Setup
            let image = Image::builder()
                .width_request(64)
                .height_request(64)
                .halign(Start)
                .valign(Center)
                .build();

            // Info Box Setup
            let info_box = GtkBox::new(Vertical, 2);
            let title_label = Label::builder()
                .label("Loading...")
                .halign(Start)
                .xalign(0.0)
                .ellipsize(EllipsizeEnd)
                .css_classes(["title-4"])
                .build();
            let subtitle_label = Label::builder()
                .label("")
                .halign(Start)
                .xalign(0.0)
                .ellipsize(EllipsizeEnd)
                .css_classes(["dim-label"])
                .build();

            info_box.append(&title_label);
            info_box.append(&subtitle_label);
            info_box.set_hexpand(true);
            info_box.set_valign(Center);

            // Hi-Res Icon Container
            let hires_icon_container = GtkBox::new(Vertical, 0);
            let hires_icon = Image::builder()
                .halign(Center)
                .valign(Center)
                .visible(false)
                .tooltip_text("Hi-Res Audio")
                .pixel_size(24)
                .build();
            hires_icon_container.append(&hires_icon);
            hires_icon_container.set_halign(End);
            hires_icon_container.set_valign(Center);
            hires_icon_container.set_margin_start(8);

            // Explicit Content Indicator
            let explicit_label = Label::builder()
                .label("E")
                .halign(Center)
                .valign(Center)
                .visible(false)
                .tooltip_text("Explicit Content")
                .css_classes(["error", "caption", "explicit-indicator"])
                .build();
            let explicit_container = GtkBox::new(Vertical, 0);
            explicit_container.append(&explicit_label);
            explicit_container.set_halign(End);
            explicit_container.set_valign(Center);
            explicit_container.set_margin_start(4);

            // SplitButton Setup (Download & Queue)
            let queue_button = Button::builder()
                .label("Add to queue")
                .halign(Start)
                .css_classes(["model"])
                .build();

            let popover_box = GtkBox::new(Vertical, 0);
            popover_box.append(&queue_button);

            let popover = Popover::builder().child(&popover_box).build();

            let split_button = SplitButton::builder()
                .label("Download")
                .tooltip_text("Download")
                .popover(&popover)
                .halign(End)
                .valign(Center)
                .css_classes(["suggested-action"])
                .build();

            main_box.append(&image);
            main_box.append(&info_box);
            main_box.append(&explicit_container);
            main_box.append(&hires_icon_container);
            main_box.append(&split_button);

            list_item.set_child(Some(main_box.upcast_ref::<Widget>()));
        });

        let search_page_clone = self.clone();
        factory.connect_bind(move |_, list_item_obj: &Object| {
            let list_item = list_item_obj.downcast_ref::<ListItem>().unwrap();

            // Early return check for safety
            let item = match list_item.item() {
                Some(i) => i,
                None => return,
            };

            let string_obj = match item.downcast_ref::<StringObject>() {
                Some(s) => s,
                None => return,
            };

            let data: String = string_obj.string().to_string();

            // Parse logic
            let (
                id,
                title,
                artist,
                extra_info,
                image_url,
                is_track,
                bit_depth,
                sampling_rate,
                track_count,
                is_explicit,
            ) = if data.starts_with("ALBUM:") {
                let parts: Vec<&str> = data.splitn(10, "|").collect();
                if parts.len() < 10 {
                    return;
                }
                (
                    parts[0].strip_prefix("ALBUM:").unwrap_or("").to_string(),
                    parts[1],
                    parts[2],
                    parts[3], // Year for albums
                    parts[5],
                    false,
                    parts[6].parse().unwrap_or(0),
                    parts[7].parse::<f64>().unwrap_or(0.0),
                    parts[8].parse().unwrap_or(0),
                    parts[9].parse().unwrap_or(false),
                )
            } else if data.starts_with("TRACK:") {
                let parts: Vec<&str> = data.splitn(9, "|").collect();
                if parts.len() < 9 {
                    return;
                }
                (
                    parts[0].strip_prefix("TRACK:").unwrap_or("").to_string(),
                    parts[1],
                    parts[2],
                    parts[3], // Album for tracks
                    parts[5],
                    true,
                    parts[6].parse().unwrap_or(0),
                    parts[7].parse::<f64>().unwrap_or(0.0),
                    0, // track_count not applicable for tracks
                    parts[8].parse().unwrap_or(false),
                )
            } else {
                return;
            };

            // Build subtitle with all information
            let mut subtitle_parts = vec![artist.to_string(), extra_info.to_string()];

            // Add track count for albums
            if !is_track && track_count > 0 {
                subtitle_parts.push(format!("{} tracks", track_count));
            }

            // Add quality information if available
            let quality_info = if bit_depth > 0 && sampling_rate > 0.0 {
                // Format sampling rate without trailing .0 for whole numbers
                let sampling_rate_str = if sampling_rate.fract() == 0.0 {
                    format!("{:.0}", sampling_rate)
                } else {
                    format!("{:.1}", sampling_rate)
                };
                format!("{}-bit/{}kHz", bit_depth, sampling_rate_str)
            } else {
                String::new()
            };

            if !quality_info.is_empty() {
                subtitle_parts.push(quality_info);
            }

            let subtitle = subtitle_parts.join(" • ");

            // Determine if this is Hi-Res content
            // Hi-Res is defined as bit depth >= 24 OR sampling rate > 48.0 kHz
            let is_hires = (bit_depth >= 24) || (sampling_rate > 48.0);

            if let Some(child) = list_item.child() {
                let main_box = child.downcast_ref::<GtkBox>().unwrap();

                let image_widget = main_box
                    .first_child()
                    .and_then(|w| w.downcast::<Image>().ok());
                let info_box = main_box
                    .first_child()
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.downcast::<GtkBox>().ok());

                // Retrieve the SplitButton (it is the last child now)
                let split_button = main_box
                    .last_child()
                    .and_then(|w| w.downcast::<SplitButton>().ok());

                // Update Labels
                if let Some(info_box) = info_box {
                    if let Some(l1) = info_box
                        .first_child()
                        .and_then(|w| w.downcast::<Label>().ok())
                    {
                        l1.set_label(title);
                    }
                    if let Some(l2) = info_box
                        .first_child()
                        .and_then(|w| w.next_sibling())
                        .and_then(|w| w.downcast::<Label>().ok())
                    {
                        l2.set_label(&subtitle);
                    }
                }

                // Handle Explicit Content Indicator - it's the third child (after image and info_box)
                if let Some(explicit_container) = main_box
                    .first_child()
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.downcast::<GtkBox>().ok())
                    && let Some(explicit_label) = explicit_container
                        .first_child()
                        .and_then(|w| w.downcast::<Label>().ok())
                {
                    explicit_label.set_visible(is_explicit);
                }

                // Handle Hi-Res icon - it's the fourth child (after image, info_box, and explicit_container)
                if let Some(hires_icon_container) = main_box
                    .first_child()
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.next_sibling())
                    .and_then(|w| w.downcast::<GtkBox>().ok())
                    && let Some(hires_icon) = hires_icon_container
                        .first_child()
                        .and_then(|w| w.downcast::<Image>().ok())
                {
                    if is_hires {
                        // Load the custom Hi-Res icon from the project root
                        hires_icon.set_from_file(Some("./assets/hires.png"));
                        hires_icon.set_visible(true);
                    } else {
                        hires_icon.set_visible(false);
                    }
                }

                // Handle Image
                if let Some(image) = image_widget {
                    // Reset to placeholder state immediately
                    image.set_icon_name(Some("audio-x-generic-symbolic"));

                    // Set explicit pixel size (80px) instead of IconSize::Normal (16px)
                    image.set_pixel_size(80);
                    image.set_visible(true);

                    if !image_url.is_empty() {
                        let image_clone = image.clone();
                        let url_clone = image_url.to_string();

                        MainContext::default().spawn_local(async move {
                            if let Some(texture) = load_image_from_url(&url_clone).await {
                                image_clone.set_paintable(Some(&texture));

                                // Ensure pixel size constraint remains 80 so it fills the box
                                image_clone.set_pixel_size(80);
                            } else {
                                // Fallback on failure
                                image_clone.set_icon_name(Some("audio-x-generic-symbolic"));
                                image_clone.set_pixel_size(80);
                            }
                        });
                    }
                }

                // Handle SplitButton Logic
                if let Some(split_btn) = split_button {
                    let id_clone = id.clone();
                    let download_type = if is_track {
                        DownloadType::Track(id_clone)
                    } else {
                        DownloadType::Album(id_clone)
                    };

                    // 1. Connect the main "Download" action
                    let search_page_clone1 = search_page_clone.clone();
                    let download_type1 = download_type.clone();

                    split_btn.connect_clicked(move |_| {
                        search_page_clone1.handle_download_click(download_type1.clone());
                    });

                    // 2. Retrieve the internal "Add to Queue" button to connect its signal
                    // Hierarchy: SplitButton -> Popover -> Box -> Button
                    if let Some(popover) = split_btn.popover() {
                        // FIX: Explicitly specify |c: Widget| to help the compiler infer types
                        if let Some(box_widget) = popover
                            .child()
                            .and_then(|c: Widget| c.downcast::<GtkBox>().ok())
                            && let Some(queue_btn) = box_widget
                                .first_child()
                                .and_then(|c: Widget| c.downcast::<Button>().ok())
                        {
                            let search_page_clone2 = search_page_clone.clone();
                            let download_type2 = download_type.clone();
                            let popover_clone = popover.clone();

                            queue_btn.connect_clicked(move |_| {
                                // Perform action
                                search_page_clone2
                                    .handle_add_to_queue_click(download_type2.clone());

                                // Close the menu immediately
                                popover_clone.popdown();
                            });
                        }
                    }
                }
            }
        });

        self.results_list.set_factory(Some(&factory));
    }

    /// Retrieves the currently selected search scope from the UI dropdown.
    ///
    /// This method reads the selected index from the scope selector dropdown
    /// and maps it to the corresponding `SearchScope` enum variant:
    /// - Index 0 → `SearchScope::All`
    /// - Index 1 → `SearchScope::Albums`
    /// - Index 2 → `SearchScope::Tracks`
    ///
    /// If an unexpected index is encountered (which shouldn't happen with the
    /// current UI setup), it defaults to `SearchScope::All`.
    ///
    /// # Returns
    ///
    /// Returns the `SearchScope` corresponding to the currently selected dropdown option.
    fn get_current_scope(&self) -> SearchScope {
        // Use the selected index to map back to the enum
        match self.scope_selector.selected() {
            0 => SearchScope::All,
            1 => SearchScope::Albums,
            2 => SearchScope::Tracks,
            _ => SearchScope::All, // Fallback
        }
    }

    /// Executes a search query against the Qobuz API based on current UI state.
    ///
    /// This method performs the complete search workflow:
    /// 1. Validates that the search query is not empty
    /// 2. Checks if a search is already in progress to prevent concurrent requests
    /// 3. Sets the loading state and displays a persistent loading toast
    /// 4. Executes the asynchronous search using the configured API service
    /// 5. Handles various error conditions with appropriate user feedback
    /// 6. Displays results or error messages upon completion
    ///
    /// The method respects the currently selected search scope and query text
    /// from the UI components.
    fn perform_search(&self) {
        let query = self.search_entry.text().to_string();
        if query.trim().is_empty() {
            // Clear results on empty query, but don't show an error toast if scope changes to trigger this
            self.list_model.remove_all();
            self.show_info_toast("Enter a search query");
            return;
        }

        let scope = self.get_current_scope();
        let entity = scope.to_api_query();

        // 1. Check if a search is already running. If so, ignore the redundant event.
        if *self.is_loading.borrow() {
            return;
        }

        // 2. Set the loading state immediately.
        *self.is_loading.borrow_mut() = true;

        // Show loading feedback
        let scope_str = match scope {
            SearchScope::All => "All",
            SearchScope::Albums => "Albums",
            SearchScope::Tracks => "Tracks",
        };
        let loading_toast = Toast::new(&format!("Searching {}...", scope_str));
        loading_toast.set_timeout(0); // Persistent until dismissed
        self.toast_overlay.add_toast(loading_toast.clone());

        let search_page_clone = self.clone();
        let query_clone = query.clone();

        MainContext::default().spawn_local(async move {
            let result_action = if let Some(service) = &search_page_clone.api_service {
                // Fixed API call: Passing the correct entity filter
                service
                    .search_catalog(&query_clone, Some(20), None, entity, None)
                    .await
            } else {
                // 3. Cleanup: Dismiss toast and reset loading flag
                search_page_clone.toast_overlay.dismiss_all();
                *search_page_clone.is_loading.borrow_mut() = false;
                search_page_clone.show_error_toast("API service not initialized");
                return; // Exit the async block early
            };

            // 4. Cleanup: Dismiss toast and reset loading flag immediately after await
            search_page_clone.toast_overlay.dismiss_all();
            *search_page_clone.is_loading.borrow_mut() = false;

            match result_action {
                Ok(search_result) => {
                    search_page_clone.display_search_results(search_result);
                }

                Err(e) => {
                    let error_message = match e {
                        AuthenticationError { .. } => {
                            "Authentication required. Please log in again.".to_string()
                        }

                        HttpError(_) => "Network error. Please check your connection.".to_string(),

                        ApiErrorResponse { message, .. } => {
                            format!("Search failed: {}", message)
                        }

                        _ => format!("Search failed: {}", e),
                    };
                    search_page_clone.show_error_toast(&error_message);
                }
            }
        });
    }

    /// Processes and displays search results from the Qobuz API.
    ///
    /// This method takes a `SearchResult` from the Qobuz API and transforms it into
    /// a format suitable for display in the ListView. It handles both albums and tracks,
    /// extracting relevant metadata, cleaning HTML entities, formatting durations,
    /// and selecting appropriate cover art URLs.
    ///
    /// The method uses a custom string format to encode search results:
    /// - Albums: `"ALBUM:{id}|{title}|{artist}|{year}|{duration}|{image_url}|{bit_depth}|{sampling_rate}|{track_count}"`
    /// - Tracks: `"TRACK:{id}|{title}|{artist}|{album}|{duration}|{image_url}|{bit_depth}|{sampling_rate}"`
    ///
    /// This encoded format is then stored in `StringObject` instances for the ListView.
    ///
    /// # Arguments
    ///
    /// * `search_result` - The `SearchResult` containing albums and/or tracks from the Qobuz API
    fn display_search_results(&self, search_result: SearchResult) {
        let mut results_data = Vec::new();

        // Process albums
        if let Some(albums) = search_result.albums
            && let Some(items) = albums.items
        {
            for album in items {
                if let Some(id) = &album.id
                    && let Some(title) = &album.title
                {
                    // Clean title to remove HTML entities and pipes
                    let safe_title = unescape_html(title);

                    let artist_name = album
                        .artist
                        .as_ref()
                        .and_then(|a| a.name.as_ref())
                        .map(|n| unescape_html(n))
                        .unwrap_or_else(|| "Unknown Artist".to_string());

                    let release_date = if let Some(released_at) = album.released_at {
                        let (date_str, _) = timestamp_to_date_and_year(released_at);
                        date_str.unwrap_or_else(|| "Unknown".to_string())
                    } else {
                        "Unknown".to_string()
                    };

                    let duration_str = if let Some(duration) = album.duration {
                        format_duration(duration)
                    } else {
                        "Unknown".to_string()
                    };

                    // Extract audio quality information
                    let bit_depth = get_album_bit_depth(&album);
                    let sampling_rate = get_album_sampling_rate(&album);
                    let track_count = if let Some(ref tracks_data) = album.tracks {
                        tracks_data
                            .items
                            .as_ref()
                            .map(|items| items.len())
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    // Extract explicit content indicator
                    let is_explicit = album.parental_warning.unwrap_or(false);

                    // Get cover art URL - use the largest available image
                    let image_url = if let Some(image) = &album.image {
                        image
                            .extralarge
                            .as_ref()
                            .or(image.large.as_ref())
                            .or(image.medium.as_ref())
                            .or(image.small.as_ref())
                            .or(image.thumbnail.as_ref())
                            .or(image.back.as_ref())
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    let data = format!(
                        "ALBUM:{}|{}|{}|{}|{}|{}|{}|{:.1}|{}|{}",
                        id,
                        safe_title,
                        artist_name,
                        release_date,
                        duration_str,
                        image_url,
                        bit_depth,
                        sampling_rate,
                        track_count,
                        is_explicit
                    );
                    results_data.push(data);
                }
            }
        }

        // Process tracks
        if let Some(tracks) = search_result.tracks
            && let Some(items) = tracks.items
        {
            for track in items {
                if let Some(id) = track.id
                    && let Some(title) = &track.title
                {
                    // Clean title to remove HTML entities and pipes
                    let safe_title = unescape_html(title);

                    let performer_name = track
                        .performer
                        .as_ref()
                        .and_then(|p| p.name.as_ref())
                        .map(|n| unescape_html(n))
                        .unwrap_or_else(|| "Unknown Artist".to_string());

                    let album_name = track
                        .album
                        .as_ref()
                        .and_then(|a| a.title.as_ref())
                        .map(|n| unescape_html(n))
                        .unwrap_or_else(|| "Unknown Album".to_string());

                    let duration_str = if let Some(duration) = track.duration {
                        format_duration(duration)
                    } else {
                        "Unknown".to_string()
                    };

                    // Extract audio quality information
                    let bit_depth = get_track_bit_depth(&track);
                    let sampling_rate = get_track_sampling_rate(&track);

                    // Extract explicit content indicator
                    let is_explicit = track.parental_warning.unwrap_or(false);

                    // Get cover art URL from track's album
                    let image_url = if let Some(album) = &track.album
                        && let Some(image) = &album.image
                    {
                        image
                            .extralarge
                            .as_ref()
                            .or(image.large.as_ref())
                            .or(image.medium.as_ref())
                            .or(image.small.as_ref())
                            .or(image.thumbnail.as_ref())
                            .or(image.back.as_ref())
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    let data = format!(
                        "TRACK:{}|{}|{}|{}|{}|{}|{}|{:.1}|{}",
                        id,
                        safe_title,
                        performer_name,
                        album_name,
                        duration_str,
                        image_url,
                        bit_depth,
                        sampling_rate,
                        is_explicit
                    );
                    results_data.push(data);
                }
            }
        }

        // Create StringObject list for ListView
        let string_objects: Vec<StringObject> = results_data
            .into_iter()
            .map(|data| StringObject::new(&data))
            .collect();

        // Clear existing items
        self.list_model.remove_all();
        // Add new items
        for string_obj in string_objects {
            self.list_model.append(&string_obj);
        }

        let n_items = self.list_model.n_items();
        if n_items == 0 {
            self.show_info_toast("No results found");
        } else {
            self.show_success_toast(&format!("Found {} results", n_items));
        }
    }

    /// Handles download button clicks from search results.
    ///
    /// This method is called when a user clicks the "Download" button on a search
    /// result item. It executes the configured download callback if available,
    /// or displays a fallback informational toast if no callback is set.
    ///
    /// # Arguments
    ///
    /// * `download_type` - The `DownloadType` indicating what content the user wants to download
    fn handle_download_click(&self, download_type: DownloadType) {
        // Call the download callback if set
        if let Some(callback) = &*self.on_download_request.borrow() {
            callback(download_type.clone());
        } else {
            match &download_type {
                DownloadType::Album(id) => {
                    self.show_info_toast(&format!("Would download album: {}", id));
                }
                DownloadType::Track(id) => {
                    self.show_info_toast(&format!("Would download track: {}", id));
                }
            }
        }
    }

    /// Handles "Add to Queue" button clicks from search results.
    ///
    /// This method is called when a user selects "Add to Queue" from a search result
    /// item's dropdown menu. It executes the configured queue callback if available,
    /// or displays a fallback informational toast if no callback is set.
    ///
    /// Note that when a callback is successfully executed, no toast is shown here
    /// because the main window typically handles the toast notification for queue operations.
    ///
    /// # Arguments
    ///
    /// * `download_type` - The `DownloadType` indicating what content the user wants to add to the queue
    fn handle_add_to_queue_click(&self, download_type: DownloadType) {
        // Call the add to queue callback if set
        if let Some(callback) = &*self.on_add_to_queue_request.borrow() {
            callback(download_type.clone());
            // Don't show toast here - main window handles the toast notification
        } else {
            // Fallback behavior if no callback is set
            match &download_type {
                DownloadType::Album(id) => {
                    self.show_info_toast(&format!("Would add album {} to queue", id));
                }
                DownloadType::Track(id) => {
                    self.show_info_toast(&format!("Would add track {} to queue", id));
                }
            }
        }
    }

    /// Displays an error toast notification to the user.
    ///
    /// Creates a temporary toast message with the specified error text that
    /// automatically disappears after a short duration.
    ///
    /// # Arguments
    ///
    /// * `message` - The error message text to display
    fn show_error_toast(&self, message: &str) {
        let toast = Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    /// Displays an informational toast notification to the user.
    ///
    /// Creates a temporary toast message with the specified informational text that
    /// automatically disappears after 3 seconds.
    ///
    /// # Arguments
    ///
    /// * `message` - The informational message text to display
    fn show_info_toast(&self, message: &str) {
        let toast = Toast::new(message);
        toast.set_timeout(3000); // 3 seconds
        self.toast_overlay.add_toast(toast);
    }

    /// Displays a success toast notification to the user.
    ///
    /// Creates a temporary toast message with the specified success text that
    /// automatically disappears after a short duration.
    ///
    /// # Arguments
    ///
    /// * `message` - The success message text to display
    fn show_success_toast(&self, message: &str) {
        let toast = Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }
}

/// Formats a duration in seconds into a human-readable MM:SS string.
///
/// This helper function converts a duration given in seconds into a formatted
/// string with minutes and seconds, ensuring that seconds are always displayed
/// with two digits (e.g., "3:05" instead of "3:5").
///
/// # Arguments
///
/// * `seconds` - The duration in seconds to format
///
/// # Returns
///
/// Returns a formatted string in the format "MM:SS".
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::search::format_duration;
///
/// assert_eq!(format_duration(65), "1:05");
/// assert_eq!(format_duration(120), "2:00");
/// assert_eq!(format_duration(7), "0:07");
/// ```
fn format_duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    format!("{}:{:02}", minutes, remaining_seconds)
}

/// Decodes common HTML entities found in Qobuz API responses to plain text.
///
/// This function handles the most common HTML entities that appear in Qobuz
/// metadata and converts them to their plain text equivalents. It also replaces
/// pipe characters (`|`) with " - " to prevent conflicts with the internal
/// data storage format used by the ListView factory.
///
/// The function handles the following HTML entities:
/// - `&` → `&`
/// - `<` → `<`
/// - `>` → `>`
/// - `"` → `"`
/// - `&#039;` and `'` → `'`
///
/// # Arguments
///
/// * `text` - The input string potentially containing HTML entities
///
/// # Returns
///
/// Returns a cleaned string with HTML entities decoded and pipe characters replaced.
///
/// # Examples
///
/// ```rust
/// use qobuz_downloader_rust::ui::search::unescape_html;
///
/// assert_eq!(unescape_html("Artist & Band"), "Artist & Band");
/// assert_eq!(unescape_html("Song Title | Album"), "Song Title  -  Album");
/// assert_eq!(unescape_html("Don&#039;t Stop"), "Don't Stop");
/// ```
fn unescape_html(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("|", " - ") // Replace pipes to prevent string parsing errors in factory logic
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
/// use libadwaita::gtk::glib::MainContext;
/// use qobuz_downloader_rust::ui::search::load_image_from_url;
///
/// // In an async context with MainContext
/// MainContext::default().spawn_local(async move {
///     if let Some(texture) = load_image_from_url("https://example.com/cover.jpg").await {
///         // Use the texture
///     }
/// });
/// ```
async fn load_image_from_url(url: &str) -> Option<Texture> {
    if url.is_empty() {
        return None;
    }

    // Validate URL format
    if !url.starts_with("http") {
        return None;
    }

    let file = File::for_uri(url);

    match file.load_bytes_future().await {
        Ok((bytes, _)) => Texture::from_bytes(&bytes).ok(),
        Err(_) => None,
    }
}

/// Helper function to safely extract bit depth from an album.
///
/// This function provides a safe way to access the maximum_bit_depth field
/// from Qobuz API album data, handling cases where the field might not be
/// available or the API structure changes.
fn get_album_bit_depth(album: &Album) -> u32 {
    album.maximum_bit_depth.map(|v| v as u32).unwrap_or(0)
}

/// Helper function to safely extract sampling rate from an album.
///
/// This function provides a safe way to access the maximum_sampling_rate field
/// from Qobuz API album data, handling cases where the field might not be
/// available or the API structure changes.
/// Note: The sampling rate is stored as kHz (e.g., 44.1 for 44.1kHz)
fn get_album_sampling_rate(album: &Album) -> f64 {
    album.maximum_sampling_rate.unwrap_or(0.0)
}

/// Helper function to safely extract bit depth from a track.
///
/// This function provides a safe way to access the maximum_bit_depth field
/// from Qobuz API track data, handling cases where the field might not be
/// available or the API structure changes.
fn get_track_bit_depth(track: &Track) -> u32 {
    track.maximum_bit_depth.map(|v| v as u32).unwrap_or(0)
}

/// Helper function to safely extract sampling rate from a track.
///
/// This function provides a safe way to access the maximum_sampling_rate field
/// from Qobuz API track data, handling cases where the field might not be
/// available or the API structure changes.
/// Note: The sampling rate is stored as kHz (e.g., 44.1 for 44.1kHz)
fn get_track_sampling_rate(track: &Track) -> f64 {
    track.maximum_sampling_rate.unwrap_or(0.0)
}

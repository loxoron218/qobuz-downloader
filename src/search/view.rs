//! Search view UI matching the original implementation: `SearchEntry`, scope selector,
//! Hi-Res and explicit content indicators, inline `SplitButton` for download/queue actions.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
    thread::spawn,
};

use {
    async_channel::{Receiver, SendError, Sender, unbounded},
    libadwaita::{
        HeaderBar, NavigationView, SplitButton, Toast, ToastOverlay, ToolbarView,
        gdk::{Key, Texture},
        glib::{
            MainContext,
            Propagation::{self, Proceed, Stop},
            object::Cast,
        },
        gtk::{
            Align::{Center, End, Start},
            Box, Button, DropDown, EventControllerKey, GestureClick, Image, Label, ListBox,
            ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture,
            PolicyType::Automatic,
            Popover,
            PropagationPhase::Capture,
            ScrolledWindow, SearchEntry,
            SelectionMode::Single,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{
            BoxExt, ButtonExt, EditableExt, EventControllerExt, GestureSingleExt, ListBoxRowExt,
            PopoverExt, WidgetExt,
        },
    },
    num_traits::AsPrimitive,
    parking_lot::Mutex,
    qobuz_api::{api::service::QobuzApiService, models::search::SearchResult},
    tracing::warn,
};

use crate::{
    app::AppState,
    browse::{
        BrowseEvent, browse_album, browse_artist, browse_playlist,
        detail_common::resolve_thumbnail_url,
    },
    cover_art::cache::CoverArtCache,
    download::progress::{DownloadCommand, DownloadItem, DownloadTask},
    preferences::settings::AppSettings,
    search::controller::{
        SearchController,
        SearchEvent::{self, Error, Results},
        SearchScope::{self, All},
    },
    ui::debounce_elapsed,
};

/// Target for double-click activation navigation.
enum ActivationTarget {
    /// Browse album detail.
    Album(String),
    /// Browse artist detail.
    Artist(i32),
    /// Browse playlist detail.
    Playlist(String),
}

/// A picture widget awaiting an asynchronously fetched cover art.
struct AsyncCoverFetch {
    /// The picture widget to update once the cover is fetched.
    picture: Picture,
    /// The entity id (artist or playlist) whose cover is fetched.
    id: String,
    /// Whether the id refers to an artist (vs. a playlist).
    is_artist: bool,
}

/// Category of a search result for section grouping.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SearchCategory {
    /// Track results.
    Tracks,
    /// Album results.
    Albums,
    /// Artist results.
    Artists,
    /// Playlist results.
    Playlists,
}

impl SearchCategory {
    /// Returns the display label for this category.
    const fn label(self) -> &'static str {
        match self {
            Self::Tracks => "Tracks",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }

    /// Returns the corresponding search scope for this category.
    const fn to_scope(self) -> SearchScope {
        match self {
            Self::Tracks => SearchScope::Tracks,
            Self::Albums => SearchScope::Albums,
            Self::Artists => SearchScope::Artists,
            Self::Playlists => SearchScope::Playlists,
        }
    }
}

/// Shared context passed through search result processing.
struct SearchCtx {
    /// Results list box.
    list_box: ListBox,
    /// Search result items vector.
    items: Arc<Mutex<Vec<SearchResultItem>>>,
    /// Cover art texture cache.
    cover_art_cache: CoverArtCache,
    /// Channel sender for loaded cover art textures.
    texture_sender: Sender<(String, Option<Texture>)>,
    /// Channel sender for cover-art load/registration events.
    texture_register: Sender<TextureEvent>,
    /// Toast overlay for search feedback.
    toast_overlay: ToastOverlay,
    /// Whether a search is currently in progress.
    is_loading: Arc<AtomicBool>,
    /// User settings for default quality and output directory.
    settings: Arc<Mutex<AppSettings>>,
    /// Channel sender for download commands.
    cmd_sender: Sender<DownloadCommand>,
    /// Shared API client for fetching artist/playlist details.
    api_service: Arc<Mutex<QobuzApiService>>,
    /// Current search scope.
    scope: Arc<Mutex<SearchScope>>,
    /// Scope selector dropdown for category navigation.
    scope_selector: DropDown,
}

/// Structured search result item with full display data.
#[derive(Clone, Debug)]
enum SearchResultItem {
    /// Track result.
    Track {
        /// Track ID.
        id: i32,
        /// Track title.
        title: String,
        /// Artist name.
        artist: String,
        /// Album name.
        album: String,
        /// Cover art thumbnail URL.
        cover_url: Option<String>,
        /// Duration in seconds.
        duration: i32,
        /// Audio bit depth.
        bit_depth: i32,
        /// Audio sampling rate in kHz.
        sampling_rate: f64,
        /// Explicit content flag.
        is_explicit: bool,
    },
    /// Album result.
    Album {
        /// Album ID.
        id: String,
        /// Album title.
        title: String,
        /// Artist name.
        artist: String,
        /// Cover art thumbnail URL.
        cover_url: Option<String>,
        /// Total duration in seconds.
        duration: i32,
        /// Maximum audio bit depth.
        bit_depth: i32,
        /// Maximum audio sampling rate in kHz.
        sampling_rate: f64,
        /// Number of tracks.
        track_count: i32,
        /// Release year.
        year: String,
        /// Explicit content flag.
        is_explicit: bool,
    },
    /// Artist result.
    Artist {
        /// Artist ID.
        id: i32,
        /// Artist name.
        name: String,
        /// Cover art thumbnail URL.
        cover_url: Option<String>,
    },
    /// Playlist result.
    Playlist {
        /// Playlist ID.
        id: String,
        /// Playlist name.
        name: String,
        /// Cover art thumbnail URL.
        cover_url: Option<String>,
        /// Explicit content flag.
        is_explicit: bool,
    },
}

/// Widgets from the search view needed for external event handling.
#[derive(Clone)]
pub struct SearchWidgets {
    /// Root container widget.
    pub root: ToolbarView,
    /// Search entry widget.
    pub search_entry: SearchEntry,
    /// Scope selector dropdown for category navigation.
    scope_selector: DropDown,
    /// Current search scope.
    scope: Arc<Mutex<SearchScope>>,
    /// Header bar for custom back button.
    header_bar: HeaderBar,
}

impl SearchWidgets {
    /// Sets up ESC key and back button navigation to pop the `NavigationView` or reset scope.
    pub fn setup_esc_navigation(&self, navigation_view: &NavigationView) {
        let key_controller = EventControllerKey::new();
        let nav_view = navigation_view.clone();
        let scope_selector = self.scope_selector.clone();
        let scope = Arc::clone(&self.scope);

        key_controller.set_propagation_phase(Capture);
        key_controller.connect_key_pressed(move |_, key, _, _| {
            handle_key_pressed(key, &scope, &nav_view, &scope_selector)
        });

        self.root.add_controller(key_controller);

        let back_button = Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back")
            .build();

        let nav_view_clone = navigation_view.clone();
        let back_scope = Arc::clone(&self.scope);
        let back_selector = self.scope_selector.clone();
        back_button.connect_clicked(move |_| {
            handle_back_clicked(&back_scope, &nav_view_clone, &back_selector);
        });

        self.header_bar.pack_start(&back_button);
    }
}

/// Cover art load and registration events for the texture receiver.
enum TextureEvent {
    /// A picture widget registered itself for a cover URL.
    Register {
        /// The cover URL.
        url: String,
        /// The picture widget awaiting the cover.
        picture: Picture,
    },
    /// A cover art texture finished loading for a URL.
    Loaded {
        /// The cover URL.
        url: String,
        /// The loaded texture, if any.
        texture: Option<Texture>,
    },
}

/// Handles ESC key press to pop the navigation view or reset the search scope.
fn handle_key_pressed(
    key: Key,
    scope: &Arc<Mutex<SearchScope>>,
    nav_view: &NavigationView,
    scope_selector: &DropDown,
) -> Propagation {
    if key != Key::Escape {
        return Proceed;
    }
    if *scope.lock() == All {
        nav_view.pop();
        return Stop;
    }
    scope_selector.set_selected(0);
    Stop
}

/// Handles back button click to pop the navigation view or reset the search scope.
fn handle_back_clicked(
    scope: &Arc<Mutex<SearchScope>>,
    nav_view: &NavigationView,
    scope_selector: &DropDown,
) {
    if *scope.lock() == All {
        nav_view.pop();
        return;
    }
    scope_selector.set_selected(0);
}

/// Builds the search scaffold UI.
fn build_search_scaffold(
    saved_scope: u32,
) -> (
    ToolbarView,
    SearchEntry,
    DropDown,
    ListBox,
    ToastOverlay,
    HeaderBar,
) {
    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();

    let title_label = Label::new(Some("Search"));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    toolbar.add_top_bar(&header);

    let toast_overlay = ToastOverlay::new();

    let scope_selector =
        DropDown::from_strings(&["All", "Albums", "Tracks", "Artists", "Playlists"]);
    scope_selector.set_selected(saved_scope);

    let search_entry = SearchEntry::builder()
        .placeholder_text("Search for albums or tracks...")
        .hexpand(true)
        .build();

    let header_box = Box::new(Horizontal, 8);
    header_box.set_margin_top(16);
    header_box.set_margin_bottom(16);
    header_box.set_margin_start(16);
    header_box.set_margin_end(16);
    header_box.append(&scope_selector);
    header_box.append(&search_entry);

    let list_box = ListBox::new();
    list_box.set_selection_mode(Single);
    list_box.set_vexpand(true);
    list_box.add_css_class("rich-list");

    let results_scrolled = ScrolledWindow::new();
    results_scrolled.set_policy(Automatic, Automatic);
    results_scrolled.set_min_content_height(400);
    results_scrolled.set_vexpand(true);
    results_scrolled.set_child(Some(&list_box));

    let content_box = Box::new(Vertical, 0);
    content_box.append(&header_box);
    content_box.append(&results_scrolled);

    toast_overlay.set_child(Some(&content_box));
    toolbar.set_content(Some(&toast_overlay));

    (
        toolbar,
        search_entry,
        scope_selector,
        list_box,
        toast_overlay,
        header,
    )
}

/// Sets up double-click gesture handler to browse detail views.
fn setup_results_activation(
    list_box: &ListBox,
    items: &Arc<Mutex<Vec<SearchResultItem>>>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    browse_sender: Sender<BrowseEvent>,
    toast_overlay: &ToastOverlay,
) {
    let list_box_owned = list_box.clone();
    let items_for_gesture = Arc::clone(items);
    let api_for_gesture = Arc::clone(api_service);
    let sender_for_gesture = browse_sender.clone();
    let toast_for_gesture = toast_overlay.clone();
    let gesture = GestureClick::new();
    gesture.set_button(1);
    let last_nav_ms: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    gesture.connect_pressed(move |_, n_press, _x, y| {
        if n_press != 2 {
            return;
        }
        if !debounce_elapsed(&last_nav_ms, 500) {
            return;
        }
        let row = list_box_owned.row_at_y(y.as_());
        let Some(row) = row else {
            return;
        };
        let position = row.index().unsigned_abs();
        let items_ref = items_for_gesture.lock();
        let target = find_item_at_position(&items_ref, position);
        drop(items_ref);
        let Some(target) = target else {
            return;
        };
        match target {
            ActivationTarget::Album(id) => {
                let toast = Toast::new("Opening album…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_album(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
            ActivationTarget::Artist(id) => {
                let toast = Toast::new("Opening artist…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_artist(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
            ActivationTarget::Playlist(id) => {
                let toast = Toast::new("Opening playlist…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_playlist(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
        }
    });
    list_box.add_controller(gesture);

    let list_box_for_key = list_box.clone();
    let items_for_key = Arc::clone(items);
    let api_for_key = Arc::clone(api_service);
    let browse_for_key = browse_sender;
    let toast_for_key = toast_overlay.clone();
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key != Key::Return && key != Key::KP_Enter {
            return Proceed;
        }
        let row = list_box_for_key.selected_row();
        let Some(row) = row else {
            return Proceed;
        };
        let position = row.index().unsigned_abs();
        let items_ref = items_for_key.lock();
        let target = find_item_at_position(&items_ref, position);
        drop(items_ref);
        let Some(target) = target else {
            return Proceed;
        };
        match target {
            ActivationTarget::Album(id) => {
                let toast = Toast::new("Opening album…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_album(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
            ActivationTarget::Artist(id) => {
                let toast = Toast::new("Opening artist…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_artist(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
            ActivationTarget::Playlist(id) => {
                let toast = Toast::new("Opening playlist…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_playlist(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
        }
        Proceed
    });
    list_box.add_controller(key_controller);
}

/// Sets up scope selector to re-trigger search with new scope.
fn setup_scope_selector(
    scope_selector: &DropDown,
    controller: &SearchController,
    search_entry: &SearchEntry,
    search_sender: Sender<SearchEvent>,
    scope: Arc<Mutex<SearchScope>>,
    settings: Arc<Mutex<AppSettings>>,
) {
    let scope_controller = controller.clone();
    let scope_entry = search_entry.clone();

    scope_selector.connect_selected_item_notify(move |widget| {
        let idx = widget.selected();
        let new_scope = SearchScope::from_u32(idx);
        *scope.lock() = new_scope;

        let mut s = settings.lock();
        s.search_scope = idx;
        drop(s);

        if !scope_entry.text().trim().is_empty() {
            scope_controller.search_scoped(
                scope_entry.text().as_ref(),
                new_scope,
                search_sender.clone(),
            );
        }
    });
}

/// Builds the search view UI and returns the root widget and widget references.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `cmd_sender` - Channel sender for download commands
/// * `browse_sender` - Channel sender for browse events (album navigation)
pub fn build(
    state: &AppState,
    cmd_sender: Sender<DownloadCommand>,
    browse_sender: Sender<BrowseEvent>,
) -> SearchWidgets {
    let controller = SearchController::new(Arc::clone(&state.api_service));
    let (search_sender, search_receiver) = unbounded::<SearchEvent>();

    let saved_scope = state.settings.lock().search_scope;
    let (toolbar, search_entry, scope_selector, list_box, toast_overlay, header_bar) =
        build_search_scaffold(saved_scope);

    let (texture_sender, texture_receiver) = unbounded::<(String, Option<Texture>)>();
    let (texture_register, texture_event_receiver) = unbounded::<TextureEvent>();
    let texture_forward = texture_register.clone();
    MainContext::default().spawn_local(async move {
        while let Ok((url, texture)) = texture_receiver.recv().await {
            let result = texture_forward
                .send(TextureEvent::Loaded { url, texture })
                .await;
            log_texture_send_error(result);
        }
    });
    setup_texture_receiver(texture_event_receiver);

    let items: Arc<Mutex<Vec<SearchResultItem>>> = Arc::new(Mutex::new(Vec::new()));
    let is_loading: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let cover_art_cache = state.cover_art_cache.clone();

    let scope: Arc<Mutex<SearchScope>> = Arc::new(Mutex::new(SearchScope::from_u32(saved_scope)));

    let list_box_for_activation = list_box.clone();

    let ctx = SearchCtx {
        list_box,
        items: Arc::clone(&items),
        cover_art_cache,
        texture_sender,
        texture_register,
        toast_overlay: toast_overlay.clone(),
        is_loading: Arc::clone(&is_loading),
        settings: Arc::clone(&state.settings),
        cmd_sender,
        api_service: Arc::clone(&state.api_service),
        scope: Arc::clone(&scope),
        scope_selector: scope_selector.clone(),
    };

    connect_search_entry(
        &search_entry,
        &controller,
        search_sender.clone(),
        &is_loading,
        &toast_overlay,
        &scope,
    );
    setup_search_receiver(search_receiver, ctx);

    setup_results_activation(
        &list_box_for_activation,
        &items,
        &state.api_service,
        browse_sender,
        &toast_overlay,
    );
    let sw_scope = Arc::clone(&scope);

    setup_scope_selector(
        &scope_selector,
        &controller,
        &search_entry,
        search_sender,
        scope,
        Arc::clone(&state.settings),
    );

    SearchWidgets {
        root: toolbar,
        search_entry,
        scope_selector,
        scope: sw_scope,
        header_bar,
    }
}

/// Pushes formatted duration and quality string parts if non-empty.
fn push_quality_parts(parts: &mut Vec<String>, duration: i32, bit_depth: i32, sampling_rate: f64) {
    if duration > 0 {
        parts.push(format_duration(duration));
    }
    let q = quality_string(bit_depth, sampling_rate);
    if !q.is_empty() {
        parts.push(q);
    }
}

/// Formats a subtitle string for a search result item.
fn build_subtitle(item: &SearchResultItem) -> String {
    match item {
        SearchResultItem::Track {
            artist,
            album,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone(), album.clone()];
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        SearchResultItem::Album {
            artist,
            year,
            track_count,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone()];
            if !year.is_empty() {
                parts.push(year.clone());
            }
            if *track_count > 0 {
                parts.push(format!("{track_count} tracks"));
            }
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        SearchResultItem::Artist { .. } => String::from("Artist"),
        SearchResultItem::Playlist { .. } => String::from("Playlist"),
    }
}

/// Connects search entry signals to trigger scoped searches.
fn connect_search_entry(
    entry: &SearchEntry,
    controller: &SearchController,
    sender: Sender<SearchEvent>,
    is_loading: &Arc<AtomicBool>,
    toast_overlay: &ToastOverlay,
    scope: &Arc<Mutex<SearchScope>>,
) {
    let controller_search_started = controller.clone();
    let sender_clone = sender.clone();
    let is_loading_clone = Arc::clone(is_loading);
    let toast_overlay_clone = toast_overlay.clone();
    let scope_clone = Arc::clone(scope);

    entry.connect_search_started(move |entry| {
        let query = entry.text().to_string();
        if query.trim().is_empty() {
            return;
        }
        let s = *scope_clone.lock();
        trigger_search(
            &controller_search_started,
            &query,
            &sender_clone,
            &is_loading_clone,
            &toast_overlay_clone,
            s,
        );
    });

    let controller_activate = controller.clone();
    let is_loading_activate = Arc::clone(is_loading);
    let toast_overlay_activate = toast_overlay.clone();
    let scope_activate = Arc::clone(scope);

    entry.connect_activate(move |entry| {
        let query = entry.text().to_string();
        if query.trim().is_empty() {
            return;
        }
        let s = *scope_activate.lock();
        trigger_search(
            &controller_activate,
            &query,
            &sender,
            &is_loading_activate,
            &toast_overlay_activate,
            s,
        );
    });
}

/// Creates a data row for a search result item.
///
/// Returns the row and, for artist/playlist items lacking cover art, the info needed
/// to fetch and apply their cover asynchronously.
fn create_data_row(
    item: &SearchResultItem,
    ctx: &SearchCtx,
) -> (ListBoxRow, Option<AsyncCoverFetch>) {
    let picture = Picture::new();
    picture.set_size_request(64, 64);
    picture.add_css_class("thumbnail");
    picture.set_tooltip_text(Some("Cover art"));

    let row_box = Box::new(Horizontal, 16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);

    row_box.append(&picture);
    row_box.append(&build_info_box(item));
    row_box.append(&build_explicit_indicator(item));
    row_box.append(&build_hires_indicator(item));
    row_box.append(&create_split_button(item, ctx));

    attach_cover_art(item, &picture, ctx);

    let pending_fetch = match item {
        SearchResultItem::Artist { id, cover_url, .. } if cover_url.is_none() => {
            Some(AsyncCoverFetch {
                picture,
                id: id.to_string(),
                is_artist: true,
            })
        }
        SearchResultItem::Playlist { id, cover_url, .. } if cover_url.is_none() => {
            Some(AsyncCoverFetch {
                picture,
                id: id.clone(),
                is_artist: false,
            })
        }
        _ => None,
    };

    let row = ListBoxRow::new();
    row.set_child(Some(&row_box));

    (row, pending_fetch)
}

/// Attaches cover art texture to the picture widget.
fn attach_cover_art(item: &SearchResultItem, picture: &Picture, ctx: &SearchCtx) {
    let url = match item {
        SearchResultItem::Track { cover_url, .. }
        | SearchResultItem::Album { cover_url, .. }
        | SearchResultItem::Playlist { cover_url, .. }
        | SearchResultItem::Artist { cover_url, .. } => cover_url.clone(),
    };

    if let Some(url) = url {
        if let Some(texture) = ctx.cover_art_cache.get(&url) {
            picture.set_paintable(Some(&texture));
        } else {
            register_cover_texture(&ctx.texture_register, url.clone(), picture.clone());
            ctx.cover_art_cache
                .start_load(url, ctx.texture_sender.clone());
        }
    }
}

/// Builds the info box containing title and subtitle labels.
fn build_info_box(item: &SearchResultItem) -> Box {
    let info_box = Box::new(Vertical, 2);
    info_box.set_hexpand(true);
    info_box.set_valign(Center);

    let title = match item {
        SearchResultItem::Track { title, .. } | SearchResultItem::Album { title, .. } => {
            title.clone()
        }
        SearchResultItem::Artist { name, .. } | SearchResultItem::Playlist { name, .. } => {
            name.clone()
        }
    };
    let subtitle = build_subtitle(item);

    let title_label = Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(EllipsizeEnd);
    title_label.add_css_class("title-4");

    let subtitle_label = Label::new(Some(&subtitle));
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_ellipsize(EllipsizeEnd);
    subtitle_label.add_css_class("dim-label");

    info_box.append(&title_label);
    info_box.append(&subtitle_label);

    info_box
}

/// Builds the explicit content indicator ("E" label).
fn build_explicit_indicator(item: &SearchResultItem) -> Box {
    let is_explicit = match item {
        SearchResultItem::Track { is_explicit, .. }
        | SearchResultItem::Album { is_explicit, .. }
        | SearchResultItem::Playlist { is_explicit, .. } => *is_explicit,
        SearchResultItem::Artist { .. } => false,
    };

    let container = Box::new(Vertical, 0);
    let label = Label::new(Some("E"));
    label.set_halign(Center);
    label.set_valign(Center);
    label.set_visible(is_explicit);
    label.set_tooltip_text(Some("Explicit Content"));
    label.set_css_classes(&["error", "caption", "explicit-indicator"]);
    container.append(&label);
    container.set_halign(End);
    container.set_valign(Center);
    container.set_margin_start(4);

    container
}

/// Builds the Hi-Res audio indicator icon.
fn build_hires_indicator(item: &SearchResultItem) -> Box {
    let is_hires = match item {
        SearchResultItem::Track {
            bit_depth,
            sampling_rate,
            ..
        }
        | SearchResultItem::Album {
            bit_depth,
            sampling_rate,
            ..
        } => *bit_depth >= 24 || *sampling_rate > 48.0,
        _ => false,
    };

    let container = Box::new(Vertical, 0);
    let icon = Image::builder()
        .halign(Center)
        .valign(Center)
        .visible(is_hires)
        .tooltip_text("Hi-Res Audio")
        .pixel_size(24)
        .build();
    if is_hires {
        icon.set_from_file(Some("./assets/hires.png"));
    }
    container.append(&icon);
    container.set_halign(End);
    container.set_valign(Center);
    container.set_margin_start(8);

    container
}

/// Creates a section header row with a ">" navigation button for scope filtering.
fn create_section_header(category: SearchCategory, ctx: &SearchCtx) -> ListBoxRow {
    let label = Label::new(Some(category.label()));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label.set_margin_start(12);
    label.set_margin_end(6);
    label.set_margin_top(12);
    label.set_margin_bottom(6);
    label.set_hexpand(true);

    let arrow = Image::from_icon_name("go-next-symbolic");
    arrow.set_margin_end(12);
    arrow.set_margin_top(12);
    arrow.set_margin_bottom(6);

    let header_box = Box::new(Horizontal, 0);
    header_box.append(&label);
    header_box.append(&arrow);

    let row = ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);
    row.set_child(Some(&header_box));

    let scope_selector = ctx.scope_selector.clone();
    let scope_category = category;
    let gesture = GestureClick::new();
    gesture.connect_pressed(move |_, _, _, _| {
        scope_selector.set_selected(scope_category.to_scope().to_u32());
    });
    row.add_controller(gesture);

    row
}

/// Enqueues a download using settings for quality and output directory.
fn enqueue_from_settings(
    settings: &Arc<Mutex<AppSettings>>,
    cmd_sender: &Sender<DownloadCommand>,
    item: &DownloadItem,
) {
    let (quality, output_dir) = {
        let s = settings.lock();
        (s.default_quality, s.download_directory.clone())
    };
    let task = DownloadTask::new(item.clone(), quality, output_dir);
    if let Err(err) = cmd_sender.send_blocking(DownloadCommand::Enqueue { task }) {
        warn!(error = %err, "Failed to enqueue download");
    }
}

/// Creates a `SplitButton` with download and add-to-queue actions.
fn create_split_button(item: &SearchResultItem, ctx: &SearchCtx) -> SplitButton {
    let queue_button = Button::builder()
        .label("Add to queue")
        .halign(Start)
        .css_classes(["model"])
        .build();

    let popover_box = Box::new(Vertical, 0);
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

    let download_item = match item {
        SearchResultItem::Track {
            id,
            title,
            artist,
            cover_url,
            ..
        } => DownloadItem::Track {
            track_id: *id,
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        SearchResultItem::Album {
            id,
            title,
            artist,
            cover_url,
            ..
        } => DownloadItem::Album {
            album_id: id.clone(),
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        SearchResultItem::Playlist {
            id,
            name,
            cover_url,
            ..
        } => DownloadItem::Playlist {
            playlist_id: id.clone(),
            title: name.clone(),
            cover_url: cover_url.clone(),
        },
        SearchResultItem::Artist { id, name, .. } => DownloadItem::Artist {
            artist_id: *id,
            name: name.clone(),
            cover_url: None,
        },
    };

    {
        let settings = Arc::clone(&ctx.settings);
        let cmd_sender = ctx.cmd_sender.clone();
        let item = download_item.clone();
        let toast_overlay = ctx.toast_overlay.clone();
        split_button.connect_clicked(move |_| {
            enqueue_from_settings(&settings, &cmd_sender, &item);
            let toast = Toast::new("Added to download queue");
            toast.set_timeout(2);
            toast_overlay.add_toast(toast);
        });
    }

    {
        let settings = Arc::clone(&ctx.settings);
        let cmd_sender = ctx.cmd_sender.clone();
        let item = download_item;
        let p = popover;
        let toast_overlay = ctx.toast_overlay.clone();
        queue_button.connect_clicked(move |_| {
            enqueue_from_settings(&settings, &cmd_sender, &item);
            let toast = Toast::new("Added to download queue");
            toast.set_timeout(2);
            toast_overlay.add_toast(toast);
            p.popdown();
        });
    }

    split_button
}

/// Formats a duration in seconds to `MM:SS` format.
fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Handles a search event by updating the list box or showing an error toast.
fn handle_search_event(event: SearchEvent, ctx: &SearchCtx) {
    ctx.toast_overlay.dismiss_all();
    ctx.is_loading.store(false, Relaxed);

    match event {
        Results { result, query } => {
            if query.trim().is_empty() {
                return;
            }
            populate_results(ctx, &result, &query);
        }
        Error { error, .. } => {
            let toast = Toast::new(&format!("Search failed: {error}"));
            toast.set_timeout(4);
            ctx.toast_overlay.add_toast(toast);
        }
    }
}

/// Applies a newly loaded texture to every picture registered for its URL.
fn handle_texture(_url: &str, texture: Option<Texture>, pictures: &[Picture]) {
    let Some(texture) = texture else { return };
    for picture in pictures {
        picture.set_paintable(Some(&texture));
    }
}

/// Adds album items to the item vector.
fn populate_album_items(
    result: &SearchResult,
    items: &Arc<Mutex<Vec<SearchResultItem>>>,
    limit_to_five: bool,
) {
    let Some(albums) = &result.albums else { return };
    let Some(album_items) = &albums.items else {
        return;
    };
    for (i, album) in album_items.iter().enumerate() {
        if limit_to_five && i >= 5 {
            break;
        }
        let Some(id) = album.id.clone() else { continue };
        let title = album.title.as_deref().unwrap_or("Unknown Album");
        let artist = album
            .artist
            .as_ref()
            .and_then(|a| a.name.as_deref())
            .unwrap_or("Unknown Artist");
        let cover_url = resolve_thumbnail_url(album.image.as_ref());
        let duration = album.duration.unwrap_or(0);
        let bit_depth = album.maximum_bit_depth.unwrap_or(0);
        let sampling_rate = album.maximum_sampling_rate.unwrap_or(0.0);
        let track_count = album.tracks_count.unwrap_or(0);
        let year = album
            .release_date_original
            .as_deref()
            .and_then(|d| d.split('-').next())
            .unwrap_or("")
            .to_string();
        let is_explicit = false;
        items.lock().push(SearchResultItem::Album {
            id,
            title: title.to_string(),
            artist: artist.to_string(),
            cover_url,
            duration,
            bit_depth,
            sampling_rate,
            track_count,
            year,
            is_explicit,
        });
    }
}

/// Adds artist items to the item vector.
fn populate_artist_items(
    result: &SearchResult,
    items: &Arc<Mutex<Vec<SearchResultItem>>>,
    limit_to_five: bool,
) {
    let Some(artists) = &result.artists else {
        return;
    };
    let Some(artist_items) = &artists.items else {
        return;
    };
    for (i, artist) in artist_items.iter().enumerate() {
        if limit_to_five && i >= 5 {
            break;
        }
        let Some(id) = artist.id else { continue };
        let name = artist.name.as_deref().unwrap_or("Unknown Artist");
        let cover_url = resolve_thumbnail_url(artist.image.as_ref());
        items.lock().push(SearchResultItem::Artist {
            id,
            name: name.to_string(),
            cover_url,
        });
    }
}

/// Adds playlist items to the item vector.
fn populate_playlist_items(
    result: &SearchResult,
    items: &Arc<Mutex<Vec<SearchResultItem>>>,
    limit_to_five: bool,
) {
    let Some(playlists) = &result.playlists else {
        return;
    };
    let Some(playlist_items) = &playlists.items else {
        return;
    };
    for (i, playlist) in playlist_items.iter().enumerate() {
        if limit_to_five && i >= 5 {
            break;
        }
        let id = playlist.id.clone().unwrap_or_default();
        let name = playlist.name.as_deref().unwrap_or("Unknown Playlist");
        let cover_url = playlist.best_image_url(false);
        let is_explicit = false;
        items.lock().push(SearchResultItem::Playlist {
            id,
            name: name.to_string(),
            cover_url,
            is_explicit,
        });
    }
}

/// Fetches the cover URL for an artist or playlist id.
fn fetch_cover_url(
    api_service: &Arc<Mutex<QobuzApiService>>,
    id: &str,
    is_artist: bool,
) -> Option<String> {
    if is_artist {
        fetch_artist_cover_url(api_service, id)
    } else {
        fetch_playlist_cover_url(api_service, id)
    }
}

/// Fetches a cover URL and sends it (with its index) via the channel.
fn send_fetched_url(
    tx: &Sender<(usize, Option<String>)>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    idx: usize,
    id: &str,
    is_artist: bool,
) {
    let url = fetch_cover_url(api_service, id, is_artist);
    if tx.send_blocking((idx, url)).is_err() {
        warn!(id = %id, "Failed to send cover URL to channel");
    }
}

/// Fetches missing artist/playlist cover art asynchronously and applies it to the
/// associated picture widget.
///
/// The worker thread only fetches cover URLs; the picture widgets themselves remain
/// owned by the main-thread receiver, since GTK widgets are not thread-safe.
///
/// # Arguments
///
/// * `pending` - Picture widgets (with their entity id and kind) whose cover art still needs to be
///   fetched
/// * `ctx` - Shared search context
fn fetch_missing_images(pending: Vec<AsyncCoverFetch>, ctx: &SearchCtx) {
    if pending.is_empty() {
        return;
    }

    let api_service = Arc::clone(&ctx.api_service);
    let cover_art_cache = ctx.cover_art_cache.clone();
    let texture_sender = ctx.texture_sender.clone();
    let texture_register = ctx.texture_register.clone();

    let fetch_requests: Vec<(String, bool)> = pending
        .iter()
        .map(|f| (f.id.clone(), f.is_artist))
        .collect();

    let (tx, rx) = unbounded::<(usize, Option<String>)>();

    spawn(move || {
        for (idx, (id, is_artist)) in fetch_requests.into_iter().enumerate() {
            send_fetched_url(&tx, &api_service, idx, &id, is_artist);
        }
    });

    MainContext::default().spawn_local(async move {
        while let Ok((idx, url)) = rx.recv().await {
            apply_cover_fetch(
                idx,
                url,
                &pending,
                &cover_art_cache,
                &texture_sender,
                &texture_register,
            );
        }
    });
}

/// Applies a fetched cover result for the given index to its picture widget.
fn apply_cover_fetch(
    idx: usize,
    url: Option<String>,
    pending: &[AsyncCoverFetch],
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    let Some(fetch) = pending.get(idx) else {
        return;
    };
    apply_fetched_cover(
        fetch,
        url,
        cover_art_cache,
        texture_sender,
        texture_register,
    );
}

/// Applies a fetched cover URL to the matching picture widget.
fn apply_fetched_cover(
    fetch: &AsyncCoverFetch,
    url: Option<String>,
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    let picture = fetch.picture.clone();
    if let Some(url) = url {
        update_picture_with_cover(
            picture,
            url,
            cover_art_cache,
            texture_sender,
            texture_register,
        );
    } else {
        set_fallback_icon(&picture, fetch.is_artist);
    }
}

/// Sets fallback icon on picture widget when no image is available.
fn set_fallback_icon(picture: &Picture, is_artist: bool) {
    let icon = if is_artist {
        "avatar-default-symbolic"
    } else {
        "audio-x-generic-symbolic"
    };

    if let Some(parent) = picture.parent()
        && let Ok(box_) = parent.downcast::<Box>()
    {
        box_.remove(picture);

        let img = Image::from_icon_name(icon);
        img.set_size_request(64, 64);
        img.add_css_class("thumbnail");
        box_.prepend(&img);
    }
}

/// Updates a picture widget with cover art from cache or starts loading.
fn update_picture_with_cover(
    picture: Picture,
    url: String,
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    if let Some(texture) = cover_art_cache.get(&url) {
        picture.set_paintable(Some(&texture));
    } else {
        register_cover_texture(texture_register, url.clone(), picture);
        cover_art_cache.start_load(url, texture_sender.clone());
    }
}

/// Fetches artist cover URL from API.
fn fetch_artist_cover_url(api_service: &Arc<Mutex<QobuzApiService>>, id: &str) -> Option<String> {
    let artist_id = match id.parse::<i32>() {
        Ok(id) => id,
        Err(e) => {
            warn!(error = %e, id = %id, "Failed to parse artist ID for cover URL");
            return None;
        }
    };
    let api = api_service.lock();
    let artist = match api.get_artist(artist_id, None) {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id = %artist_id, "Failed to fetch artist for cover URL");
            return None;
        }
    };
    let url = resolve_thumbnail_url(artist.image.as_ref());
    let result = url;
    drop(api);
    result
}

/// Fetches playlist cover URL from API.
fn fetch_playlist_cover_url(api_service: &Arc<Mutex<QobuzApiService>>, id: &str) -> Option<String> {
    let api = api_service.lock();
    let playlist = match api.get_playlist(id, None) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, playlist_id = %id, "Failed to fetch playlist for cover URL");
            return None;
        }
    };
    let url = playlist.best_image_url(false);
    let result = url;
    drop(api);
    result
}

/// Shows an empty-state message in the list box when no results are found.
fn show_empty_search_state(list_box: &ListBox, query: &str) {
    let empty_label = Label::builder()
        .label(format!(
            "No results found for \"{query}\". Try a different search term."
        ))
        .css_classes(["dim-label"])
        .halign(Start)
        .margin_top(24)
        .margin_start(16)
        .margin_end(16)
        .wrap(true)
        .build();
    list_box.append(&empty_label);
}

/// Clears the list box and repopulates with sectioned results.
fn populate_results(ctx: &SearchCtx, result: &SearchResult, query: &str) {
    while let Some(child) = ctx.list_box.first_child() {
        ctx.list_box.remove(&child);
    }
    ctx.items.lock().clear();

    let is_all = matches!(*ctx.scope.lock(), SearchScope::All);

    populate_track_items(result, &ctx.items, is_all);
    populate_album_items(result, &ctx.items, is_all);
    populate_artist_items(result, &ctx.items, is_all);
    populate_playlist_items(result, &ctx.items, is_all);

    let items_empty = ctx.items.lock().is_empty();
    if items_empty {
        show_empty_search_state(&ctx.list_box, query);
        return;
    }

    let items_ref = ctx.items.lock();
    let mut current_category = None;
    let mut pending_fetches = Vec::new();

    for item in items_ref.iter() {
        let category = item_category(item);

        let needs_header = current_category != Some(category);

        if needs_header {
            current_category = Some(category);
            ctx.list_box.append(&create_section_header(category, ctx));
        }

        let (row, pending) = create_data_row(item, ctx);
        ctx.list_box.append(&row);
        if let Some(pending) = pending {
            pending_fetches.push(pending);
        }
    }
    drop(items_ref);

    fetch_missing_images(pending_fetches, ctx);
}

/// Returns the search category for an item.
const fn item_category(item: &SearchResultItem) -> SearchCategory {
    match item {
        SearchResultItem::Track { .. } => SearchCategory::Tracks,
        SearchResultItem::Album { .. } => SearchCategory::Albums,
        SearchResultItem::Artist { .. } => SearchCategory::Artists,
        SearchResultItem::Playlist { .. } => SearchCategory::Playlists,
    }
}

/// Adds track items to the item vector.
fn populate_track_items(
    result: &SearchResult,
    items: &Arc<Mutex<Vec<SearchResultItem>>>,
    limit_to_five: bool,
) {
    let Some(tracks) = &result.tracks else { return };
    let Some(track_items) = &tracks.items else {
        return;
    };
    for (i, track) in track_items.iter().enumerate() {
        if limit_to_five && i >= 5 {
            break;
        }
        let Some(id) = track.id else { continue };
        let title = track.title.as_deref().unwrap_or("Unknown Track");
        let artist = track
            .performer
            .as_ref()
            .and_then(|a| a.name.as_deref())
            .unwrap_or("Unknown Artist");
        let album = track
            .album
            .as_ref()
            .and_then(|a| a.title.as_deref())
            .unwrap_or("Unknown Album");
        let cover_url = resolve_thumbnail_url(track.album.as_ref().and_then(|a| a.image.as_ref()));
        let duration = track.duration.unwrap_or(0);
        let bit_depth = track
            .audio_info
            .as_ref()
            .and_then(|a| a.bit_depth)
            .or(track.maximum_bit_depth)
            .unwrap_or(0);
        let sampling_rate = track
            .audio_info
            .as_ref()
            .and_then(|a| a.sampling_rate)
            .or(track.maximum_sampling_rate)
            .unwrap_or(0.0);
        let is_explicit = track.parental_warning.unwrap_or(false);
        items.lock().push(SearchResultItem::Track {
            id,
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            cover_url,
            duration,
            bit_depth,
            sampling_rate,
            is_explicit,
        });
    }
}

/// Formats a quality string from bit depth and sampling rate.
fn quality_string(bit_depth: i32, sampling_rate: f64) -> String {
    if bit_depth > 0 && sampling_rate > 0.0 {
        let rate_str = if sampling_rate.fract() == 0.0 {
            format!("{sampling_rate:.0}")
        } else {
            format!("{sampling_rate:.1}")
        };
        format!("{bit_depth}-bit/{rate_str}kHz")
    } else {
        String::new()
    }
}

/// Finds an item at the given `ListBox` row position (accounting for section headers).
fn find_item_at_position(items: &[SearchResultItem], row_index: u32) -> Option<ActivationTarget> {
    let mut pos = 0u32;
    let mut current_category: Option<SearchCategory> = None;
    for item in items {
        let category = item_category(item);
        if current_category != Some(category) {
            current_category = Some(category);
            pos = pos.saturating_add(1);
        }
        if pos != row_index {
            pos = pos.saturating_add(1);
            continue;
        }
        return match item {
            SearchResultItem::Album { id, .. } => Some(ActivationTarget::Album(id.clone())),
            SearchResultItem::Artist { id, .. } => Some(ActivationTarget::Artist(*id)),
            SearchResultItem::Playlist { id, .. } => Some(ActivationTarget::Playlist(id.clone())),
            SearchResultItem::Track { .. } => None,
        };
    }
    None
}

/// Sets up the search event receiver to update the list box.
fn setup_search_receiver(receiver: Receiver<SearchEvent>, ctx: SearchCtx) {
    MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_search_event(event, &ctx);
        }
    });
}

/// Logs a texture-forward send failure if the channel is closed.
fn log_texture_send_error(result: Result<(), SendError<TextureEvent>>) {
    if let Err(e) = result {
        warn!(error = %e, "Failed to forward texture load event");
    }
}

/// Registers a picture for a cover URL, logging send failures.
fn register_cover_texture(sender: &Sender<TextureEvent>, url: String, picture: Picture) {
    if let Err(e) = sender.send_blocking(TextureEvent::Register { url, picture }) {
        warn!(error = %e, "Failed to register cover texture");
    }
}

/// Sets up the cover art texture receiver to update pictures when textures load.
///
/// The receiver owns the URL-to-picture registry on the main thread, since GTK
/// widgets are not thread-safe.
fn setup_texture_receiver(receiver: Receiver<TextureEvent>) {
    MainContext::default().spawn_local(async move {
        let mut registry = HashMap::<String, Vec<Picture>>::new();
        while let Ok(event) = receiver.recv().await {
            apply_texture_event(event, &mut registry);
        }
    });
}

/// Applies a cover art load/registration event to the picture registry.
fn apply_texture_event(event: TextureEvent, registry: &mut HashMap<String, Vec<Picture>>) {
    match event {
        TextureEvent::Register { url, picture } => {
            registry.entry(url).or_default().push(picture);
        }
        TextureEvent::Loaded { url, texture } => {
            if let Some(pictures) = registry.remove(&url) {
                handle_texture(&url, texture, &pictures);
            }
        }
    }
}

/// Triggers a scoped search with loading toast feedback.
fn trigger_search(
    controller: &SearchController,
    query: &str,
    sender: &Sender<SearchEvent>,
    is_loading: &Arc<AtomicBool>,
    toast_overlay: &ToastOverlay,
    scope: SearchScope,
) {
    if is_loading.load(Relaxed) {
        return;
    }
    is_loading.store(true, Relaxed);

    let toast = Toast::new("Searching...");
    toast.set_timeout(0);
    toast_overlay.add_toast(toast);

    controller.search_scoped(query, scope, sender.clone());
}

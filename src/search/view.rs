//! Search view UI matching the original implementation: `SearchEntry`, scope selector,
//! Hi-Res and explicit content indicators, inline `SplitButton` for download/queue actions.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        HeaderBar, NavigationView, SplitButton, Toast, ToastOverlay, ToolbarView,
        glib::{
            MainContext,
            Propagation::{Proceed, Stop},
        },
        gtk::{
            Align::{Center, End, Start},
            Box as GtkBox, Button, DropDown, EventControllerKey, Image, Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture,
            PolicyType::Automatic,
            Popover, ScrolledWindow, SearchEntry,
            SelectionMode::Single,
            gdk::{Key, Texture},
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{BoxExt, ButtonExt, EditableExt, ListBoxRowExt, PopoverExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::models::search::SearchResult,
    tracing::warn,
};

use crate::{
    app::AppState,
    cover_art::cache::CoverArtCache,
    download::progress::{DownloadCommand, DownloadItem, DownloadTask},
    preferences::settings::AppSettings,
    search::controller::{
        SearchController,
        SearchEvent::{self, Error, Results},
        SearchScope,
    },
};

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
    fn label(self) -> &'static str {
        match self {
            Self::Tracks => "Tracks",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }
}

/// Shared context passed through search result processing.
struct SearchCtx {
    /// Results list box.
    list_box: ListBox,
    /// Search result items vector.
    items: Rc<RefCell<Vec<SearchResultItem>>>,
    /// Cover art texture cache.
    cover_art_cache: CoverArtCache,
    /// Channel sender for loaded cover art textures.
    texture_sender: Sender<(String, Option<Texture>)>,
    /// Picture widgets registered per cover URL for texture updates.
    picture_map: Rc<RefCell<HashMap<String, Vec<Picture>>>>,
    /// Toast overlay for search feedback.
    toast_overlay: ToastOverlay,
    /// Whether a search is currently in progress.
    is_loading: Rc<RefCell<bool>>,
    /// User settings for default quality and output directory.
    settings: Arc<Mutex<AppSettings>>,
    /// Channel sender for download commands.
    cmd_sender: Sender<DownloadCommand>,
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
}

impl SearchWidgets {
    /// Sets up ESC key navigation to pop the `NavigationView`.
    pub fn setup_esc_navigation(&self, navigation_view: &NavigationView) {
        let key_controller = EventControllerKey::new();
        let nav_view = navigation_view.clone();

        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            Key::Escape => {
                nav_view.pop();
                Stop
            }
            _ => Proceed,
        });

        self.search_entry.add_controller(key_controller);
    }
}

/// Builds the search view UI and returns the root widget and widget references.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `cmd_sender` - Channel sender for download commands
pub fn build(state: &AppState, cmd_sender: Sender<DownloadCommand>) -> SearchWidgets {
    let controller = SearchController::new(Arc::clone(&state.api_service));
    let (search_sender, search_receiver) = unbounded::<SearchEvent>();

    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();

    let title_label = Label::new(Some("Search"));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    toolbar.add_top_bar(&header);

    let toast_overlay = ToastOverlay::new();

    let saved_scope = state.settings.lock().search_scope;
    let scope_selector =
        DropDown::from_strings(&["All", "Albums", "Tracks", "Artists", "Playlists"]);
    scope_selector.set_selected(saved_scope);

    let search_entry = SearchEntry::builder()
        .placeholder_text("Search for albums or tracks...")
        .hexpand(true)
        .build();

    let header_box = GtkBox::new(Horizontal, 8);
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

    let content_box = GtkBox::new(Vertical, 0);
    content_box.append(&header_box);
    content_box.append(&results_scrolled);

    toast_overlay.set_child(Some(&content_box));
    toolbar.set_content(Some(&toast_overlay));

    let (texture_sender, texture_receiver) = unbounded::<(String, Option<Texture>)>();
    let picture_map: Rc<RefCell<HashMap<String, Vec<Picture>>>> =
        Rc::new(RefCell::new(HashMap::new()));
    setup_texture_receiver(texture_receiver, Rc::clone(&picture_map));

    let items: Rc<RefCell<Vec<SearchResultItem>>> = Rc::new(RefCell::new(Vec::new()));
    let is_loading: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let cover_art_cache = state.cover_art_cache.clone();

    let scope: Rc<RefCell<SearchScope>> = Rc::new(RefCell::new(SearchScope::from_u32(saved_scope)));

    let ctx = SearchCtx {
        list_box,
        items,
        cover_art_cache,
        texture_sender,
        picture_map: Rc::clone(&picture_map),
        toast_overlay: toast_overlay.clone(),
        is_loading: Rc::clone(&is_loading),
        settings: Arc::clone(&state.settings),
        cmd_sender,
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

    let scope_settings = Arc::clone(&state.settings);

    let controller_clone = controller;
    let search_entry_clone = search_entry.clone();
    let search_sender_clone = search_sender;
    let scope_clone = Rc::clone(&scope);

    scope_selector.connect_selected_item_notify(move |scope_widget| {
        let idx = scope_widget.selected();
        let new_scope = SearchScope::from_u32(idx);
        *scope_clone.borrow_mut() = new_scope;

        let mut settings = scope_settings.lock();
        settings.search_scope = idx;
        drop(settings);

        if !search_entry_clone.text().trim().is_empty() {
            controller_clone.search_scoped(
                search_entry_clone.text().as_ref(),
                new_scope,
                search_sender_clone.clone(),
            );
        }
    });

    SearchWidgets {
        root: toolbar,
        search_entry,
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
    is_loading: &Rc<RefCell<bool>>,
    toast_overlay: &ToastOverlay,
    scope: &Rc<RefCell<SearchScope>>,
) {
    let controller_search_started = controller.clone();
    let sender_clone = sender.clone();
    let is_loading_clone = Rc::clone(is_loading);
    let toast_overlay_clone = toast_overlay.clone();
    let scope_clone = Rc::clone(scope);

    entry.connect_search_started(move |entry| {
        let query = entry.text().to_string();
        if query.trim().is_empty() {
            return;
        }
        let s = *scope_clone.borrow();
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
    let is_loading_activate = Rc::clone(is_loading);
    let toast_overlay_activate = toast_overlay.clone();
    let scope_activate = Rc::clone(scope);

    entry.connect_activate(move |entry| {
        let query = entry.text().to_string();
        if query.trim().is_empty() {
            return;
        }
        let s = *scope_activate.borrow();
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
fn create_data_row(item: &SearchResultItem, ctx: &SearchCtx) -> ListBoxRow {
    let picture = Picture::new();
    picture.set_size_request(64, 64);
    picture.add_css_class("thumbnail");

    let row_box = GtkBox::new(Horizontal, 16);
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

    let row = ListBoxRow::new();
    row.set_child(Some(&row_box));

    row
}

/// Attaches cover art texture to the picture widget.
fn attach_cover_art(item: &SearchResultItem, picture: &Picture, ctx: &SearchCtx) {
    let cover_url = match item {
        SearchResultItem::Track { cover_url, .. }
        | SearchResultItem::Album { cover_url, .. }
        | SearchResultItem::Playlist { cover_url, .. } => cover_url.clone(),
        SearchResultItem::Artist { .. } => None,
    };

    if let Some(url) = cover_url {
        if let Some(texture) = ctx.cover_art_cache.get(&url) {
            picture.set_paintable(Some(&texture));
        } else {
            ctx.picture_map
                .borrow_mut()
                .entry(url.clone())
                .or_default()
                .push(picture.clone());
            ctx.cover_art_cache
                .start_load(url, ctx.texture_sender.clone());
        }
    }
}

/// Builds the info box containing title and subtitle labels.
fn build_info_box(item: &SearchResultItem) -> GtkBox {
    let info_box = GtkBox::new(Vertical, 2);
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
fn build_explicit_indicator(item: &SearchResultItem) -> GtkBox {
    let is_explicit = match item {
        SearchResultItem::Track { is_explicit, .. }
        | SearchResultItem::Album { is_explicit, .. }
        | SearchResultItem::Playlist { is_explicit, .. } => *is_explicit,
        SearchResultItem::Artist { .. } => false,
    };

    let container = GtkBox::new(Vertical, 0);
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
fn build_hires_indicator(item: &SearchResultItem) -> GtkBox {
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

    let container = GtkBox::new(Vertical, 0);
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

/// Creates a section header row with bold, non-activatable styling.
fn create_section_header(label: &str) -> ListBoxRow {
    let label_widget = Label::new(Some(label));
    label_widget.set_xalign(0.0);
    label_widget.add_css_class("heading");
    label_widget.set_margin_start(12);
    label_widget.set_margin_end(12);
    label_widget.set_margin_top(12);
    label_widget.set_margin_bottom(6);

    let row = ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);
    row.set_child(Some(&label_widget));

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
        split_button.connect_clicked(move |_| enqueue_from_settings(&settings, &cmd_sender, &item));
    }

    {
        let settings = Arc::clone(&ctx.settings);
        let cmd_sender = ctx.cmd_sender.clone();
        let item = download_item;
        let p = popover;
        queue_button.connect_clicked(move |_| {
            enqueue_from_settings(&settings, &cmd_sender, &item);
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
    *ctx.is_loading.borrow_mut() = false;

    match event {
        Results { result, query } => {
            if query.trim().is_empty() {
                return;
            }
            populate_results(ctx, &result);
        }
        Error { error, .. } => {
            let toast = Toast::new(&format!("Search failed: {error}"));
            toast.set_timeout(4);
            ctx.toast_overlay.add_toast(toast);
        }
    }
}

/// Updates picture widgets for a newly loaded texture.
fn handle_texture(
    url: &str,
    texture: Option<Texture>,
    picture_map: &Rc<RefCell<HashMap<String, Vec<Picture>>>>,
) {
    let Some(texture) = texture else { return };
    let map = picture_map.borrow();
    let Some(pictures) = map.get(url) else { return };
    for picture in pictures {
        picture.set_paintable(Some(&texture));
    }
}

/// Adds album items to the item vector.
fn populate_album_items(result: &SearchResult, items: &Rc<RefCell<Vec<SearchResultItem>>>) {
    let Some(albums) = &result.albums else { return };
    let Some(album_items) = &albums.items else {
        return;
    };
    for album in album_items {
        let Some(id) = album.id.clone() else { continue };
        let title = album.title.as_deref().unwrap_or("Unknown Album");
        let artist = album
            .artist
            .as_ref()
            .and_then(|a| a.name.as_deref())
            .unwrap_or("Unknown Artist");
        let cover_url = album.image.as_ref().and_then(|img| img.thumbnail.clone());
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
        items.borrow_mut().push(SearchResultItem::Album {
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
fn populate_artist_items(result: &SearchResult, items: &Rc<RefCell<Vec<SearchResultItem>>>) {
    let Some(artists) = &result.artists else {
        return;
    };
    let Some(artist_items) = &artists.items else {
        return;
    };
    for artist in artist_items {
        let Some(id) = artist.id else { continue };
        let name = artist.name.as_deref().unwrap_or("Unknown Artist");
        items.borrow_mut().push(SearchResultItem::Artist {
            id,
            name: name.to_string(),
        });
    }
}

/// Adds playlist items to the item vector.
fn populate_playlist_items(result: &SearchResult, items: &Rc<RefCell<Vec<SearchResultItem>>>) {
    let Some(playlists) = &result.playlists else {
        return;
    };
    let Some(playlist_items) = &playlists.items else {
        return;
    };
    for playlist in playlist_items {
        let id = playlist.id.clone().unwrap_or_default();
        let name = playlist.name.as_deref().unwrap_or("Unknown Playlist");
        let cover_url = playlist
            .image
            .as_ref()
            .and_then(|img| img.thumbnail.clone());
        let is_explicit = false;
        items.borrow_mut().push(SearchResultItem::Playlist {
            id,
            name: name.to_string(),
            cover_url,
            is_explicit,
        });
    }
}

/// Clears the list box and repopulates with sectioned results.
fn populate_results(ctx: &SearchCtx, result: &SearchResult) {
    while let Some(row) = ctx.list_box.first_child() {
        ctx.list_box.remove(&row);
    }
    ctx.items.borrow_mut().clear();

    populate_track_items(result, &ctx.items);
    populate_album_items(result, &ctx.items);
    populate_artist_items(result, &ctx.items);
    populate_playlist_items(result, &ctx.items);

    let items_ref = ctx.items.borrow();
    let mut current_category = None;

    for item in items_ref.iter() {
        let category = match item {
            SearchResultItem::Track { .. } => SearchCategory::Tracks,
            SearchResultItem::Album { .. } => SearchCategory::Albums,
            SearchResultItem::Artist { .. } => SearchCategory::Artists,
            SearchResultItem::Playlist { .. } => SearchCategory::Playlists,
        };

        let needs_header = current_category != Some(category);

        if needs_header {
            current_category = Some(category);
            ctx.list_box
                .append(&create_section_header(category.label()));
        }

        let row = create_data_row(item, ctx);
        ctx.list_box.append(&row);
    }
}

/// Adds track items to the item vector.
fn populate_track_items(result: &SearchResult, items: &Rc<RefCell<Vec<SearchResultItem>>>) {
    let Some(tracks) = &result.tracks else { return };
    let Some(track_items) = &tracks.items else {
        return;
    };
    for track in track_items {
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
        let cover_url = track
            .album
            .as_ref()
            .and_then(|a| a.image.as_ref()?.thumbnail.clone());
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
        items.borrow_mut().push(SearchResultItem::Track {
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

/// Sets up the search event receiver to update the list box.
fn setup_search_receiver(receiver: Receiver<SearchEvent>, ctx: SearchCtx) {
    MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_search_event(event, &ctx);
        }
    });
}

/// Sets up the cover art texture receiver to update pictures when textures load.
fn setup_texture_receiver(
    receiver: Receiver<(String, Option<Texture>)>,
    picture_map: Rc<RefCell<HashMap<String, Vec<Picture>>>>,
) {
    MainContext::default().spawn_local(async move {
        while let Ok((url, texture)) = receiver.recv().await {
            handle_texture(&url, texture, &picture_map);
        }
    });
}

/// Triggers a scoped search with loading toast feedback.
fn trigger_search(
    controller: &SearchController,
    query: &str,
    sender: &Sender<SearchEvent>,
    is_loading: &Rc<RefCell<bool>>,
    toast_overlay: &ToastOverlay,
    scope: SearchScope,
) {
    if *is_loading.borrow() {
        return;
    }
    *is_loading.borrow_mut() = true;

    let toast = Toast::new("Searching...");
    toast.set_timeout(0);
    toast_overlay.add_toast(toast);

    controller.search_scoped(query, scope, sender.clone());
}

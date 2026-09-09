//! Search view UI matching the original implementation: `SearchEntry`, scope selector,
//! Hi-Res and explicit content indicators, inline `SplitButton` for download/queue actions.

pub mod imagery;
pub mod listing;
pub mod row;
pub mod scaffold;

use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

use {
    async_channel::Sender,
    libadwaita::{
        HeaderBar, NavigationView, Toast, ToastOverlay, ToolbarView,
        gdk::Texture,
        glib::Propagation::Proceed,
        gtk::{
            Button, DropDown, EventControllerKey, GestureClick, ListBox, PropagationPhase::Capture,
            SearchEntry,
        },
        prelude::{ButtonExt, EventControllerExt, GestureSingleExt, ListBoxRowExt, WidgetExt},
    },
    num_traits::AsPrimitive,
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
};

use crate::{
    browse::{BrowseEvent, browse_album, browse_artist, browse_playlist},
    cover_art::cache::CoverArtCache,
    download::progress::DownloadCommand,
    preferences::settings::AppSettings,
    search::{
        controller::SearchScope::{self, Albums, Artists, Playlists, Tracks},
        view::{
            ActivationTarget::{Album, Artist, Playlist},
            imagery::TextureEvent,
            listing::find_item_at_position,
            scaffold::{handle_back_clicked, handle_key_pressed},
        },
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
            Self::Tracks => Tracks,
            Self::Albums => Albums,
            Self::Artists => Artists,
            Self::Playlists => Playlists,
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
#[derive(Clone, Debug)]
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
        let key_pressed = key_controller.connect_key_pressed(move |_, key, _, _| {
            handle_key_pressed(key, &scope, &nav_view, &scope_selector)
        });
        for _ in [black_box(key_pressed)] {}

        self.root.add_controller(key_controller);

        let back_button = Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back")
            .build();

        let nav_view_clone = navigation_view.clone();
        let back_scope = Arc::clone(&self.scope);
        let back_selector = self.scope_selector.clone();
        let back_clicked = back_button.connect_clicked(move |_| {
            handle_back_clicked(&back_scope, &nav_view_clone, &back_selector);
        });
        for _ in [black_box(back_clicked)] {}

        self.header_bar.pack_start(&back_button);
    }
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
    let gesture_pressed = gesture.connect_pressed(move |_, n_press, _, y| {
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
            Album(id) => {
                let toast = Toast::new("Opening album…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_album(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
            Artist(id) => {
                let toast = Toast::new("Opening artist…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_artist(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
            Playlist(id) => {
                let toast = Toast::new("Opening playlist…");
                toast.set_timeout(2);
                toast_for_gesture.add_toast(toast);
                browse_playlist(Arc::clone(&api_for_gesture), id, sender_for_gesture.clone());
            }
        }
    });
    for _ in [black_box(gesture_pressed)] {}
    list_box.add_controller(gesture);

    let list_box_for_key = list_box.clone();
    let items_for_key = Arc::clone(items);
    let api_for_key = Arc::clone(api_service);
    let browse_for_key = browse_sender;
    let toast_for_key = toast_overlay.clone();
    let key_controller = EventControllerKey::new();
    let key_pressed = key_controller.connect_key_pressed(move |_, key, _, _| {
        let key_name = key.name();
        let is_return = key_name.as_deref() == Some("Return");
        let is_kp_enter = key_name.as_deref() == Some("KP_Enter");
        if !is_return && !is_kp_enter {
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
            Album(id) => {
                let toast = Toast::new("Opening album…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_album(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
            Artist(id) => {
                let toast = Toast::new("Opening artist…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_artist(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
            Playlist(id) => {
                let toast = Toast::new("Opening playlist…");
                toast.set_timeout(2);
                toast_for_key.add_toast(toast);
                browse_playlist(Arc::clone(&api_for_key), id, browse_for_key.clone());
            }
        }
        Proceed
    });
    for _ in [black_box(key_pressed)] {}
    list_box.add_controller(key_controller);
}

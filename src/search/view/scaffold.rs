//! Search scaffold UI: entry, scope selector, and top-level view builder.

use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        HeaderBar, NavigationView, Toast, ToastOverlay, ToolbarView,
        gdk::{Key, Texture},
        glib::{
            MainContext,
            Propagation::{self, Proceed, Stop},
        },
        gtk::{
            Box, DropDown, Label, ListBox,
            Orientation::{Horizontal, Vertical},
            PolicyType::Automatic,
            ScrolledWindow, SearchEntry,
            SelectionMode::Single,
        },
        prelude::{BoxExt, EditableExt, WidgetExt},
    },
    parking_lot::Mutex,
};

use crate::{
    app::AppState,
    browse::BrowseEvent,
    download::progress::DownloadCommand,
    preferences::settings::AppSettings,
    search::{
        controller::{
            SearchController, SearchEvent,
            SearchScope::{self, All},
        },
        view::{
            SearchCtx, SearchResultItem, SearchWidgets,
            imagery::{
                TextureEvent::{self, Loaded},
                log_texture_send_error, setup_texture_receiver,
            },
            listing::setup_search_receiver,
            setup_results_activation,
        },
    },
};

/// Handles ESC key press to pop the navigation view or reset the search scope.
pub(super) fn handle_key_pressed(
    key: Key,
    scope: &Arc<Mutex<SearchScope>>,
    nav_view: &NavigationView,
    scope_selector: &DropDown,
) -> Propagation {
    if key.name().as_deref() != Some("Escape") {
        return Proceed;
    }
    if *scope.lock() == All {
        let popped = nav_view.pop();
        for _ in [black_box(popped)] {}
        return Stop;
    }
    scope_selector.set_selected(0);
    Stop
}

/// Handles back button click to pop the navigation view or reset the search scope.
pub(super) fn handle_back_clicked(
    scope: &Arc<Mutex<SearchScope>>,
    nav_view: &NavigationView,
    scope_selector: &DropDown,
) {
    if *scope.lock() == All {
        let popped = nav_view.pop();
        for _ in [black_box(popped)] {}
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

    let selected_notify = scope_selector.connect_selected_item_notify(move |widget| {
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
    for _ in [black_box(selected_notify)] {}
}

/// Builds the search view UI and returns the root widget and widget references.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `cmd_sender` - Channel sender for download commands
/// * `browse_sender` - Channel sender for browse events (album navigation)
#[must_use]
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
    drop(MainContext::default().spawn_local(async move {
        while let Ok((url, texture)) = texture_receiver.recv().await {
            let result = texture_forward.send(Loaded { url, texture }).await;
            log_texture_send_error(result);
        }
    }));
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

    let search_started = entry.connect_search_started(move |entry| {
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
    for _ in [black_box(search_started)] {}

    let controller_activate = controller.clone();
    let is_loading_activate = Arc::clone(is_loading);
    let toast_overlay_activate = toast_overlay.clone();
    let scope_activate = Arc::clone(scope);

    let activated = entry.connect_activate(move |entry| {
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
    for _ in [black_box(activated)] {}
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

//! Search result listing: population, sections, and event handling.

use std::{
    hint::black_box,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use {
    async_channel::Receiver,
    libadwaita::{
        Toast,
        glib::MainContext,
        gtk::{
            Align::Start, Box, GestureClick, Image, Label, ListBox, ListBoxRow,
            Orientation::Horizontal,
        },
        prelude::{BoxExt, ListBoxRowExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::models::search::SearchResult,
};

use crate::{
    browse::detail_stage::exhibit::resolve_thumbnail_url,
    search::{
        query::{
            SearchEvent::{self, Error, Results},
            SearchScope::All,
        },
        view::{
            ActivationTarget::{
                self, Album as TargetAlbum, Artist as TargetArtist, Playlist as TargetPlaylist,
            },
            SearchCategory::{self, Albums, Artists, Playlists, Tracks},
            SearchCtx,
            SearchResultItem::{self, Album, Artist, Playlist, Track},
            card::create_data_row,
            thumbnails::fetch_missing_images,
        },
    },
};

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
    let header_pressed = gesture.connect_pressed(move |_, _, _, _| {
        scope_selector.set_selected(scope_category.to_scope().to_u32());
    });
    for _ in [black_box(header_pressed)] {}
    row.add_controller(gesture);

    row
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
        items.lock().push(Album {
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
        items.lock().push(Artist {
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
        items.lock().push(Playlist {
            id,
            name: name.to_string(),
            cover_url,
            is_explicit,
        });
    }
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

    let is_all = matches!(*ctx.scope.lock(), All);

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
        Track { .. } => Tracks,
        Album { .. } => Albums,
        Artist { .. } => Artists,
        Playlist { .. } => Playlists,
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
        items.lock().push(Track {
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

/// Finds an item at the given `ListBox` row position (accounting for section headers).
pub(super) fn find_item_at_position(
    items: &[SearchResultItem],
    row_index: u32,
) -> Option<ActivationTarget> {
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
            Album { id, .. } => Some(TargetAlbum(id.clone())),
            Artist { id, .. } => Some(TargetArtist(*id)),
            Playlist { id, .. } => Some(TargetPlaylist(id.clone())),
            Track { .. } => None,
        };
    }
    None
}

/// Sets up the search event receiver to update the list box.
pub(super) fn setup_search_receiver(receiver: Receiver<SearchEvent>, ctx: SearchCtx) {
    drop(MainContext::default().spawn_local(async move {
        while let Ok(event) = receiver.recv().await {
            handle_search_event(event, &ctx);
        }
    }));
}

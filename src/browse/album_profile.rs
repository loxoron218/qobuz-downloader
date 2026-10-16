//! Album detail view UI.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        ToastOverlay, ToolbarView,
        gtk::{
            Align::Start, Box, Button, DropDown, Image, Label, Orientation::Vertical,
            accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::api::content::{albums::Album, tracks::Track},
};

use crate::{
    browse::detail_stage::{
        controls::{
            append_separator, append_title_label, append_track_count_duration,
            build_detail_controls, build_header_scroll, build_item_section, connect_download_click,
            send_enqueue, wrap_toast_overlay,
        },
        exhibit::{load_cover_art, resolve_image_url},
        track_row::build_track_row,
    },
    download::progress::{DownloadCommand, DownloadItem::Album as AlbumItem, DownloadTask},
    preferences::settings::AppSettings,
};

/// Widgets returned by the album detail view builder.
#[derive(Clone, Debug)]
pub struct AlbumDetailWidgets {
    /// Root toolbar view widget.
    pub root: ToolbarView,
    /// Container box where track rows are appended.
    pub track_container: Box,
    /// Download album button.
    pub download_button: Button,
    /// Quality selector dropdown.
    pub quality_dropdown: DropDown,
    /// Toast overlay for download feedback.
    pub toast_overlay: ToastOverlay,
}

/// Builds the album detail view with metadata only (no tracks yet).
///
/// Phase 1 of two-phase loading: shows album info immediately with a
/// "Loading tracks..." placeholder. Call `populate_tracks()` later to
/// fill in the actual track list and wire the download button.
///
/// # Arguments
///
/// * `album` - Album metadata
///
/// # Returns
///
/// Album detail view widgets with an empty track container.
#[must_use]
pub fn build_meta(album: &Album) -> AlbumDetailWidgets {
    let title = album.title.as_deref().unwrap_or("Album");
    let header_scroll = build_header_scroll(title);
    let content = &header_scroll.content;

    let info_section = build_album_info_section(album);
    content.append(&info_section);

    let (quality_dropdown, download_button) =
        build_detail_controls(content, "Download Album", true);
    download_button.set_sensitive(false);

    let (tracks_header, track_container) = build_item_section("Tracks", 4);
    content.append(&tracks_header);
    content.append(&track_container);

    let loading_label = Label::builder()
        .label("Loading tracks...")
        .css_classes(["dim-label"])
        .tooltip_text("Loading album tracks")
        .xalign(0.0)
        .build();
    loading_label.update_property(&[PropertyLabel("Loading album tracks")]);
    track_container.append(&loading_label);

    let toast_overlay = wrap_toast_overlay(&header_scroll);

    AlbumDetailWidgets {
        root: header_scroll.toolbar,
        track_container,
        download_button,
        quality_dropdown,
        toast_overlay,
    }
}

/// Populates the track list and wires the download button (Phase 2).
///
/// Call this after tracks are fetched to fill the previously-empty
/// track container and enable downloads.
///
/// # Arguments
///
/// * `widgets` - Widgets returned by `build_meta()`
/// * `album` - Album metadata
/// * `tracks` - Track listings
/// * `settings` - Application settings (for download directory)
/// * `cmd_sender` - Channel to enqueue downloads
pub fn populate_tracks(
    widgets: &AlbumDetailWidgets,
    album: &Album,
    tracks: &[Track],
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
) {
    while let Some(child) = widgets.track_container.first_child() {
        widgets.track_container.remove(&child);
    }

    for track in tracks {
        let track_row = build_track_row(track);
        widgets.track_container.append(&track_row);
    }

    widgets.download_button.set_sensitive(true);

    wire_download_button(
        &widgets.download_button,
        &widgets.quality_dropdown,
        album,
        settings,
        cmd_sender,
        &widgets.toast_overlay,
    );
}

/// Builds the album info section with cover art, artist, metadata, and separator.
fn build_album_info_section(album: &Album) -> Box {
    let section = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .css_classes(["album-info-section"])
        .tooltip_text("Album information")
        .build();

    let cover = Image::builder()
        .pixel_size(250)
        .halign(Start)
        .valign(Start)
        .css_classes(["album-cover"])
        .tooltip_text("Album cover art")
        .build();
    section.append(&cover);

    let cover_url = resolve_image_url(album.image.as_ref());
    if let Some(url) = cover_url {
        load_cover_art(&cover, url);
    }

    let artist_name = album
        .artist
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist");
    append_title_label(&section, artist_name, "title-2");

    let genre = album
        .genre
        .as_ref()
        .and_then(|g| g.name.as_deref())
        .unwrap_or("");
    let release_year = album
        .release_date_original
        .as_deref()
        .and_then(|d| d.split('-').next())
        .unwrap_or("");
    let meta_parts: Vec<&str> = [genre, release_year]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect();
    if !meta_parts.is_empty() {
        let meta_label = Label::builder()
            .label(meta_parts.join(" • "))
            .css_classes(["dim-label"])
            .tooltip_text("Album metadata")
            .xalign(0.0)
            .ellipsize(End)
            .build();
        section.append(&meta_label);
    }

    let track_count = album.tracks_count.unwrap_or(0);
    let total_duration = album.duration.unwrap_or(0);
    append_track_count_duration(&section, track_count, total_duration);

    append_separator(&section);

    section.update_property(&[PropertyLabel("Album information")]);
    section
}

/// Wires the download button to enqueue the entire album as a single download task.
fn wire_download_button(
    button: &Button,
    quality_dropdown: &DropDown,
    album: &Album,
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
    toast_overlay: &ToastOverlay,
) {
    let artist_name = album
        .artist
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist")
        .to_string();
    let album_title = album
        .title
        .as_deref()
        .unwrap_or("Unknown Album")
        .to_string();
    let cover_url = album.image.as_ref().and_then(|img| img.thumbnail.clone());
    let album_id = album.id.clone().unwrap_or_default();

    connect_download_click(
        button,
        quality_dropdown,
        settings,
        toast_overlay,
        move |quality, base_dir| {
            let item = AlbumItem {
                album_id: album_id.clone(),
                title: album_title.clone(),
                artist: artist_name.clone(),
                cover_url: cover_url.clone(),
            };
            let task = DownloadTask::new(item, quality, base_dir);
            send_enqueue(&cmd_sender, task);
        },
    );
}

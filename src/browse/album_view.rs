//! Album detail view UI.

use {
    libadwaita::gtk::{Align::Start, Image, pango::EllipsizeMode::End},
    qobuz_api_rust_refactor::models::{album::Album, track::Track},
};

use crate::{
    browse::detail_common::{
        append_separator, append_title_label, append_track_count_duration, build_detail_controls,
        build_header_scroll, build_item_section, build_track_row, collect_track_info,
        connect_download_click, load_cover_art, resolve_image_url, send_enqueue,
    },
    download::{
        progress::{DownloadCommand, DownloadItem, DownloadTask},
        worker::album_output_dir,
    },
    preferences::settings::AppSettings,
};

// Shared imports (Arc, Sender, ToolbarView, Box, Button, DropDown, Label, Mutex…)
include!("common_imports.rs");

/// Widgets returned by the album detail view builder.
#[derive(Clone)]
pub struct AlbumDetailWidgets {
    /// Root toolbar view widget.
    pub root: ToolbarView,
    /// Container box where track rows are appended.
    pub track_container: Box,
    /// Download album button.
    pub download_button: Button,
    /// Quality selector dropdown.
    pub quality_dropdown: DropDown,
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
/// Album detail view widgets with an empty track container
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

    let loading_label = Label::new(Some("Loading tracks..."));
    loading_label.add_css_class("dim-label");
    loading_label.set_xalign(0.0);
    track_container.append(&loading_label);

    AlbumDetailWidgets {
        root: header_scroll.toolbar,
        track_container,
        download_button,
        quality_dropdown,
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
        tracks,
        settings,
        cmd_sender,
    );
}

/// Builds the album info section with cover art, artist, metadata, and separator.
fn build_album_info_section(album: &Album) -> Box {
    let section = Box::new(Vertical, 12);

    let cover = Image::new();
    cover.set_pixel_size(250);
    cover.set_halign(Start);
    cover.set_valign(Start);
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
        let meta_label = Label::new(Some(&meta_parts.join(" • ")));
        meta_label.add_css_class("dim-label");
        meta_label.set_xalign(0.0);
        meta_label.set_ellipsize(End);
        section.append(&meta_label);
    }

    let track_count = album.tracks_count.unwrap_or(0);
    let total_duration = album.duration.unwrap_or(0);
    append_track_count_duration(&section, track_count, total_duration);

    append_separator(&section);

    section
}

/// Wires the download button to enqueue all album tracks as individual downloads.
fn wire_download_button(
    button: &Button,
    quality_dropdown: &DropDown,
    album: &Album,
    tracks: &[Track],
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
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

    let track_info = collect_track_info(tracks);

    connect_download_click(
        button,
        quality_dropdown,
        settings,
        move |quality, base_dir| {
            let album_dir = album_output_dir(&base_dir, &artist_name, &album_title, quality);

            for (track_id, track_title, _) in &track_info {
                let item = DownloadItem::Track {
                    track_id: *track_id,
                    title: track_title.clone(),
                    artist: artist_name.clone(),
                    cover_url: cover_url.clone(),
                };
                let task = DownloadTask::new(item, quality, album_dir.clone());
                send_enqueue(&cmd_sender, task);
            }
        },
    );
}

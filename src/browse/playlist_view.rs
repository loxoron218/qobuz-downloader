//! Playlist detail view UI.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        ToastOverlay, ToolbarView,
        gtk::{Box, Button, DropDown, Label, Orientation::Vertical},
        prelude::{BoxExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::models::playlist::Playlist,
};

use crate::{
    browse::detail_common::{
        append_separator, append_title_label, append_track_count_duration, build_cover_art,
        build_detail_controls, build_header_scroll, build_item_section, build_track_row,
        connect_download_click, send_enqueue, strip_html_tags, wrap_toast_overlay,
    },
    download::progress::{DownloadCommand, DownloadItem::Playlist as PlaylistItem, DownloadTask},
    preferences::settings::AppSettings,
};

/// Builds the playlist detail view with full metadata and tracks.
///
/// Playlists from the API include tracks in the same response, so this is a single-phase build.
///
/// # Arguments
///
/// * `playlist` - Playlist metadata with tracks
/// * `settings` - Application settings (for download directory)
/// * `cmd_sender` - Channel to enqueue downloads
///
/// # Returns
///
/// Root toolbar view
pub fn build(
    playlist: &Playlist,
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
) -> ToolbarView {
    let name = playlist.name.as_deref().unwrap_or("Playlist");
    let header_scroll = build_header_scroll(name);
    let content = &header_scroll.content;

    let info_section = build_playlist_info_section(playlist);
    content.append(&info_section);

    let (quality_dropdown, download_button) =
        build_detail_controls(content, "Download Playlist", false);

    let (tracks_header, track_container) = build_item_section("Tracks", 4);
    content.append(&tracks_header);
    content.append(&track_container);

    {
        let tracks = playlist
            .tracks
            .as_ref()
            .and_then(|t| t.items.as_ref())
            .map(Vec::as_slice)
            .unwrap_or_default();

        for track in tracks {
            let track_row = build_track_row(track);
            track_container.append(&track_row);
        }
    }

    let toast_overlay = wrap_toast_overlay(&header_scroll);

    wire_download_button(
        &download_button,
        &quality_dropdown,
        playlist,
        settings,
        cmd_sender,
        &toast_overlay,
    );

    header_scroll.toolbar
}

/// Builds the playlist info section with cover art, creator, metadata, and description.
fn build_playlist_info_section(playlist: &Playlist) -> Box {
    let section = Box::new(Vertical, 12);

    build_cover_art(&section, playlist.best_image_url(true));

    let name = playlist.name.as_deref().unwrap_or("Unknown Playlist");
    append_title_label(&section, name, "title-1");

    let creator = playlist.creator_name().unwrap_or("Unknown Creator");
    append_title_label(&section, &format!("By {creator}"), "title-4");

    let track_count = playlist.tracks_count.unwrap_or(0);
    let total_duration = playlist.duration.unwrap_or(0);
    append_track_count_duration(&section, track_count, total_duration);

    if let Some(desc) = playlist.description.as_deref().filter(|d| !d.is_empty()) {
        let cleaned = strip_html_tags(desc);
        if !cleaned.is_empty() {
            let desc_label = Label::new(Some(&cleaned));
            desc_label.set_xalign(0.0);
            desc_label.set_wrap(true);
            desc_label.add_css_class("dim-label");
            section.append(&desc_label);
        }
    }

    append_separator(&section);

    section
}

/// Wires the download button to enqueue the entire playlist as a single download task.
fn wire_download_button(
    button: &Button,
    quality_dropdown: &DropDown,
    playlist: &Playlist,
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
    toast_overlay: &ToastOverlay,
) {
    let playlist_title = playlist
        .name
        .as_deref()
        .unwrap_or("Unknown Playlist")
        .to_string();

    let cover_url = playlist.best_image_url(true);
    let id = playlist.id.clone().unwrap_or_default();

    connect_download_click(
        button,
        quality_dropdown,
        settings,
        toast_overlay,
        move |quality, base_dir| {
            let item = PlaylistItem {
                playlist_id: id.clone(),
                title: playlist_title.clone(),
                cover_url: cover_url.clone(),
            };
            let task = DownloadTask::new(item, quality, base_dir);
            send_enqueue(&cmd_sender, task);
        },
    );
}

//! Album detail view UI.

use std::sync::Arc;

use {
    async_channel::{Sender, bounded},
    libadwaita::{
        HeaderBar, ToolbarView,
        gio::spawn_blocking,
        glib::MainContext,
        gtk::{
            Align::{Center, Start},
            Box, Button, DropDown, Image, Label,
            Orientation::{Horizontal, Vertical},
            PolicyType::Automatic,
            ScrolledWindow,
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, ButtonExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::models::{album::Album, track::Track},
    tracing::{error, warn},
};

use crate::{
    cover_art::{bytes_to_texture, fetch_image_bytes},
    download::{
        progress::{
            DownloadCommand::{self, Enqueue},
            DownloadItem, DownloadTask,
            Quality::{self, Flac16_44, Flac24_96, Flac24_192, Mp3_320},
        },
        worker::album_output_dir,
    },
    preferences::settings::AppSettings,
};

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
    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();
    let title = album.title.as_deref().unwrap_or("Album");
    let title_label = Label::new(Some(title));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    toolbar.add_top_bar(&header);

    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_policy(Automatic, Automatic);

    let content = Box::new(Vertical, 12);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);

    let info_section = build_album_info_section(album);
    content.append(&info_section);

    let quality_dropdown = DropDown::from_strings(&[
        "MP3 320kbps",
        "FLAC 16-bit / 44.1kHz",
        "FLAC 24-bit / 96kHz",
        "FLAC 24-bit / 192kHz",
    ]);
    quality_dropdown.set_selected(1);

    let quality_row = Box::new(Horizontal, 8);
    quality_row.set_valign(Center);
    let ql = Label::new(Some("Quality:"));
    ql.set_xalign(0.0);
    quality_row.append(&ql);
    quality_row.append(&quality_dropdown);
    content.append(&quality_row);

    let download_button = Button::builder()
        .label("Download Album")
        .css_classes(["suggested-action", "pill"])
        .halign(Start)
        .sensitive(false)
        .build();
    content.append(&download_button);

    let tracks_header = Label::new(Some("Tracks"));
    tracks_header.add_css_class("heading");
    tracks_header.set_xalign(0.0);
    tracks_header.set_margin_top(12);
    content.append(&tracks_header);

    let track_container = Box::new(Vertical, 4);
    track_container.set_margin_top(4);

    let loading_label = Label::new(Some("Loading tracks..."));
    loading_label.add_css_class("dim-label");
    loading_label.set_xalign(0.0);
    track_container.append(&loading_label);

    content.append(&track_container);

    scrolled.set_child(Some(&content));
    toolbar.set_content(Some(&scrolled));

    AlbumDetailWidgets {
        root: toolbar,
        track_container,
        download_button,
        quality_dropdown,
    }
}

/// Sends a download task to the queue.
fn send_enqueue(cmd_sender: &Sender<DownloadCommand>, task: DownloadTask) {
    if let Err(e) = cmd_sender.send_blocking(Enqueue { task }) {
        error!(error = %e, "Failed to enqueue album track");
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

/// Maps `DropDown` index to quality.
fn quality_from_index(index: u32) -> Quality {
    match index {
        0 => Mp3_320,
        2 => Flac24_96,
        3 => Flac24_192,
        _ => Flac16_44,
    }
}

/// Formats duration in seconds to MM:SS.
fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Builds the album info section with cover art, artist, metadata, and separator.
fn build_album_info_section(album: &Album) -> Box {
    let section = Box::new(Vertical, 12);

    let cover = Image::new();
    cover.set_pixel_size(200);
    cover.set_halign(Center);
    cover.set_valign(Center);

    section.append(&cover);

    let cover_url = album.image.as_ref().and_then(|img| {
        img.large
            .clone()
            .or_else(|| img.medium.clone())
            .or_else(|| img.thumbnail.clone())
    });
    if let Some(url) = cover_url {
        load_cover_art(&cover, url);
    }

    let artist_name = album
        .artist
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist");
    let artist_label = Label::new(Some(artist_name));
    artist_label.add_css_class("title-2");
    artist_label.set_xalign(0.0);
    section.append(&artist_label);

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
        section.append(&meta_label);
    }

    let track_count = album.tracks_count.unwrap_or(0);
    let total_duration = album.duration.unwrap_or(0);
    let duration_str = if total_duration > 0 {
        format!("{track_count} tracks • {}", format_duration(total_duration))
    } else {
        format!("{track_count} tracks")
    };
    let info_label = Label::new(Some(&duration_str));
    info_label.add_css_class("dim-label");
    info_label.set_xalign(0.0);
    section.append(&info_label);

    let separator = Label::new(Some(""));
    separator.add_css_class("separator");
    section.append(&separator);

    section
}

/// Builds a single track row widget.
fn build_track_row(track: &Track) -> Box {
    let row = Box::new(Horizontal, 8);
    row.set_margin_top(4);
    row.set_margin_bottom(4);

    let track_num = track.track_number.unwrap_or(0);
    let num_label = Label::new(Some(&format!("{track_num:02}.")));
    num_label.set_xalign(0.0);
    num_label.set_width_chars(4);
    num_label.add_css_class("dim-label");
    row.append(&num_label);

    let title = track.title.as_deref().unwrap_or("Unknown Track");
    let title_label = Label::new(Some(title));
    title_label.set_hexpand(true);
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(End);
    row.append(&title_label);

    let duration = track.duration.unwrap_or(0);
    let dur_label = Label::new(Some(&format_duration(duration)));
    dur_label.add_css_class("dim-label");
    dur_label.set_xalign(1.0);
    row.append(&dur_label);

    let is_hires = track.hires.unwrap_or(false)
        || track.maximum_bit_depth.unwrap_or(0) >= 24
        || track.maximum_sampling_rate.unwrap_or(0.0) > 48.0;
    if is_hires {
        let hires_label = Label::new(Some("Hi-Res"));
        hires_label.set_margin_start(8);
        hires_label.add_css_class("badge");
        row.append(&hires_label);
    }

    row
}

/// Loads cover art asynchronously from a URL.
fn load_cover_art(cover: &Image, url: String) {
    let cover_clone = cover.clone();
    let (tx, rx) = bounded::<Vec<u8>>(1);
    spawn_blocking(move || fetch_cover_art_blocking(&url, &tx));
    MainContext::default().spawn_local(async move {
        let Ok(bytes) = rx.recv().await else {
            return;
        };
        let Some(tex) = bytes_to_texture(bytes) else {
            return;
        };
        cover_clone.set_paintable(Some(&tex));
    });
}

/// Fetches cover art bytes in a blocking context.
fn fetch_cover_art_blocking(url: &str, tx: &Sender<Vec<u8>>) {
    let Some(bytes) = fetch_image_bytes(url) else {
        return;
    };
    if tx.send_blocking(bytes).is_err() {
        warn!(url = %url, "Failed to send image bytes to channel");
    }
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

    let track_info: Vec<(i32, String, i32)> = tracks
        .iter()
        .filter_map(|t| {
            let id = t.id?;
            let title = t.title.as_deref().unwrap_or("Unknown Track").to_string();
            let num = t.track_number.unwrap_or(0);
            Some((id, title, num))
        })
        .collect();

    let quality_dropdown = quality_dropdown.clone();

    button.connect_clicked(move |_| {
        let quality = quality_from_index(quality_dropdown.selected());
        let base_dir = {
            let s = settings.lock();
            s.download_directory.clone()
        };

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
    });
}

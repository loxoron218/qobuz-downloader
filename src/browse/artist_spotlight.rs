//! Artist detail view UI.

use std::{
    hint::black_box,
    sync::{Arc, atomic::AtomicU64},
};

use {
    async_channel::Sender,
    libadwaita::{
        Toast, ToastOverlay, ToolbarView,
        gtk::{
            Align::Start,
            Box, Button, DropDown, GestureClick, Image, Label,
            Orientation::{Horizontal, Vertical},
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, GestureSingleExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::api::{
        content::{albums::Album, artists::Artist},
        service::QobuzApiService,
    },
};

use crate::{
    browse::{
        BrowseEvent, browse_album,
        detail_stage::{
            controls::{
                append_separator, append_title_label, build_detail_controls, build_header_scroll,
                build_item_section, connect_download_click, send_enqueue, wrap_toast_overlay,
            },
            exhibit::{build_cover_art, load_cover_art, resolve_image_url, strip_html_tags},
            track_row::format_duration,
        },
    },
    download::progress::{DownloadCommand, DownloadItem::Artist as ItemArtist, DownloadTask},
    preferences::settings::AppSettings,
    shell::debounce_elapsed,
};

/// Builds the artist detail view with albums from the artist catalog.
///
/// # Arguments
///
/// * `artist` - Artist metadata
/// * `albums` - Albums from the artist catalog
/// * `settings` - Application settings (for download directory)
/// * `cmd_sender` - Channel to enqueue downloads
///
/// # Returns
///
/// Root toolbar view.
pub fn build(
    artist: &Artist,
    albums: &[Album],
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    browse_sender: &Sender<BrowseEvent>,
) -> ToolbarView {
    let name = artist.name.as_deref().unwrap_or("Artist");
    let header_scroll = build_header_scroll(name);
    let content = &header_scroll.content;

    let toast_overlay = wrap_toast_overlay(&header_scroll);

    let info_section = build_artist_info_section(artist);
    content.append(&info_section);

    let (quality_dropdown, download_button) =
        build_detail_controls(content, "Download All Albums", false);

    let (albums_header, album_container) = build_item_section("Albums", 6);
    content.append(&albums_header);
    content.append(&album_container);

    for album in albums {
        let album_row = build_album_row(album);
        album_container.append(&album_row);

        if let Some(album_id) = album.id.clone() {
            let gesture = GestureClick::new();
            gesture.set_button(1);
            attach_album_nav_handler(
                &gesture,
                album_id,
                api_service,
                browse_sender,
                &toast_overlay,
            );
            album_row.add_controller(gesture);
        }
    }

    wire_download_button(
        &download_button,
        &quality_dropdown,
        artist,
        settings,
        cmd_sender,
        &toast_overlay,
    );

    header_scroll.toolbar
}

/// Builds the artist info section with name and image.
fn build_artist_info_section(artist: &Artist) -> Box {
    let section = Box::builder()
        .orientation(Vertical)
        .spacing(8)
        .css_classes(["artist-info-section"])
        .tooltip_text("Artist information")
        .build();

    build_cover_art(
        &section,
        resolve_image_url(artist.image.as_ref().or(artist.picture.as_ref())),
    );

    let name = artist.name.as_deref().unwrap_or("Unknown Artist");
    append_title_label(&section, name, "title-1");

    let albums_count = artist.albums_count.unwrap_or(0);
    let info_label = Label::builder()
        .label(format!("{albums_count} albums"))
        .css_classes(["dim-label"])
        .tooltip_text("Album count")
        .xalign(0.0)
        .build();
    section.append(&info_label);

    if let Some(bio) = artist
        .biography
        .as_ref()
        .and_then(|b| b.summary.as_deref().filter(|s| !s.is_empty()))
    {
        let cleaned = strip_html_tags(bio);
        if !cleaned.is_empty() {
            let bio_label = Label::builder()
                .label(&cleaned)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .tooltip_text("Artist biography")
                .build();
            section.append(&bio_label);
        }
    }

    append_separator(&section);

    section.update_property(&[PropertyLabel("Artist information")]);
    section
}

/// Builds a single album row widget with cover art thumbnail.
fn build_album_row(album: &Album) -> Box {
    let row = Box::builder()
        .orientation(Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["album-row"])
        .tooltip_text("Album row")
        .build();

    let cover = Image::builder()
        .pixel_size(64)
        .halign(Start)
        .valign(Start)
        .css_classes(["album-thumbnail"])
        .tooltip_text("Album cover thumbnail")
        .build();

    let cover_url = resolve_image_url(album.image.as_ref());
    if let Some(url) = cover_url {
        load_cover_art(&cover, url);
    }
    row.append(&cover);

    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(Start)
        .css_classes(["album-row-info"])
        .tooltip_text("Album details")
        .build();

    let title = album.title.as_deref().unwrap_or("Unknown Album");
    let title_label = Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(End)
        .css_classes(["album-row-title"])
        .tooltip_text("Album title")
        .build();
    info_box.append(&title_label);

    let metadata_parts: Vec<String> = {
        let mut parts = Vec::new();
        if let Some(count) = album.tracks_count {
            parts.push(format!("{count} tracks"));
        }
        if let Some(dur) = album.duration.filter(|&d| d > 0) {
            parts.push(format_duration(dur));
        }
        if let Some(year) = album
            .release_date_original
            .as_deref()
            .and_then(|d| d.split('-').next())
        {
            parts.push(year.to_string());
        }
        parts
    };
    if !metadata_parts.is_empty() {
        let meta_label = Label::builder()
            .label(metadata_parts.join(" • "))
            .css_classes(["dim-label"])
            .tooltip_text("Album metadata")
            .xalign(0.0)
            .ellipsize(End)
            .build();
        info_box.append(&meta_label);
    }

    row.append(&info_box);
    row.update_property(&[PropertyLabel("Album row")]);
    row
}

/// Attaches a single-click gesture to navigate to the album's detail page.
fn attach_album_nav_handler(
    gesture: &GestureClick,
    album_id: String,
    api_service: &Arc<Mutex<QobuzApiService>>,
    browse_sender: &Sender<BrowseEvent>,
    toast_overlay: &ToastOverlay,
) {
    let api = Arc::clone(api_service);
    let sender = browse_sender.clone();
    let id = album_id;
    let overlay = toast_overlay.clone();
    let last_nav_ms: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let album_nav_handler = gesture.connect_pressed(move |_, _, _, _| {
        if !debounce_elapsed(&last_nav_ms, 500) {
            return;
        }
        let toast = Toast::new("Opening album…");
        toast.set_timeout(2);
        overlay.add_toast(toast);
        browse_album(Arc::clone(&api), id.clone(), sender.clone());
    });
    for _ in [black_box(album_nav_handler)] {}
}

/// Wires the download button to enqueue the entire artist as a single download task.
fn wire_download_button(
    button: &Button,
    quality_dropdown: &DropDown,
    artist: &Artist,
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
    toast_overlay: &ToastOverlay,
) {
    let artist_name = artist
        .name
        .as_deref()
        .unwrap_or("Unknown Artist")
        .to_string();
    let artist_id = artist.id.unwrap_or(0);
    let cover_url = resolve_image_url(artist.image.as_ref().or(artist.picture.as_ref()));

    connect_download_click(
        button,
        quality_dropdown,
        settings,
        toast_overlay,
        move |quality, base_dir| {
            let item = ItemArtist {
                artist_id,
                name: artist_name.clone(),
                cover_url: cover_url.clone(),
            };
            let task = DownloadTask::new(item, quality, base_dir);
            send_enqueue(&cmd_sender, task);
        },
    );
}

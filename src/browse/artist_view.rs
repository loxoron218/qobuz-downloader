//! Artist detail view UI.

use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use {
    async_channel::Sender,
    libadwaita::{
        Toast, ToastOverlay, ToolbarView,
        gtk::{
            Align::Start,
            Box, Button, DropDown, GestureClick, Image, Label,
            Orientation::{Horizontal, Vertical},
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, GestureSingleExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::{
        api::service::QobuzApiService,
        models::{album::Album, artist::Artist},
    },
};

use crate::{
    browse::{
        BrowseEvent, browse_album,
        detail_common::{
            append_separator, append_title_label, build_cover_art, build_detail_controls,
            build_header_scroll, build_item_section, connect_download_click, format_duration,
            load_cover_art, resolve_image_url, send_enqueue, strip_html_tags, wrap_toast_overlay,
        },
    },
    download::progress::{DownloadCommand, DownloadItem::Artist as ItemArtist, DownloadTask},
    preferences::settings::AppSettings,
};

/// Widgets returned by the artist detail view builder.
#[derive(Clone)]
pub struct ArtistDetailWidgets {
    /// Root toolbar view widget.
    pub root: ToolbarView,
    /// Container box where album rows are appended.
    pub album_container: Box,
    /// Download all button.
    pub download_button: Button,
    /// Quality selector dropdown.
    pub quality_dropdown: DropDown,
    /// Toast overlay for transient notifications.
    pub toast_overlay: ToastOverlay,
}

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
/// Artist detail view widgets
pub fn build(
    artist: &Artist,
    albums: &[Album],
    settings: Arc<Mutex<AppSettings>>,
    cmd_sender: Sender<DownloadCommand>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    browse_sender: &Sender<BrowseEvent>,
) -> ArtistDetailWidgets {
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

    ArtistDetailWidgets {
        root: header_scroll.toolbar,
        album_container,
        download_button,
        quality_dropdown,
        toast_overlay,
    }
}

/// Builds the artist info section with name and image.
fn build_artist_info_section(artist: &Artist) -> Box {
    let section = Box::new(Vertical, 8);

    build_cover_art(
        &section,
        resolve_image_url(artist.image.as_ref().or(artist.picture.as_ref())),
    );

    let name = artist.name.as_deref().unwrap_or("Unknown Artist");
    append_title_label(&section, name, "title-1");

    let albums_count = artist.albums_count.unwrap_or(0);
    let info_label = Label::new(Some(&format!("{albums_count} albums")));
    info_label.add_css_class("dim-label");
    info_label.set_xalign(0.0);
    section.append(&info_label);

    if let Some(bio) = artist
        .biography
        .as_ref()
        .and_then(|b| b.summary.as_deref().filter(|s| !s.is_empty()))
    {
        let cleaned = strip_html_tags(bio);
        if !cleaned.is_empty() {
            let bio_label = Label::new(Some(&cleaned));
            bio_label.set_xalign(0.0);
            bio_label.set_wrap(true);
            bio_label.add_css_class("dim-label");
            section.append(&bio_label);
        }
    }

    append_separator(&section);

    section
}

/// Builds a single album row widget with cover art thumbnail.
fn build_album_row(album: &Album) -> Box {
    let row = Box::new(Horizontal, 8);
    row.set_margin_top(4);
    row.set_margin_bottom(4);

    let cover = Image::new();
    cover.set_pixel_size(64);
    cover.set_halign(Start);
    cover.set_valign(Start);

    let cover_url = resolve_image_url(album.image.as_ref());
    if let Some(url) = cover_url {
        load_cover_art(&cover, url);
    }
    row.append(&cover);

    let info_box = Box::new(Vertical, 2);
    info_box.set_hexpand(true);
    info_box.set_valign(Start);

    let title = album.title.as_deref().unwrap_or("Unknown Album");
    let title_label = Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(End);
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
        let meta_label = Label::new(Some(&metadata_parts.join(" • ")));
        meta_label.add_css_class("dim-label");
        meta_label.set_xalign(0.0);
        meta_label.set_ellipsize(End);
        info_box.append(&meta_label);
    }

    row.append(&info_box);
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
    let last_nav_ms: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    gesture.connect_pressed(move |_, _, _, _| {
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let now = dur.as_secs() * 1000 + u64::from(dur.subsec_millis());
        if now - last_nav_ms.get() < 500 {
            return;
        }
        last_nav_ms.set(now);
        let toast = Toast::new("Opening album…");
        toast.set_timeout(2);
        overlay.add_toast(toast);
        browse_album(Arc::clone(&api), id.clone(), sender.clone());
    });
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

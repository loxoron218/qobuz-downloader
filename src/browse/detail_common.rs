//! Shared widgets and utilities for detail views.

use std::{path::PathBuf, sync::Arc};

use {
    async_channel::{Sender, bounded},
    libadwaita::{
        HeaderBar, Toast, ToastOverlay, ToolbarView,
        gio::spawn_blocking,
        glib::MainContext,
        gtk::{
            Align::{Center, Fill, Start},
            Box, Button, DropDown, Image, Label,
            Orientation::{Horizontal, Vertical},
            Picture,
            PolicyType::Automatic,
            ScrolledWindow, Widget, gdk,
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, ButtonExt, TextureExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::models::{album::Image as ModelImage, track::Track},
    tracing::{error, warn},
};

use crate::{
    cover_art::{bytes_to_texture, fetch_image_bytes},
    download::progress::{
        DownloadCommand::{self, Enqueue},
        DownloadTask,
    },
    preferences::settings::AppSettings,
    types::Quality::{self, Flac16_44, Flac24_96, Flac24_192, Mp3_320},
};

/// Widgets for the header bar + scrolled content layout.
pub struct HeaderScrollWidgets {
    /// Top toolbar with header bar.
    pub toolbar: ToolbarView,
    /// Content box inside the scrolled window.
    pub content: Box,
}

/// Builds the standard detail view chrome: header bar, scrolled window, and content box.
pub fn build_header_scroll(title: &str) -> HeaderScrollWidgets {
    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();
    let title_label = Label::new(Some(title));
    title_label.add_css_class("title");
    title_label.set_ellipsize(End);
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

    scrolled.set_child(Some(&content));
    toolbar.set_content(Some(&scrolled));

    HeaderScrollWidgets { toolbar, content }
}

/// Wraps the toolbar content in a `ToastOverlay` for transient notifications.
pub fn wrap_toast_overlay(header_scroll: &HeaderScrollWidgets) -> ToastOverlay {
    let toast_overlay = ToastOverlay::new();
    if let Some(child) = header_scroll.toolbar.content() {
        header_scroll.toolbar.set_content(None::<&Widget>);
        toast_overlay.set_child(Some(&child));
    }
    header_scroll.toolbar.set_content(Some(&toast_overlay));
    toast_overlay
}

/// Creates and appends a title label with standard configuration.
pub fn append_title_label(parent: &Box, text: &str, css_class: &str) {
    let label = Label::new(Some(text));
    label.add_css_class(css_class);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(End);
    parent.append(&label);
}

/// Creates a quality selector dropdown.
pub fn build_quality_dropdown() -> DropDown {
    let dropdown = DropDown::from_strings(&[
        "MP3 320kbps",
        "FLAC 16-bit / 44.1kHz",
        "FLAC 24-bit / 96kHz",
        "FLAC 24-bit / 192kHz",
    ]);
    dropdown.set_selected(1);
    dropdown
}

/// Builds a quality selector row with label.
pub fn build_quality_row(dropdown: &DropDown, valign_center: bool) -> Box {
    let quality_row = Box::new(Horizontal, 8);
    if valign_center {
        quality_row.set_valign(Center);
    } else {
        quality_row.set_valign(Start);
    }
    let ql = Label::new(Some("Quality:"));
    ql.set_xalign(0.0);
    quality_row.append(&ql);
    quality_row.append(dropdown);
    quality_row
}

/// Builds a download action button.
pub fn build_download_button(label: &str) -> Button {
    Button::builder()
        .label(label)
        .css_classes(["suggested-action", "pill"])
        .halign(Start)
        .build()
}

/// Builds a section header label and item container.
pub fn build_item_section(header_text: &str, spacing: i32) -> (Label, Box) {
    let header = Label::new(Some(header_text));
    header.add_css_class("heading");
    header.set_xalign(0.0);
    header.set_margin_top(12);

    let container = Box::new(Vertical, spacing);
    container.set_margin_top(4);

    (header, container)
}

/// Appends a separator label to a section.
pub fn append_separator(section: &Box) {
    let separator = Label::new(Some(""));
    separator.add_css_class("separator");
    section.append(&separator);
}

/// Appends a track count and duration label to a section.
pub fn append_track_count_duration(section: &Box, track_count: i32, total_duration: i32) {
    let duration_str = if total_duration > 0 {
        format!("{track_count} tracks • {}", format_duration(total_duration))
    } else {
        format!("{track_count} tracks")
    };
    let info_label = Label::new(Some(&duration_str));
    info_label.add_css_class("dim-label");
    info_label.set_xalign(0.0);
    info_label.set_ellipsize(End);
    section.append(&info_label);
}

/// Maps `DropDown` index to quality.
pub fn quality_from_index(index: u32) -> Quality {
    match index {
        0 => Mp3_320,
        2 => Flac24_96,
        3 => Flac24_192,
        _ => Flac16_44,
    }
}

/// Formats duration in seconds to MM:SS.
pub fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Sends a download task to the queue.
pub fn send_enqueue(cmd_sender: &Sender<DownloadCommand>, task: DownloadTask) {
    if let Err(e) = cmd_sender.send_blocking(Enqueue { task }) {
        error!(error = %e, "Failed to enqueue download task");
    }
}

/// Builds a single track row widget.
pub fn build_track_row(track: &Track) -> Box {
    let row = Box::new(Horizontal, 8);
    row.set_margin_top(4);
    row.set_margin_bottom(4);

    let disc_num = track.media_number.unwrap_or(1);
    let disc_label = Label::new(Some(&disc_num.to_string()));
    disc_label.set_xalign(0.0);
    disc_label.set_width_chars(3);
    disc_label.add_css_class("dim-label");
    row.append(&disc_label);

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
        let hires_icon = Image::builder()
            .halign(Center)
            .valign(Center)
            .tooltip_text("Hi-Res Audio")
            .pixel_size(16)
            .build();
        hires_icon.set_from_file(Some("./assets/hires.png"));
        hires_icon.set_margin_start(8);
        row.append(&hires_icon);
    }

    row
}

/// Builds quality dropdown row and download button, appending them to content.
pub fn build_detail_controls(
    content: &Box,
    button_label: &str,
    valign_center: bool,
) -> (DropDown, Button) {
    let quality_dropdown = build_quality_dropdown();
    let quality_row = build_quality_row(&quality_dropdown, valign_center);
    content.append(&quality_row);

    let download_button = build_download_button(button_label);
    content.append(&download_button);

    (quality_dropdown, download_button)
}

/// Connects a download button click to extract quality and base directory, then calls the callback.
pub fn connect_download_click<F>(
    button: &Button,
    quality_dropdown: &DropDown,
    settings: Arc<Mutex<AppSettings>>,
    toast_overlay: &ToastOverlay,
    f: F,
) where
    F: Fn(Quality, PathBuf) + 'static,
{
    let quality_dropdown = quality_dropdown.clone();
    let toast_overlay = toast_overlay.clone();
    button.connect_clicked(move |_| {
        let quality = quality_from_index(quality_dropdown.selected());
        let base_dir = settings.lock().download_directory.clone();
        let toast = Toast::new("Added to download queue");
        toast.set_timeout(2);
        toast_overlay.add_toast(toast);
        f(quality, base_dir);
    });
}

/// Creates a cover art section that adapts based on the image aspect ratio.
///
/// Square images use a fixed-size `Image` (250px, left-aligned). Rectangular
/// images use an expanding `Picture` that fills the available width.
pub fn build_cover_art(section: &Box, url: Option<String>) {
    let image = Image::new();
    image.set_pixel_size(250);
    image.set_halign(Start);
    image.set_valign(Start);
    section.append(&image);

    let picture = Picture::new();
    picture.set_hexpand(true);
    picture.set_halign(Fill);
    picture.set_size_request(250, 250);
    picture.set_visible(false);
    section.append(&picture);

    let Some(url) = url else {
        return;
    };

    spawn_cover_load(url, move |tex| {
        let w = tex.width();
        let h = tex.height();
        let ratio = f64::from(w) / f64::from(h);
        if (ratio - 1.0).abs() < 0.20 {
            image.set_paintable(Some(&tex));
        } else {
            picture.set_paintable(Some(&tex));
            image.set_visible(false);
            picture.set_visible(true);
        }
    });
}

/// Resolves the best available cover art URL from an optional image.
///
/// Tries multiple image sizes in order of preference, falling back to the
/// generic `url` field (which some endpoints return instead of a size map).
pub fn resolve_image_url(img: Option<&ModelImage>) -> Option<String> {
    let img = img?;
    img.large
        .clone()
        .or_else(|| img.extra_large.clone())
        .or_else(|| img.medium.clone())
        .or_else(|| img.thumbnail.clone())
        .or_else(|| img.small.clone())
        .or_else(|| img.url.clone())
}

/// Spawns the common fetch → decode → apply pipeline for cover art.
fn spawn_cover_load(url: String, apply: impl FnOnce(gdk::Texture) + 'static) {
    let (tx, rx) = bounded::<Vec<u8>>(1);
    spawn_blocking(move || fetch_cover_art_blocking(&url, &tx));
    MainContext::default().spawn_local(async move {
        let Ok(bytes) = rx.recv().await else {
            return;
        };
        let Some(tex) = bytes_to_texture(bytes) else {
            return;
        };
        apply(tex);
    });
}

/// Loads cover art asynchronously from a URL into an `Image` widget.
pub fn load_cover_art(cover: &Image, url: String) {
    let cover_clone = cover.clone();
    spawn_cover_load(url, move |tex| {
        cover_clone.set_paintable(Some(&tex));
    });
}

/// Strips HTML tags from a string, replacing `<br>` with newlines and paragraph boundaries with
/// double newlines.
pub fn strip_html_tags(input: &str) -> String {
    let result = input
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<BR>", "\n")
        .replace("<BR/>", "\n")
        .replace("<BR />", "\n")
        .replace("</p><p>", "\n\n")
        .replace("</p> <p>", "\n\n")
        .replace("</P><P>", "\n\n")
        .replace("</P> <P>", "\n\n")
        .replace("<p>", "")
        .replace("</p>", "")
        .replace("<P>", "")
        .replace("</P>", "");

    let mut cleaned = String::with_capacity(result.len());
    let mut in_tag = false;
    for ch in result.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => cleaned.push(ch),
            _ => {}
        }
    }

    cleaned.trim().to_string()
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

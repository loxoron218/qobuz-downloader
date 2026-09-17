//! Search result rows: data rows, indicators, and download actions.

use std::{hint::black_box, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        SplitButton, Toast,
        gtk::{
            Align::{Center, End as AlignEnd, Start},
            Box, Button, Image, Label, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture, Popover,
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, ButtonExt, ListBoxRowExt, PopoverExt, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::warn,
};

use crate::{
    download::progress::{
        DownloadCommand::{self, Enqueue},
        DownloadItem::{
            self, Album as DownloadAlbum, Artist as DownloadArtist, Playlist as DownloadPlaylist,
            Track as DownloadTrack,
        },
        DownloadTask,
    },
    preferences::settings::AppSettings,
    search::view::{
        SearchCtx,
        SearchResultItem::{self, Album, Artist, Playlist, Track},
        thumbnails::{AsyncCoverFetch, attach_cover_art},
    },
};

/// Pushes formatted duration and quality string parts if non-empty.
fn push_quality_parts(parts: &mut Vec<String>, duration: i32, bit_depth: i32, sampling_rate: f64) {
    if duration > 0 {
        parts.push(format_duration(duration));
    }
    let q = quality_string(bit_depth, sampling_rate);
    if !q.is_empty() {
        parts.push(q);
    }
}

/// Formats a subtitle string for a search result item.
fn build_subtitle(item: &SearchResultItem) -> String {
    match item {
        Track {
            artist,
            album,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone(), album.clone()];
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        Album {
            artist,
            year,
            track_count,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone()];
            if !year.is_empty() {
                parts.push(year.clone());
            }
            if *track_count > 0 {
                parts.push(format!("{track_count} tracks"));
            }
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        Artist { .. } => String::from("Artist"),
        Playlist { .. } => String::from("Playlist"),
    }
}

/// Creates a data row for a search result item.
///
/// Returns the row and, for artist/playlist items lacking cover art, the info needed
/// to fetch and apply their cover asynchronously.
#[must_use]
pub fn create_data_row(
    item: &SearchResultItem,
    ctx: &SearchCtx,
) -> (ListBoxRow, Option<AsyncCoverFetch>) {
    let picture = Picture::new();
    picture.set_size_request(64, 64);
    picture.add_css_class("thumbnail");
    picture.set_tooltip_text(Some("Cover art"));

    let row_box = Box::new(Horizontal, 16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);

    row_box.append(&picture);
    row_box.append(&build_info_box(item));
    row_box.append(&build_explicit_indicator(item));
    row_box.append(&build_hires_indicator(item));
    row_box.append(&create_split_button(item, ctx));

    attach_cover_art(item, &picture, ctx);

    let pending_fetch = match item {
        Artist { id, cover_url, .. } if cover_url.is_none() => Some(AsyncCoverFetch {
            picture,
            id: id.to_string(),
            is_artist: true,
        }),
        Playlist { id, cover_url, .. } if cover_url.is_none() => Some(AsyncCoverFetch {
            picture,
            id: id.clone(),
            is_artist: false,
        }),
        _ => None,
    };

    let row = ListBoxRow::new();
    row.set_child(Some(&row_box));

    (row, pending_fetch)
}

/// Builds the info box containing title and subtitle labels.
fn build_info_box(item: &SearchResultItem) -> Box {
    let info_box = Box::new(Vertical, 2);
    info_box.set_hexpand(true);
    info_box.set_valign(Center);

    let title = match item {
        Track { title, .. } | Album { title, .. } => title.clone(),
        Artist { name, .. } | Playlist { name, .. } => name.clone(),
    };
    let subtitle = build_subtitle(item);

    let title_label = Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(End);
    title_label.add_css_class("title-4");

    let subtitle_label = Label::new(Some(&subtitle));
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_ellipsize(End);
    subtitle_label.add_css_class("dim-label");

    info_box.append(&title_label);
    info_box.append(&subtitle_label);

    info_box
}

/// Builds the explicit content indicator ("E" label).
fn build_explicit_indicator(item: &SearchResultItem) -> Box {
    let is_explicit = match item {
        Track { is_explicit, .. } | Album { is_explicit, .. } | Playlist { is_explicit, .. } => {
            *is_explicit
        }
        Artist { .. } => false,
    };

    let container = Box::new(Vertical, 0);
    let label = Label::new(Some("E"));
    label.set_halign(Center);
    label.set_valign(Center);
    label.set_visible(is_explicit);
    label.set_tooltip_text(Some("Explicit Content"));
    label.set_css_classes(&["error", "caption", "explicit-indicator"]);
    container.append(&label);
    container.set_halign(AlignEnd);
    container.set_valign(Center);
    container.set_margin_start(4);

    container
}

/// Builds the Hi-Res audio indicator icon.
fn build_hires_indicator(item: &SearchResultItem) -> Box {
    let is_hires = match item {
        Track {
            bit_depth,
            sampling_rate,
            ..
        }
        | Album {
            bit_depth,
            sampling_rate,
            ..
        } => *bit_depth >= 24 || *sampling_rate > 48.0,
        _ => false,
    };

    let container = Box::new(Vertical, 0);
    let icon = Image::builder()
        .halign(Center)
        .valign(Center)
        .visible(is_hires)
        .tooltip_text("Hi-Res Audio")
        .pixel_size(24)
        .build();
    if is_hires {
        icon.set_from_file(Some("./assets/hires.png"));
    }
    container.append(&icon);
    container.set_halign(AlignEnd);
    container.set_valign(Center);
    container.set_margin_start(8);

    container
}

/// Enqueues a download using settings for quality and output directory.
fn enqueue_from_settings(
    settings: &Arc<Mutex<AppSettings>>,
    cmd_sender: &Sender<DownloadCommand>,
    item: &DownloadItem,
) {
    let (quality, output_dir) = {
        let s = settings.lock();
        (s.default_quality, s.download_directory.clone())
    };
    let task = DownloadTask::new(item.clone(), quality, output_dir);
    if let Err(err) = cmd_sender.send_blocking(Enqueue { task }) {
        warn!(error = %err, "Failed to enqueue download");
    }
}

/// Creates a `SplitButton` with download and add-to-queue actions.
fn create_split_button(item: &SearchResultItem, ctx: &SearchCtx) -> SplitButton {
    let queue_button = Button::builder()
        .label("Add to queue")
        .halign(Start)
        .css_classes(["model"])
        .build();

    let popover_box = Box::new(Vertical, 0);
    popover_box.append(&queue_button);

    let popover = Popover::builder().child(&popover_box).build();

    let split_button = SplitButton::builder()
        .label("Download")
        .tooltip_text("Download")
        .popover(&popover)
        .halign(AlignEnd)
        .valign(Center)
        .css_classes(["suggested-action"])
        .build();

    let download_item = match item {
        Track {
            id,
            title,
            artist,
            cover_url,
            ..
        } => DownloadTrack {
            track_id: *id,
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        Album {
            id,
            title,
            artist,
            cover_url,
            ..
        } => DownloadAlbum {
            album_id: id.clone(),
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        Playlist {
            id,
            name,
            cover_url,
            ..
        } => DownloadPlaylist {
            playlist_id: id.clone(),
            title: name.clone(),
            cover_url: cover_url.clone(),
        },
        Artist { id, name, .. } => DownloadArtist {
            artist_id: *id,
            name: name.clone(),
            cover_url: None,
        },
    };

    {
        let settings = Arc::clone(&ctx.settings);
        let cmd_sender = ctx.cmd_sender.clone();
        let item = download_item.clone();
        let toast_overlay = ctx.toast_overlay.clone();
        let download_clicked = split_button.connect_clicked(move |_| {
            enqueue_from_settings(&settings, &cmd_sender, &item);
            let toast = Toast::new("Added to download queue");
            toast.set_timeout(2);
            toast_overlay.add_toast(toast);
        });
        for _ in [black_box(download_clicked)] {}
    }

    {
        let settings = Arc::clone(&ctx.settings);
        let cmd_sender = ctx.cmd_sender.clone();
        let item = download_item;
        let p = popover;
        let toast_overlay = ctx.toast_overlay.clone();
        let queue_clicked = queue_button.connect_clicked(move |_| {
            enqueue_from_settings(&settings, &cmd_sender, &item);
            let toast = Toast::new("Added to download queue");
            toast.set_timeout(2);
            toast_overlay.add_toast(toast);
            p.popdown();
        });
        for _ in [black_box(queue_clicked)] {}
    }

    split_button
}

/// Formats a duration in seconds to `MM:SS` format.
fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Formats a quality string from bit depth and sampling rate.
fn quality_string(bit_depth: i32, sampling_rate: f64) -> String {
    if bit_depth > 0 && sampling_rate > 0.0 {
        let rate_str = if sampling_rate.fract() == 0.0 {
            format!("{sampling_rate:.0}")
        } else {
            format!("{sampling_rate:.1}")
        };
        format!("{bit_depth}-bit/{rate_str}kHz")
    } else {
        String::new()
    }
}

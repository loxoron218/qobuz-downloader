//! Search result rows: data rows, indicators, and download actions.

use std::{hint::black_box, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        SplitButton, Toast,
        gtk::{
            Align::{Center, End, Start},
            Box, Button, Image, Label, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture, Popover,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, PopoverExt},
    },
    parking_lot::Mutex,
    tracing::warn,
};

use crate::{
    download::progress::{
        DownloadCommand::{self, Enqueue},
        DownloadItem::{self, Album, Artist, Playlist, Track},
        DownloadTask,
    },
    preferences::settings::AppSettings,
    search::view::{
        SearchCtx,
        SearchResultItem::{
            self, Album as ResultAlbum, Artist as ResultArtist, Playlist as ResultPlaylist,
            Track as ResultTrack,
        },
        card_text::build_subtitle,
        thumbnails::{AsyncCoverFetch, attach_cover_art},
    },
};

/// Creates a data row for a search result item.
///
/// Returns the row and, for artist/playlist items lacking cover art, the info needed
/// to fetch and apply their cover asynchronously.
#[must_use]
pub fn create_data_row(
    item: &SearchResultItem,
    ctx: &SearchCtx,
) -> (ListBoxRow, Option<AsyncCoverFetch>) {
    let picture = Picture::builder()
        .width_request(64)
        .height_request(64)
        .css_classes(["thumbnail"])
        .tooltip_text("Cover art")
        .build();

    let row_box = Box::builder()
        .orientation(Horizontal)
        .spacing(16)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .css_classes(["search-row"])
        .tooltip_text("Search result")
        .build();

    row_box.append(&picture);
    row_box.append(&build_info_box(item));
    row_box.append(&build_explicit_indicator(item));
    row_box.append(&build_hires_indicator(item));
    row_box.append(&create_split_button(item, ctx));

    attach_cover_art(item, &picture, ctx);

    let pending_fetch = match item {
        ResultArtist { id, cover_url, .. } if cover_url.is_none() => Some(AsyncCoverFetch {
            picture,
            id: id.to_string(),
            is_artist: true,
        }),
        ResultPlaylist { id, cover_url, .. } if cover_url.is_none() => Some(AsyncCoverFetch {
            picture,
            id: id.clone(),
            is_artist: false,
        }),
        _ => None,
    };

    let row = ListBoxRow::builder()
        .child(&row_box)
        .css_classes(["search-result-row"])
        .tooltip_text("Search result row")
        .build();
    row.update_property(&[PropertyLabel("Search result row")]);

    (row, pending_fetch)
}

/// Builds the info box containing title and subtitle labels.
fn build_info_box(item: &SearchResultItem) -> Box {
    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(Center)
        .css_classes(["search-info-box"])
        .tooltip_text("Result details")
        .build();

    let title = match item {
        ResultTrack { title, .. } | ResultAlbum { title, .. } => title.clone(),
        ResultArtist { name, .. } | ResultPlaylist { name, .. } => name.clone(),
    };
    let subtitle = build_subtitle(item);

    let title_label = Label::builder()
        .label(&title)
        .xalign(0.0)
        .ellipsize(EllipsizeEnd)
        .css_classes(["title-4"])
        .tooltip_text("Result title")
        .build();

    let subtitle_label = Label::builder()
        .label(&subtitle)
        .xalign(0.0)
        .ellipsize(EllipsizeEnd)
        .css_classes(["dim-label"])
        .tooltip_text("Result subtitle")
        .build();

    info_box.append(&title_label);
    info_box.append(&subtitle_label);
    info_box.update_property(&[PropertyLabel("Result details")]);

    info_box
}

/// Builds the explicit content indicator ("E" label).
fn build_explicit_indicator(item: &SearchResultItem) -> Box {
    let is_explicit = match item {
        ResultTrack { is_explicit, .. }
        | ResultAlbum { is_explicit, .. }
        | ResultPlaylist { is_explicit, .. } => *is_explicit,
        ResultArtist { .. } => false,
    };

    let container = Box::builder()
        .orientation(Vertical)
        .halign(End)
        .valign(Center)
        .margin_start(4)
        .css_classes(["explicit-container"])
        .tooltip_text("Explicit content indicator")
        .build();
    let label = Label::builder()
        .label("E")
        .halign(Center)
        .valign(Center)
        .visible(is_explicit)
        .css_classes(["error", "caption", "explicit-indicator"])
        .tooltip_text("Explicit Content")
        .build();
    container.append(&label);
    container.update_property(&[PropertyLabel("Explicit content indicator")]);

    container
}

/// Builds the Hi-Res audio indicator icon.
fn build_hires_indicator(item: &SearchResultItem) -> Box {
    let is_hires = match item {
        ResultTrack {
            bit_depth,
            sampling_rate,
            ..
        }
        | ResultAlbum {
            bit_depth,
            sampling_rate,
            ..
        } => *bit_depth >= 24 || *sampling_rate > 48.0,
        _ => false,
    };

    let container = Box::builder()
        .orientation(Vertical)
        .halign(End)
        .valign(Center)
        .margin_start(8)
        .css_classes(["hires-container"])
        .tooltip_text("Hi-Res audio indicator")
        .build();
    container.update_property(&[PropertyLabel("Hi-Res audio indicator")]);
    let icon = Image::builder()
        .halign(Center)
        .valign(Center)
        .visible(is_hires)
        .tooltip_text("Hi-Res Audio")
        .css_classes(["hires-icon"])
        .pixel_size(24)
        .build();
    if is_hires {
        icon.set_from_file(Some("./assets/hires.png"));
    }
    container.append(&icon);

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
        .css_classes(["model", "text-button"])
        .tooltip_text("Add to download queue")
        .build();

    let popover_box = Box::builder()
        .orientation(Vertical)
        .css_classes(["queue-popover-box"])
        .tooltip_text("Queue options")
        .build();
    popover_box.append(&queue_button);

    let popover = Popover::builder()
        .child(&popover_box)
        .css_classes(["queue-popover"])
        .tooltip_text("Download options")
        .build();

    let split_button = SplitButton::builder()
        .label("Download")
        .tooltip_text("Download")
        .popover(&popover)
        .halign(End)
        .valign(Center)
        .css_classes(["suggested-action", "text-button"])
        .build();
    split_button.update_property(&[PropertyLabel("Download split button")]);

    let download_item = match item {
        ResultTrack {
            id,
            title,
            artist,
            cover_url,
            ..
        } => Track {
            track_id: *id,
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        ResultAlbum {
            id,
            title,
            artist,
            cover_url,
            ..
        } => Album {
            album_id: id.clone(),
            title: title.clone(),
            artist: artist.clone(),
            cover_url: cover_url.clone(),
        },
        ResultPlaylist {
            id,
            name,
            cover_url,
            ..
        } => Playlist {
            playlist_id: id.clone(),
            title: name.clone(),
            cover_url: cover_url.clone(),
        },
        ResultArtist { id, name, .. } => Artist {
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

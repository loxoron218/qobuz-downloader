//! Cover art helpers for detail views.
//!
//! Provides cover art widget builders, image URL resolvers, async loading
//! helpers, and HTML description cleanup for detail pages.

use {
    async_channel::{Sender, bounded},
    libadwaita::{
        gio::spawn_blocking,
        glib::MainContext,
        gtk::{
            Align::{Fill, Start},
            Box, Image, Picture,
            gdk::Texture,
        },
        prelude::{BoxExt, TextureExt, WidgetExt},
    },
    qobuz_api::models::album::Image as ModelImage,
    tracing::warn,
};

use crate::cover_art::{bytes_to_texture, fetch_image_bytes};

/// Creates a cover art section that adapts based on the image aspect ratio.
///
/// Square images use a fixed-size `Image` (250px, left-aligned). Rectangular
/// images use an expanding `Picture` that fills the available width.
///
/// # Arguments
///
/// * `section` - Container the cover widgets are appended to.
/// * `url` - Cover art URL loaded asynchronously when present.
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
///
/// # Arguments
///
/// * `img` - Image model holding the size map.
///
/// # Returns
///
/// * `Option<String>` - Best available image URL, if any.
#[must_use]
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

/// Resolves the best available thumbnail URL from an optional image.
///
/// Prefers the small thumbnail sizes, falling back to the generic `url` field.
///
/// # Arguments
///
/// * `img` - Image model holding the size map.
///
/// # Returns
///
/// * `Option<String>` - Best available thumbnail URL, if any.
#[must_use]
pub fn resolve_thumbnail_url(img: Option<&ModelImage>) -> Option<String> {
    let img = img?;
    img.thumbnail
        .clone()
        .or_else(|| img.small.clone())
        .or_else(|| img.url.clone())
}

/// Spawns the common fetch → decode → apply pipeline for cover art.
///
/// # Arguments
///
/// * `url` - Cover art URL fetched on a blocking thread.
/// * `apply` - Callback applying the decoded texture on the main context.
pub fn spawn_cover_load(url: String, apply: impl FnOnce(Texture) + 'static) {
    let (tx, rx) = bounded::<Vec<u8>>(1);
    drop(spawn_blocking(move || fetch_cover_art_blocking(&url, &tx)));
    drop(MainContext::default().spawn_local(async move {
        let Ok(bytes) = rx.recv().await else {
            return;
        };
        let Some(tex) = bytes_to_texture(bytes) else {
            return;
        };
        apply(tex);
    }));
}

/// Loads cover art asynchronously from a URL into an `Image` widget.
///
/// # Arguments
///
/// * `cover` - Image widget the loaded texture is applied to.
/// * `url` - Cover art URL fetched on a blocking thread.
pub fn load_cover_art(cover: &Image, url: String) {
    let cover_clone = cover.clone();
    spawn_cover_load(url, move |tex| {
        cover_clone.set_paintable(Some(&tex));
    });
}

/// Strips HTML tags from a string, replacing `<br>` with newlines and paragraph boundaries with
/// double newlines.
///
/// # Arguments
///
/// * `input` - HTML string to clean.
///
/// # Returns
///
/// * `String` - Plain text with tags removed and line breaks preserved.
#[must_use]
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
///
/// # Arguments
///
/// * `url` - Cover art URL to fetch.
/// * `tx` - Channel the fetched bytes are sent on.
fn fetch_cover_art_blocking(url: &str, tx: &Sender<Vec<u8>>) {
    let Some(bytes) = fetch_image_bytes(url) else {
        return;
    };
    if tx.send_blocking(bytes).is_err() {
        warn!(url = %url, "Failed to send image bytes to channel");
    }
}

//! Search cover art fetching and texture application.

use std::{collections::HashMap, hint::black_box, sync::Arc, thread::spawn};

use {
    async_channel::{Receiver, SendError, Sender, unbounded},
    libadwaita::{
        gdk::Texture,
        glib::{MainContext, object::Cast},
        gtk::{Box, Image, Picture},
        prelude::{BoxExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
    tracing::warn,
};

use crate::{
    browse::detail_common::portrait::resolve_thumbnail_url,
    cover_art::cache::CoverArtCache,
    search::view::{
        SearchCtx,
        SearchResultItem::{self, Album, Artist, Playlist, Track},
        imagery::TextureEvent::{Loaded, Register},
    },
};

/// A picture widget awaiting an asynchronously fetched cover art.
pub(super) struct AsyncCoverFetch {
    /// The picture widget to update once the cover is fetched.
    pub(super) picture: Picture,
    /// The entity id (artist or playlist) whose cover is fetched.
    pub(super) id: String,
    /// Whether the id refers to an artist (vs. a playlist).
    pub(super) is_artist: bool,
}

/// Cover art load and registration events for the texture receiver.
pub(super) enum TextureEvent {
    /// A picture widget registered itself for a cover URL.
    Register {
        /// The cover URL.
        url: String,
        /// The picture widget awaiting the cover.
        picture: Picture,
    },
    /// A cover art texture finished loading for a URL.
    Loaded {
        /// The cover URL.
        url: String,
        /// The loaded texture, if any.
        texture: Option<Texture>,
    },
}

/// Attaches cover art texture to the picture widget.
pub(super) fn attach_cover_art(item: &SearchResultItem, picture: &Picture, ctx: &SearchCtx) {
    let url = match item {
        Track { cover_url, .. }
        | Album { cover_url, .. }
        | Playlist { cover_url, .. }
        | Artist { cover_url, .. } => cover_url.clone(),
    };

    if let Some(url) = url {
        if let Some(texture) = ctx.cover_art_cache.get(&url) {
            picture.set_paintable(Some(&texture));
        } else {
            register_cover_texture(&ctx.texture_register, url.clone(), picture.clone());
            ctx.cover_art_cache
                .start_load(url, ctx.texture_sender.clone());
        }
    }
}

/// Fetches the cover URL for an artist or playlist id.
fn fetch_cover_url(
    api_service: &Arc<Mutex<QobuzApiService>>,
    id: &str,
    is_artist: bool,
) -> Option<String> {
    if is_artist {
        fetch_artist_cover_url(api_service, id)
    } else {
        fetch_playlist_cover_url(api_service, id)
    }
}

/// Fetches a cover URL and sends it (with its index) via the channel.
fn send_fetched_url(
    tx: &Sender<(usize, Option<String>)>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    idx: usize,
    id: &str,
    is_artist: bool,
) {
    let url = fetch_cover_url(api_service, id, is_artist);
    if tx.send_blocking((idx, url)).is_err() {
        warn!(id = %id, "Failed to send cover URL to channel");
    }
}

/// Fetches missing artist/playlist cover art asynchronously and applies it to the
/// associated picture widget.
///
/// The worker thread only fetches cover URLs; the picture widgets themselves remain
/// owned by the main-thread receiver, since GTK widgets are not thread-safe.
///
/// # Arguments
///
/// * `pending` - Picture widgets (with their entity id and kind) whose cover art still needs to be
///   fetched
/// * `ctx` - Shared search context
pub(super) fn fetch_missing_images(pending: Vec<AsyncCoverFetch>, ctx: &SearchCtx) {
    if pending.is_empty() {
        return;
    }

    let api_service = Arc::clone(&ctx.api_service);
    let cover_art_cache = ctx.cover_art_cache.clone();
    let texture_sender = ctx.texture_sender.clone();
    let texture_register = ctx.texture_register.clone();

    let fetch_requests: Vec<(String, bool)> = pending
        .iter()
        .map(|f| (f.id.clone(), f.is_artist))
        .collect();

    let (tx, rx) = unbounded::<(usize, Option<String>)>();

    drop(spawn(move || {
        for (idx, (id, is_artist)) in fetch_requests.into_iter().enumerate() {
            send_fetched_url(&tx, &api_service, idx, &id, is_artist);
        }
    }));

    drop(MainContext::default().spawn_local(async move {
        while let Ok((idx, url)) = rx.recv().await {
            apply_cover_fetch(
                idx,
                url,
                &pending,
                &cover_art_cache,
                &texture_sender,
                &texture_register,
            );
        }
    }));
}

/// Applies a fetched cover result for the given index to its picture widget.
fn apply_cover_fetch(
    idx: usize,
    url: Option<String>,
    pending: &[AsyncCoverFetch],
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    let Some(fetch) = pending.get(idx) else {
        return;
    };
    apply_fetched_cover(
        fetch,
        url,
        cover_art_cache,
        texture_sender,
        texture_register,
    );
}

/// Applies a fetched cover URL to the matching picture widget.
fn apply_fetched_cover(
    fetch: &AsyncCoverFetch,
    url: Option<String>,
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    let picture = fetch.picture.clone();
    if let Some(url) = url {
        update_picture_with_cover(
            picture,
            url,
            cover_art_cache,
            texture_sender,
            texture_register,
        );
    } else {
        set_fallback_icon(&picture, fetch.is_artist);
    }
}

/// Sets fallback icon on picture widget when no image is available.
fn set_fallback_icon(picture: &Picture, is_artist: bool) {
    let icon = if is_artist {
        "avatar-default-symbolic"
    } else {
        "audio-x-generic-symbolic"
    };

    if let Some(parent) = picture.parent()
        && let Ok(box_) = parent.downcast::<Box>()
    {
        box_.remove(picture);

        let img = Image::from_icon_name(icon);
        img.set_size_request(64, 64);
        img.add_css_class("thumbnail");
        box_.prepend(&img);
    }
}

/// Updates a picture widget with cover art from cache or starts loading.
fn update_picture_with_cover(
    picture: Picture,
    url: String,
    cover_art_cache: &CoverArtCache,
    texture_sender: &Sender<(String, Option<Texture>)>,
    texture_register: &Sender<TextureEvent>,
) {
    if let Some(texture) = cover_art_cache.get(&url) {
        picture.set_paintable(Some(&texture));
    } else {
        register_cover_texture(texture_register, url.clone(), picture);
        cover_art_cache.start_load(url, texture_sender.clone());
    }
}

/// Fetches artist cover URL from API.
fn fetch_artist_cover_url(api_service: &Arc<Mutex<QobuzApiService>>, id: &str) -> Option<String> {
    let artist_id = match id.parse::<i32>() {
        Ok(id) => id,
        Err(e) => {
            warn!(error = %e, id = %id, "Failed to parse artist ID for cover URL");
            return None;
        }
    };
    let api = api_service.lock();
    let artist = match api.get_artist(artist_id, None) {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id = %artist_id, "Failed to fetch artist for cover URL");
            return None;
        }
    };
    let url = resolve_thumbnail_url(artist.image.as_ref());
    let result = url;
    drop(api);
    result
}

/// Fetches playlist cover URL from API.
fn fetch_playlist_cover_url(api_service: &Arc<Mutex<QobuzApiService>>, id: &str) -> Option<String> {
    let api = api_service.lock();
    let playlist = match api.get_playlist(id, None) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, playlist_id = %id, "Failed to fetch playlist for cover URL");
            return None;
        }
    };
    let url = playlist.best_image_url(false);
    let result = url;
    drop(api);
    result
}

/// Applies a newly loaded texture to every picture registered for its URL.
fn handle_texture(url: &str, texture: Option<Texture>, pictures: &[Picture]) {
    for _ in [black_box(url)] {}
    let Some(texture) = texture else { return };
    for picture in pictures {
        picture.set_paintable(Some(&texture));
    }
}

/// Logs a texture-forward send failure if the channel is closed.
pub(super) fn log_texture_send_error(result: Result<(), SendError<TextureEvent>>) {
    if let Err(e) = result {
        warn!(error = %e, "Failed to forward texture load event");
    }
}

/// Registers a picture for a cover URL, logging send failures.
fn register_cover_texture(sender: &Sender<TextureEvent>, url: String, picture: Picture) {
    if let Err(e) = sender.send_blocking(Register { url, picture }) {
        warn!(error = %e, "Failed to register cover texture");
    }
}

/// Sets up the cover art texture receiver to update pictures when textures load.
///
/// The receiver owns the URL-to-picture registry on the main thread, since GTK
/// widgets are not thread-safe.
pub(super) fn setup_texture_receiver(receiver: Receiver<TextureEvent>) {
    drop(MainContext::default().spawn_local(async move {
        let mut registry = HashMap::<String, Vec<Picture>>::new();
        while let Ok(event) = receiver.recv().await {
            apply_texture_event(event, &mut registry);
        }
    }));
}

/// Applies a cover art load/registration event to the picture registry.
fn apply_texture_event(event: TextureEvent, registry: &mut HashMap<String, Vec<Picture>>) {
    match event {
        Register { url, picture } => {
            registry.entry(url).or_default().push(picture);
        }
        Loaded { url, texture } => {
            if let Some(pictures) = registry.remove(&url) {
                handle_texture(&url, texture, &pictures);
            }
        }
    }
}

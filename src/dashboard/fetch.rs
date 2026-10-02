//! Metadata retrieval and download enqueueing for the dashboard.

use std::{path::PathBuf, sync::Arc};

use {
    async_channel::{Sender, bounded},
    libadwaita::{
        Toast, ToastOverlay, gio::spawn_blocking, glib::MainContext, gtk::Button,
        prelude::WidgetExt,
    },
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
    tracing::{error, warn},
};

use crate::{
    audio_quality::Quality,
    browse::detail_stage::exhibit::resolve_thumbnail_url,
    dashboard::url_parse::ParsedUrl::{self, Album, Playlist, Track},
    download::progress::{
        DownloadCommand::{self, Enqueue},
        DownloadItem::{
            self, Album as DownloadAlbum, Playlist as DownloadPlaylist, Track as DownloadTrack,
        },
        DownloadTask,
    },
};

/// Context for enqueuing a download after metadata fetch.
#[derive(Debug)]
pub struct DownloadCtx {
    /// Persistent fetching toast to dismiss on completion.
    pub fetching_toast: Toast,
    /// Download button to re-enable after fetch.
    pub download_button: Button,
    /// Command sender for enqueuing.
    pub cmd_sender: Sender<DownloadCommand>,
    /// Toast overlay for error feedback.
    pub toast_overlay: ToastOverlay,
}

/// Fetched metadata for a download item.
struct FetchedMeta {
    /// Display title.
    title: String,
    /// Display artist.
    artist: String,
    /// Cover art URL.
    cover_url: Option<String>,
}

/// Fetches album metadata from the Qobuz API.
fn fetch_album_meta(api: &QobuzApiService, album_id: &str) -> Option<FetchedMeta> {
    let album = match api.get_album(album_id, None) {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, album_id = %album_id, "Failed to fetch album metadata");
            return None;
        }
    };
    let title = album
        .title
        .as_deref()
        .unwrap_or("Unknown Album")
        .to_string();
    let artist = album
        .artist
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist")
        .to_string();
    let cover_url = resolve_thumbnail_url(album.image.as_ref());
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Fetches track metadata from the Qobuz API.
fn fetch_track_meta(api: &QobuzApiService, track_id: i32) -> Option<FetchedMeta> {
    let track = match api.get_track(track_id) {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, track_id = %track_id, "Failed to fetch track metadata");
            return None;
        }
    };
    let title = track
        .title
        .as_deref()
        .unwrap_or("Unknown Track")
        .to_string();
    let artist = track
        .performer
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist")
        .to_string();
    let cover_url = resolve_thumbnail_url(track.album.as_ref().and_then(|a| a.image.as_ref()));
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Fetches playlist metadata from the Qobuz API.
fn fetch_playlist_meta(api: &QobuzApiService, playlist_id: &str) -> Option<FetchedMeta> {
    let playlist = match api.get_playlist(playlist_id, Some("tracks")) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, playlist_id = %playlist_id, "Failed to fetch playlist metadata");
            return None;
        }
    };
    let title = playlist
        .name
        .as_deref()
        .unwrap_or("Unknown Playlist")
        .to_string();
    let artist = playlist
        .creator_name()
        .unwrap_or("Unknown Creator")
        .to_string();
    let cover_url = playlist.best_image_url(true);
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Creates a persistent toast (no auto-dismiss) and adds it to the overlay.
#[must_use]
pub fn create_persistent_toast(message: &str, overlay: &ToastOverlay) -> Toast {
    let toast = Toast::new(message);
    toast.set_timeout(0);
    overlay.add_toast(toast.clone());
    toast
}

/// Fetches metadata via `spawn_blocking`, then enqueues the download on the main context.
pub fn fetch_and_enqueue(
    api_service: &Arc<Mutex<QobuzApiService>>,
    parsed: ParsedUrl,
    quality: Quality,
    output_dir: PathBuf,
    ctx: DownloadCtx,
) {
    let (tx, rx) = bounded::<Option<FetchedMeta>>(1);
    let api_service = Arc::clone(api_service);
    let parsed_spawn = parsed.clone();

    drop(spawn_blocking(move || {
        let api = api_service.lock();
        let meta = match &parsed_spawn {
            Album(id) => fetch_album_meta(&api, id),
            Track(id) => fetch_track_meta(&api, id.parse::<i32>().unwrap_or(0)),
            Playlist(id) => fetch_playlist_meta(&api, id),
        };
        drop(api);
        if let Err(e) = tx.send_blocking(meta) {
            error!(error = %e, "Failed to send fetched metadata");
        }
    }));

    drop(MainContext::default().spawn_local(async move {
        let meta = rx.recv().await.unwrap_or(None);
        ctx.fetching_toast.dismiss();
        ctx.download_button.set_sensitive(true);

        let Some(meta) = meta else {
            let toast = Toast::new("Failed to fetch metadata from Qobuz");
            toast.set_timeout(3);
            ctx.toast_overlay.add_toast(toast);
            return;
        };

        let item = build_download_item(&parsed, &meta);
        let task = DownloadTask::new(item, quality, output_dir);
        if let Err(err) = ctx.cmd_sender.send(Enqueue { task }).await {
            warn!(error = %err, "Failed to enqueue dashboard download");
        }
    }));
}

/// Constructs a `DownloadItem` from a parsed URL and fetched metadata.
fn build_download_item(parsed: &ParsedUrl, meta: &FetchedMeta) -> DownloadItem {
    match parsed {
        Album(id) => DownloadAlbum {
            album_id: id.clone(),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            cover_url: meta.cover_url.clone(),
        },
        Track(id) => DownloadTrack {
            track_id: id.parse::<i32>().unwrap_or(0),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            cover_url: meta.cover_url.clone(),
        },
        Playlist(id) => DownloadPlaylist {
            playlist_id: id.clone(),
            title: meta.title.clone(),
            cover_url: meta.cover_url.clone(),
        },
    }
}

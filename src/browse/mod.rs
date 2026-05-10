//! Browse module for album, artist, and playlist details.

pub mod album_view;
pub mod artist_view;
pub mod playlist_view;

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::gio::spawn_blocking,
    parking_lot::Mutex,
    qobuz_api_rust_refactor::{
        api::service::QobuzApiService,
        models::{album::Album, artist::Artist, playlist::Playlist, track::Track},
    },
    tracing::{error, info, warn},
};

/// Events sent from background browse operations to the GUI thread.
#[derive(Clone, Debug)]
pub enum BrowseEvent {
    /// Album metadata loaded; UI can show the page immediately.
    AlbumMeta {
        /// The album metadata.
        album: Album,
    },
    /// Album tracks loaded; UI should populate the track list and wire downloads.
    AlbumTracks {
        /// The album (same as in `AlbumMeta`).
        album: Album,
        /// Track listings for the album.
        tracks: Vec<Track>,
    },
    /// Artist detail loaded.
    Artist {
        /// The artist.
        artist: Artist,
    },
    /// Playlist detail loaded.
    Playlist {
        /// The playlist.
        playlist: Playlist,
    },
    /// Browse request failed.
    Error {
        /// Description of what was being browsed.
        context: String,
        /// Error description.
        error: String,
    },
}

/// Sends a browse event over the channel and logs a warning on failure.
fn send_event(sender: &Sender<BrowseEvent>, event: BrowseEvent) {
    if let Err(e) = sender.send_blocking(event) {
        warn!(error = %e, "Failed to send browse event");
    }
}

/// Loads album details in two phases: metadata first, then tracks.
///
/// Phase 1 sends `BrowseEvent::AlbumMeta` immediately after fetching the album
/// metadata, so the UI can show the album page without waiting for tracks.
/// Phase 2 sends `BrowseEvent::AlbumTracks` after all track details are fetched.
///
/// # Arguments
///
/// * `api_service` - Shared API service
/// * `album_id` - Album ID to fetch
/// * `sender` - Channel to send events on
pub fn browse_album(
    api_service: Arc<Mutex<QobuzApiService>>,
    album_id: String,
    sender: Sender<BrowseEvent>,
) {
    spawn_blocking(move || {
        let api = api_service.lock();

        let album = match api.get_album(&album_id, Some("track_ids")) {
            Ok(album) => album,
            Err(e) => {
                error!(album_id = %album_id, error = %e, "Failed to browse album");
                send_event(
                    &sender,
                    BrowseEvent::Error {
                        context: format!("Loading album {album_id}"),
                        error: format!("{e}"),
                    },
                );
                return;
            }
        };

        let track_ids = album.track_ids.clone().unwrap_or_default();

        // Phase 1: send metadata immediately so the UI can display the album page.
        info!(album_id = %album_id, "Album metadata loaded");
        send_event(
            &sender,
            BrowseEvent::AlbumMeta {
                album: album.clone(),
            },
        );

        // Phase 2: fetch all track details.
        let tracks: Vec<Track> = track_ids
            .iter()
            .filter_map(|&track_id| match api.get_track(track_id) {
                Ok(track) => Some(track),
                Err(e) => {
                    error!(track_id, error = %e, "Failed to fetch track for album");
                    None
                }
            })
            .collect();
        drop(api);

        info!(album_id = %album_id, track_count = %tracks.len(), "Album tracks loaded");
        send_event(&sender, BrowseEvent::AlbumTracks { album, tracks });
    });
}

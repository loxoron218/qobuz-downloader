//! Browse module for album, artist, and playlist details.

pub mod album_view;
pub mod artist_view;
pub mod detail_common;
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
    /// Artist detail loaded with album catalog.
    Artist {
        /// The artist.
        artist: Artist,
        /// Albums from the artist catalog.
        albums: Vec<Album>,
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

/// Loads playlist details including all tracks.
///
/// Playlists from the API include tracks in the same response when `extra="tracks"` is passed,
/// so this is a single-phase fetch.
///
/// # Arguments
///
/// * `api_service` - Shared API service
/// * `playlist_id` - Playlist ID to fetch
/// * `sender` - Channel to send events on
pub fn browse_playlist(
    api_service: Arc<Mutex<QobuzApiService>>,
    playlist_id: String,
    sender: Sender<BrowseEvent>,
) {
    spawn_blocking(move || {
        let api = api_service.lock();

        let playlist = match api.get_playlist(&playlist_id, Some("tracks")) {
            Ok(p) => p,
            Err(e) => {
                error!(playlist_id = %playlist_id, error = %e, "Failed to browse playlist");
                send_event(
                    &sender,
                    BrowseEvent::Error {
                        context: format!("Loading playlist {playlist_id}"),
                        error: format!("{e}"),
                    },
                );
                return;
            }
        };

        drop(api);
        info!(playlist_id = %playlist_id, "Playlist loaded");
        send_event(&sender, BrowseEvent::Playlist { playlist });
    });
}

/// Loads artist details and their album catalog.
///
/// Fetches artist metadata via `get_artist` and album catalog via `get_release_list`.
/// Both requests are made within the same `spawn_blocking` closure.
///
/// # Arguments
///
/// * `api_service` - Shared API service
/// * `artist_id` - Artist ID to fetch
/// * `sender` - Channel to send events on
pub fn browse_artist(
    api_service: Arc<Mutex<QobuzApiService>>,
    artist_id: i32,
    sender: Sender<BrowseEvent>,
) {
    spawn_blocking(move || {
        let api = api_service.lock();

        let artist = match api.get_artist(artist_id, None) {
            Ok(a) => a,
            Err(e) => {
                error!(artist_id = %artist_id, error = %e, "Failed to browse artist");
                send_event(
                    &sender,
                    BrowseEvent::Error {
                        context: format!("Loading artist {artist_id}"),
                        error: format!("{e}"),
                    },
                );
                return;
            }
        };

        let mut albums = match api.get_release_list(artist_id, Some(50), None) {
            Ok(releases) => releases
                .items
                .unwrap_or_default()
                .into_iter()
                .map(|a| *a)
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!(artist_id = %artist_id, error = %e, "Failed to fetch artist release list");
                Vec::new()
            }
        };
        albums.sort_by(|a, b| {
            let date_a = a.release_date_original.as_deref().unwrap_or("");
            let date_b = b.release_date_original.as_deref().unwrap_or("");
            date_b.cmp(date_a)
        });

        drop(api);
        info!(artist_id = %artist_id, album_count = %albums.len(), "Artist detail loaded");
        send_event(&sender, BrowseEvent::Artist { artist, albums });
    });
}

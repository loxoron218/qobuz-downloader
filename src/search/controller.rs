//! Search controller logic for catalog search with scope support.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::Relaxed},
};

use {
    async_channel::Sender,
    libadwaita::gio::spawn_blocking,
    parking_lot::Mutex,
    qobuz_api_rust_refactor::{
        api::service::QobuzApiService, errors::QobuzApiError, models::search::SearchResult,
    },
    tracing::{error, info},
};

/// Manages search queries and dispatches results to the GUI thread.
#[derive(Clone)]
pub struct SearchController {
    /// Shared API client.
    api_service: Arc<Mutex<QobuzApiService>>,
    /// Monotonic counter for discarding stale results.
    query_counter: Arc<AtomicU64>,
}

impl SearchController {
    /// Creates a new search controller.
    pub fn new(api_service: Arc<Mutex<QobuzApiService>>) -> Self {
        Self {
            api_service,
            query_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Submits a scoped search query.
    pub fn search_scoped(&self, query: &str, scope: SearchScope, sender: Sender<SearchEvent>) {
        if query.trim().is_empty() {
            return;
        }

        let query_id = self.query_counter.fetch_add(1, Relaxed);
        let current_counter = Arc::clone(&self.query_counter);
        let api_service = Arc::clone(&self.api_service);
        let query = query.to_string();

        spawn_blocking(move || {
            execute_search_scoped(
                query,
                scope,
                query_id,
                &current_counter,
                &api_service,
                &sender,
            );
        });
    }
}

/// Events sent from background search to the GUI thread.
#[derive(Clone, Debug)]
pub enum SearchEvent {
    /// Search failed.
    Error {
        /// Error description.
        error: String,
    },
    /// Search completed successfully.
    Results {
        /// The original query string.
        query: String,
        /// The search results.
        result: SearchResult,
    },
}

/// Search scope for filtering results by content type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchScope {
    /// Search across all content types.
    All,
    /// Search only albums.
    Albums,
    /// Search only tracks.
    Tracks,
    /// Search only artists.
    Artists,
    /// Search only playlists.
    Playlists,
}

impl SearchScope {
    /// Converts a u32 index to a `SearchScope`.
    #[must_use]
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Albums,
            2 => Self::Tracks,
            3 => Self::Artists,
            4 => Self::Playlists,
            _ => Self::All,
        }
    }

    /// Converts this `SearchScope` to a u32 index for the dropdown.
    pub fn to_u32(self) -> u32 {
        match self {
            Self::All => 0,
            Self::Albums => 1,
            Self::Tracks => 2,
            Self::Artists => 3,
            Self::Playlists => 4,
        }
    }
}

/// Executes a scoped search query on a background thread.
fn execute_search_scoped(
    query: String,
    scope: SearchScope,
    query_id: u64,
    current_counter: &Arc<AtomicU64>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    sender: &Sender<SearchEvent>,
) {
    let result = match scope {
        SearchScope::All => api_service.lock().search_catalog(&query, Some(20), None),
        SearchScope::Albums => {
            let albums = api_service.lock().search_albums(&query, Some(20), None);
            albums.map(|a| SearchResult {
                albums: Some(a),
                artists: None,
                tracks: None,
                playlists: None,
            })
        }
        SearchScope::Tracks => {
            let tracks = api_service.lock().search_tracks(&query, Some(20), None);
            tracks.map(|t| SearchResult {
                albums: None,
                artists: None,
                tracks: Some(t),
                playlists: None,
            })
        }
        SearchScope::Artists => {
            let artists = api_service.lock().search_artists(&query, Some(20), None);
            artists.map(|a| SearchResult {
                albums: None,
                artists: Some(a),
                tracks: None,
                playlists: None,
            })
        }
        SearchScope::Playlists => {
            let playlists = api_service.lock().search_playlists(&query, Some(20), None);
            playlists.map(|p| SearchResult {
                albums: None,
                artists: None,
                tracks: None,
                playlists: Some(p),
            })
        }
    };

    if is_stale_query(current_counter, query_id, &query) {
        return;
    }

    let event = search_result_to_event(result, query);

    if let Err(err) = sender.send_blocking(event) {
        error!(error = %err, "Failed to send search event");
    }
}

/// Checks if the current search query is stale.
fn is_stale_query(counter: &Arc<AtomicU64>, query_id: u64, query: &str) -> bool {
    let stale = counter.load(Relaxed) != query_id + 1;
    if stale {
        info!(query = %query, "Discarding stale search result");
    }
    stale
}

/// Converts a search result into a `SearchEvent`.
fn search_result_to_event(
    result: Result<SearchResult, QobuzApiError>,
    query: String,
) -> SearchEvent {
    match result {
        Ok(result) => {
            info!(query = %query, "Search completed");
            SearchEvent::Results { query, result }
        }
        Err(err) => {
            error!(error = %err, query = %query, "Search failed");
            SearchEvent::Error {
                error: format!("{err}"),
            }
        }
    }
}

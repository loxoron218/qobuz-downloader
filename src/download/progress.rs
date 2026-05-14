//! Download progress tracking and quality types.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::SystemTime,
};

use {libadwaita::gtk::gdk::Texture, num_traits::AsPrimitive, parking_lot::Mutex};

use crate::types::Quality;

/// Global unique ID counter for download tasks.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Commands sent to the download worker.
#[derive(Clone, Debug)]
pub enum DownloadCommand {
    /// Cancel a specific download.
    Cancel {
        /// The task ID to cancel.
        id: u64,
    },
    /// Add a download task to the queue.
    Enqueue {
        /// The task to enqueue.
        task: DownloadTask,
    },
    /// Shut down the download worker.
    Shutdown,
}

/// Events sent from download worker to GUI.
#[derive(Clone, Debug)]
pub enum DownloadEvent {
    /// Download completed successfully.
    Completed {
        /// Task ID.
        id: u64,
    },
    /// Download failed permanently.
    Failed {
        /// Task ID.
        id: u64,
        /// Error description.
        error: String,
    },
    /// Download has started (a slot was available).
    Started {
        /// Task ID.
        id: u64,
    },
    /// Download progress update (for batch downloads).
    Progress {
        /// Task ID.
        id: u64,
        /// Number of items completed so far.
        items_completed: u32,
        /// Total number of items (if known).
        total_items: Option<u32>,
    },
}

/// What is being downloaded.
#[derive(Clone, Debug)]
pub enum DownloadItem {
    /// Full album download.
    Album {
        /// Album ID.
        album_id: String,
        /// Album title for display.
        title: String,
        /// Artist name for display.
        artist: String,
        /// Cover art URL.
        cover_url: Option<String>,
    },
    /// Artist discography download.
    Artist {
        /// Artist ID.
        artist_id: i32,
        /// Artist name for display.
        name: String,
        /// Cover art URL.
        cover_url: Option<String>,
    },
    /// Playlist download.
    Playlist {
        /// Playlist ID for fetching tracks.
        playlist_id: String,
        /// Playlist name for display.
        title: String,
        /// Cover art URL.
        cover_url: Option<String>,
    },
    /// Single track download.
    Track {
        /// Track ID.
        track_id: i32,
        /// Track title for display.
        title: String,
        /// Artist name for display.
        artist: String,
        /// Cover art URL.
        cover_url: Option<String>,
    },
}

impl DownloadItem {
    /// Returns the display title for this item.
    pub fn title(&self) -> &str {
        match self {
            Self::Artist { name, .. } => name.as_str(),
            Self::Album { title, .. }
            | Self::Playlist { title, .. }
            | Self::Track { title, .. } => title.as_str(),
        }
    }

    /// Returns the display subtitle (artist name or empty).
    pub fn subtitle(&self) -> &str {
        match self {
            Self::Album { artist, .. } | Self::Track { artist, .. } => artist.as_str(),
            Self::Artist { .. } | Self::Playlist { .. } => "",
        }
    }

    /// Returns the cover art URL if available.
    pub fn cover_url(&self) -> Option<&str> {
        match self {
            Self::Album { cover_url, .. }
            | Self::Artist { cover_url, .. }
            | Self::Playlist { cover_url, .. }
            | Self::Track { cover_url, .. } => cover_url.as_deref(),
        }
    }
}

/// Byte-level download progress.
#[derive(Clone, Debug, Default)]
pub struct DownloadProgress {
    /// Bytes received so far.
    pub bytes_downloaded: u64,
    /// Total file size (may be unknown).
    pub total_bytes: Option<u64>,
    /// Number of items completed (for batch downloads).
    pub items_completed: u32,
    /// Total number of items (for batch downloads, if known).
    pub total_items: Option<u32>,
}

impl DownloadProgress {
    /// Returns the download percentage if total is known.
    pub fn percentage(&self) -> Option<f64> {
        if let Some(total) = self.total_bytes.filter(|&total| total > 0) {
            return Some(
                (AsPrimitive::<f64>::as_(self.bytes_downloaded) / AsPrimitive::<f64>::as_(total))
                    * 100.0,
            );
        }
        if let Some(total) = self
            .total_items
            .filter(|&total| total > 0 && self.items_completed > 0)
        {
            return Some(
                (AsPrimitive::<f64>::as_(self.items_completed) / AsPrimitive::<f64>::as_(total))
                    * 100.0,
            );
        }
        None
    }
}

/// Row data for the download queue `ListView` model.
#[derive(Clone, Debug)]
pub struct DownloadRowData {
    /// Download task.
    pub task: DownloadTask,
    /// Cached cover art texture.
    pub texture: Option<Texture>,
}

/// State machine for download lifecycle.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum DownloadStatus {
    /// Currently downloading.
    Active,
    /// User cancelled.
    Cancelled,
    /// Successfully finished.
    Completed,
    /// Errored out.
    Failed,
    /// Waiting for a download slot.
    #[default]
    Queued,
}

/// Represents a single download operation.
#[derive(Clone, Debug)]
pub struct DownloadTask {
    /// Completion time.
    pub completed_at: Option<SystemTime>,
    /// Unique task identifier.
    pub id: u64,
    /// What to download.
    pub item: DownloadItem,
    /// Destination directory.
    pub output_dir: PathBuf,
    /// Byte-level progress.
    pub progress: DownloadProgress,
    /// Audio quality level.
    pub quality: Quality,
    /// Current state.
    pub status: DownloadStatus,
}

impl DownloadTask {
    /// Creates a new download task with the given parameters.
    pub fn new(item: DownloadItem, quality: Quality, output_dir: PathBuf) -> Self {
        Self {
            id: next_id(),
            item,
            quality,
            output_dir,
            status: DownloadStatus::Queued,
            progress: DownloadProgress::default(),
            completed_at: None,
        }
    }
}

/// Returns the next unique download task ID.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Relaxed)
}

/// Marks all queued and active tasks as cancelled.
///
/// Does NOT clear the map — workers need to check task status
/// when processing enqueued commands.
///
/// # Arguments
///
/// * `tasks` - The task map to update
pub fn cancel_all_tasks(tasks: &Mutex<HashMap<u64, DownloadTask>>) {
    let mut map = tasks.lock();
    for task in map
        .values_mut()
        .filter(|t| t.status == DownloadStatus::Active || t.status == DownloadStatus::Queued)
    {
        task.status = DownloadStatus::Cancelled;
        task.completed_at = Some(SystemTime::now());
    }
}

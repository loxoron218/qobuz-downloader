//! Download progress tracking and quality types.

use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
    path::PathBuf,
};

use {
    chrono::{DateTime, Local},
    libadwaita::gtk::gdk::Texture,
    num_traits::AsPrimitive,
    parking_lot::Mutex,
    qobuz_api_rust_refactor::models::file_url::quality::{
        FLAC_16_44, FLAC_24_96, FLAC_24_192, MP3_320,
    },
    serde::{Deserialize, Serialize},
    uuid::Uuid,
};

/// Commands sent to the download worker.
#[derive(Clone, Debug)]
pub enum DownloadCommand {
    /// Cancel a specific download.
    Cancel {
        /// The task ID to cancel.
        id: Uuid,
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
        id: Uuid,
    },
    /// Download failed permanently.
    Failed {
        /// Task ID.
        id: Uuid,
    },
    /// Download has started (a slot was available).
    Started {
        /// Task ID.
        id: Uuid,
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
}

impl DownloadProgress {
    /// Returns the download percentage if total is known.
    pub fn percentage(&self) -> Option<f64> {
        self.total_bytes.filter(|&total| total > 0).map(|total| {
            (AsPrimitive::<f64>::as_(self.bytes_downloaded) / AsPrimitive::<f64>::as_(total))
                * 100.0
        })
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
    pub completed_at: Option<DateTime<Local>>,
    /// Unique task identifier.
    pub id: Uuid,
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
            id: Uuid::new_v4(),
            item,
            quality,
            output_dir,
            status: DownloadStatus::Queued,
            progress: DownloadProgress::default(),
            completed_at: None,
        }
    }
}

/// Audio quality selection wrapping API library constants.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Quality {
    /// MP3 320kbps (`format_id` = 5).
    Mp3_320,
    /// FLAC 16-bit / 44.1kHz (`format_id` = 6).
    #[default]
    Flac16_44,
    /// FLAC 24-bit / 96kHz (`format_id` = 7).
    Flac24_96,
    /// FLAC 24-bit / 192kHz (`format_id` = 27).
    Flac24_192,
}

impl Quality {
    /// Returns the file extension for this quality level.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp3_320 => "mp3",
            Self::Flac16_44 | Self::Flac24_96 | Self::Flac24_192 => "flac",
        }
    }
}

impl Display for Quality {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Mp3_320 => write!(f, "MP3 320kbps"),
            Self::Flac16_44 => write!(f, "FLAC 16-bit / 44.1kHz"),
            Self::Flac24_96 => write!(f, "FLAC 24-bit / 96kHz"),
            Self::Flac24_192 => write!(f, "FLAC 24-bit / 192kHz"),
        }
    }
}

impl TryFrom<i32> for Quality {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            v if v == MP3_320 => Ok(Self::Mp3_320),
            v if v == FLAC_16_44 => Ok(Self::Flac16_44),
            v if v == FLAC_24_96 => Ok(Self::Flac24_96),
            v if v == FLAC_24_192 => Ok(Self::Flac24_192),
            _ => Err(format!("Unknown quality format_id: {value}")),
        }
    }
}

impl From<Quality> for i32 {
    fn from(quality: Quality) -> Self {
        match quality {
            Quality::Mp3_320 => MP3_320,
            Quality::Flac16_44 => FLAC_16_44,
            Quality::Flac24_96 => FLAC_24_96,
            Quality::Flac24_192 => FLAC_24_192,
        }
    }
}

/// Cancels all queued and active tasks and clears the task map.
///
/// # Arguments
///
/// * `tasks` - The task map to clear
pub fn cancel_all_tasks(tasks: &Mutex<HashMap<Uuid, DownloadTask>>) {
    let mut map = tasks.lock();
    for task in map
        .values_mut()
        .filter(|t| t.status == DownloadStatus::Active || t.status == DownloadStatus::Queued)
    {
        task.status = DownloadStatus::Cancelled;
        task.completed_at = Some(Local::now());
    }
    map.clear();
}

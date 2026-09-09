//! Download execution, cancellation, and result reporting.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::SystemTime,
};

use {
    async_channel::Sender,
    parking_lot::Mutex,
    qobuz_api::{
        api::service::QobuzApiService, errors::QobuzApiError::Canceled,
        metadata::config::MetadataConfig, sanitize::sanitize_filename,
    },
    tracing::{error, info},
};

use crate::{
    download::{
        progress::{
            DownloadEvent::{self, Completed, Failed, Progress, Started},
            DownloadItem::{self, Album, Artist, Playlist, Track},
            DownloadStatus::{
                Active, Cancelled, Completed as StatusCompleted, Failed as ItemFailed,
            },
            DownloadTask,
        },
        worker::album_output_dir,
    },
    errors::AppError::{self, Api, Download},
    types::Quality,
};

/// Executes an enqueued download inline on the calling worker thread.
///
/// Skips tasks already marked `Cancelled`.
pub fn handle_enqueued_download<
    S: BuildHasher + Send + Sync + 'static,
    R: BuildHasher + Send + Sync + 'static,
>(
    evt_sender: &Sender<DownloadEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
    task: &DownloadTask,
) {
    let task_id = task.id;

    if tasks
        .lock()
        .get(&task_id)
        .is_some_and(|t| t.status == Cancelled)
    {
        return;
    }

    let cancel_flag = Arc::new(AtomicBool::new(false));
    drop(
        cancel_signals
            .lock()
            .insert(task_id, Arc::clone(&cancel_flag)),
    );

    drop(tasks.lock().insert(task_id, task.clone()));

    if let Err(err) = evt_sender.send_blocking(Started { id: task_id }) {
        error!(error = %err, "Failed to send download started event");
    }

    if let Some(t) = tasks.lock().get_mut(&task_id) {
        t.status = Active;
    }

    let evt_sender_clone = evt_sender.clone();
    let tasks_clone = Arc::clone(tasks);
    let progress_callback = move |items_completed: u32, total_items: u32| {
        if let Err(err) = evt_sender_clone.send_blocking(Progress {
            id: task_id,
            items_completed,
            total_items: Some(total_items),
        }) {
            error!(error = %err, "Failed to send progress event");
        }
        if let Some(t) = tasks_clone.lock().get_mut(&task_id) {
            t.progress.items_completed = items_completed;
            t.progress.total_items = Some(total_items);
        }
    };

    let result = if cancel_flag.load(Relaxed) {
        Err(Download("Download cancelled".to_string()))
    } else {
        execute_download(
            api_service,
            &task.item,
            task.quality,
            task.output_dir.as_path(),
            Arc::clone(&cancel_flag),
            progress_callback,
        )
    };

    drop(cancel_signals.lock().remove(&task_id));

    let evt_sender_fin = evt_sender.clone();
    let tasks_fin = Arc::clone(tasks);
    handle_download_result(&evt_sender_fin, &tasks_fin, task_id, result);
}

/// Sends a download event, logging any error if the receiver was dropped.
fn send_event(evt_sender: &Sender<DownloadEvent>, event: DownloadEvent) {
    if let Err(err) = evt_sender.send_blocking(event) {
        error!(error = %err, "Failed to send download event");
    }
}

/// Updates task state and sends the appropriate event for a completed/failed download.
fn handle_download_result<S: BuildHasher>(
    evt_sender: &Sender<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    task_id: u64,
    result: Result<PathBuf, AppError>,
) {
    match result {
        Ok(path) => {
            info!(id = task_id, path = %path.display(), "Download completed");
            let mut map = tasks.lock();
            if let Some(t) = map.get_mut(&task_id) {
                t.status = StatusCompleted;
                t.completed_at = Some(SystemTime::now());
            }
            drop(map);
            send_event(evt_sender, Completed { id: task_id });
        }
        Err(err) => {
            let err_str = err.to_string();
            if is_cancelled_error(&err) {
                info!(id = task_id, error = %err_str, "Download aborted due to cancellation");
                mark_download_failed(tasks, task_id);
                return;
            }
            error!(id = task_id, error = %err_str, "Download failed");
            mark_download_failed(tasks, task_id);
            send_event(
                evt_sender,
                Failed {
                    id: task_id,
                    error: err_str,
                },
            );
        }
    }
}

/// Marks a download task as failed in the tasks map, preserving Cancelled status.
fn mark_download_failed<S: BuildHasher>(
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    task_id: u64,
) {
    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&task_id) {
        if t.status != Cancelled {
            t.status = ItemFailed;
        }
        t.completed_at = Some(SystemTime::now());
    }
}

/// Checks if an error is a cancellation error.
fn is_cancelled_error(err: &AppError) -> bool {
    match err {
        Api(e) => matches!(e, Canceled),
        Download(msg) => msg == "Download cancelled",
        _ => false,
    }
}

/// Handles a cancel command by setting the cancellation signal and marking the task as cancelled.
pub fn handle_cancel<S: BuildHasher, R: BuildHasher>(
    evt_sender: &Sender<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
    id: u64,
) {
    info!(id = id, "Download cancelled");

    if let Some(flag) = cancel_signals.lock().get(&id) {
        flag.store(true, Relaxed);
    }

    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&id) {
        t.status = Cancelled;
        t.completed_at = Some(SystemTime::now());
    }
    drop(map);
    send_event(
        evt_sender,
        Failed {
            id,
            error: "Download cancelled by user".to_string(),
        },
    );
}

/// Executes a single download using the API service.
///
/// # Arguments
///
/// * `api_service` - API service reference
/// * `item` - Download item to fetch
/// * `quality` - Audio format ID for download
/// * `output_dir` - Output directory for downloaded files
/// * `cancel` - Cancellation flag checked during download
/// * `progress_callback` - Called after each item in batch downloads (`items_completed`,
///   `total_items`)
///
/// # Errors
///
/// Returns `Api` if the download fails.
fn execute_download<F>(
    api_service: &Arc<Mutex<QobuzApiService>>,
    item: &DownloadItem,
    quality: Quality,
    output_dir: &Path,
    cancel: Arc<AtomicBool>,
    progress_callback: F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u32, u32) + Send + Sync + 'static,
{
    let format_id: i32 = quality.into();

    {
        let mut api = api_service.lock();
        match item {
            Album {
                album_id,
                title,
                artist,
                ..
            } => {
                let album = api
                    .get_album(album_id, Some("track_ids"))
                    .map_err(AppError::from)?;
                let track_ids = album.track_ids.unwrap_or_default();
                let album_dir = album_output_dir(output_dir, artist, title, quality);
                download_album_tracks(
                    &mut api,
                    &track_ids,
                    format_id,
                    &album_dir,
                    cancel.as_ref(),
                    &progress_callback,
                )
            }
            Artist { artist_id, .. } => {
                let paths = api
                    .download_artist_cancellable(
                        *artist_id,
                        format_id,
                        output_dir,
                        Some(&MetadataConfig::default()),
                        None,
                        Some(cancel),
                    )
                    .map_err(AppError::from)?;
                paths
                    .into_iter()
                    .next()
                    .ok_or_else(|| Download("No tracks downloaded for artist".to_string()))
            }
            Playlist { playlist_id, .. } => {
                let paths = api
                    .download_playlist_cancellable(
                        playlist_id,
                        format_id,
                        output_dir,
                        Some(&MetadataConfig::default()),
                        Some(cancel),
                    )
                    .map_err(AppError::from)?;
                paths
                    .into_iter()
                    .next()
                    .ok_or_else(|| Download("No tracks downloaded".to_string()))
            }
            Track { track_id, .. } => {
                let track = api.get_track(*track_id).map_err(AppError::from)?;
                let album_artist = track
                    .album
                    .as_ref()
                    .and_then(|a| a.artist.as_ref())
                    .and_then(|ar| ar.name.as_deref());
                let performer = track.performer.as_ref().and_then(|p| p.name.as_deref());
                let artist_name = album_artist.or(performer).unwrap_or("Unknown Artist");
                let album_title = track
                    .album
                    .as_ref()
                    .and_then(|a| a.title.as_deref())
                    .unwrap_or("Unknown Album");
                let track_dir = output_dir
                    .join(sanitize_filename(artist_name))
                    .join(sanitize_filename(album_title));
                api.download_track_cancellable(
                    *track_id,
                    format_id,
                    &track_dir,
                    Some(&MetadataConfig::default()),
                    Some(cancel.as_ref()),
                )
                .map_err(AppError::from)
            }
        }
    }
}

/// Downloads all tracks from an album.
///
/// # Arguments
///
/// * `api` - API service reference
/// * `track_ids` - List of track IDs to download
/// * `format_id` - Audio format ID for download
/// * `output_dir` - Output directory for downloaded files
/// * `cancel` - Cancellation flag checked between tracks
/// * `progress_callback` - Called after each track download with (completed, total)
///
/// # Errors
///
/// Returns `Download` if no tracks could be downloaded.
fn download_album_tracks<F>(
    api: &mut QobuzApiService,
    track_ids: &[i32],
    format_id: i32,
    output_dir: &Path,
    cancel: &AtomicBool,
    progress_callback: &F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u32, u32),
{
    let total = u32::try_from(track_ids.len()).unwrap_or_default();
    let mut last_path: Option<PathBuf> = None;
    for (i, &tid) in track_ids.iter().enumerate() {
        if cancel.load(Relaxed) {
            return Err(Download("Download cancelled".to_string()));
        }
        match api.download_track_cancellable(
            tid,
            format_id,
            output_dir,
            Some(&MetadataConfig::default()),
            Some(cancel),
        ) {
            Ok(path) => {
                last_path = Some(path);
            }
            Err(Canceled) => {
                return Err(Download("Download cancelled".to_string()));
            }
            Err(e) => {
                error!(track_id = tid, error = %e, "Failed to download album track");
            }
        }
        let completed = u32::try_from(i).map_or(total, |n| n.saturating_add(1));
        progress_callback(completed, total);
    }
    last_path.ok_or_else(|| Download("No tracks downloaded".to_string()))
}

//! Download manager with concurrency control.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering::{Relaxed, SeqCst},
        },
    },
    thread::{Scope, scope},
};

use {
    async_channel::{Receiver, Sender, bounded, unbounded},
    chrono::Local,
    libadwaita::gio::spawn_blocking,
    parking_lot::Mutex,
    qobuz_api_rust_refactor::{api::service::QobuzApiService, errors::QobuzApiError::Canceled},
    tracing::{error, info},
    uuid::Uuid,
};

use crate::{
    download::{
        progress::{
            DownloadCommand::{self, Cancel, Enqueue, Shutdown},
            DownloadEvent::{self, Completed, Failed, Progress, Started},
            DownloadItem::{self, Album, Artist, Playlist, Track},
            DownloadStatus::{
                Active, Cancelled, Completed as StatusCompleted, Failed as ItemFailed,
            },
            DownloadTask, Quality,
        },
        worker::album_output_dir,
    },
    errors::AppError::{self, Api, Download},
};

/// Number of persistent download worker threads.
const WORKER_COUNT: usize = 3;

/// Manages download queue, concurrency slots, and task tracking.
pub struct DownloadManager {
    /// Shared API client.
    api_service: Arc<Mutex<QobuzApiService>>,
    /// Command channel sender.
    cmd_sender: Sender<DownloadCommand>,
    /// Command channel receiver.
    cmd_receiver: Receiver<DownloadCommand>,
    /// Event channel sender.
    evt_sender: Sender<DownloadEvent>,
    /// Event channel receiver.
    evt_receiver: Receiver<DownloadEvent>,
    /// Tracked download tasks.
    tasks: Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
}

impl DownloadManager {
    /// Creates a new download manager with the given API service.
    pub fn new(api_service: Arc<Mutex<QobuzApiService>>) -> Self {
        let (cmd_sender, cmd_receiver) = bounded::<DownloadCommand>(16);
        let (evt_sender, evt_receiver) = unbounded::<DownloadEvent>();

        Self {
            api_service,
            cmd_sender,
            cmd_receiver,
            evt_sender,
            evt_receiver,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the command sender for enqueuing/cancelling downloads.
    pub fn cmd_sender(&self) -> Sender<DownloadCommand> {
        self.cmd_sender.clone()
    }

    /// Returns the event receiver for receiving download progress updates.
    pub fn evt_receiver(&self) -> Receiver<DownloadEvent> {
        self.evt_receiver.clone()
    }

    /// Returns a shared handle to the tasks map for view access.
    pub fn tasks_handle(&self) -> Arc<Mutex<HashMap<Uuid, DownloadTask>>> {
        Arc::clone(&self.tasks)
    }

    /// Starts the download worker loop in a background thread.
    pub fn start_worker(&self) {
        let api_service = Arc::clone(&self.api_service);
        let cmd_sender = self.cmd_sender.clone();
        let cmd_receiver = self.cmd_receiver.clone();
        let evt_sender = self.evt_sender.clone();
        let tasks = Arc::clone(&self.tasks);

        spawn_blocking(move || {
            run_download_worker(
                &cmd_sender,
                &cmd_receiver,
                &evt_sender,
                &api_service,
                &tasks,
            );
        });
    }
}

/// Bundled references shared by download worker functions.
struct WorkerCtx<'a> {
    /// Command channel sender.
    cmd_sender: &'a Sender<DownloadCommand>,
    /// Command channel receiver.
    cmd_receiver: &'a Receiver<DownloadCommand>,
    /// Event channel sender.
    evt_sender: &'a Sender<DownloadEvent>,
    /// Shared API client.
    api_service: &'a Arc<Mutex<QobuzApiService>>,
    /// Tracked download tasks.
    tasks: &'a Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
    /// Per-task cancellation signals.
    cancel_signals: &'a Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
    /// Shutdown flag.
    shutdown: &'a Arc<AtomicBool>,
}

/// Spawns persistent worker threads that pull commands from the shared channel.
///
/// Each worker processes one download at a time — the fixed worker count replaces an explicit
/// semaphore, bounding concurrency and memory (no per-download OS thread spawn).
fn run_download_worker(
    cmd_sender: &Sender<DownloadCommand>,
    cmd_receiver: &Receiver<DownloadCommand>,
    evt_sender: &Sender<DownloadEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
) {
    let shutdown = Arc::new(AtomicBool::new(false));
    let cancel_signals: Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    scope(|s| {
        let ctx = WorkerCtx {
            cmd_sender,
            cmd_receiver,
            evt_sender,
            api_service,
            tasks,
            cancel_signals: &cancel_signals,
            shutdown: &shutdown,
        };
        for _ in 0..WORKER_COUNT {
            spawn_single_worker(s, &ctx);
        }
    });
}

/// Spawns a single worker thread on the given scope.
fn spawn_single_worker<'scope>(s: &'scope Scope<'scope, '_>, ctx: &WorkerCtx<'_>) {
    let cmd_sender = ctx.cmd_sender.clone();
    let cmd_receiver = ctx.cmd_receiver.clone();
    let evt_sender = ctx.evt_sender.clone();
    let api_service = Arc::clone(ctx.api_service);
    let tasks = Arc::clone(ctx.tasks);
    let cancel_signals = Arc::clone(ctx.cancel_signals);
    let shutdown = Arc::clone(ctx.shutdown);

    s.spawn(move || {
        worker_loop(&WorkerCtx {
            cmd_sender: &cmd_sender,
            cmd_receiver: &cmd_receiver,
            evt_sender: &evt_sender,
            api_service: &api_service,
            tasks: &tasks,
            cancel_signals: &cancel_signals,
            shutdown: &shutdown,
        });
    });
}

/// Broadcasts `Shutdown` to wake all other workers.
fn broadcast_shutdown(cmd_sender: &Sender<DownloadCommand>) {
    for _ in 1..WORKER_COUNT {
        if let Err(e) = cmd_sender.send_blocking(Shutdown) {
            error!(error = %e, "Failed to send shutdown command to worker");
        }
    }
}

/// Single persistent worker: pulls commands from the channel and processes them inline.
///
/// The worker count itself gates concurrency — no external semaphore needed.
fn worker_loop(ctx: &WorkerCtx<'_>) {
    while let Ok(cmd) = ctx.cmd_receiver.recv_blocking() {
        match cmd {
            Enqueue { task } => {
                handle_enqueued_download(
                    ctx.evt_sender,
                    ctx.api_service,
                    ctx.tasks,
                    ctx.cancel_signals,
                    &task,
                );
            }
            Cancel { id } => {
                handle_cancel(ctx.evt_sender, ctx.tasks, ctx.cancel_signals, id);
            }
            Shutdown => {
                let was_first = !ctx.shutdown.swap(true, SeqCst);
                was_first.then(|| broadcast_shutdown(ctx.cmd_sender));
                return;
            }
        }
    }
}

/// Executes an enqueued download inline on the calling worker thread.
fn handle_enqueued_download(
    evt_sender: &Sender<DownloadEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
    cancel_signals: &Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
    task: &DownloadTask,
) {
    let task_id = task.id;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    cancel_signals
        .lock()
        .insert(task_id, Arc::clone(&cancel_flag));

    tasks.lock().insert(task_id, task.clone());

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
            &cancel_flag,
            progress_callback,
        )
    };

    cancel_signals.lock().remove(&task_id);

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
fn handle_download_result(
    evt_sender: &Sender<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
    task_id: Uuid,
    result: Result<PathBuf, AppError>,
) {
    match result {
        Ok(path) => {
            info!(id = %task_id, path = %path.display(), "Download completed");
            let mut map = tasks.lock();
            if let Some(t) = map.get_mut(&task_id) {
                t.status = StatusCompleted;
                t.completed_at = Some(Local::now());
            }
            drop(map);
            send_event(evt_sender, Completed { id: task_id });
        }
        Err(err) => {
            if is_cancelled_error(&err) {
                info!(id = %task_id, "Download aborted due to cancellation");
            } else {
                error!(id = %task_id, error = %err, "Download failed");
            }
            mark_download_failed(tasks, task_id);
            send_event(evt_sender, Failed { id: task_id });
        }
    }
}

/// Marks a download task as failed in the tasks map, preserving Cancelled status.
fn mark_download_failed(tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>, task_id: Uuid) {
    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&task_id) {
        if t.status != Cancelled {
            t.status = ItemFailed;
        }
        t.completed_at = Some(Local::now());
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
fn handle_cancel(
    evt_sender: &Sender<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
    cancel_signals: &Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
    id: Uuid,
) {
    info!(id = %id, "Download cancelled");

    if let Some(flag) = cancel_signals.lock().get(&id) {
        flag.store(true, Relaxed);
    }

    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&id) {
        t.status = Cancelled;
        t.completed_at = Some(Local::now());
    }
    drop(map);
    send_event(evt_sender, Failed { id });
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
    cancel: &AtomicBool,
    progress_callback: F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u32, u32) + Send + Sync + 'static,
{
    let format_id: i32 = quality.into();

    {
        let mut api = api_service.lock();
        match item {
            Album { album_id, .. } => {
                let album = api
                    .get_album(album_id, Some("track_ids"))
                    .map_err(AppError::from)?;
                let track_ids = album.track_ids.unwrap_or_default();
                download_album_tracks(
                    &mut api,
                    &track_ids,
                    format_id,
                    output_dir,
                    cancel,
                    &progress_callback,
                )
            }
            Artist { artist_id, .. } => execute_artist_download(
                &mut api,
                *artist_id,
                format_id,
                output_dir,
                cancel,
                progress_callback,
            ),
            Playlist {
                playlist_id, title, ..
            } => {
                let playlist_dir = album_output_dir(output_dir, "Playlists", title, quality);
                let track_ids = extract_playlist_track_ids(&api, playlist_id)?;
                let total = u32::try_from(track_ids.len()).unwrap_or_default();
                let progress_callback = move |completed: u32| progress_callback(completed, total);
                download_playlist_tracks(
                    &mut api,
                    track_ids,
                    format_id,
                    &playlist_dir,
                    cancel,
                    progress_callback,
                )
            }
            Track { track_id, .. } => api
                .download_track_cancellable(*track_id, format_id, output_dir, None, Some(cancel))
                .map_err(AppError::from),
        }
    }
}

/// Extracts track IDs from a playlist.
///
/// # Errors
///
/// Returns `Download` if the playlist has no tracks.
fn extract_playlist_track_ids(
    api: &QobuzApiService,
    playlist_id: &str,
) -> Result<Vec<i32>, AppError> {
    let playlist = api
        .get_playlist(playlist_id, Some("tracks"))
        .map_err(AppError::from)?;
    playlist
        .tracks
        .and_then(|t| t.items)
        .map(|items| items.into_iter().filter_map(|t| t.id).collect())
        .filter(|ids: &Vec<i32>| !ids.is_empty())
        .ok_or_else(|| Download("No tracks in playlist".to_string()))
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
        match api.download_track_cancellable(tid, format_id, output_dir, None, Some(cancel)) {
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
        let completed = u32::try_from(i + 1).unwrap_or(total);
        progress_callback(completed, total);
    }
    last_path.ok_or_else(|| Download("No tracks downloaded".to_string()))
}

/// Downloads all tracks from a playlist.
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
fn download_playlist_tracks<F>(
    api: &mut QobuzApiService,
    track_ids: Vec<i32>,
    format_id: i32,
    output_dir: &Path,
    cancel: &AtomicBool,
    progress_callback: F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u32) + Send + Sync + 'static,
{
    let mut last_path = None;
    let mut completed = 0;
    for track_id in track_ids {
        if cancel.load(Relaxed) {
            return Err(Download("Download cancelled".to_string()));
        }
        match api.download_track_cancellable(track_id, format_id, output_dir, None, Some(cancel)) {
            Ok(path) => last_path = Some(path),
            Err(Canceled) => {
                return Err(Download("Download cancelled".to_string()));
            }
            Err(e)
                if e.to_string().contains("416")
                    || e.to_string().contains("Range Not Satisfiable") =>
            {
                info!(
                    track_id,
                    "File unavailable or already exists (416), skipping"
                );
            }
            Err(e) if last_path.is_none() => return Err(AppError::from(e)),
            Err(_) => {}
        }
        completed += 1;
        progress_callback(completed);
    }
    last_path.ok_or_else(|| Download("No tracks downloaded".to_string()))
}

/// Downloads all albums by an artist via the release list.
///
/// # Arguments
///
/// * `api` - API service reference
/// * `artist_id` - Artist ID
/// * `format_id` - Audio format ID for download
/// * `output_dir` - Output directory for downloaded files
/// * `cancel` - Cancellation flag checked between albums
/// * `progress_callback` - Called after each album download with (completed, total)
///
/// # Errors
///
/// Returns `Api` if the release list API call fails, or `Download` if no albums could be
/// downloaded.
fn execute_artist_download<F>(
    api: &mut QobuzApiService,
    artist_id: i32,
    format_id: i32,
    output_dir: &Path,
    cancel: &AtomicBool,
    progress_callback: F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u32, u32) + Send + Sync + 'static,
{
    let releases = api
        .get_release_list(artist_id, Some(50), None)
        .map_err(AppError::from)?;
    let album_items = releases.items.unwrap_or_default();
    if album_items.is_empty() {
        return Err(Download("No releases found for artist".to_string()));
    }
    let total = u32::try_from(album_items.len()).unwrap_or_default();
    let mut first_path: Option<PathBuf> = None;
    let mut completed = 0;
    for album in &album_items {
        if cancel.load(Relaxed) {
            return Err(Download("Download cancelled".to_string()));
        }
        let Some(album_id) = &album.id else {
            continue;
        };
        match api.download_album(album_id, format_id, output_dir, None, None) {
            Ok(paths) => {
                first_path = first_path.or_else(|| paths.into_iter().next());
            }
            Err(e)
                if e.to_string().contains("416")
                    || e.to_string().contains("Range Not Satisfiable") =>
            {
                info!(album = %album_id, "Album unavailable or already exists (416), skipping");
            }
            Err(e) => {
                error!(error = %e, album = %album_id, "Skipping failed album in artist download");
            }
        }
        completed += 1;
        progress_callback(completed, total);
    }
    first_path.ok_or_else(|| Download("No albums downloaded for artist".to_string()))
}

//! Download manager with concurrency control.

pub mod execution;
pub mod pool;

use std::{
    collections::HashMap,
    fmt::{Debug, Formatter, Result},
    hash::BuildHasher,
    sync::{Arc, atomic::AtomicBool},
};

use {
    async_channel::{Receiver, Sender, bounded, unbounded},
    libadwaita::gio::spawn_blocking,
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
};

use crate::download::{
    manager::pool::run_download_worker,
    progress::{DownloadCommand, DownloadEvent, DownloadTask},
};

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
    tasks: Arc<Mutex<HashMap<u64, DownloadTask>>>,
    /// Per-task cancellation signals, exposed for UI direct flag-setting.
    cancel_signals: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
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
            cancel_signals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the command sender for enqueuing/cancelling downloads.
    #[must_use]
    pub fn cmd_sender(&self) -> Sender<DownloadCommand> {
        self.cmd_sender.clone()
    }

    /// Returns the event receiver for receiving download progress updates.
    #[must_use]
    pub fn evt_receiver(&self) -> Receiver<DownloadEvent> {
        self.evt_receiver.clone()
    }

    /// Returns a shared handle to the tasks map for view access.
    #[must_use]
    pub fn tasks_handle(&self) -> Arc<Mutex<HashMap<u64, DownloadTask>>> {
        Arc::clone(&self.tasks)
    }

    /// Returns a shared handle to the cancel signals map for direct cancellation.
    #[must_use]
    pub fn cancel_signals_handle(&self) -> Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>> {
        Arc::clone(&self.cancel_signals)
    }

    /// Starts the download worker loop in a background thread.
    pub fn start_worker(&self) {
        let api_service = Arc::clone(&self.api_service);
        let cmd_sender = self.cmd_sender.clone();
        let cmd_receiver = self.cmd_receiver.clone();
        let evt_sender = self.evt_sender.clone();
        let tasks = Arc::clone(&self.tasks);
        let cancel_signals = Arc::clone(&self.cancel_signals);

        drop(spawn_blocking(move || {
            run_download_worker(
                &cmd_sender,
                &cmd_receiver,
                &evt_sender,
                &api_service,
                &tasks,
                &cancel_signals,
            );
        }));
    }
}

impl Debug for DownloadManager {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("DownloadManager").finish_non_exhaustive()
    }
}

/// Bundled references shared by download worker functions.
struct WorkerCtx<'a, S: BuildHasher, R: BuildHasher> {
    /// Command channel sender.
    cmd_sender: &'a Sender<DownloadCommand>,
    /// Command channel receiver.
    cmd_receiver: &'a Receiver<DownloadCommand>,
    /// Event channel sender.
    evt_sender: &'a Sender<DownloadEvent>,
    /// Shared API client.
    api_service: &'a Arc<Mutex<QobuzApiService>>,
    /// Tracked download tasks.
    tasks: &'a Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    /// Per-task cancellation signals.
    cancel_signals: &'a Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
    /// Shutdown flag.
    shutdown: &'a Arc<AtomicBool>,
}

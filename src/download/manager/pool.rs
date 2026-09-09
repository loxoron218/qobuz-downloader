//! Fixed-size download worker pool with shutdown coordination.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    thread::{Scope, scope},
};

use {
    async_channel::{Receiver, Sender},
    parking_lot::Mutex,
    qobuz_api::api::service::QobuzApiService,
    tracing::error,
};

use crate::download::{
    manager::{
        WorkerCtx,
        execution::{handle_cancel, handle_enqueued_download},
    },
    progress::{
        DownloadCommand::{self, Cancel, Enqueue, Shutdown},
        DownloadEvent, DownloadTask,
    },
};

/// Number of persistent download worker threads.
const WORKER_COUNT: usize = 3;

/// Spawns persistent worker threads that pull commands from the shared channel.
///
/// Each worker processes one download at a time — the fixed worker count replaces an explicit
/// semaphore, bounding concurrency and memory (no per-download OS thread spawn).
pub fn run_download_worker<
    S: BuildHasher + Send + Sync + 'static,
    R: BuildHasher + Send + Sync + 'static,
>(
    cmd_sender: &Sender<DownloadCommand>,
    cmd_receiver: &Receiver<DownloadCommand>,
    evt_sender: &Sender<DownloadEvent>,
    api_service: &Arc<Mutex<QobuzApiService>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
) {
    let shutdown = Arc::new(AtomicBool::new(false));

    scope(|s| {
        let ctx = WorkerCtx {
            cmd_sender,
            cmd_receiver,
            evt_sender,
            api_service,
            tasks,
            cancel_signals,
            shutdown: &shutdown,
        };
        for _ in 0..WORKER_COUNT {
            spawn_single_worker(s, &ctx);
        }
    });
}

/// Spawns a single worker thread on the given scope.
fn spawn_single_worker<
    'scope,
    S: BuildHasher + Send + Sync + 'static,
    R: BuildHasher + Send + Sync + 'static,
>(
    s: &'scope Scope<'scope, '_>,
    ctx: &WorkerCtx<'_, S, R>,
) {
    let cmd_sender = ctx.cmd_sender.clone();
    let cmd_receiver = ctx.cmd_receiver.clone();
    let evt_sender = ctx.evt_sender.clone();
    let api_service = Arc::clone(ctx.api_service);
    let tasks = Arc::clone(ctx.tasks);
    let cancel_signals = Arc::clone(ctx.cancel_signals);
    let shutdown = Arc::clone(ctx.shutdown);

    drop(s.spawn(move || {
        worker_loop(&WorkerCtx {
            cmd_sender: &cmd_sender,
            cmd_receiver: &cmd_receiver,
            evt_sender: &evt_sender,
            api_service: &api_service,
            tasks: &tasks,
            cancel_signals: &cancel_signals,
            shutdown: &shutdown,
        });
    }));
}

/// Broadcasts `Shutdown` to wake all other workers.
fn broadcast_shutdown(cmd_sender: &Sender<DownloadCommand>) {
    for _ in 1..WORKER_COUNT {
        if let Err(e) = cmd_sender.send_blocking(Shutdown) {
            error!(error = %e, "Failed to send shutdown command to worker");
        }
    }
}

/// Broadcasts `Shutdown` to wake all other workers.
///
/// Only the first worker to observe shutdown broadcasts, so the remaining
/// workers wake without an extra nesting level in the worker loop.
fn handle_first_shutdown(shutdown: &AtomicBool, cmd_sender: &Sender<DownloadCommand>) {
    if !shutdown.swap(true, SeqCst) {
        broadcast_shutdown(cmd_sender);
    }
}

/// Single persistent worker: pulls commands from the channel and processes them inline.
///
/// The worker count itself gates concurrency — no external semaphore needed.
fn worker_loop<S: BuildHasher + Send + Sync + 'static, R: BuildHasher + Send + Sync + 'static>(
    ctx: &WorkerCtx<'_, S, R>,
) {
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
                handle_first_shutdown(ctx.shutdown, ctx.cmd_sender);
                return;
            }
        }
    }
}

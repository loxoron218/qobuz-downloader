//! Download monitor UI for active downloads and history.
//!
//! Uses a `PreferencesGroup` + `Clamp` layout with a `Stack` switching between
//! an empty `StatusPage` and a `ListView` with `SignalListItemFactory`.
//! Matches the original `qobuz-downloader` download page UX.

pub mod binding;
pub mod ledger;
pub mod traversal;

use std::{
    collections::{HashMap, hash_map::RandomState},
    hash::BuildHasher,
    sync::Arc,
};

use {
    async_channel::Sender,
    libadwaita::{PreferencesGroup, gio::ListStore},
    parking_lot::Mutex,
};

use crate::download::progress::{DownloadCommand, DownloadTask};

/// Widgets for the embedded download queue section (no `ToolbarView` wrapping).
#[derive(Clone, Debug)]
pub struct QueueSection {
    /// The preferences group containing the queue header and stack.
    pub group: PreferencesGroup,
}

/// Mutable shared state threaded through the download queue row lifecycle.
struct RowContext<S: BuildHasher = RandomState> {
    /// Command sender for issuing download commands.
    cmd_sender: Arc<Sender<DownloadCommand>>,
    /// Shared task map for task status lookups and updates.
    tasks: Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    /// List model backing the queue view.
    model: ListStore,
}

/// Returns an owned copy of a row context for use in a `move` closure.
fn clone_row_context<S: BuildHasher>(ctx: &RowContext<S>) -> RowContext<S> {
    RowContext {
        cmd_sender: Arc::clone(&ctx.cmd_sender),
        tasks: Arc::clone(&ctx.tasks),
        model: ctx.model.clone(),
    }
}

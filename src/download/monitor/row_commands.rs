//! Shared download row commands used by the queue factory and event loop.
//!
//! Model refresh plus cancel dispatch shared by
//! [`binding`](crate::download::monitor::binding) and
//! [`ledger`](crate::download::monitor::ledger) so those siblings never import
//! each other.

use std::{collections::HashMap, hash::BuildHasher, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        gio::ListStore,
        glib::BoxedAnyObject,
        prelude::{Cast, ListModelExt},
    },
    parking_lot::Mutex,
    tracing::error,
};

use crate::download::progress::{DownloadCommand, DownloadRowData, DownloadTask};

/// Sends a download command without blocking the UI thread.
///
/// # Arguments
///
/// * `cmd_sender` - Sends download commands
/// * `cmd` - Command to dispatch
pub fn send_cancel_command(cmd_sender: &Arc<Sender<DownloadCommand>>, cmd: DownloadCommand) {
    if let Err(e) = cmd_sender.try_send(cmd) {
        error!(error = %e, "Failed to send cancel command");
    }
}

/// Refreshes a specific item in the model by finding its position and splicing.
///
/// # Arguments
///
/// * `model` - List model backing the queue view
/// * `id` - Task identifier to refresh
/// * `tasks` - Shared task map for fresh task data
pub fn refresh_model_item<S>(
    model: &ListStore,
    id: u64,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
) where
    S: BuildHasher,
{
    let n = model.n_items();
    for i in 0..n {
        let Some(obj) = model.item(i) else {
            continue;
        };
        let Ok(boxed) = obj.downcast::<BoxedAnyObject>() else {
            continue;
        };
        let model_id = {
            let data = boxed.borrow::<DownloadRowData>();
            data.task.id
        };
        if model_id != id {
            continue;
        }
        let map = tasks.lock();
        let task_clone = map.get(&id).cloned();
        drop(map);
        let Some(task) = task_clone else { break };
        let data = boxed.borrow::<DownloadRowData>();
        let cached_texture = data.texture.clone();
        drop(data);
        let new_row = DownloadRowData {
            task,
            texture: cached_texture,
        };
        let new_boxed = BoxedAnyObject::new(new_row);
        model.splice(i, 1, &[new_boxed]);
        break;
    }
}

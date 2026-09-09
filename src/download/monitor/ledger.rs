//! Download queue section builder and event loop.
//!
//! Builds the `PreferencesGroup` queue UI and drives it from download events.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::SystemTime,
};

use {
    async_channel::{Receiver, Sender},
    libadwaita::{
        PreferencesGroup, StatusPage,
        gio::ListStore,
        glib::{BoxedAnyObject, MainContext},
        gtk::{
            Align::Start,
            Box, Button, Label, ListView, NoSelection,
            Orientation::{Horizontal, Vertical},
            PolicyType::Automatic,
            ScrolledWindow, Stack,
        },
        prelude::{BoxExt, ButtonExt, Cast, ListModelExt, PreferencesGroupExt, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::{error, warn},
};

use crate::download::{
    monitor::{QueueSection, binding::setup_download_queue_factory},
    progress::{
        DownloadCommand::{self, Cancel},
        DownloadEvent::{self, Completed, Failed, Progress, Started},
        DownloadRowData,
        DownloadStatus::{
            Active, Cancelled, Completed as StatusCompleted, Failed as ItemFailed, Queued,
        },
        DownloadTask, cancel_all_tasks,
    },
};

/// Builds the download queue section (`PreferencesGroup` with header, empty/active
/// states) and starts the event loop. Returns the section components for embedding
/// into a dashboard or other container.
///
/// # Arguments
///
/// * `evt_receiver` - Receives download progress events
/// * `cmd_sender` - Sends download commands
/// * `tasks` - Shared tasks map
pub fn build_queue_section<S: BuildHasher + 'static, R: BuildHasher + 'static>(
    evt_receiver: Receiver<DownloadEvent>,
    cmd_sender: Sender<DownloadCommand>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
) -> QueueSection {
    let download_queue_group = PreferencesGroup::builder().build();

    let queue_header_box = Box::new(Horizontal, 12);

    let queue_title_label = Label::builder()
        .label("Download Queue")
        .css_classes(["heading"])
        .halign(Start)
        .build();

    let queue_subtitle_label = Label::builder()
        .label("Active downloads and queued items")
        .css_classes(["dim-label"])
        .halign(Start)
        .build();

    let header_content = Box::new(Vertical, 8);
    header_content.append(&queue_title_label);
    header_content.append(&queue_subtitle_label);
    header_content.set_hexpand(true);
    header_content.set_halign(Start);

    let cancel_all_button = Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel all downloads")
        .css_classes(["flat"])
        .sensitive(false)
        .build();

    queue_header_box.append(&header_content);
    queue_header_box.append(&cancel_all_button);

    download_queue_group.add(&queue_header_box);

    let stack = Stack::new();

    let empty_page = StatusPage::builder()
        .icon_name("folder-download-symbolic")
        .title("No Active Downloads")
        .description("Your download queue is empty. Search for music to start downloading.")
        .vexpand(true)
        .build();

    let empty_stack_page = stack.add_named(&empty_page, Some("empty"));
    for _ in [black_box(empty_stack_page)] {}

    let model = ListStore::new::<BoxedAnyObject>();
    let tasks_for_factory = Arc::clone(tasks);
    let cmd_sender_rc = Arc::new(cmd_sender);
    let no_selection = NoSelection::new(Some(model.clone()));
    let queue_list = ListView::new(
        Some(no_selection),
        Some(setup_download_queue_factory(
            &cmd_sender_rc,
            &tasks_for_factory,
            &model,
        )),
    );

    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .child(&queue_list)
        .build();
    scrolled.set_policy(Automatic, Automatic);

    let content_stack_page = stack.add_named(&scrolled, Some("content"));
    for _ in [black_box(content_stack_page)] {}
    stack.set_visible_child_name("empty");

    download_queue_group.add(&stack);

    {
        let btn = cancel_all_button.clone();
        let visible_handler = stack.connect_visible_child_name_notify(move |s| {
            btn.set_sensitive(s.visible_child_name() != Some("empty".into()));
        });
        for _ in [black_box(visible_handler)] {}
    }

    {
        let model = model;
        let stack = stack;
        let tasks_owned = Arc::clone(tasks);
        let cancel_signals = Arc::clone(cancel_signals);

        setup_cancel_all(
            &cancel_all_button,
            Arc::clone(&tasks_owned),
            Arc::clone(&cmd_sender_rc),
            cancel_signals,
            model.clone(),
            stack.clone(),
        );

        spawn_download_event_loop(evt_receiver, model, stack, tasks_owned);
    }

    QueueSection {
        group: download_queue_group,
    }
}

/// Processes download events on the main loop and updates the queue model.
fn spawn_download_event_loop<S: BuildHasher + 'static>(
    evt_receiver: Receiver<DownloadEvent>,
    model: ListStore,
    stack: Stack,
    tasks: Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
) {
    drop(MainContext::default().spawn_local(async move {
        while let Ok(event) = evt_receiver.recv().await {
            handle_event(&event, &model, &stack, &tasks);
        }
    }));
}

/// Sets cancel flags for all given task IDs.
fn set_cancel_signals<S: BuildHasher>(
    ids: &[u64],
    cancel_signals: &Mutex<HashMap<u64, Arc<AtomicBool>, S>>,
) {
    let mut signals = cancel_signals.lock();
    for &id in ids {
        signals
            .entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Relaxed);
    }
}

/// Sets up the "Cancel All" button signal handler.
fn setup_cancel_all<S: BuildHasher + 'static, R: BuildHasher + 'static>(
    button: &Button,
    tasks: Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cmd_sender: Arc<Sender<DownloadCommand>>,
    cancel_signals: Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
    model: ListStore,
    stack: Stack,
) {
    let cancel_all_handler = button.connect_clicked(move |_| {
        let ids: Vec<u64> = {
            let map = tasks.lock();
            map.iter()
                .filter(|(_, t)| matches!(t.status, Queued | Active))
                .map(|(id, _)| *id)
                .collect()
        };

        set_cancel_signals(&ids, &cancel_signals);

        for &id in &ids {
            send_cancel_command(&cmd_sender, Cancel { id });
        }

        cancel_all_tasks(&tasks);
        model.remove_all();
        stack.set_visible_child_name("empty");
    });
    for _ in [black_box(cancel_all_handler)] {}
}

/// Sends a cancel command via the sender, logging failures.
pub(super) fn send_cancel_command(cmd_sender: &Arc<Sender<DownloadCommand>>, cmd: DownloadCommand) {
    if let Err(e) = cmd_sender.try_send(cmd) {
        error!(error = %e, "Failed to send cancel command");
    }
}

/// Handles a download event and updates the model.
fn handle_event<S: BuildHasher>(
    event: &DownloadEvent,
    model: &ListStore,
    stack: &Stack,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
) {
    match event {
        Started { id } => {
            let map = tasks.lock();
            if let Some(task) = map.get(id) {
                let row = DownloadRowData {
                    task: task.clone(),
                    texture: None,
                };
                let boxed = BoxedAnyObject::new(row);
                model.append(&boxed);
            }
            drop(map);
            if model.n_items() > 0 {
                stack.set_visible_child_name("content");
            }
        }
        Progress {
            id,
            items_completed,
            total_items,
        } => {
            let mut map = tasks.lock();
            if let Some(task) = map.get_mut(id) {
                task.progress.items_completed = *items_completed;
                task.progress.total_items = *total_items;
            }
            drop(map);
            refresh_model_item(model, *id, tasks);
        }
        Completed { id, .. } => {
            let mut map = tasks.lock();
            if let Some(task) = map.get_mut(id) {
                task.status = StatusCompleted;
                task.completed_at = Some(SystemTime::now());
            }
            drop(map);
            refresh_model_item(model, *id, tasks);
        }
        Failed { id, error } => {
            if error.contains("cancelled") {
                warn!(task_id = id, error = %error, "Download cancelled by user");
            } else {
                error!(task_id = id, error = %error, "Download failed");
            }
            let mut map = tasks.lock();
            if let Some(task) = map.get_mut(id).filter(|t| t.status != Cancelled) {
                task.status = ItemFailed;
            }
            if let Some(task) = map.get_mut(id) {
                task.completed_at = Some(SystemTime::now());
            }
            drop(map);
            refresh_model_item(model, *id, tasks);
        }
    }
}

/// Refreshes a specific item in the model by finding its position and splicing.
pub(super) fn refresh_model_item<S: BuildHasher>(
    model: &ListStore,
    id: u64,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
) {
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

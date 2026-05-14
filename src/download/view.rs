//! Download view UI for active downloads and history.
//!
//! Uses a `PreferencesGroup` + `Clamp` layout with a `Stack` switching between
//! an empty `StatusPage` and a `ListView` with `SignalListItemFactory`.
//! Matches the original `qobuz-downloader-rs` download page UX.

use std::{
    collections::HashMap,
    rc::Rc,
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
        gdk::Texture,
        gio::{ListStore, spawn_blocking},
        glib::{BoxedAnyObject, MainContext, Object, object::ObjectType},
        gtk::{
            Align::{Center, Start},
            Box, Button,
            IconSize::Large,
            Image, Label, ListItem, ListView, NoSelection,
            Orientation::{Horizontal, Vertical},
            PolicyType::Automatic,
            ProgressBar, ScrolledWindow, SignalListItemFactory, Stack, Widget,
            pango::EllipsizeMode::End,
            prelude::{Cast, IsA},
        },
        prelude::{BoxExt, ButtonExt, ListItemExt, ListModelExt, PreferencesGroupExt, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::{error, warn},
};

use crate::{
    cover_art::{bytes_to_texture, fetch_image_bytes},
    download::progress::{
        DownloadCommand::{self, Cancel},
        DownloadEvent::{self, Completed, Failed, Progress, Started},
        DownloadRowData,
        DownloadStatus::{
            Active, Cancelled, Completed as StatusCompleted, Failed as ItemFailed, Queued,
        },
        DownloadTask, cancel_all_tasks,
    },
};

/// Widgets for the embedded download queue section (no `ToolbarView` wrapping).
#[derive(Clone)]
pub struct QueueSection {
    /// The preferences group containing the queue header and stack.
    pub group: PreferencesGroup,
}

/// Runs the event-processing loop for download events.
async fn run_event_loop(
    evt_receiver: Receiver<DownloadEvent>,
    model: ListStore,
    stack: Stack,
    tasks: Arc<Mutex<HashMap<u64, DownloadTask>>>,
) {
    while let Ok(event) = evt_receiver.recv().await {
        handle_event(&event, &model, &stack, &tasks);
    }
}

/// Builds the download queue section (`PreferencesGroup` with header, empty/active
/// states) and starts the event loop. Returns the section components for embedding
/// into a dashboard or other container.
///
/// # Arguments
///
/// * `evt_receiver` - Receives download progress events
/// * `cmd_sender` - Sends download commands
/// * `tasks` - Shared tasks map
pub fn build_queue_section(
    evt_receiver: Receiver<DownloadEvent>,
    cmd_sender: Sender<DownloadCommand>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
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

    stack.add_named(&empty_page, Some("empty"));

    let model = ListStore::new::<BoxedAnyObject>();
    let tasks_for_factory = Arc::clone(tasks);
    let cmd_sender_rc = Rc::new(cmd_sender);
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

    stack.add_named(&scrolled, Some("content"));
    stack.set_visible_child_name("empty");

    download_queue_group.add(&stack);

    {
        let btn = cancel_all_button.clone();
        stack.connect_visible_child_name_notify(move |s| {
            btn.set_sensitive(s.visible_child_name() != Some("empty".into()));
        });
    }

    {
        let model = model;
        let stack = stack;
        let tasks_owned = Arc::clone(tasks);
        let cancel_signals = Arc::clone(cancel_signals);

        setup_cancel_all(
            &cancel_all_button,
            Arc::clone(&tasks_owned),
            Rc::clone(&cmd_sender_rc),
            cancel_signals,
            model.clone(),
            stack.clone(),
        );

        MainContext::default().spawn_local(run_event_loop(evt_receiver, model, stack, tasks_owned));
    }

    QueueSection {
        group: download_queue_group,
    }
}

/// Sets cancel flags for all given task IDs.
fn set_cancel_signals(ids: &[u64], cancel_signals: &Mutex<HashMap<u64, Arc<AtomicBool>>>) {
    let mut signals = cancel_signals.lock();
    for &id in ids {
        signals
            .entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Relaxed);
    }
}

/// Sets up the "Cancel All" button signal handler.
fn setup_cancel_all(
    button: &Button,
    tasks: Arc<Mutex<HashMap<u64, DownloadTask>>>,
    cmd_sender: Rc<Sender<DownloadCommand>>,
    cancel_signals: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    model: ListStore,
    stack: Stack,
) {
    button.connect_clicked(move |_| {
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
}

/// Sends a cancel command via the sender, logging failures.
fn send_cancel_command(cmd_sender: &Rc<Sender<DownloadCommand>>, cmd: DownloadCommand) {
    if let Err(e) = cmd_sender.try_send(cmd) {
        error!(error = %e, "Failed to send cancel command");
    }
}

/// Wires the cancel button to send a `Cancel` command and update the UI immediately.
/// The task ID is looked up from a shared map keyed by button pointer address.
fn wire_cancel_button(
    button: &Button,
    cmd_sender: &Rc<Sender<DownloadCommand>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>,
    model: &ListStore,
    task_map: &Arc<Mutex<HashMap<usize, u64>>>,
) {
    let cmd_sender = Rc::clone(cmd_sender);
    let tasks = Arc::clone(tasks);
    let model = model.clone();
    let task_map = Arc::clone(task_map);
    let btn_key = button.as_ptr() as usize;

    button.connect_clicked(move |_| {
        let Some(id) = task_map.lock().get(&btn_key).copied() else {
            return;
        };
        send_cancel_command(&cmd_sender, Cancel { id });
        mark_task_cancelled(&tasks, id);
        refresh_model_item(&model, id, &tasks);
    });
}

/// Marks a task as cancelled in the tasks map.
fn mark_task_cancelled(tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>, id: u64) {
    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&id) {
        t.status = Cancelled;
        t.completed_at = Some(SystemTime::now());
    }
}

/// Sets up the `SignalListItemFactory` for download queue items.
fn setup_download_queue_factory(
    cmd_sender: &Rc<Sender<DownloadCommand>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>,
    model: &ListStore,
) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let tasks = Arc::clone(tasks);
    let task_map: Arc<Mutex<HashMap<usize, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    factory.connect_setup({
        let cmd_sender = Rc::clone(cmd_sender);
        let tasks = Arc::clone(&tasks);
        let model = model.clone();
        let task_map = Arc::clone(&task_map);
        move |_, list_item_obj| {
            setup_download_row(list_item_obj, &cmd_sender, &tasks, &model, &task_map);
        }
    });

    factory.connect_bind({
        let task_map = Arc::clone(&task_map);
        move |_, list_item_obj| {
            bind_download_row(list_item_obj, &task_map);
        }
    });

    factory
}

/// Creates the widget structure for a single download queue row and registers it.
fn setup_download_row(
    list_item_obj: &Object,
    cmd_sender: &Rc<Sender<DownloadCommand>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>,
    model: &ListStore,
    task_map: &Arc<Mutex<HashMap<usize, u64>>>,
) {
    let Some(list_item) = list_item_obj.downcast_ref::<ListItem>() else {
        return;
    };

    let main_box = Box::new(Horizontal, 16);
    main_box.set_margin_top(8);
    main_box.set_margin_bottom(8);
    main_box.set_margin_start(16);
    main_box.set_margin_end(16);

    let cover_image = Image::builder()
        .halign(Start)
        .valign(Center)
        .tooltip_text("Cover art")
        .build();
    cover_image.set_pixel_size(72);

    let metadata_box = Box::new(Vertical, 4);
    metadata_box.set_hexpand(true);
    metadata_box.set_valign(Center);

    let title_label = Label::builder()
        .halign(Start)
        .xalign(0.0)
        .ellipsize(End)
        .css_classes(["title-4"])
        .build();

    let subtitle_label = Label::builder()
        .halign(Start)
        .xalign(0.0)
        .ellipsize(End)
        .css_classes(["dim-label"])
        .build();

    let status_label = Label::builder()
        .halign(Start)
        .xalign(0.0)
        .css_classes(["caption"])
        .build();

    metadata_box.append(&title_label);
    metadata_box.append(&subtitle_label);
    metadata_box.append(&status_label);

    let progress_bar = ProgressBar::builder().show_text(true).hexpand(true).build();

    let progress_container = Box::new(Vertical, 4);
    progress_container.set_hexpand(true);
    progress_container.set_valign(Center);
    progress_container.append(&progress_bar);

    let cancel_button = Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel download")
        .css_classes(["flat", "circular"])
        .build();

    let action_container = Box::new(Vertical, 0);
    action_container.set_valign(Center);
    action_container.append(&cancel_button);

    main_box.append(&cover_image);
    main_box.append(&metadata_box);
    main_box.append(&progress_container);
    main_box.append(&action_container);

    wire_cancel_button(&cancel_button, cmd_sender, tasks, model, task_map);

    list_item.set_child(Some(&main_box));
}

/// Binds download task data to row widgets within a `ListItem`.
fn bind_download_row(list_item_obj: &Object, task_map: &Arc<Mutex<HashMap<usize, u64>>>) {
    let Some(list_item) = list_item_obj.downcast_ref::<ListItem>() else {
        return;
    };

    let Some(obj) = list_item.item() else {
        return;
    };
    let Ok(boxed) = obj.downcast::<BoxedAnyObject>() else {
        return;
    };

    let data = boxed.borrow::<DownloadRowData>();
    let task = data.task.clone();
    let texture = data.texture.clone();
    drop(data);

    let Some(child) = list_item.child() else {
        return;
    };
    let Some(main_box) = child.downcast_ref::<Box>() else {
        return;
    };

    let Some(cover_image) = first_child_of_box::<Image>(main_box) else {
        return;
    };
    let Some(metadata_box) = second_child_of_box::<Box>(main_box) else {
        return;
    };
    let Some(progress_container) = third_child_of_box::<Box>(main_box) else {
        return;
    };
    let Some(action_container) = last_child_of_box::<Box>(main_box) else {
        return;
    };

    let title_label = nth_child_of::<Label>(&metadata_box, 0);
    let subtitle_label = nth_child_of::<Label>(&metadata_box, 1);
    let status_label = nth_child_of::<Label>(&metadata_box, 2);
    let progress_bar = first_child_of::<ProgressBar>(&progress_container);
    let cancel_button = first_child_of::<Button>(&action_container);

    if let Some(label) = title_label {
        label.set_label(task.item.title());
    }
    if let Some(label) = subtitle_label {
        label.set_label(task.item.subtitle());
    }
    if let Some(label) = status_label {
        update_status_label(&label, &task);
    }
    if let Some(bar) = progress_bar {
        update_progress_bar(&bar, &task);
    }
    if let Some(btn) = cancel_button {
        update_cancel_button(&btn, &task);
        task_map.lock().insert(btn.as_ptr() as usize, task.id);
    }

    load_cover_texture(&cover_image, &task, texture.as_ref(), &boxed);
}

/// Returns the first child of a Box cast to T.
fn first_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let Ok(w) = container.first_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the first child of any container cast to T.
fn first_child_of<T: IsA<Widget>>(container: &impl IsA<Widget>) -> Option<T> {
    let Ok(w) = container.first_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the second child of a Box (`first_child` -> `next_sibling`) cast to T.
fn second_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let w = container.first_child().and_then(|w| w.next_sibling())?;
    let Ok(w) = w.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the third child of a Box cast to T.
fn third_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let w = container
        .first_child()
        .and_then(|w| w.next_sibling())
        .and_then(|w| w.next_sibling())?;
    let Ok(w) = w.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the last child of a Box cast to T.
fn last_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let Ok(w) = container.last_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the nth child of a container cast to T.
fn nth_child_of<T: IsA<Widget>>(container: &impl IsA<Widget>, n: usize) -> Option<T> {
    let mut child = container.first_child();
    for _ in 0..n {
        child = child.and_then(|w| w.next_sibling());
    }
    let Ok(w) = child?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Updates the status label text based on the download task state.
fn update_status_label(label: &Label, task: &DownloadTask) {
    match task.status {
        Queued => label.set_label("Queued"),
        Active => label.set_label("Downloading..."),
        StatusCompleted => label.set_label("Completed"),
        Cancelled => label.set_label("Cancelled"),
        ItemFailed => label.set_label("Failed"),
    }
}

/// Updates the progress bar visibility, fraction, and text.
fn update_progress_bar(bar: &ProgressBar, task: &DownloadTask) {
    match task.status {
        Active => {
            bar.set_visible(true);
            let fraction = task.progress.percentage().map_or(0.0, |p| p / 100.0);
            bar.set_fraction(fraction);
            bar.set_text(Some(&format!("{:.0}%", fraction * 100.0)));
        }
        _ => {
            bar.set_visible(false);
        }
    }
}

/// Updates the cancel button visibility based on download state.
fn update_cancel_button(button: &Button, task: &DownloadTask) {
    match task.status {
        Queued | Active => button.set_visible(true),
        _ => button.set_visible(false),
    }
}

/// Loads cover art texture asynchronously, with caching in the model data.
fn load_cover_texture(
    cover_image: &Image,
    task: &DownloadTask,
    cached_texture: Option<&Texture>,
    boxed: &BoxedAnyObject,
) {
    if let Some(tex) = cached_texture {
        cover_image.set_paintable(Some(tex));
        cover_image.set_pixel_size(72);
        return;
    }

    let Some(cover_url) = task.item.cover_url() else {
        cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
        cover_image.set_icon_size(Large);
        cover_image.set_pixel_size(72);
        return;
    };

    if cover_url.is_empty() {
        cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
        cover_image.set_icon_size(Large);
        cover_image.set_pixel_size(72);
        return;
    }

    cover_image.set_icon_name(Some("audio-x-generic-symbolic"));
    cover_image.set_icon_size(Large);
    cover_image.set_pixel_size(72);

    let cover_image_clone = cover_image.clone();
    let url_clone = cover_url.to_string();
    let boxed_clone = boxed.clone();

    let (tx, rx) = async_channel::bounded::<Vec<u8>>(1);
    spawn_blocking(move || {
        if let Some(bytes) = fetch_image_bytes(&url_clone)
            && let Err(e) = tx.send_blocking(bytes)
        {
            error!(error = %e, "Failed to send image bytes over channel");
        }
    });
    MainContext::default().spawn_local(apply_cover_texture(rx, cover_image_clone, boxed_clone));
}

/// Receives image bytes, converts to a texture, and applies to the image widget.
async fn apply_cover_texture(rx: Receiver<Vec<u8>>, image: Image, boxed: BoxedAnyObject) {
    let Ok(bytes) = rx.recv().await else {
        return;
    };
    let Some(tex) = bytes_to_texture(bytes) else {
        return;
    };
    image.set_paintable(Some(&tex));
    image.set_pixel_size(72);
    let mut data = boxed.borrow_mut::<DownloadRowData>();
    data.texture = Some(tex);
}

/// Handles a download event and updates the model.
fn handle_event(
    event: &DownloadEvent,
    model: &ListStore,
    stack: &Stack,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>,
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
fn refresh_model_item(model: &ListStore, id: u64, tasks: &Arc<Mutex<HashMap<u64, DownloadTask>>>) {
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

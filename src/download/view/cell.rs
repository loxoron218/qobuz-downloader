//! Download queue row factory and cell bindings.
//!
//! Builds queue rows and binds task data to their widgets.

use std::{collections::HashMap, hash::BuildHasher, hint::black_box, sync::Arc, time::SystemTime};

use {
    async_channel::Sender,
    libadwaita::{
        gdk::Texture,
        gio::ListStore,
        glib::{BoxedAnyObject, Object},
        gtk::{
            Align::{Center, Start},
            Box, Button,
            IconSize::Large,
            Image, Label, ListItem,
            Orientation::{Horizontal, Vertical},
            ProgressBar, SignalListItemFactory,
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, ButtonExt, Cast, ListItemExt, WidgetExt},
    },
    parking_lot::Mutex,
};

use crate::{
    browse::detail_common::portrait::spawn_cover_load,
    download::{
        progress::{
            DownloadCommand::{self, Cancel},
            DownloadRowData,
            DownloadStatus::{
                Active, Cancelled, Completed as StatusCompleted, Failed as ItemFailed, Queued,
            },
            DownloadTask,
        },
        view::{
            RowContext, TaskMap, clone_row_context,
            hierarchy::{
                first_child_of, first_child_of_box, last_child_of_box, nth_child_of,
                second_child_of_box, third_child_of_box,
            },
            queue::{refresh_model_item, send_cancel_command},
        },
    },
};

/// Sets up the `SignalListItemFactory` for download queue items.
pub(super) fn setup_download_queue_factory<S: BuildHasher + 'static>(
    cmd_sender: &Arc<Sender<DownloadCommand>>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    model: &ListStore,
) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let ctx = RowContext {
        cmd_sender: Arc::clone(cmd_sender),
        tasks: Arc::clone(tasks),
        model: model.clone(),
    };
    let setup_handler = factory.connect_setup({
        let ctx = clone_row_context(&ctx);
        move |_, list_item_obj| {
            setup_download_row(list_item_obj, &ctx);
        }
    });
    for _ in [black_box(setup_handler)] {}

    let bind_handler = factory.connect_bind(move |_, list_item_obj| {
        bind_download_row(list_item_obj);
    });
    for _ in [black_box(bind_handler)] {}

    factory
}

/// Creates the widget structure for a single download queue row and registers it.
fn setup_download_row<S: BuildHasher + 'static>(list_item_obj: &Object, ctx: &RowContext<S>) {
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

    wire_cancel_button(&cancel_button, list_item, ctx);

    list_item.set_child(Some(&main_box));
}

/// Binds download task data to row widgets within a `ListItem`.
fn bind_download_row(list_item_obj: &Object) {
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
    }

    load_cover_texture(&cover_image, &task, texture.as_ref(), &boxed);
}

/// Wires the cancel button to send a `Cancel` command and update the UI immediately.
/// The task ID is read from the row's currently bound item.
fn wire_cancel_button<S: BuildHasher + 'static>(
    button: &Button,
    list_item: &ListItem,
    ctx: &RowContext<S>,
) {
    let ctx = clone_row_context(ctx);
    let list_item = list_item.clone();

    let cancel_handler = button.connect_clicked(move |_| {
        let Some(item) = list_item.item() else {
            return;
        };
        let Ok(boxed) = item.downcast::<BoxedAnyObject>() else {
            return;
        };
        let data = boxed.borrow::<DownloadRowData>();
        let id = data.task.id;
        send_cancel_command(&ctx.cmd_sender, Cancel { id });
        mark_task_cancelled(&ctx.tasks, id);
        refresh_model_item(&ctx.model, id, &ctx.tasks);
    });
    for _ in [black_box(cancel_handler)] {}
}

/// Marks a task as cancelled in the tasks map.
fn mark_task_cancelled<S: BuildHasher>(tasks: &TaskMap<S>, id: u64) {
    let mut map = tasks.lock();
    if let Some(t) = map.get_mut(&id) {
        t.status = Cancelled;
        t.completed_at = Some(SystemTime::now());
    }
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
    let boxed_clone = boxed.clone();

    spawn_cover_load(cover_url.to_string(), move |tex| {
        cover_image_clone.set_paintable(Some(&tex));
        cover_image_clone.set_pixel_size(72);
        let mut data = boxed_clone.borrow_mut::<DownloadRowData>();
        data.texture = Some(tex);
    });
}

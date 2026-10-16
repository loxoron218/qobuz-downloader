//! Detail view widget builders for browse pages.
//!
//! Provides shared header, section, track row, quality selector, and download
//! control builders used by album, artist, and playlist detail views.

use std::{hint::black_box, path::PathBuf, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        HeaderBar, Toast, ToastOverlay, ToolbarView,
        glib::Object,
        gtk::{
            Align::{Center, Start},
            Box, Button, DropDown, Label,
            Orientation::{Horizontal, Vertical},
            PolicyType::Automatic,
            ScrolledWindow, StringList, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::error,
};

use crate::{
    audio_quality::Quality::{self, Flac16_44, Flac24_96, Flac24_192, Mp3_320},
    browse::detail_stage::track_row::format_duration,
    download::progress::{
        DownloadCommand::{self, Enqueue},
        DownloadTask,
    },
    preferences::settings::AppSettings,
};

/// Widgets for the header bar + scrolled content layout.
#[derive(Clone, Debug)]
pub struct HeaderScrollWidgets {
    /// Top toolbar with header bar.
    pub toolbar: ToolbarView,
    /// Content box inside the scrolled window.
    pub content: Box,
}

/// Builds the standard detail view chrome: header bar, scrolled window, and content box.
///
/// # Arguments
///
/// * `title` - Title shown in the header bar.
///
/// # Returns
///
/// * `HeaderScrollWidgets` - Toolbar and content box for the detail view.
#[must_use]
pub fn build_header_scroll(title: &str) -> HeaderScrollWidgets {
    let title_label = Label::builder()
        .label(title)
        .css_classes(["title"])
        .tooltip_text("Detail view title")
        .ellipsize(End)
        .build();
    title_label.update_property(&[PropertyLabel("Detail view title")]);
    let header = HeaderBar::builder()
        .title_widget(&title_label)
        .css_classes(["detail-header-bar"])
        .tooltip_text("Detail header")
        .build();
    let toolbar = ToolbarView::builder()
        .css_classes(["detail-toolbar"])
        .tooltip_text("Detail view")
        .build();
    toolbar.add_top_bar(&header);

    let content = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .css_classes(["detail-content"])
        .tooltip_text("Detail content")
        .build();
    content.update_property(&[PropertyLabel("Detail content")]);

    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(Automatic)
        .vscrollbar_policy(Automatic)
        .child(&content)
        .css_classes(["frame", "detail-scrolled-window"])
        .tooltip_text("Scrollable detail content")
        .build();
    toolbar.set_content(Some(&scrolled));

    HeaderScrollWidgets { toolbar, content }
}

/// Wraps the toolbar content in a `ToastOverlay` for transient notifications.
///
/// # Arguments
///
/// * `header_scroll` - Header and content widgets whose toolbar content is wrapped.
///
/// # Returns
///
/// * `ToastOverlay` - Overlay now holding the previous toolbar content.
#[must_use]
pub fn wrap_toast_overlay(header_scroll: &HeaderScrollWidgets) -> ToastOverlay {
    let toast_overlay: ToastOverlay = Object::builder::<ToastOverlay>()
        .property("css-classes", vec!["detail-toast-overlay".to_string()])
        .property("tooltip-text", "Notification overlay")
        .build();
    toast_overlay.update_property(&[PropertyLabel("Notification overlay")]);
    if let Some(child) = header_scroll.toolbar.content() {
        header_scroll.toolbar.set_content(None::<&Widget>);
        toast_overlay.set_child(Some(&child));
    }
    header_scroll.toolbar.set_content(Some(&toast_overlay));
    toast_overlay
}

/// Creates and appends a title label with standard configuration.
///
/// # Arguments
///
/// * `parent` - Container the label is appended to.
/// * `text` - Label text.
/// * `css_class` - CSS class applied to the label.
pub fn append_title_label(parent: &Box, text: &str, css_class: &str) {
    let label = Label::builder()
        .label(text)
        .css_classes([css_class])
        .tooltip_text(text)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(End)
        .build();
    label.update_property(&[PropertyLabel(text)]);
    parent.append(&label);
}

/// Creates a quality selector dropdown.
///
/// # Returns
///
/// * `DropDown` - Dropdown with the supported download qualities.
#[must_use]
pub fn build_quality_dropdown() -> DropDown {
    let model = StringList::new(&[
        "MP3 320kbps",
        "FLAC 16-bit / 44.1kHz",
        "FLAC 24-bit / 96kHz",
        "FLAC 24-bit / 192kHz",
    ]);
    let dropdown = DropDown::builder()
        .model(&model)
        .selected(1)
        .css_classes(["quality-dropdown"])
        .tooltip_text("Select download quality")
        .build();
    dropdown.update_property(&[PropertyLabel("Download quality selector")]);
    dropdown
}

/// Builds a quality selector row with label.
///
/// # Arguments
///
/// * `dropdown` - Quality selector appended to the row.
/// * `valign_center` - Whether the row is vertically centered.
///
/// # Returns
///
/// * `Box` - Horizontal row holding the label and dropdown.
#[must_use]
pub fn build_quality_row(dropdown: &DropDown, valign_center: bool) -> Box {
    let quality_row = Box::builder()
        .orientation(Horizontal)
        .spacing(8)
        .css_classes(["quality-row"])
        .tooltip_text("Download quality options")
        .build();
    if valign_center {
        quality_row.set_valign(Center);
    } else {
        quality_row.set_valign(Start);
    }
    quality_row.update_property(&[PropertyLabel("Download quality options")]);
    let ql = Label::builder()
        .label("Quality:")
        .css_classes(["quality-label"])
        .tooltip_text("Quality selector label")
        .xalign(0.0)
        .build();
    quality_row.append(&ql);
    quality_row.append(dropdown);
    quality_row
}

/// Builds a download action button.
///
/// # Arguments
///
/// * `label` - Button label.
///
/// # Returns
///
/// * `Button` - Styled download button.
#[must_use]
pub fn build_download_button(label: &str) -> Button {
    let button = Button::builder()
        .label(label)
        .css_classes(["suggested-action", "pill", "text-button"])
        .tooltip_text("Download selected quality")
        .halign(Start)
        .build();
    button.update_property(&[PropertyLabel("Download button")]);
    button
}

/// Builds a section header label and item container.
///
/// # Arguments
///
/// * `header_text` - Section header text.
/// * `spacing` - Spacing between items in the container.
///
/// # Returns
///
/// * `(Label, Box)` - Header label and vertical item container.
#[must_use]
pub fn build_item_section(header_text: &str, spacing: i32) -> (Label, Box) {
    let header = Label::builder()
        .label(header_text)
        .css_classes(["heading"])
        .tooltip_text(header_text)
        .xalign(0.0)
        .margin_top(12)
        .build();
    header.update_property(&[PropertyLabel(header_text)]);

    let container = Box::builder()
        .orientation(Vertical)
        .spacing(spacing)
        .margin_top(4)
        .css_classes(["item-section"])
        .tooltip_text("Item list")
        .build();
    container.update_property(&[PropertyLabel("Item list")]);

    (header, container)
}

/// Appends a separator label to a section.
///
/// # Arguments
///
/// * `section` - Section the separator is appended to.
pub fn append_separator(section: &Box) {
    let separator = Label::builder()
        .label("")
        .css_classes(["separator"])
        .tooltip_text("Section separator")
        .build();
    separator.update_property(&[PropertyLabel("Section separator")]);
    section.append(&separator);
}

/// Appends a track count and duration label to a section.
///
/// # Arguments
///
/// * `section` - Section the info label is appended to.
/// * `track_count` - Number of tracks.
/// * `total_duration` - Total duration in seconds.
pub fn append_track_count_duration(section: &Box, track_count: i32, total_duration: i32) {
    let duration_str = if total_duration > 0 {
        format!("{track_count} tracks • {}", format_duration(total_duration))
    } else {
        format!("{track_count} tracks")
    };
    let info_label = Label::builder()
        .label(&duration_str)
        .css_classes(["dim-label"])
        .tooltip_text("Track count and duration")
        .xalign(0.0)
        .ellipsize(End)
        .build();
    info_label.update_property(&[PropertyLabel("Track count and duration")]);
    section.append(&info_label);
}

/// Maps `DropDown` index to quality.
///
/// # Arguments
///
/// * `index` - Selected dropdown index.
///
/// # Returns
///
/// * `Quality` - Quality matching the index, defaulting to CD quality.
#[must_use]
pub const fn quality_from_index(index: u32) -> Quality {
    match index {
        0 => Mp3_320,
        2 => Flac24_96,
        3 => Flac24_192,
        _ => Flac16_44,
    }
}

/// Sends a download task to the queue.
///
/// # Arguments
///
/// * `cmd_sender` - Channel the enqueue command is sent on.
/// * `task` - Download task to enqueue.
pub fn send_enqueue(cmd_sender: &Sender<DownloadCommand>, task: DownloadTask) {
    if let Err(e) = cmd_sender.send_blocking(Enqueue { task }) {
        error!(error = %e, "Failed to enqueue download task");
    }
}

/// Builds quality dropdown row and download button, appending them to content.
///
/// # Arguments
///
/// * `content` - Container the controls are appended to.
/// * `button_label` - Download button label.
/// * `valign_center` - Whether the quality row is vertically centered.
///
/// # Returns
///
/// * `(DropDown, Button)` - Quality selector and download button.
#[must_use]
pub fn build_detail_controls(
    content: &Box,
    button_label: &str,
    valign_center: bool,
) -> (DropDown, Button) {
    let quality_dropdown = build_quality_dropdown();
    let quality_row = build_quality_row(&quality_dropdown, valign_center);
    content.append(&quality_row);

    let download_button = build_download_button(button_label);
    content.append(&download_button);

    (quality_dropdown, download_button)
}

/// Connects a download button click to extract quality and base directory, then calls the callback.
///
/// # Arguments
///
/// * `button` - Button whose click signal is connected.
/// * `quality_dropdown` - Dropdown the quality is read from on click.
/// * `settings` - Application settings holding the download directory.
/// * `toast_overlay` - Overlay the confirmation toast is shown on.
/// * `f` - Callback receiving the selected quality and base directory.
pub fn connect_download_click<F>(
    button: &Button,
    quality_dropdown: &DropDown,
    settings: Arc<Mutex<AppSettings>>,
    toast_overlay: &ToastOverlay,
    f: F,
) where
    F: Fn(Quality, PathBuf) + 'static,
{
    let quality_dropdown = quality_dropdown.clone();
    let toast_overlay = toast_overlay.clone();
    let download_handler = button.connect_clicked(move |_| {
        let quality = quality_from_index(quality_dropdown.selected());
        let base_dir = settings.lock().download_directory.clone();
        let toast = Toast::new("Added to download queue");
        toast.set_timeout(2);
        toast_overlay.add_toast(toast);
        f(quality, base_dir);
    });
    for _ in [black_box(download_handler)] {}
}

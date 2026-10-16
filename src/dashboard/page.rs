//! Dashboard page construction and quality settings.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    hint::black_box,
    sync::{Arc, atomic::AtomicBool},
};

use {
    async_channel::{Receiver, Sender},
    libadwaita::{
        ComboRow, EntryRow, HeaderBar, PreferencesGroup, ToastOverlay, ToolbarView,
        glib::Object,
        gtk::{
            Align::Center, Button, Label, StringList, accessible::Property::Label as PropertyLabel,
        },
        prelude::{
            AccessibleExtManual, BoxExt, ButtonExt, ComboRowExt, EditableExt, PreferencesGroupExt,
            WidgetExt,
        },
    },
    parking_lot::Mutex,
    tracing::{info, warn},
};

use crate::{
    app::AppState,
    audio_quality::Quality::{self, Flac16_44, Flac24_96, Flac24_192, Mp3_320},
    dashboard::{
        fetch::{DownloadCtx, create_persistent_toast, fetch_and_enqueue},
        url_parse::try_parse_download_url,
    },
    download::{
        monitor::ledger::build_queue_section,
        progress::{DownloadCommand, DownloadEvent, DownloadTask},
    },
    preferences::settings::save_settings,
    shell::{build_content_clamp, wrap_clamp_in_scrolled},
};

/// Widgets from the dashboard page for external event handling.
#[derive(Clone, Debug)]
pub struct DashboardWidgets {
    /// Root container widget.
    pub root: ToolbarView,
    /// Header bar widget.
    pub header: HeaderBar,
}

/// Download control widgets for the dashboard.
struct DownloadControls {
    /// Group holding the download rows.
    group: PreferencesGroup,
    /// URL entry row.
    url_entry: EntryRow,
    /// Quality selector.
    quality_combo: ComboRow,
    /// Download action button.
    download_button: Button,
}

/// Maps a `ComboRow` selected index to a `Quality` value.
const fn combo_index_to_quality(index: u32) -> Quality {
    match index {
        0 => Mp3_320,
        2 => Flac24_96,
        3 => Flac24_192,
        _ => Flac16_44,
    }
}

/// Maps a `Quality` value to the `ComboRow` selected index.
const fn quality_to_combo_index(quality: Quality) -> u32 {
    match quality {
        Mp3_320 => 0,
        Flac16_44 => 1,
        Flac24_96 => 2,
        Flac24_192 => 3,
    }
}

/// Updates the default quality in settings and persists to disk.
fn update_saved_quality(state: &AppState, quality: Quality) {
    let mut settings = state.settings.lock();
    settings.default_quality = quality;
    drop(settings);
    save_current_settings(state);
}

/// Persists current settings to disk, logging any error silently.
fn save_current_settings(state: &AppState) {
    let settings = state.settings.lock();
    if let Err(err) = save_settings(&settings) {
        warn!(error = %err, "Failed to save settings");
    }
}

/// Builds the dashboard page and returns the root widget and widget references.
///
/// # Arguments
///
/// * `state` - Shared application state
/// * `cmd_sender` - Channel sender for download commands
/// * `evt_receiver` - Channel receiver for download events
/// * `tasks` - Shared task map from the download manager
/// * `cancel_signals` - Shared cancel signals map for direct cancellation
pub fn build<S: BuildHasher + 'static, R: BuildHasher + 'static>(
    state: &AppState,
    cmd_sender: Sender<DownloadCommand>,
    evt_receiver: Receiver<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<u64, DownloadTask, S>>>,
    cancel_signals: &Arc<Mutex<HashMap<u64, Arc<AtomicBool>, R>>>,
) -> DashboardWidgets {
    let title_label = Label::builder()
        .label("Dashboard")
        .css_classes(["dashboard-title"])
        .tooltip_text("Dashboard")
        .build();
    let header = HeaderBar::builder()
        .title_widget(&title_label)
        .css_classes(["dashboard-header-bar"])
        .tooltip_text("Dashboard header")
        .build();
    let toolbar = ToolbarView::builder()
        .css_classes(["dashboard-toolbar"])
        .tooltip_text("Dashboard")
        .build();
    toolbar.update_property(&[PropertyLabel("Dashboard")]);
    toolbar.add_top_bar(&header);

    let toast_overlay: ToastOverlay = Object::builder::<ToastOverlay>()
        .property("css-classes", vec!["dashboard-toast-overlay".to_string()])
        .property("tooltip-text", "Notification overlay")
        .build();

    let (main_clamp, main_box) = build_content_clamp();

    let download_controls = build_download_controls(state);
    let download_group = download_controls.group;
    let url_entry = download_controls.url_entry;
    let quality_combo = download_controls.quality_combo;
    let download_button = download_controls.download_button;
    download_group.add(&url_entry);
    download_group.add(&quality_combo);
    download_group.add(&download_button);
    main_box.append(&download_group);

    let queue_section =
        build_queue_section(evt_receiver, cmd_sender.clone(), tasks, cancel_signals);
    main_box.append(&queue_section.group);

    main_clamp.set_child(Some(&main_box));

    let scrolled = wrap_clamp_in_scrolled(&main_clamp);

    toast_overlay.set_child(Some(&scrolled));
    toolbar.set_content(Some(&toast_overlay));

    let download_button_btn = download_button.clone();
    let api_service = Arc::clone(&state.api_service);
    let state = state.clone();

    let download_handler = download_button.connect_clicked(move |_| {
        let text = url_entry.text().to_string();
        let Some(parsed) = try_parse_download_url(&text, &toast_overlay) else {
            return;
        };
        let quality = combo_index_to_quality(quality_combo.selected());
        info!(quality = %quality, input = %text, "Dashboard download initiated");
        update_saved_quality(&state, quality);
        let output_dir = state.settings.lock().download_directory.clone();
        save_current_settings(&state);

        download_button_btn.set_sensitive(false);
        let fetching_toast = create_persistent_toast("Fetching metadata...", &toast_overlay);

        fetch_and_enqueue(
            &api_service,
            parsed,
            quality,
            output_dir,
            DownloadCtx {
                fetching_toast,
                download_button: download_button_btn.clone(),
                cmd_sender: cmd_sender.clone(),
                toast_overlay: toast_overlay.clone(),
            },
        );
    });
    for _ in [black_box(download_handler)] {}

    DashboardWidgets {
        root: toolbar,
        header,
    }
}

/// Builds the dashboard download group with entry, quality, and button.
///
/// # Arguments
///
/// * `state` - Shared application state for the default quality.
///
/// # Returns
///
/// Download control widgets.
fn build_download_controls(state: &AppState) -> DownloadControls {
    let download_group = PreferencesGroup::builder()
        .title("Download")
        .description("Paste a Qobuz URL or ID to start downloading")
        .css_classes(["download-group"])
        .tooltip_text("Download from URL")
        .build();

    let url_entry = EntryRow::builder()
        .title("Qobuz URL or ID")
        .css_classes(["url-entry"])
        .tooltip_text("Paste a Qobuz album, track, playlist, or artist URL or ID")
        .build();

    let quality_model = StringList::new(&[
        "MP3 320kbps",
        "FLAC 16-bit / 44.1kHz",
        "FLAC 24-bit / 96kHz",
        "FLAC 24-bit / 192kHz",
    ]);

    let default_quality = {
        let settings = state.settings.lock();
        settings.default_quality
    };

    let quality_combo = ComboRow::builder()
        .title("Audio Quality")
        .subtitle("Higher quality requires more storage space")
        .model(&quality_model)
        .selected(quality_to_combo_index(default_quality))
        .css_classes(["quality-combo"])
        .tooltip_text("Select default audio quality")
        .build();

    let download_button = Button::builder()
        .label("Download")
        .css_classes(["suggested-action", "pill", "text-button"])
        .tooltip_text("Start download")
        .halign(Center)
        .hexpand(true)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(24)
        .margin_end(24)
        .build();

    download_button.update_property(&[PropertyLabel("Start download")]);
    DownloadControls {
        group: download_group,
        url_entry,
        quality_combo,
        download_button,
    }
}

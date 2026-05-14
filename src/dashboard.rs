//! Dashboard page — the default landing page after authentication.
//!
//! Contains a URL/ID entry for direct downloads, a quality selector, a download
//! button, and an embedded download queue section matching the original
//! `qobuz-downloader-rs` layout.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use {
    async_channel::{Receiver, Sender, bounded},
    libadwaita::{
        ComboRow, EntryRow, HeaderBar, PreferencesGroup, Toast, ToastOverlay, ToolbarView,
        gio::spawn_blocking,
        glib::MainContext,
        gtk::{Align::Center, Button, Label, StringList},
        prelude::{BoxExt, ButtonExt, ComboRowExt, EditableExt, PreferencesGroupExt, WidgetExt},
    },
    parking_lot::Mutex,
    qobuz_api_rust_refactor::api::service::QobuzApiService,
    regex::Regex,
    tracing::{error, warn},
    uuid::Uuid,
};

use crate::{
    app::AppState,
    download::{
        progress::{
            DownloadCommand::{self, Enqueue},
            DownloadEvent,
            DownloadItem::{self, Album, Playlist, Track},
            DownloadTask,
        },
        view::build_queue_section,
    },
    preferences::settings::save_settings,
    types::Quality::{self, Flac16_44, Flac24_96, Flac24_192, Mp3_320},
    ui::{build_content_clamp, wrap_clamp_in_scrolled},
};

/// Widgets from the dashboard page for external event handling.
#[derive(Clone)]
pub struct DashboardWidgets {
    /// Root container widget.
    pub root: ToolbarView,
    /// Header bar widget.
    pub header: HeaderBar,
}

/// Context for enqueuing a download after metadata fetch.
struct DownloadCtx {
    /// Persistent fetching toast to dismiss on completion.
    fetching_toast: Toast,
    /// Download button to re-enable after fetch.
    download_button: Button,
    /// Command sender for enqueuing.
    cmd_sender: Sender<DownloadCommand>,
    /// Toast overlay for error feedback.
    toast_overlay: ToastOverlay,
}

/// Fetched metadata for a download item.
struct FetchedMeta {
    /// Display title.
    title: String,
    /// Display artist.
    artist: String,
    /// Cover art URL.
    cover_url: Option<String>,
}

/// Download type parsed from a Qobuz URL or ID.
#[derive(Clone, Debug)]
enum ParsedUrl {
    /// Album download.
    Album(String),
    /// Track download.
    Track(String),
    /// Playlist download.
    Playlist(String),
}

/// Parses a Qobuz URL or direct ID into a download type.
///
/// # Arguments
///
/// * `input` - The URL or ID string to parse
///
/// # Returns
///
/// `Some(ParsedUrl)` if the input is valid, `None` otherwise.
fn parse_qobuz_url(input: &str) -> Option<ParsedUrl> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Some(ParsedUrl::Album(trimmed.to_string()));
    }

    let playlist_re = match Regex::new(
        r"https?://(?:www\.|play\.|open\.)?qobuz\.com/(?:[a-z]{2}-[a-z]{2}/)?playlists?/[^/]+/(\d+)",
    ) {
        Ok(re) => re,
        Err(e) => {
            warn!(error = %e, "Failed to compile playlist regex");
            return None;
        }
    };

    let album_play_re =
        match Regex::new(r"https?://(?:play\.|open\.)?qobuz\.com/album/([a-zA-Z0-9-]+)") {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile album play regex");
                return None;
            }
        };
    let track_play_re =
        match Regex::new(r"https?://(?:play\.|open\.)?qobuz\.com/track/([a-zA-Z0-9-]+)") {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile track play regex");
                return None;
            }
        };

    let old_album_re =
        match Regex::new(r"https?://(?:www\.)?qobuz\.com/(?:[a-z]{2}-[a-z]{2}/)?album/[^/]+/(\d+)")
        {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile old album regex");
                return None;
            }
        };
    let old_track_re =
        match Regex::new(r"https?://(?:www\.)?qobuz\.com/(?:[a-z]{2}-[a-z]{2}/)?track/[^/]+/(\d+)")
        {
            Ok(re) => re,
            Err(e) => {
                warn!(error = %e, "Failed to compile old track regex");
                return None;
            }
        };

    if let Some(caps) = album_play_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(ParsedUrl::Album(id.as_str().to_string()));
    }
    if let Some(caps) = track_play_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(ParsedUrl::Track(id.as_str().to_string()));
    }
    if let Some(caps) = playlist_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(ParsedUrl::Playlist(id.as_str().to_string()));
    }
    if let Some(caps) = old_album_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(ParsedUrl::Album(id.as_str().to_string()));
    }
    if let Some(caps) = old_track_re.captures(trimmed)
        && let Some(id) = caps.get(1)
    {
        return Some(ParsedUrl::Track(id.as_str().to_string()));
    }

    None
}

/// Maps a `ComboRow` selected index to a `Quality` value.
fn combo_index_to_quality(index: u32) -> Quality {
    match index {
        0 => Mp3_320,
        2 => Flac24_96,
        3 => Flac24_192,
        _ => Flac16_44,
    }
}

/// Maps a `Quality` value to the `ComboRow` selected index.
fn quality_to_combo_index(quality: Quality) -> u32 {
    match quality {
        Mp3_320 => 0,
        Flac16_44 => 1,
        Flac24_96 => 2,
        Flac24_192 => 3,
    }
}

/// Fetches album metadata from the Qobuz API.
fn fetch_album_meta(api: &QobuzApiService, album_id: &str) -> Option<FetchedMeta> {
    let album = match api.get_album(album_id, None) {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, album_id = %album_id, "Failed to fetch album metadata");
            return None;
        }
    };
    let title = album
        .title
        .as_deref()
        .unwrap_or("Unknown Album")
        .to_string();
    let artist = album
        .artist
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist")
        .to_string();
    let cover_url = album.image.as_ref().and_then(|img| {
        img.thumbnail
            .clone()
            .or_else(|| img.small.clone())
            .or_else(|| img.url.clone())
    });
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Fetches track metadata from the Qobuz API.
fn fetch_track_meta(api: &QobuzApiService, track_id: i32) -> Option<FetchedMeta> {
    let track = match api.get_track(track_id) {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, track_id = %track_id, "Failed to fetch track metadata");
            return None;
        }
    };
    let title = track
        .title
        .as_deref()
        .unwrap_or("Unknown Track")
        .to_string();
    let artist = track
        .performer
        .as_ref()
        .and_then(|a| a.name.as_deref())
        .unwrap_or("Unknown Artist")
        .to_string();
    let cover_url = track.album.as_ref().and_then(|a| {
        let img = a.image.as_ref()?;
        img.thumbnail
            .clone()
            .or_else(|| img.small.clone())
            .or_else(|| img.url.clone())
    });
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Fetches playlist metadata from the Qobuz API.
fn fetch_playlist_meta(api: &QobuzApiService, playlist_id: &str) -> Option<FetchedMeta> {
    let playlist = match api.get_playlist(playlist_id, Some("tracks")) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, playlist_id = %playlist_id, "Failed to fetch playlist metadata");
            return None;
        }
    };
    let title = playlist
        .name
        .as_deref()
        .unwrap_or("Unknown Playlist")
        .to_string();
    let artist = playlist
        .creator_name()
        .unwrap_or("Unknown Creator")
        .to_string();
    let cover_url = playlist.best_image_url(true);
    Some(FetchedMeta {
        title,
        artist,
        cover_url,
    })
}

/// Validates and parses the input text, showing an error toast if invalid.
///
/// # Returns
///
/// `Some(ParsedUrl)` if the input is valid, `None` if validation or parsing fails.
fn try_parse_download_url(text: &str, toast_overlay: &ToastOverlay) -> Option<ParsedUrl> {
    if text.trim().is_empty() {
        let toast = Toast::new("Please enter a Qobuz URL or ID");
        toast.set_timeout(3);
        toast_overlay.add_toast(toast);
        return None;
    }
    let parsed = parse_qobuz_url(text);
    if parsed.is_none() {
        let toast = Toast::new("Invalid Qobuz URL or ID format");
        toast.set_timeout(3);
        toast_overlay.add_toast(toast);
    }
    parsed
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

/// Creates a persistent toast (no auto-dismiss) and adds it to the overlay.
fn create_persistent_toast(message: &str, overlay: &ToastOverlay) -> Toast {
    let toast = Toast::new(message);
    toast.set_timeout(0);
    overlay.add_toast(toast.clone());
    toast
}

/// Fetches metadata via `spawn_blocking`, then enqueues the download on the main context.
fn fetch_and_enqueue(
    api_service: &Arc<Mutex<QobuzApiService>>,
    parsed: ParsedUrl,
    quality: Quality,
    output_dir: PathBuf,
    ctx: DownloadCtx,
) {
    let (tx, rx) = bounded::<Option<FetchedMeta>>(1);
    let api_service = Arc::clone(api_service);
    let parsed_spawn = parsed.clone();

    spawn_blocking(move || {
        let api = api_service.lock();
        let meta = match &parsed_spawn {
            ParsedUrl::Album(id) => fetch_album_meta(&api, id),
            ParsedUrl::Track(id) => fetch_track_meta(&api, id.parse::<i32>().unwrap_or(0)),
            ParsedUrl::Playlist(id) => fetch_playlist_meta(&api, id),
        };
        drop(api);
        if let Err(e) = tx.send_blocking(meta) {
            error!(error = %e, "Failed to send fetched metadata");
        }
    });

    MainContext::default().spawn_local(async move {
        let meta = rx.recv().await.unwrap_or(None);
        ctx.fetching_toast.dismiss();
        ctx.download_button.set_sensitive(true);

        let Some(meta) = meta else {
            let toast = Toast::new("Failed to fetch metadata from Qobuz");
            toast.set_timeout(3);
            ctx.toast_overlay.add_toast(toast);
            return;
        };

        let item = build_download_item(&parsed, &meta);
        let task = DownloadTask::new(item, quality, output_dir);
        if let Err(err) = ctx.cmd_sender.send(Enqueue { task }).await {
            warn!(error = %err, "Failed to enqueue dashboard download");
        }
    });
}

/// Constructs a `DownloadItem` from a parsed URL and fetched metadata.
fn build_download_item(parsed: &ParsedUrl, meta: &FetchedMeta) -> DownloadItem {
    match parsed {
        ParsedUrl::Album(id) => Album {
            album_id: id.clone(),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            cover_url: meta.cover_url.clone(),
        },
        ParsedUrl::Track(id) => Track {
            track_id: id.parse::<i32>().unwrap_or(0),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            cover_url: meta.cover_url.clone(),
        },
        ParsedUrl::Playlist(id) => Playlist {
            playlist_id: id.clone(),
            title: meta.title.clone(),
            cover_url: meta.cover_url.clone(),
        },
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
pub fn build(
    state: &AppState,
    cmd_sender: Sender<DownloadCommand>,
    evt_receiver: Receiver<DownloadEvent>,
    tasks: &Arc<Mutex<HashMap<Uuid, DownloadTask>>>,
    cancel_signals: &Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
) -> DashboardWidgets {
    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();
    header.set_title_widget(Some(&Label::new(Some("Dashboard"))));

    toolbar.add_top_bar(&header);

    let toast_overlay = ToastOverlay::new();

    let (main_clamp, main_box) = build_content_clamp();

    let download_group = PreferencesGroup::builder()
        .title("Download")
        .description("Paste a Qobuz URL or ID to start downloading")
        .build();

    let url_entry = EntryRow::builder().title("Qobuz URL or ID").build();

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
        .build();

    let download_button = Button::builder()
        .label("Download")
        .css_classes(["suggested-action", "pill"])
        .halign(Center)
        .hexpand(true)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();

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

    download_button.connect_clicked(move |_| {
        let text = url_entry.text().to_string();
        let Some(parsed) = try_parse_download_url(&text, &toast_overlay) else {
            return;
        };
        let quality = combo_index_to_quality(quality_combo.selected());
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

    DashboardWidgets {
        root: toolbar,
        header,
    }
}

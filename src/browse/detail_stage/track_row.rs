//! Track row widgets for detail views.
//!
//! Single-track row builder with disc, number, title, duration, and Hi-Res
//! marker plus duration formatting shared by album and playlist views.

use {
    libadwaita::{
        gtk::{
            Align::Center, Box, Image, Label, Orientation::Horizontal,
            accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt},
    },
    qobuz_api::api::content::tracks::Track,
};

/// Formats duration in seconds to MM:SS.
///
/// # Arguments
///
/// * `seconds` - Duration in seconds.
///
/// # Returns
///
/// * `String` - Duration formatted as minutes and zero-padded seconds.
#[must_use]
pub fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Builds a single track row widget.
///
/// # Arguments
///
/// * `track` - Track metadata shown in the row.
///
/// # Returns
///
/// * `Box` - Horizontal row with disc, number, title, duration, and Hi-Res marker.
#[must_use]
pub fn build_track_row(track: &Track) -> Box {
    let row = Box::builder()
        .orientation(Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["track-row"])
        .tooltip_text("Track row")
        .build();
    row.update_property(&[PropertyLabel("Track row")]);

    let disc_num = track.media_number.unwrap_or(1);
    let disc_label = Label::builder()
        .label(disc_num.to_string())
        .css_classes(["dim-label"])
        .tooltip_text("Disc number")
        .xalign(0.0)
        .width_chars(3)
        .build();
    row.append(&disc_label);

    let track_num = track.track_number.unwrap_or(0);
    let num_label = Label::builder()
        .label(format!("{track_num:02}."))
        .css_classes(["dim-label"])
        .tooltip_text("Track number")
        .xalign(0.0)
        .width_chars(4)
        .build();
    row.append(&num_label);

    let title = track.title.as_deref().unwrap_or("Unknown Track");
    let title_label = Label::builder()
        .label(title)
        .css_classes(["track-title"])
        .tooltip_text("Track title")
        .hexpand(true)
        .xalign(0.0)
        .ellipsize(End)
        .build();
    row.append(&title_label);

    let duration = track.duration.unwrap_or(0);
    let dur_label = Label::builder()
        .label(format_duration(duration))
        .css_classes(["dim-label"])
        .tooltip_text("Track duration")
        .xalign(1.0)
        .build();
    row.append(&dur_label);

    let is_hires = track.hires.unwrap_or(false)
        || track.maximum_bit_depth.unwrap_or(0) >= 24
        || track.maximum_sampling_rate.unwrap_or(0.0) > 48.0;
    if is_hires {
        let hires_icon = Image::builder()
            .halign(Center)
            .valign(Center)
            .tooltip_text("Hi-Res Audio")
            .css_classes(["hires-icon"])
            .pixel_size(16)
            .margin_start(8)
            .build();
        hires_icon.set_from_file(Some("./assets/hires.png"));
        row.append(&hires_icon);
    }

    row
}

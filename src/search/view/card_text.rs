//! Search result text formatting: subtitles, durations, and quality strings.

use crate::search::view::SearchResultItem::{self, Album, Artist, Playlist, Track};

/// Pushes formatted duration and quality string parts if non-empty.
pub fn push_quality_parts(
    parts: &mut Vec<String>,
    duration: i32,
    bit_depth: i32,
    sampling_rate: f64,
) {
    if duration > 0 {
        parts.push(format_duration(duration));
    }
    let q = quality_string(bit_depth, sampling_rate);
    if !q.is_empty() {
        parts.push(q);
    }
}

/// Formats a subtitle string for a search result item.
#[must_use]
pub fn build_subtitle(item: &SearchResultItem) -> String {
    match item {
        Track {
            artist,
            album,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone(), album.clone()];
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        Album {
            artist,
            year,
            track_count,
            duration,
            bit_depth,
            sampling_rate,
            ..
        } => {
            let mut parts = vec![artist.clone()];
            if !year.is_empty() {
                parts.push(year.clone());
            }
            if *track_count > 0 {
                parts.push(format!("{track_count} tracks"));
            }
            push_quality_parts(&mut parts, *duration, *bit_depth, *sampling_rate);
            parts.join(" • ")
        }
        Artist { .. } => String::from("Artist"),
        Playlist { .. } => String::from("Playlist"),
    }
}

/// Formats a duration in seconds to `MM:SS` format.
#[must_use]
pub fn format_duration(seconds: i32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{mins}:{secs:02}")
}

/// Formats a quality string from bit depth and sampling rate.
#[must_use]
pub fn quality_string(bit_depth: i32, sampling_rate: f64) -> String {
    if bit_depth > 0 && sampling_rate > 0.0 {
        let rate_str = if sampling_rate.fract() == 0.0 {
            format!("{sampling_rate:.0}")
        } else {
            format!("{sampling_rate:.1}")
        };
        format!("{bit_depth}-bit/{rate_str}kHz")
    } else {
        String::new()
    }
}

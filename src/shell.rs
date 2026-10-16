//! Application shell scaffolding utilities.

use std::{
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{SystemTime, UNIX_EPOCH},
};

use libadwaita::{
    Clamp,
    gtk::{
        Box, Orientation::Vertical, PolicyType::Automatic, ScrolledWindow,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

/// Debounces repeated events by timestamp, returning true when the gap since the last
/// accepted event is at least `min_gap_ms` milliseconds.
///
/// Updates the shared timestamp when the event is accepted.
///
/// # Arguments
///
/// * `last_ts` - Atomic timestamp tracking the last accepted event
/// * `min_gap_ms` - Minimum gap between accepted events
///
/// # Returns
///
/// `true` if the event should be processed, `false` if it is too soon.
pub fn debounce_elapsed(last_ts: &AtomicU64, min_gap_ms: u64) -> bool {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let now = dur
        .as_secs()
        .saturating_mul(1000)
        .saturating_add(u64::from(dur.subsec_millis()));
    if now.saturating_sub(last_ts.load(Relaxed)) < min_gap_ms {
        return false;
    }
    last_ts.store(now, Relaxed);
    true
}

/// Creates a content clamp with standard page margins.
///
/// Returns a tuple of `(Clamp, Box)` where the clamp has a maximum size of 800 and
/// the box is vertically oriented with 24px margins on all sides. Append children
/// to the box, then call `clamp.set_child(Some(&box))` and wrap the clamp in a
/// `Box(Vertical, 0)` → `ScrolledWindow`.
///
/// # Returns
///
/// A tuple of `(Clamp, Box)`.
#[must_use]
pub fn build_content_clamp() -> (Clamp, Box) {
    let main_clamp = Clamp::builder()
        .maximum_size(800)
        .css_classes(["content-clamp"])
        .tooltip_text("Centered content container")
        .build();
    main_clamp.update_property(&[Label("Centered content container")]);
    let main_box = Box::builder()
        .orientation(Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .css_classes(["content-box"])
        .tooltip_text("Page content")
        .build();
    main_box.update_property(&[Label("Page content")]);
    (main_clamp, main_box)
}

/// Wraps a clamp in a `Box(Vertical, 0)` and then in a `ScrolledWindow`.
///
/// This is the standard pattern for scrollable content pages using a clamp layout.
///
/// # Arguments
///
/// * `clamp` - The clamp widget to wrap
///
/// # Returns
///
/// A `ScrolledWindow` containing the clamp.
#[must_use]
pub fn wrap_clamp_in_scrolled(clamp: &Clamp) -> ScrolledWindow {
    let wrapper = Box::builder()
        .orientation(Vertical)
        .css_classes(["scrolled-wrapper"])
        .tooltip_text("Scrollable content wrapper")
        .build();
    wrapper.update_property(&[Label("Scrollable content wrapper")]);
    wrapper.append(clamp);
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(Automatic)
        .vscrollbar_policy(Automatic)
        .child(&wrapper)
        .css_classes(["frame", "content-scrolled-window"])
        .tooltip_text("Scrollable page")
        .build();
    scrolled.update_property(&[Label("Scrollable page")]);
    scrolled
}

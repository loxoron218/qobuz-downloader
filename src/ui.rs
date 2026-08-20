//! Shared UI scaffolding utilities.

use std::{
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{SystemTime, UNIX_EPOCH},
};

use libadwaita::{
    Clamp,
    gtk::{Box, Orientation::Vertical, PolicyType::Automatic, ScrolledWindow},
    prelude::{BoxExt, WidgetExt},
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
    let now = dur.as_secs() * 1000 + u64::from(dur.subsec_millis());
    if now - last_ts.load(Relaxed) < min_gap_ms {
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
pub fn build_content_clamp() -> (Clamp, Box) {
    let main_clamp = Clamp::builder().maximum_size(800).build();
    let main_box = Box::new(Vertical, 24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
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
pub fn wrap_clamp_in_scrolled(clamp: &Clamp) -> ScrolledWindow {
    let wrapper = Box::new(Vertical, 0);
    wrapper.append(clamp);
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .child(&wrapper)
        .build();
    scrolled.set_policy(Automatic, Automatic);
    scrolled
}

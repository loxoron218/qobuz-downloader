//! Shared UI scaffolding utilities.

use libadwaita::{
    Clamp,
    gtk::{Box, Orientation::Vertical, PolicyType::Automatic, ScrolledWindow},
    prelude::{BoxExt, WidgetExt},
};

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

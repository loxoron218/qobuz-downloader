//! Shared UI scaffolding utilities.

use libadwaita::{
    Clamp, HeaderBar, ToolbarView,
    gtk::{Box, Orientation::Vertical, PolicyType::Automatic, ScrolledWindow},
    prelude::{BoxExt, WidgetExt},
};

/// Creates a `ToolbarView` with a header bar and a vertically-oriented content box
/// with standard margins.
///
/// # Arguments
///
/// * `title_widget` - Widget to set as the header bar's title (e.g. `Label`, `Entry`)
///
/// # Returns
///
/// A tuple of `(ToolbarView, HeaderBar, Box)` where `Box` is the content container.
pub fn create_page_scaffold(title_widget: &impl WidgetExt) -> (ToolbarView, HeaderBar, Box) {
    let toolbar = ToolbarView::new();
    let header = HeaderBar::new();
    header.set_title_widget(Some(title_widget));
    toolbar.add_top_bar(&header);

    let content_box = Box::new(Vertical, 12);
    content_box.set_margin_start(12);
    content_box.set_margin_end(12);
    content_box.set_margin_top(6);
    content_box.set_margin_bottom(6);

    (toolbar, header, content_box)
}

/// Wraps the content box in a scrolled window and sets it as the toolbar's content.
pub fn wrap_content_in_scrolled(toolbar: &ToolbarView, content_box: &Box) -> ScrolledWindow {
    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(Automatic, Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(content_box));
    toolbar.set_content(Some(&scrolled));
    scrolled
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

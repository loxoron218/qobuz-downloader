//! Widget hierarchy helpers for download rows.
//!
//! Small accessors for navigating `ListItem` row containers.

use libadwaita::{
    gtk::{Box, Widget},
    prelude::{Cast, IsA, WidgetExt},
};

/// Returns the first child of a Box cast to T.
pub(super) fn first_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let Ok(w) = container.first_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the first child of any container cast to T.
pub(super) fn first_child_of<T: IsA<Widget>>(container: &impl IsA<Widget>) -> Option<T> {
    let Ok(w) = container.first_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the second child of a Box (`first_child` -> `next_sibling`) cast to T.
pub(super) fn second_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let w = container.first_child().and_then(|w| w.next_sibling())?;
    let Ok(w) = w.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the third child of a Box cast to T.
pub(super) fn third_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let w = container
        .first_child()
        .and_then(|w| w.next_sibling())
        .and_then(|w| w.next_sibling())?;
    let Ok(w) = w.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the last child of a Box cast to T.
pub(super) fn last_child_of_box<T: IsA<Widget>>(container: &Box) -> Option<T> {
    let Ok(w) = container.last_child()?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

/// Returns the nth child of a container cast to T.
pub(super) fn nth_child_of<T: IsA<Widget>>(container: &impl IsA<Widget>, n: usize) -> Option<T> {
    let mut child = container.first_child();
    for _ in 0..n {
        child = child.and_then(|w| w.next_sibling());
    }
    let Ok(w) = child?.downcast::<T>() else {
        return None;
    };
    Some(w)
}

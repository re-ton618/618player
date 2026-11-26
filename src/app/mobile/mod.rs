//! The phone's views: the same store and messages as the desktop's
//! (`super::view`), laid out for one screen and touch.
//!
//! For now the desktop's views, kept clear of the status and navigation
//! bars, stand in until the phone's own are drawn here: their own root,
//! pages and sizes (48–56 pt touch targets, 14–16 pt text), never flags in
//! the desktop's views.

use nsh_base::theme;
use nsh_core::prelude::*;

/// The screen's content.
pub(crate) fn root() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background)
        .p_safe_area()
        .child(super::view::root())
}

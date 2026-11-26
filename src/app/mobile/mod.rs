//! The phone's views: the same store and messages as the desktop's
//! (`super::view`), laid out for one screen and touch.
//!
//! The bars are the phone's own; the workspace and the playlist prompt are
//! the desktop's until theirs are drawn here: their own pages and sizes
//! (48–56 pt touch targets, 14–16 pt text), never flags in the desktop's
//! views.

mod playback;
mod top_bar;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::view::{playlist_prompt, workspace};

/// The top bar, border included.
const TOP_BAR_HEIGHT: f32 = 52.0;
/// A square icon cell of a bar: a touch target.
const BAR_CELL_SIZE: f32 = 52.0;
/// Icons in the bars.
const BAR_ICON_SIZE: f32 = 18.0;
/// The screen's padding, and the room between top-level regions.
const SCREEN_PADDING: f32 = 6.0;
const REGION_GAP: f32 = 6.0;

/// The screen's content, clear of the status and navigation bars: the top
/// bar, the workspace and the playback bar. No resize edges: the system
/// owns the window.
pub(crate) fn root() -> impl IntoElement {
    crate::ui::use_app_scrollbars();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .text_size(px(11.0))
        .p_safe_area()
        .child(
            view()
                .flex_1()
                .p(px(SCREEN_PADDING))
                .gap(px(REGION_GAP))
                .child(top_bar::top_bar())
                .child(workspace())
                .child(playback::playback_bar()),
        )
        .child(playlist_prompt())
}

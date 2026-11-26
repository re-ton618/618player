mod add_tab_menu;
mod library;
mod playback;
mod tab_strip;
mod top_bar;
mod volume_control;
mod window_frame;
mod workspace;

use nsh_base::theme;
use nsh_core::prelude::*;

pub(super) const TOP_BAR_HEIGHT: f32 = 56.0;
pub(super) const PLAYBACK_BAR_HEIGHT: f32 = 56.0;
const DESKTOP_PADDING: f32 = 12.0;
const SECTION_GAP: f32 = 12.0;

/// The main window's content: the top bar, the workspace and the playback
/// bar, over the edges that resize the frameless window.
pub(crate) fn root() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view()
                .flex_1()
                .p(px(DESKTOP_PADDING))
                .gap(px(SECTION_GAP))
                .child(top_bar::top_bar())
                .child(workspace::workspace())
                .child(playback::playback_bar()),
        )
        .child(window_frame::resize_edges())
}

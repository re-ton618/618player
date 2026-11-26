mod add_tab_menu;
mod library;
mod now_playing;
mod playback;
mod settings;
mod tab_strip;
mod top_bar;
mod volume_control;
mod window_frame;
mod workspace;

use nsh_base::theme;
use nsh_core::prelude::*;

/// The app bar (the window's title bar), border included.
pub(super) const TOP_BAR_HEIGHT: f32 = 40.0;
/// The playback bar: an app bar at the bottom, border included.
pub(super) const PLAYBACK_BAR_HEIGHT: f32 = 40.0;
/// An icon cell of an app bar.
pub(super) const BAR_CELL_WIDTH: f32 = 40.0;
/// Icons in bars and headers.
pub(super) const BAR_ICON_SIZE: f32 = 12.0;
/// The window's padding, and the room between top-level regions.
const WINDOW_PADDING: f32 = 8.0;
const REGION_GAP: f32 = 8.0;

/// The main window's content: the top bar, the workspace and the playback
/// bar, over the edges that resize the frameless window.
pub(crate) fn root() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .text_size(px(11.0))
        .child(
            view()
                .flex_1()
                .p(px(WINDOW_PADDING))
                .gap(px(REGION_GAP))
                .child(top_bar::top_bar())
                .child(workspace::workspace())
                .child(playback::playback_bar()),
        )
        .child(window_frame::resize_edges())
}

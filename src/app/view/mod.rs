mod library;
mod playback;
mod top_bar;
mod window_frame;

use iced::widget::{column, container};
use iced::{Element, Fill};

use super::{App, Message};
use crate::theme;

pub(super) const TOP_BAR_HEIGHT: f32 = 56.0;
pub(super) const PLAYBACK_BAR_HEIGHT: f32 = 56.0;
pub(super) const DESKTOP_PADDING: f32 = 12.0;
pub(super) const SECTION_GAP: f32 = 12.0;
pub(super) const INITIAL_LIBRARY_HEIGHT: f32 =
    640.0 - TOP_BAR_HEIGHT - PLAYBACK_BAR_HEIGHT - DESKTOP_PADDING * 2.0 - SECTION_GAP * 2.0;
pub(super) const ROW_HEIGHT: f32 = 32.0;
pub(super) const OVERSCAN_ROWS: usize = 5;

pub(super) fn library_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("library-scroll")
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let content = container(
        column![top_bar::view(app), library::view(app), playback::view()]
            .spacing(SECTION_GAP)
            .width(Fill)
            .height(Fill),
    )
    .padding(DESKTOP_PADDING)
    .width(Fill)
    .height(Fill)
    .style(theme::root_style)
    .into();

    if cfg!(target_os = "windows") {
        window_frame::view(content)
    } else {
        content
    }
}

use std::ops::Range;

use iced::widget::{Column, container, rule, scrollable, sensor, space, text};
use iced::{Element, Fill};

use super::{OVERSCAN_ROWS, ROW_HEIGHT};
use crate::app::{App, Message};
use crate::theme;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let track_count = app.visible_tracks.len();
    let visible = visible_range(track_count, app.scroll_offset, app.library_height);
    let mut rows = Column::new().width(Fill);

    if visible.start > 0 {
        rows = rows.push(space().height(visible.start as f32 * ROW_HEIGHT));
    }

    for &track_index in &app.visible_tracks[visible.clone()] {
        let path = &app.tracks[track_index];

        rows = rows
            .push(
                container(text(path.to_string_lossy()).size(15))
                    .width(Fill)
                    .height(ROW_HEIGHT - 1.0)
                    .padding([0, 16])
                    .center_y(Fill),
            )
            .push(rule::horizontal(1).style(theme::divider_style));
    }

    if visible.end < track_count {
        rows = rows.push(space().height((track_count - visible.end) as f32 * ROW_HEIGHT));
    }

    let library = scrollable(rows)
        .width(Fill)
        .height(Fill)
        .on_scroll(Message::Scrolled)
        .style(theme::scrollable_style);

    container(
        sensor(library)
            .on_show(Message::Resized)
            .on_resize(Message::Resized),
    )
    .width(Fill)
    .height(Fill)
    .style(theme::section_style)
    .into()
}

fn visible_range(track_count: usize, offset: f32, viewport_height: f32) -> Range<usize> {
    let first_visible = (offset / ROW_HEIGHT).floor() as usize;
    let start = first_visible.saturating_sub(OVERSCAN_ROWS).min(track_count);
    let visible_rows = (viewport_height / ROW_HEIGHT).ceil() as usize;
    let end = first_visible
        .saturating_add(visible_rows)
        .saturating_add(OVERSCAN_ROWS)
        .min(track_count);

    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_is_overscanned_and_bounded() {
        assert_eq!(visible_range(500, 320.0, 320.0), 5..25);
        assert_eq!(visible_range(8, 0.0, 640.0), 0..8);
        assert_eq!(visible_range(500, 15_900.0, 320.0), 491..500);
    }
}

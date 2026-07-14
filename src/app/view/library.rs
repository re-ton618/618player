use std::ops::Range;

use iced::widget::{Button, Column, button, container, row, rule, scrollable, sensor, space, text};
use iced::{Element, Fill, Length};

use super::{OVERSCAN_ROWS, ROW_HEIGHT, library_scroll_id};
use crate::app::{App, Message};
use crate::library::{SortColumn, SortDirection, Track};
use crate::theme;

const HEADER_HEIGHT: f32 = ROW_HEIGHT - 1.0;
const TITLE_PORTION: u16 = 5;
const ARTIST_PORTION: u16 = 3;
const ALBUM_PORTION: u16 = 4;
const YEAR_WIDTH: f32 = 58.0;
const TIME_WIDTH: f32 = 58.0;
const CELL_GAP: f32 = 12.0;
const HORIZONTAL_PADDING: f32 = 14.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let track_count = app.visible_tracks.len();
    let visible = visible_range(track_count, app.scroll_offset, app.library_height);
    let mut rows = Column::new().width(Fill);

    if visible.start > 0 {
        rows = rows.push(space().height(visible.start as f32 * ROW_HEIGHT));
    }

    for &track_index in &app.visible_tracks[visible.clone()] {
        let track = &app.tracks[track_index];
        rows = rows
            .push(track_row(track))
            .push(rule::horizontal(1).style(theme::divider_style));
    }

    if visible.end < track_count {
        rows = rows.push(space().height((track_count - visible.end) as f32 * ROW_HEIGHT));
    }

    let library = scrollable(rows)
        .id(library_scroll_id())
        .width(Fill)
        .height(Fill)
        .on_scroll(Message::Scrolled)
        .style(theme::scrollable_style);
    let content = Column::new()
        .push(rule::horizontal(1).style(theme::divider_style))
        .push(header(app))
        .push(rule::horizontal(1).style(theme::divider_style))
        .push(library)
        .width(Fill)
        .height(Fill);

    container(
        sensor(content)
            .on_show(Message::Resized)
            .on_resize(Message::Resized),
    )
    .width(Fill)
    .height(Fill)
    .style(theme::section_style)
    .into()
}

fn header(app: &App) -> Element<'_, Message> {
    container(
        row![
            header_button("TITLE", SortColumn::Title, TITLE_PORTION, app),
            header_button("ARTIST", SortColumn::Artist, ARTIST_PORTION, app),
            header_button("ALBUM", SortColumn::Album, ALBUM_PORTION, app),
            fixed_header_button("YEAR", SortColumn::Year, YEAR_WIDTH, app),
            fixed_header_button("TIME", SortColumn::Duration, TIME_WIDTH, app),
        ]
        .spacing(CELL_GAP)
        .height(Fill),
    )
    .padding([0.0, HORIZONTAL_PADDING])
    .width(Fill)
    .height(HEADER_HEIGHT)
    .into()
}

fn header_button<'a>(
    label: &'a str,
    column: SortColumn,
    portion: u16,
    app: &App,
) -> Button<'a, Message> {
    button(
        container(
            text(sort_label(label, column, app))
                .size(12)
                .font(theme::STRONG_FONT),
        )
        .width(Fill)
        .height(Fill)
        .center_y(Fill),
    )
    .width(Length::FillPortion(portion))
    .height(Fill)
    .padding(0)
    .style(theme::header_button_style)
    .on_press(Message::SortChanged(column))
}

fn fixed_header_button<'a>(
    label: &'a str,
    column: SortColumn,
    width: f32,
    app: &App,
) -> Button<'a, Message> {
    button(
        container(
            text(sort_label(label, column, app))
                .size(12)
                .font(theme::STRONG_FONT),
        )
        .width(Fill)
        .height(Fill)
        .center_y(Fill)
        .align_right(Fill),
    )
    .width(width)
    .height(Fill)
    .padding(0)
    .style(theme::header_button_style)
    .on_press(Message::SortChanged(column))
}

fn sort_label(label: &str, column: SortColumn, app: &App) -> String {
    if app.sort_column != column {
        return label.into();
    }
    let marker = match app.sort_direction {
        SortDirection::Ascending => " ^",
        SortDirection::Descending => " v",
    };
    format!("{label}{marker}")
}

fn track_row(track: &Track) -> Element<'_, Message> {
    let year = track.year.map_or_else(String::new, |year| year.to_string());
    let row = row![
        text_cell(&track.title, Length::FillPortion(TITLE_PORTION), false),
        text_cell(
            track.artist.as_deref().unwrap_or_default(),
            Length::FillPortion(ARTIST_PORTION),
            true
        ),
        text_cell(
            track.album.as_deref().unwrap_or_default(),
            Length::FillPortion(ALBUM_PORTION),
            true
        ),
        text_cell(year, Length::Fixed(YEAR_WIDTH), true).align_right(Fill),
        text_cell(track.formatted_duration(), Length::Fixed(TIME_WIDTH), true).align_right(Fill),
    ]
    .spacing(CELL_GAP)
    .height(Fill);

    button(row)
        .width(Fill)
        .height(ROW_HEIGHT - 1.0)
        .padding([0.0, HORIZONTAL_PADDING])
        .style(theme::track_row_style)
        .on_press(Message::TrackPressed(track.id))
        .into()
}

fn text_cell<'a>(
    value: impl text::IntoFragment<'a>,
    width: Length,
    muted: bool,
) -> iced::widget::Container<'a, Message> {
    let cell = container(text(value).size(14).wrapping(text::Wrapping::None))
        .width(width)
        .height(Fill)
        .center_y(Fill)
        .clip(true);

    if muted {
        cell.style(theme::muted_text_style)
    } else {
        cell
    }
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

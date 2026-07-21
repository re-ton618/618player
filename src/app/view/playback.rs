use iced::widget::{button, container, image, row, rule, space, text};
use iced::{Center, ContentFit, Element, Fill, Theme};

use super::PLAYBACK_BAR_HEIGHT;
use super::volume_control;
use crate::app::{App, Message};
use crate::theme;

const SQUARE_SIZE: f32 = PLAYBACK_BAR_HEIGHT - 2.0 * theme::CHROME_BORDER_WIDTH;
const TRANSPORT_BUTTON_WIDTH: f32 = SQUARE_SIZE;
const CONTROL_REGION_WIDTH: f32 = TRANSPORT_BUTTON_WIDTH * 3.0 + 3.0 + 53.0;
const SIDE_REGION_WIDTH: f32 = CONTROL_REGION_WIDTH + SQUARE_SIZE + 1.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let controls = row![
        transport_button("|<", theme::transport_button_style),
        rule::vertical(1).style(theme::border_style),
        transport_button(">", theme::play_button_style),
        rule::vertical(1).style(theme::border_style),
        transport_button(">|", theme::transport_button_style),
        rule::vertical(1).style(theme::border_style),
    ]
    .height(Fill);

    let left = row![
        artwork_cell(app),
        rule::vertical(1).style(theme::border_style),
        controls,
        space().width(Fill),
    ]
    .width(SIDE_REGION_WIDTH)
    .height(Fill)
    .align_y(Center);

    let progress = container(
        row![
            text("0:00").size(10).font(theme::STRONG_FONT),
            container(space())
                .width(Fill)
                .height(3)
                .style(theme::progress_track_style),
            text("--:--").size(10).font(theme::STRONG_FONT),
        ]
        .spacing(12)
        .align_y(Center),
    )
    .width(Fill)
    .height(Fill)
    .padding([0, 18])
    .center_y(Fill)
    .style(theme::muted_text_style);

    let right = row![
        space().width(Fill),
        rule::vertical(1).style(theme::border_style),
        volume_control::view(app.playback.volume()),
    ]
    .width(SIDE_REGION_WIDTH)
    .height(Fill)
    .align_y(Center);

    container(
        row![
            left,
            rule::vertical(1).style(theme::border_style),
            progress,
            rule::vertical(1).style(theme::border_style),
            right,
        ]
        .width(Fill)
        .height(Fill)
        .align_y(Center),
    )
    .width(Fill)
    .height(PLAYBACK_BAR_HEIGHT)
    .padding(theme::CHROME_BORDER_WIDTH)
    .style(theme::top_bar_style)
    .into()
}

fn artwork_cell(app: &App) -> Element<'_, Message> {
    let content: Element<'_, Message> = app
        .current_track()
        .and_then(|_| app.playback.artwork_handle())
        .map_or_else(
            || space().into(),
            |handle| {
                image(handle.clone())
                    .width(SQUARE_SIZE)
                    .height(SQUARE_SIZE)
                    .content_fit(ContentFit::Cover)
                    .into()
            },
        );

    container(content)
        .width(SQUARE_SIZE)
        .height(Fill)
        .style(theme::artwork_placeholder_style)
        .into()
}

fn transport_button<'a>(
    label: &'a str,
    style: fn(&Theme, button::Status) -> button::Style,
) -> iced::widget::Button<'a, Message> {
    button(
        container(
            text(label)
                .size(12)
                .line_height(1.0)
                .font(theme::ICON_FONT)
                .align_x(Center)
                .align_y(Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill),
    )
    .width(TRANSPORT_BUTTON_WIDTH)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(Message::PlaybackControlPressed)
}

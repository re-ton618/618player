use iced::widget::{button, container, image, row, rule, space, text};
use iced::{Center, ContentFit, Element, Fill, Theme};

use super::PLAYBACK_BAR_HEIGHT;
use crate::app::{App, Message};
use crate::theme;

const CONTROL_REGION_WIDTH: f32 = 188.0;
const ARTWORK_IMAGE_SIZE: f32 = PLAYBACK_BAR_HEIGHT - 2.0 * theme::ARTWORK_BORDER_WIDTH;
const SIDE_REGION_WIDTH: f32 = CONTROL_REGION_WIDTH + PLAYBACK_BAR_HEIGHT;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let controls = row![
        transport_button("|<", theme::transport_button_style),
        rule::vertical(1).style(theme::divider_style),
        transport_button(">", theme::play_button_style),
        rule::vertical(1).style(theme::divider_style),
        transport_button(">|", theme::transport_button_style),
        rule::vertical(1).style(theme::divider_style),
    ]
    .height(Fill);

    let left = row![artwork_cell(app), controls, space().width(Fill)]
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

    let volume_track = row![
        container(space())
            .width(46)
            .height(3)
            .style(theme::progress_fill_style),
        container(space())
            .width(Fill)
            .height(3)
            .style(theme::progress_track_style),
    ]
    .width(72)
    .align_y(Center);

    let volume = container(
        row![text("VOL").size(10).font(theme::STRONG_FONT), volume_track]
            .spacing(12)
            .align_y(Center),
    )
    .width(SIDE_REGION_WIDTH)
    .height(Fill)
    .padding([0, 16])
    .center_y(Fill)
    .align_x(iced::alignment::Horizontal::Right)
    .style(theme::muted_text_style);

    container(
        row![
            left,
            rule::vertical(1).style(theme::divider_style),
            progress,
            rule::vertical(1).style(theme::divider_style),
            volume,
        ]
        .width(Fill)
        .height(Fill)
        .align_y(Center),
    )
    .width(Fill)
    .height(PLAYBACK_BAR_HEIGHT)
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
                    .width(ARTWORK_IMAGE_SIZE)
                    .height(ARTWORK_IMAGE_SIZE)
                    .content_fit(ContentFit::Cover)
                    .into()
            },
        );

    container(content)
        .width(PLAYBACK_BAR_HEIGHT)
        .height(PLAYBACK_BAR_HEIGHT)
        .padding(theme::ARTWORK_BORDER_WIDTH)
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
    .width(44)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(Message::PlaybackControlPressed)
}

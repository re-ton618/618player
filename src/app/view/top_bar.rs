use iced::widget::{button, container, mouse_area, row, rule, space, text, text_input};
use iced::{Center, Element, Fill, Length, Theme};

use super::TOP_BAR_HEIGHT;
use crate::app::{App, Message};
use crate::theme;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let leading_space = space().width(Length::FillPortion(1)).height(Fill);

    let search = text_input("Search tracks, artists, albums", &app.search_query)
        .on_input(Message::SearchChanged)
        .width(Fill)
        .size(14)
        .padding([9, 0])
        .style(theme::search_style);

    let search_region = container(search)
        .width(300)
        .height(Fill)
        .padding([0, 16])
        .center_y(Fill);

    let window_controls = row![
        window_button("-", Message::WindowMinimized, theme::window_button_style),
        rule::vertical(1).style(theme::divider_style),
        window_button("[]", Message::WindowMaximized, theme::window_button_style),
        rule::vertical(1).style(theme::divider_style),
        window_button("X", Message::WindowClosed, theme::close_button_style),
    ]
    .height(Fill);

    let actions = row![space().width(Fill), window_controls,]
        .width(Length::FillPortion(1))
        .height(Fill)
        .align_y(Center);

    let bar = container(
        row![
            leading_space,
            rule::vertical(1).style(theme::divider_style),
            search_region,
            rule::vertical(1).style(theme::divider_style),
            actions,
        ]
        .width(Fill)
        .height(Fill)
        .align_y(Center),
    )
    .width(Fill)
    .height(TOP_BAR_HEIGHT)
    .style(theme::top_bar_style);

    mouse_area(bar).on_press(Message::WindowDragged).into()
}

fn window_button<'a>(
    label: &'a str,
    message: Message,
    style: fn(&Theme, button::Status) -> button::Style,
) -> iced::widget::Button<'a, Message> {
    button(
        container(text(label).size(12).font(theme::ICON_FONT))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
    )
    .width(35)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(message)
}

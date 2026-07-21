use iced::widget::{button, container, mouse_area, row, rule, space, stack, text, text_input};
use iced::{Center, Element, Fill, Theme};

use super::TOP_BAR_HEIGHT;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::app::{App, Message};
use crate::theme;

const WINDOW_BUTTON_SIZE: f32 = TOP_BAR_HEIGHT - 2.0 * theme::CHROME_BORDER_WIDTH;
const FILE_BUTTON_WIDTH: f32 = 56.0;
const CHROME_ICON_SIZE: f32 = 12.0;
const SETTINGS_ICON_SIZE: f32 = CHROME_ICON_SIZE * 2.0;
const SEARCH_WIDTH: f32 = 300.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let quick_access = row![
        chrome_button(
            "File",
            Message::FileMenuPressed,
            theme::window_button_style,
            CHROME_ICON_SIZE,
        )
        .width(FILE_BUTTON_WIDTH),
        rule::vertical(1).style(theme::border_style),
        chrome_button(
            "⚙",
            Message::Tabs(TabMessage::Open(Kind::Settings)),
            theme::window_button_style,
            SETTINGS_ICON_SIZE,
        ),
        rule::vertical(1).style(theme::border_style),
    ]
    .height(Fill);

    let search = text_input("Search tracks, artists, albums", &app.search_query)
        .on_input(Message::SearchChanged)
        .width(Fill)
        .size(14)
        .padding([9, 0])
        .style(theme::search_style);

    let search_region = container(search)
        .width(SEARCH_WIDTH)
        .height(Fill)
        .padding([0, 16])
        .center_y(Fill);

    let search_cluster = row![
        rule::vertical(1).style(theme::border_style),
        search_region,
        rule::vertical(1).style(theme::border_style),
    ]
    .height(Fill);

    let window_controls = row![
        rule::vertical(1).style(theme::border_style),
        chrome_button(
            "-",
            Message::WindowMinimized,
            theme::window_button_style,
            CHROME_ICON_SIZE,
        ),
        rule::vertical(1).style(theme::border_style),
        chrome_button(
            "[]",
            Message::WindowMaximized,
            theme::window_button_style,
            CHROME_ICON_SIZE,
        ),
        rule::vertical(1).style(theme::border_style),
        chrome_button(
            "×",
            Message::WindowClosed,
            theme::close_button_style,
            CHROME_ICON_SIZE,
        ),
    ]
    .height(Fill);

    let chrome = row![quick_access, space().width(Fill), window_controls,]
        .width(Fill)
        .height(Fill)
        .align_y(Center);

    let centered_search = container(search_cluster)
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill);

    let bar = container(
        stack![chrome, centered_search]
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(TOP_BAR_HEIGHT)
    .padding(theme::CHROME_BORDER_WIDTH)
    .style(theme::top_bar_style);

    mouse_area(bar).on_press(Message::WindowDragged).into()
}

fn chrome_button<'a>(
    label: &'a str,
    message: Message,
    style: fn(&Theme, button::Status) -> button::Style,
    icon_size: f32,
) -> iced::widget::Button<'a, Message> {
    button(
        container(
            text(label)
                .size(icon_size)
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
    .width(WINDOW_BUTTON_SIZE)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(message)
}

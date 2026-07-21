use iced::widget::{Row, button, container, mouse_area, row, rule, space, text};
use iced::{Element, Fill, Length};

use super::{add_tab_menu, workspace::TAB_STRIP_HEIGHT};
use crate::app::tabs::{Kind, Message as TabMessage, Side};
use crate::app::{App, Message};
use crate::theme;

const TAB_WIDTH: f32 = 160.0;
const LABEL_WIDTH: f32 = 127.0;
const CLOSE_WIDTH: f32 = 32.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let mut open_tabs = Row::new().width(Length::Shrink).height(Fill);

    for &kind in app.tabs.order() {
        open_tabs = open_tabs
            .push(tab(kind, app.tabs.active() == Some(kind)))
            .push(rule::vertical(1).style(theme::divider_style));
    }

    let targets = mouse_area(open_tabs).on_exit(Message::Tabs(TabMessage::DragLeftStrip));

    container(
        row![targets, add_tab_menu::view(), space().width(Fill)]
            .spacing(0)
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(TAB_STRIP_HEIGHT)
    .style(theme::tab_strip_style)
    .into()
}

fn tab(kind: Kind, active: bool) -> Element<'static, Message> {
    let label = container(
        text(kind.label())
            .size(12)
            .line_height(1.0)
            .font(theme::STRONG_FONT)
            .wrapping(text::Wrapping::None),
    )
    .padding([0, 12])
    .width(LABEL_WIDTH)
    .height(Fill)
    .center_y(Fill)
    .clip(true);

    let close = button(icon_glyph("×"))
        .width(CLOSE_WIDTH)
        .height(Fill)
        .padding(0)
        .style(theme::close_button_style)
        .on_press(Message::Tabs(TabMessage::Close(kind)));

    let content = container(
        row![label, rule::vertical(1).style(theme::divider_style), close]
            .width(Fill)
            .height(Fill),
    )
    .width(TAB_WIDTH)
    .height(Fill)
    .style(move |theme| theme::tab_style(theme, active));

    mouse_area(content)
        .on_press(Message::Tabs(TabMessage::Pressed(kind)))
        .on_move(move |point| {
            Message::Tabs(TabMessage::DragOver {
                target: kind,
                side: if point.x < TAB_WIDTH / 2.0 {
                    Side::Before
                } else {
                    Side::After
                },
            })
        })
        .into()
}

fn icon_glyph(label: &'static str) -> Element<'static, Message> {
    container(
        text(label)
            .size(14)
            .line_height(1.0)
            .font(theme::ICON_FONT)
            .align_x(iced::Center)
            .align_y(iced::Center),
    )
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .into()
}

use iced::widget::{column, container, row, rule, space, text};
use iced::{Center, Element, Fill};

use super::{library, tab_strip};
use crate::app::tabs::Kind;
use crate::app::{App, Message};
use crate::theme;

pub(super) const TAB_STRIP_HEIGHT: f32 = 32.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let body = match app.tabs.active() {
        Some(Kind::Library) => library::view(app),
        Some(Kind::NowPlaying) => placeholder("NOW PLAYING", "Playback details will appear here."),
        Some(Kind::SongInformation) => {
            placeholder("SONG INFORMATION", "Song details will appear here.")
        }
        Some(Kind::Settings) => container(space())
            .width(Fill)
            .height(Fill)
            .into(),
        None => placeholder("NO TABS OPEN", "Use + to open a tab."),
    };

    let interior = column![
        tab_strip::view(app),
        rule::horizontal(1).style(theme::border_style),
        body
    ]
    .spacing(0)
    .width(Fill)
    .height(Fill);

    container(
        column![
            rule::horizontal(1).style(theme::border_style),
            row![
                rule::vertical(1).style(theme::border_style),
                interior,
                rule::vertical(1).style(theme::border_style),
            ]
            .spacing(0)
            .width(Fill)
            .height(Fill),
            rule::horizontal(1).style(theme::border_style),
        ]
        .spacing(0)
        .width(Fill)
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(theme::section_fill_style)
    .into()
}

fn placeholder(title: &'static str, detail: &'static str) -> Element<'static, Message> {
    container(
        column![
            text(title).size(12).font(theme::STRONG_FONT),
            text(detail).size(14),
        ]
        .spacing(8)
        .align_x(Center),
    )
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .style(theme::muted_text_style)
    .into()
}

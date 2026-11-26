use nsh_base::{menu, menu_item, theme};
use nsh_core::prelude::*;

use super::{BAR_ICON_SIZE, workspace::TAB_STRIP_HEIGHT};
use crate::app::Message;
use crate::app::store::store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::ui::{Icon, Tone, chrome_glyph};

/// A menu row, and the menu's inset above and below its rows.
const ROW_HEIGHT: f32 = 30.0;
const ROW_PADDING: f32 = 10.0;
const MENU_PADDING: f32 = 4.0;
const MENU_WIDTH: f32 = 180.0;
/// Clears the header's line under the trigger.
const MENU_OFFSET: f32 = 1.0;

/// The + cell, square with its own right edge, and its menu of the tabs to
/// open, hanging from it.
pub(super) fn add_tab_menu() -> impl IntoElement {
    let store = store();
    let item = move |kind: Kind| {
        menu_item()
            .on_select(move || store.dispatch(Message::Tabs(TabMessage::Open(kind))))
            .h(px(ROW_HEIGHT))
            .flex_row()
            .items_center()
            .px(px(ROW_PADDING))
            .hover(|s| s.bg(move || theme().card))
            .focus_visible(|s| s.bg(move || theme().card))
            .active(|s| s.bg(move || theme().border))
            .child(
                text(kind.label())
                    .text_size(px(11.0))
                    .number_of_lines(1)
                    .text_color(move || theme().popover_foreground),
            )
    };

    menu()
        .offset(px(MENU_OFFSET))
        .popup_style(|s| {
            s.min_w(px(MENU_WIDTH))
                .py(px(MENU_PADDING))
                .bg(move || theme().popover)
                .border_1()
                .border_color(move || theme().border)
        })
        .trigger(
            chrome_glyph(Icon::Add, BAR_ICON_SIZE, "Open tab", Tone::Quiet)
                .w(px(TAB_STRIP_HEIGHT + 1.0))
                .border_r_1()
                .border_color(move || theme().border),
        )
        .children(Kind::ALL.map(item))
}

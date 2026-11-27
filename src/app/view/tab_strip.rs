use nsh_base::theme;
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use super::workspace::TAB_STRIP_HEIGHT;
use crate::app::store::Store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::app::Message;
use crate::ui::{chrome_button, Icon, Tone};

/// A tab, its right edge included.
const TAB_WIDTH: f32 = 160.0;
const TAB_PADDING: f32 = 12.0;
const TAB_TEXT_SIZE: f32 = 11.0;
/// The tab's inline close cell, and the room before it.
const CLOSE_WIDTH: f32 = 24.0;
const CLOSE_GAP: f32 = 6.0;
const CLOSE_ICON_SIZE: f32 = 10.0;
/// How far from the pointer a dragged tab's name follows it.
const PREVIEW_OFFSET: f32 = 14.0;

/// A tab group's header: its tabs in their order, then the menu that opens
/// more, on `card` with its bottom line. The dock area makes the tabs of
/// [`tab`]s: a press on one shows it, dragging it moves it to another
/// place in a header, into another group or to a side of one. Tabs that do
/// not fit scroll sideways, the menu staying at the end.
pub(super) fn tab_bar(style: StyleRefinement) -> StyleRefinement {
    style
        .h(px(TAB_STRIP_HEIGHT + 1.0))
        .flex_shrink_0()
        .flex_row()
        .min_w(px(0.0))
        .overflow_hidden()
        .bg(move || theme().card)
        .border_b_1()
        .border_color(move || theme().border)
}

/// What a tab shows; it owns its right edge. The one its group shows
/// takes the body's `background`, the rest stay `card`; the focused one,
/// which the keyboard acts on, is in the text color, the rest muted.
pub(super) fn tab(store: Store, kind: Kind, shown: bool) -> impl IntoElement {
    let focused = store.select(move |model| model.tabs.focus() == Some(kind));
    let tabs = move |message| store.dispatch(Message::Tabs(message));

    let label = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .pl(px(TAB_PADDING))
        .justify_center()
        .overflow_hidden()
        .child(
            text(kind.label())
                .text_size(px(TAB_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || {
                    if focused.get() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        );

    view()
        .w(px(TAB_WIDTH))
        .h(px(TAB_STRIP_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .gap(px(CLOSE_GAP))
        .bg(move || {
            if shown {
                theme().background
            } else {
                theme().card
            }
        })
        .border_r_1()
        .border_color(move || theme().border)
        .on_mouse_up(move |_: &MouseEvent| tabs(TabMessage::Focus(kind)))
        .child(label)
        .child(
            chrome_button(
                Icon::Close,
                CLOSE_ICON_SIZE,
                "Close tab",
                Tone::Danger,
                move || tabs(TabMessage::Close(kind)),
            )
            .w(px(CLOSE_WIDTH)),
        )
}

/// A dragged tab's name, following the pointer.
pub(super) fn drag_preview(kind: Kind) -> impl IntoElement {
    view()
        .ml(px(PREVIEW_OFFSET))
        .mt(px(PREVIEW_OFFSET))
        .h(px(TAB_STRIP_HEIGHT))
        .px(px(TAB_PADDING))
        .justify_center()
        .bg(move || theme().popover)
        .border_1()
        .border_color(move || theme().primary)
        .child(
            text(kind.label())
                .text_size(px(TAB_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || theme().popover_foreground),
        )
}

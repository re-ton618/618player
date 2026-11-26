use nsh_base::theme;
use nsh_core::prelude::*;

use super::{add_tab_menu, workspace::TAB_STRIP_HEIGHT};
use crate::app::store::{store, Store};
use crate::app::tabs::{Kind, Message as TabMessage, Side};
use crate::app::Message;
use crate::ui::{chrome_button, Icon, Tone};

/// A tab, its right edge included: tabs are a fixed width so a drag can
/// find the tab under the pointer.
const TAB_WIDTH: f32 = 160.0;
const TAB_PADDING: f32 = 12.0;
const TAB_TEXT_SIZE: f32 = 11.0;
/// The tab's inline close cell, and the room before it.
const CLOSE_WIDTH: f32 = 24.0;
const CLOSE_GAP: f32 = 6.0;
const CLOSE_ICON_SIZE: f32 = 10.0;

/// The region header: the open tabs in their order, then the menu that
/// opens more, on `card` with its bottom line. A press on a tab activates
/// it; dragging it drops it before or after another.
pub(super) fn tab_strip() -> impl IntoElement {
    let store = store();
    let order = store.select(|model| model.tabs.order().to_vec());
    let tabs = move |message| store.dispatch(Message::Tabs(message));

    let open_tabs = view()
        .flex_row()
        .h_full()
        .on_mouse_move(move |event: &MouseEvent| {
            if event.pressed_button.is_none() {
                return;
            }
            let x = event.position.x.0;
            let index = (x / TAB_WIDTH).floor();
            let target = (index >= 0.0)
                .then(|| order.with_untracked(|order| order.get(index as usize).copied()))
                .flatten();
            match target {
                Some(target) => tabs(TabMessage::DragOver {
                    target,
                    side: if x - index * TAB_WIDTH < TAB_WIDTH / 2.0 {
                        Side::Before
                    } else {
                        Side::After
                    },
                }),
                None => tabs(TabMessage::DragLeftStrip),
            }
        })
        .on_mouse_leave(move |_: &MouseEvent| tabs(TabMessage::DragLeftStrip))
        .child(each(
            order,
            |kind: &Kind| *kind,
            move |kind, _| tab(store, kind.get_untracked()),
        ));

    view()
        .h(px(TAB_STRIP_HEIGHT + 1.0))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_b_1()
        .border_color(move || theme().border)
        .child(open_tabs)
        .child(add_tab_menu::add_tab_menu())
}

/// A tab owns its right edge. The active one takes the body's
/// `background` and the text color; the rest stay `card` and muted.
fn tab(store: Store, kind: Kind) -> impl IntoElement {
    let active = store.select(move |model| model.tabs.active() == Some(kind));
    let tabs = move |message| store.dispatch(Message::Tabs(message));

    let label = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .pl(px(TAB_PADDING))
        .justify_center()
        .overflow_hidden()
        .role(AccessibilityRole::Tab)
        .label(kind.label())
        .on_press_down(move || tabs(TabMessage::Pressed(kind)))
        .on_press_up(move || tabs(TabMessage::DragFinished))
        .child(
            text(kind.label())
                .text_size(px(TAB_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || {
                    if active.get() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        );

    view()
        .w(px(TAB_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex_row()
        .gap(px(CLOSE_GAP))
        .bg(move || theme().card)
        .style_when(active, |s| s.bg(move || theme().background))
        .border_r_1()
        .border_color(move || theme().border)
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

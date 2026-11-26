use nsh_base::theme;
use nsh_core::prelude::*;

use super::{add_tab_menu, workspace::TAB_STRIP_HEIGHT};
use crate::app::Message;
use crate::app::store::{Store, store};
use crate::app::tabs::{Kind, Message as TabMessage, Side};
use crate::ui::{Icon, Rule, Tone, caption, chrome_button, vertical_rule};

const TAB_WIDTH: f32 = 160.0;
const CLOSE_WIDTH: f32 = 32.0;
/// A tab and the divider after it.
const TAB_STRIDE: f32 = TAB_WIDTH + 1.0;

/// The open tabs in their order, then the menu that opens more. A press on
/// a tab activates it; dragging it drops it before or after another.
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
            let index = (x / TAB_STRIDE).floor();
            let target = (index >= 0.0)
                .then(|| order.with_untracked(|order| order.get(index as usize).copied()))
                .flatten();
            match target {
                Some(target) => tabs(TabMessage::DragOver {
                    target,
                    side: if x - index * TAB_STRIDE < TAB_WIDTH / 2.0 {
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
            move |kind, _| {
                let kind = kind.get_untracked();
                view()
                    .flex_row()
                    .h_full()
                    .child(tab(store, kind))
                    .child(vertical_rule(Rule::Divider))
            },
        ));

    view()
        .h(px(TAB_STRIP_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .child(open_tabs)
        .child(add_tab_menu::add_tab_menu())
        .child(vertical_rule(Rule::Divider))
}

fn tab(store: Store, kind: Kind) -> impl IntoElement {
    let active = store.select(move |model| model.tabs.active() == Some(kind));
    let tabs = move |message| store.dispatch(Message::Tabs(message));

    let label = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .px(px(12.0))
        .justify_center()
        .overflow_hidden()
        .role(AccessibilityRole::Tab)
        .label(kind.label())
        .on_press_down(move || tabs(TabMessage::Pressed(kind)))
        .on_press_up(move || tabs(TabMessage::DragFinished))
        .child(caption(kind.label(), 12.0).text_color(move || {
            if active.get() {
                theme().foreground
            } else {
                theme().muted_foreground
            }
        }));

    view()
        .w(px(TAB_WIDTH))
        .h_full()
        .flex_row()
        .bg(move || theme().card)
        .style_when(active, |s| s.bg(move || theme().secondary))
        .child(label)
        .child(vertical_rule(Rule::Divider))
        .child(
            chrome_button(Icon::Close, 10.0, "Close tab", Tone::Danger, move || {
                tabs(TabMessage::Close(kind))
            })
            .w(px(CLOSE_WIDTH)),
        )
}

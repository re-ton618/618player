use nsh_base::{input, theme};
use nsh_core::desktop::WindowControl;
use nsh_core::prelude::*;

use super::TOP_BAR_HEIGHT;
use crate::app::Message;
use crate::app::store::store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::theme::CHROME_BORDER_WIDTH;
use crate::ui::{Icon, Rule, Tone, chrome_button, chrome_glyph, chrome_text_button, vertical_rule};

const WINDOW_BUTTON_SIZE: f32 = TOP_BAR_HEIGHT - 2.0 * CHROME_BORDER_WIDTH;
const FILE_BUTTON_WIDTH: f32 = 56.0;
const CHROME_ICON_SIZE: f32 = 12.0;
const SETTINGS_ICON_SIZE: f32 = 18.0;
const SEARCH_WIDTH: f32 = 300.0;

/// The window's title bar: quick access on the left, the search centered,
/// the window buttons on the right. Its background drags the window.
pub(super) fn top_bar() -> impl IntoElement {
    let store = store();
    let window_button = |icon, label, tone, control| {
        chrome_glyph(icon, CHROME_ICON_SIZE, label, tone)
            .w(px(WINDOW_BUTTON_SIZE))
            .window_control(control)
    };

    let quick_access = view()
        .flex_row()
        .h_full()
        .child(chrome_text_button("File", FILE_BUTTON_WIDTH, move || {
            store.dispatch(Message::FileMenuPressed)
        }))
        .child(vertical_rule(Rule::Border))
        .child(
            chrome_button(
                Icon::Settings,
                SETTINGS_ICON_SIZE,
                "Settings",
                Tone::Quiet,
                move || store.dispatch(Message::Tabs(TabMessage::Open(Kind::Settings))),
            )
            .w(px(WINDOW_BUTTON_SIZE)),
        )
        .child(vertical_rule(Rule::Border));

    let window_controls = view()
        .flex_row()
        .h_full()
        .child(vertical_rule(Rule::Border))
        .child(window_button(
            Icon::Minimize,
            "Minimize",
            Tone::Quiet,
            WindowControl::Minimize,
        ))
        .child(vertical_rule(Rule::Border))
        .child(window_button(
            Icon::Maximize,
            "Maximize",
            Tone::Quiet,
            WindowControl::Maximize,
        ))
        .child(vertical_rule(Rule::Border))
        .child(window_button(
            Icon::Close,
            "Close",
            Tone::Danger,
            WindowControl::Close,
        ));

    let query = store.select(|model| model.search_query.clone());
    let search = input()
        .value(query)
        .on_change(move |text: &str| store.dispatch(Message::SearchChanged(text.to_owned())))
        .placeholder("Search tracks, artists, albums")
        .placeholder_color(|| color(theme().muted_foreground))
        .caret_color(|| color(theme().primary))
        .selection_color(|| color(theme().primary.opacity(0.35)))
        .flex_1()
        .text_size(px(14.0))
        .text_color(move || theme().foreground);

    let centered_search = view()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .flex_row()
        .justify_center()
        .pointer_events(PointerEvents::BoxNone)
        .child(vertical_rule(Rule::Border))
        .child(
            view()
                .w(px(SEARCH_WIDTH))
                .px(px(16.0))
                .flex_row()
                .items_center()
                .window_drag(false)
                .child(search),
        )
        .child(vertical_rule(Rule::Border));

    view()
        .h(px(TOP_BAR_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .p(px(CHROME_BORDER_WIDTH))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || theme().border)
        .shadow_lg()
        .window_drag(true)
        .child(quick_access)
        .child(view().flex_1())
        .child(window_controls)
        .child(centered_search)
}

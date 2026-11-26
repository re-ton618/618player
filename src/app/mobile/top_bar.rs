use nsh_base::{input, theme};
use nsh_core::prelude::*;

use super::{BAR_CELL_SIZE, BAR_ICON_SIZE, TOP_BAR_HEIGHT};
use crate::app::store::store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::app::Message;
use crate::theme as tokens;
use crate::ui::{chrome_button, Icon, Tone};

/// The search field's inset, and the room between its icon and its text.
const SEARCH_PADDING: f32 = 14.0;
const SEARCH_ICON_GAP: f32 = 10.0;
const SEARCH_TEXT_SIZE: f32 = 15.0;

/// The screen's top bar: settings on the left, the search across the
/// rest. No window buttons: the system owns the window.
pub(super) fn top_bar() -> impl IntoElement {
    let store = store();
    let border = move || theme().border;

    let settings = chrome_button(
        Icon::Settings,
        BAR_ICON_SIZE,
        "Settings",
        Tone::Quiet,
        move || store.dispatch(Message::Tabs(TabMessage::Open(Kind::Settings))),
    )
    .w(px(BAR_CELL_SIZE))
    .border_r_1()
    .border_color(border);

    let query = store.select(|model| model.search_query.clone());
    let field = input()
        .value(query)
        .on_change(move |text: &str| store.dispatch(Message::SearchChanged(text.to_owned())))
        .placeholder("Search tracks, artists, albums")
        .placeholder_color(|| color(tokens::text_faint()))
        .caret_color(|| color(theme().primary))
        .selection_color(|| color(theme().popover).blend(color(theme().primary.opacity(0.35))))
        .flex_1()
        .min_w(px(0.0))
        .text_size(px(SEARCH_TEXT_SIZE))
        .text_color(move || theme().foreground);

    let search = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .items_center()
        .gap(px(SEARCH_ICON_GAP))
        .px(px(SEARCH_PADDING))
        .focus_within(|s| s.bg(move || theme().popover))
        .child(
            Icon::Search
                .view(BAR_ICON_SIZE)
                .text_color(tokens::text_faint),
        )
        .child(field);

    view()
        .h(px(TOP_BAR_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_1()
        .border_color(border)
        .child(settings)
        .child(search)
}

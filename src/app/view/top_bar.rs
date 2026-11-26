use nsh_base::{input, theme};
use nsh_core::desktop::WindowControl;
use nsh_core::prelude::*;

use super::{BAR_CELL_WIDTH, BAR_ICON_SIZE, TOP_BAR_HEIGHT};
use crate::app::Message;
use crate::app::store::store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::theme as tokens;
use crate::ui::{Icon, Tone, chrome_button, chrome_glyph, chrome_text_button};

/// The search cell, its edges included.
const SEARCH_WIDTH: f32 = 300.0;
/// The search cell's inset, and the room between its icon and its field.
const SEARCH_PADDING: f32 = 10.0;
const SEARCH_ICON_GAP: f32 = 6.0;
const SEARCH_TEXT_SIZE: f32 = 11.0;

/// The window's title bar: quick access on the left, the search centered,
/// the window buttons on the right. Its background drags the window.
/// Leading cells own their right edge, the search both, and trailing cells
/// their left, so every seam is one line.
pub(super) fn top_bar() -> impl IntoElement {
    let store = store();
    let border = move || theme().border;
    let window_button = |icon, label, tone, control| {
        chrome_glyph(icon, BAR_ICON_SIZE, label, tone)
            .w(px(BAR_CELL_WIDTH))
            .border_l_1()
            .border_color(border)
            .window_control(control)
    };

    let quick_access = view()
        .flex_row()
        .h_full()
        .child(
            chrome_text_button("File", move || store.dispatch(Message::FileMenuPressed))
                .border_r_1()
                .border_color(border),
        )
        .child(
            chrome_button(
                Icon::Settings,
                BAR_ICON_SIZE,
                "Settings",
                Tone::Quiet,
                move || store.dispatch(Message::Tabs(TabMessage::Open(Kind::Settings))),
            )
            .w(px(BAR_CELL_WIDTH))
            .border_r_1()
            .border_color(border),
        );

    let window_controls = view()
        .flex_row()
        .h_full()
        .child(window_button(
            Icon::Minimize,
            "Minimize",
            Tone::Warning,
            WindowControl::Minimize,
        ))
        .child(window_button(
            Icon::Maximize,
            "Maximize",
            Tone::Success,
            WindowControl::Maximize,
        ))
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
        .placeholder_color(|| color(tokens::text_faint()))
        .caret_color(|| color(theme().primary))
        .selection_color(|| color(theme().popover).blend(color(theme().primary.opacity(0.35))))
        .flex_1()
        .min_w(px(0.0))
        .text_size(px(SEARCH_TEXT_SIZE))
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
        .child(
            view()
                .w(px(SEARCH_WIDTH))
                .h_full()
                .flex_row()
                .items_center()
                .gap(px(SEARCH_ICON_GAP))
                .px(px(SEARCH_PADDING))
                .border_l_1()
                .border_r_1()
                .border_color(border)
                .focus_within(|s| s.bg(move || theme().popover))
                .window_drag(false)
                .child(
                    Icon::Search
                        .view(BAR_ICON_SIZE)
                        .text_color(tokens::text_faint),
                )
                .child(search),
        );

    view()
        .h(px(TOP_BAR_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_1()
        .border_color(border)
        .window_drag(true)
        .child(quick_access)
        .child(view().flex_1())
        .child(window_controls)
        .child(centered_search)
}

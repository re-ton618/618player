use nsh_base::{menu, menu_item, theme};
use nsh_core::prelude::*;

use crate::app::Message;
use crate::app::store::store;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::ui::{Icon, Tone, chrome_glyph};

const OPTIONS: [Kind; 4] = [
    Kind::Library,
    Kind::NowPlaying,
    Kind::SongInformation,
    Kind::Settings,
];

/// The + button and its menu of the tabs to open.
pub(super) fn add_tab_menu() -> impl IntoElement {
    let store = store();
    let item = move |kind: Kind| {
        menu_item()
            .on_select(move || store.dispatch(Message::Tabs(TabMessage::Open(kind))))
            .px(px(12.0))
            .py(px(8.0))
            .hover(|s| s.bg(move || theme().accent))
            .focus_visible(|s| s.bg(move || theme().accent))
            .child(
                text(kind.label())
                    .text_size(px(13.0))
                    .text_color(move || theme().popover_foreground),
            )
    };

    menu()
        .offset(px(0.0))
        .popup_style(|s| {
            s.w(px(180.0))
                .bg(move || theme().popover)
                .border_1()
                .border_color(move || theme().border)
                .shadow_lg()
        })
        .trigger(
            chrome_glyph(Icon::Add, 14.0, "Open tab", Tone::Quiet)
                .w(px(32.0))
                .h(px(32.0)),
        )
        .children(OPTIONS.map(item))
}

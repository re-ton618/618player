use nsh_base::theme;
use nsh_core::prelude::*;

use super::{library, tab_strip};
use crate::app::store::store;
use crate::app::tabs::Kind;
use crate::ui::{Rule, caption, horizontal_rule};

pub(super) const TAB_STRIP_HEIGHT: f32 = 32.0;

/// The tab strip over the active tab's body, in a bordered section.
pub(super) fn workspace() -> impl IntoElement {
    let store = store();
    let active = store.select(|model| model.tabs.active());

    // The library stays mounted while hidden, keeping its scroll position.
    let library_body = view()
        .flex_1()
        .style_when(move || active.get() != Some(Kind::Library), |s| s.hidden())
        .child(library::library());
    let other = dynamic(move || match active.get() {
        Some(Kind::Library) => view().into_element(),
        Some(Kind::NowPlaying) => {
            placeholder("NOW PLAYING", "Playback details will appear here.").into_element()
        }
        Some(Kind::SongInformation) => {
            placeholder("SONG INFORMATION", "Song details will appear here.").into_element()
        }
        Some(Kind::Settings) => view().flex_1().into_element(),
        None => placeholder("NO TABS OPEN", "Use + to open a tab.").into_element(),
    });

    view()
        .flex_1()
        .min_h(px(0.0))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || theme().border)
        .child(tab_strip::tab_strip())
        .child(horizontal_rule(Rule::Border))
        .child(library_body)
        .child(other)
}

fn placeholder(title: &'static str, detail: &'static str) -> View {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .child(caption(title, 12.0).text_color(move || theme().muted_foreground))
        .child(
            text(detail)
                .text_size(px(14.0))
                .text_color(move || theme().muted_foreground),
        )
}

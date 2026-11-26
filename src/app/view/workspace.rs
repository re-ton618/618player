use nsh_base::theme;
use nsh_core::prelude::*;

use super::{library, now_playing, settings, tab_strip};
use crate::app::store::store;
use crate::app::tabs::Kind;
use crate::ui::strip;

/// The region header: its cells are this tall, with its bottom line under.
pub(super) const TAB_STRIP_HEIGHT: f32 = 32.0;

/// The tab strip over the active tab's body, in a bordered region.
pub(super) fn workspace() -> impl IntoElement {
    let store = store();
    let active = store.select(|model| model.tabs.active());

    // The library stays mounted while hidden, keeping its scroll position.
    let library_body = view()
        .flex_1()
        .min_h(px(0.0))
        .style_when(move || active.get() != Some(Kind::Library), |s| s.hidden())
        .child(library::library());
    let other = dynamic(move || match active.get() {
        Some(Kind::Library) => view().into_element(),
        Some(Kind::NowPlaying) => now_playing::now_playing().into_element(),
        Some(Kind::SongInformation) => message("Song details will appear here.").into_element(),
        Some(Kind::Settings) => settings::settings().into_element(),
        None => message("No tabs open. Use + to open one.").into_element(),
    });

    view()
        .flex_1()
        .min_h(px(0.0))
        .bg(move || theme().background)
        .border_1()
        .border_color(move || theme().border)
        .child(tab_strip::tab_strip())
        .child(library_body)
        .child(other)
}

/// A body with nothing to show: one plain strip that says so.
fn message(said: &'static str) -> View {
    view().flex_1().child(
        strip(said)
            .border_b_1()
            .border_color(move || theme().border),
    )
}

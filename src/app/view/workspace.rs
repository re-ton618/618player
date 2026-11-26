use nsh_base::theme;
use nsh_core::prelude::*;

use super::track_list::track_list;
use super::{now_playing, settings, song_information, tab_strip};
use crate::app::listing::Listing;
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
        .child(track_list(Listing::Library));
    let other = dynamic(move || match active.get() {
        Some(Kind::Library) => view().into_element(),
        Some(Kind::RecentlyAdded) => track_list(Listing::RecentlyAdded).into_element(),
        Some(Kind::NowPlaying) => now_playing::now_playing().into_element(),
        Some(Kind::SongInformation) => song_information::song_information().into_element(),
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

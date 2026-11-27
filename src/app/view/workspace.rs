use nsh_base::dock::{dock_area, DockAreaLayout, DockPanel, DockPlacement, DockTab, PanelId};
use nsh_base::theme;
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use super::track_list::track_list;
use super::{
    add_tab_menu, now_playing, playlists, queue_list, settings, song_information, tab_strip,
    REGION_GAP,
};
use crate::app::listing::Listing;
use crate::app::store::{store, Store};
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::app::Message;
use crate::ui::strip;

/// A tab group's header: its cells are this tall, with its bottom line
/// under.
pub(super) const TAB_STRIP_HEIGHT: f32 = 32.0;
/// The room between two tab groups, as between the window's regions. A
/// drag in it moves what the groups on its two sides take.
const GROUP_GAP: f32 = REGION_GAP;
/// The tint over where a dragged tab would land: 15% of the accent.
const DROP_TINT: f32 = 0.15;

/// The tab groups: one at first, each a bordered region of its own, a
/// header of tabs over the body of the one it shows. A tab dragged out of
/// its header shows where it would land: on a header, at that place in it;
/// on the middle of a body, in that group; on a side of one, in a new group
/// that takes that half. Groups share the room between the bars with only
/// the regions' gap between them: a drag in the gap moves what each takes,
/// and a group that loses its last tab gives its room to what was beside
/// it.
///
/// Narrower than 600, one group shows at a time, every tab in its header;
/// nothing splits there. A phone has its own (`mobile::workspace`).
pub(in crate::app) fn workspace() -> impl IntoElement {
    let store = store();
    let layout = store.select(|model| model.tabs.layout());
    let tabs = move |message| store.dispatch(Message::Tabs(message));

    dock_area()
        .layout(move || layout.get())
        .on_layout_change(move |next: &DockAreaLayout| {
            tabs(TabMessage::Arranged(next.center.clone()))
        })
        .group_style(group)
        .tab_bar_style(tab_strip::tab_bar)
        .handle_accessibility_label("Resize tab groups")
        .drop_indicator_style(|s| {
            s.bg(move || theme().primary.opacity(DROP_TINT))
                .border_1()
                .border_color(move || theme().primary)
        })
        .render_tab(move |tab: &DockTab| {
            of_panel(&tab.panel, |kind| tab_strip::tab(store, kind, tab.active))
        })
        .render_toolbar(|tab: &DockTab| {
            of_panel(&tab.panel, |kind| add_tab_menu::add_tab_menu(Some(kind)))
        })
        .render_drag_preview(|tab: &DockTab| of_panel(&tab.panel, tab_strip::drag_preview))
        .render_panel(move |panel: &DockPanel| of_panel(&panel.id, |kind| body(store, kind)))
        .render_empty(|_: &DockPlacement| empty())
        .flex_1()
        .min_h(px(0.0))
        // Every group keeps half the gap clear around it: at the outside
        // that half is taken back, so the groups end where the bars do.
        .m(px(-GROUP_GAP / 2.0))
}

/// A tab group's frame: a region like the bars, on `background` with its
/// line around it and half the gap clear on every side.
fn group(style: StyleRefinement) -> StyleRefinement {
    style
        .m(px(GROUP_GAP / 2.0))
        .bg(move || theme().background)
        .border_1()
        .border_color(move || theme().border)
}

/// What `draw` makes of the tab that a panel of the dock area is.
fn of_panel<E: IntoElement>(panel: &PanelId, draw: impl FnOnce(Kind) -> E) -> AnyElement {
    match Kind::of(panel) {
        Some(kind) => draw(kind).into_element(),
        None => view().into_element(),
    }
}

/// The body of the tab of `kind`, which fills its group under the header.
/// A press in it gives its group the focus.
fn body(store: Store, kind: Kind) -> impl IntoElement {
    let shown = store.select(move |model| model.tabs.shows(kind));
    let body = view()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .overflow_hidden()
        .on_mouse_up(move |_: &MouseEvent| {
            store.dispatch(Message::Tabs(TabMessage::Focus(kind)));
        });
    match kind {
        // The library stays mounted while hidden, keeping its scroll
        // position; the rest are made when their tab shows.
        Kind::Library => body.child(track_list(Listing::Library)),
        _ => body.child(when(move || shown.get(), move || content(kind))),
    }
}

/// What the tab of `kind` shows under its group's header.
pub(in crate::app) fn content(kind: Kind) -> AnyElement {
    match kind {
        Kind::Library => track_list(Listing::Library).into_element(),
        Kind::RecentlyAdded => track_list(Listing::RecentlyAdded).into_element(),
        Kind::MostPlayed => track_list(Listing::MostPlayed).into_element(),
        Kind::NowPlaying => now_playing::now_playing().into_element(),
        Kind::Queue => queue_list::queue_list().into_element(),
        Kind::SongInformation => song_information::song_information().into_element(),
        Kind::Playlists => playlists::playlists().into_element(),
        Kind::Settings => settings::settings().into_element(),
    }
}

/// The region with no tab open: a header with only the menu that opens
/// them, over one plain strip that says so.
fn empty() -> View {
    view()
        .flex_1()
        .style_with(group)
        .child(
            view()
                .style_with(tab_strip::tab_bar)
                .child(add_tab_menu::add_tab_menu(None)),
        )
        .child(
            strip("No tabs open. Use + to open one.")
                .border_b_1()
                .border_color(move || theme().border),
        )
}

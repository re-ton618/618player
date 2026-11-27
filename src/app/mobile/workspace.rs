//! The phone's workspace: the tab group over the Now Playing sheet, which
//! trade the room between the bars.

use nsh_base::theme;
use nsh_core::prelude::*;

use super::now_playing::{self, now_playing};
use super::split::{Shut, Split};
use super::{BAR_ICON_SIZE, REGION_GAP};
use crate::app::store::{store, Store};
use crate::app::tabs::{Half, Kind, Message as TabMessage};
use crate::app::view::content;
use crate::app::Message;
use crate::ui::{bar_menu, chrome_button, chrome_glyph, menu_row, Icon, Tone};

/// The group's header: its cells are this tall, with its bottom line under.
const STRIP_HEIGHT: f32 = 44.0;
/// The group shut: the header between the group's lines.
const HEADER_HEIGHT: f32 = STRIP_HEIGHT + 2.0;
/// How tall each region is shut.
const SHUT: Shut = Shut {
    upper: HEADER_HEIGHT,
    lower: now_playing::BAR_HEIGHT,
};
const TAB_PADDING: f32 = 14.0;
const TAB_TEXT_SIZE: f32 = 14.0;
/// A tab's close cell and the + cell: touch targets.
const CLOSE_WIDTH: f32 = 40.0;
const CLOSE_ICON_SIZE: f32 = 12.0;
const ADD_WIDTH: f32 = 48.0;
const MENU_ROW_HEIGHT: f32 = 44.0;
/// How far up or down a finger moves on a handle before it drags the
/// regions' sizes.
const DRAG_SLOP: f32 = 8.0;

/// What the two regions share: how they split the room, how much there
/// is, and how tall the upper one was when a drag began.
#[derive(Clone, Copy)]
struct Deck {
    store: Store,
    split: Signal<Split>,
    room: Signal<f32>,
    grabbed: Signal<f32>,
}

impl Deck {
    /// `half` was used: when it was shut it takes the room.
    fn reveal(self, half: Half) {
        self.split.update(|split| *split = split.revealing(half));
    }

    /// How tall `half` is; none when it takes what the other leaves.
    fn height(self, half: Half) -> Option<f32> {
        match (self.split.get(), half) {
            (Split::Upper, Half::Upper) | (Split::Lower | Split::Both(_), Half::Lower) => None,
            (Split::Upper, Half::Lower) => Some(SHUT.lower),
            (Split::Lower, Half::Upper) => Some(SHUT.upper),
            (split @ Split::Both(_), Half::Upper) => Some(split.upper(self.room.get(), SHUT)),
        }
    }

    fn grab(self) {
        let (split, room) = (self.split.get_untracked(), self.room.get_untracked());
        self.grabbed.set(split.upper(room, SHUT));
    }

    /// The finger is `by` under where it took hold.
    fn drag(self, by: f32) {
        let upper = self.grabbed.get_untracked() + by;
        self.split
            .set(Split::dragged(upper, self.room.get_untracked(), SHUT));
    }

    fn release(self) {
        let room = self.room.get_untracked();
        self.split
            .update(|split| *split = split.settled(room, SHUT));
    }
}

/// The tab group over the Now Playing sheet, each a bordered region. One
/// has the room and the other is shut (the group its header, the sheet its
/// bar) until it is used: then they trade. Dragging the gap between them,
/// the group's header, the sheet's bar or the name on the open sheet up or
/// down shows both at the sizes it is let go at; let go near an end, a
/// region is shut again.
///
/// Every tab is in the group, which keeps its last; Now Playing is the
/// sheet, never a tab.
pub(super) fn workspace(split: Signal<Split>) -> impl IntoElement {
    let deck = Deck {
        store: store(),
        split,
        room: signal(0.0),
        grabbed: signal(0.0),
    };

    let grip = resizer(
        view()
            .h(px(REGION_GAP))
            .flex_shrink_0()
            .cursor(Cursor::RowResize)
            .role(AccessibilityRole::Adjustable)
            .label("Resize Now Playing")
            .claim_touches(TouchClaim::on_start()),
        deck,
    );

    view()
        .flex_1()
        .min_h(px(0.0))
        .on_layout(move |frame: &Bounds<Points>| {
            deck.room.set((frame.size.height.0 - REGION_GAP).max(0.0));
        })
        .child(group(deck))
        .child(grip)
        .child(sheet(deck))
}

/// `handle`, once it has the touches, drags the regions' sizes.
fn resizer(handle: View, deck: Deck) -> View {
    handle
        .keep_touches(true)
        .on_responder_grant(move |_: &ResponderEvent| deck.grab())
        .on_responder_move(move |event: &ResponderEvent| deck.drag(event.gesture.delta.y.0))
        .on_responder_release(move |_: &ResponderEvent| deck.release())
        .on_responder_terminate(move |_: &ResponderEvent| deck.release())
}

/// `handle` takes the touches of a finger that goes up or down on it, and
/// drags the regions' sizes with them; a tap or a sideways move is its
/// own.
fn draggable(handle: View, deck: Deck) -> View {
    resizer(
        handle.claim_touches(TouchClaim::on_move().ask(|request: &ClaimRequest| {
            let moved = request.event.gesture.delta;
            if moved.y.0.abs() > DRAG_SLOP && moved.y.0.abs() > moved.x.0.abs() {
                request.claim();
            }
        })),
        deck,
    )
}

/// The frame of a region: on `background` with its line around it, as
/// tall as the split gives `half`.
fn region(deck: Deck, half: Half) -> View {
    view()
        .min_h(px(0.0))
        .overflow_hidden()
        .bg(move || theme().background)
        .border_1()
        .border_color(move || theme().border)
        .style_with(move |style| match deck.height(half) {
            Some(height) => style.h(px(height)).flex_shrink_0(),
            None => style.flex_1(),
        })
}

/// The Now Playing sheet: its bar while shut, a tap on which opens it;
/// else the open sheet.
fn sheet(deck: Deck) -> impl IntoElement {
    let shut = move || deck.split.get().collapsed() == Some(Half::Lower);
    let drag = move |handle: View| draggable(handle, deck);
    region(deck, Half::Lower).child(
        when(shut, move || {
            now_playing::bar(drag, move || deck.reveal(Half::Lower)).into_element()
        })
        .otherwise(move || now_playing(drag).into_element()),
    )
}

/// The tab group: its header, then every tab's body, the shown one's
/// showing.
fn group(deck: Deck) -> impl IntoElement {
    let store = deck.store;
    let tabs = store.select(|model| model.tabs.half(Half::Upper));
    region(deck, Half::Upper)
        .child(strip(deck, tabs))
        .child(each(
            tabs,
            |kind: &Kind| *kind,
            move |kind, _| body(store, kind.get_untracked()),
        ))
}

/// The group's header on `card` with its bottom line: its tabs, which
/// scroll sideways when they do not fit, the + that opens more, and the
/// rest of it. A press anywhere in it gives the group the room when it was
/// shut; a finger that goes up or down on it drags the regions' sizes.
fn strip(deck: Deck, tabs: Memo<Vec<Kind>>) -> impl IntoElement {
    let cells = scroll()
        .horizontal()
        .scrollbar(false)
        .flex_shrink(1.0)
        .min_w(px(0.0))
        .h_full()
        .child(view().flex_row().h_full().child(each(
            tabs,
            |kind: &Kind| *kind,
            move |kind, _| tab(deck, kind.get_untracked()),
        )));

    let rest = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .on_press(move || deck.reveal(Half::Upper));

    draggable(
        view()
            .h(px(STRIP_HEIGHT + 1.0))
            .flex_shrink_0()
            .flex_row()
            .min_w(px(0.0))
            .overflow_hidden()
            .bg(move || theme().card)
            .border_b_1()
            .border_color(move || theme().border)
            .child(cells)
            .child(add_menu(deck, tabs))
            .child(rest),
        deck,
    )
}

/// A tab, as wide as its name, with its right edge: the one the group
/// shows on the body's `background` in the text color, the rest on `card`
/// and muted. A press shows it; its × closes it, unless it is the last.
fn tab(deck: Deck, kind: Kind) -> impl IntoElement {
    let store = deck.store;
    let shown = store.select(move |model| model.tabs.shows(kind));
    let fixed = store.select(move |model| model.tabs.fixed(kind));

    let label = view()
        .h_full()
        .px(px(TAB_PADDING))
        .justify_center()
        .role(AccessibilityRole::Tab)
        .label(kind.label())
        .on_press(move || {
            store.dispatch(Message::Tabs(TabMessage::Open(kind)));
            deck.reveal(Half::Upper);
        })
        .child(
            text(kind.label())
                .text_size(px(TAB_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || {
                    if shown.get() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        );
    let close = when(
        move || !fixed.get(),
        move || {
            chrome_button(
                Icon::Close,
                CLOSE_ICON_SIZE,
                "Close tab",
                Tone::Danger,
                move || store.dispatch(Message::Tabs(TabMessage::Close(kind))),
            )
            .w(px(CLOSE_WIDTH))
        },
    );

    view()
        .h(px(STRIP_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || {
            if shown.get() {
                theme().background
            } else {
                theme().card
            }
        })
        .border_r_1()
        .border_color(move || theme().border)
        .child(label)
        .child(close)
}

/// The + cell with its right edge, and its menu of the tabs to open: all
/// but Now Playing, which is the sheet.
fn add_menu(deck: Deck, tabs: Memo<Vec<Kind>>) -> impl IntoElement {
    let store = deck.store;
    let row = move |kind: Kind| {
        menu_row(kind.label(), move || {
            let beside = tabs.with_untracked(|tabs| tabs.first().copied());
            store.dispatch(Message::Tabs(match beside {
                Some(beside) => TabMessage::OpenBeside { kind, beside },
                None => TabMessage::Open(kind),
            }));
            deck.reveal(Half::Upper);
        })
        .h(px(MENU_ROW_HEIGHT))
    };
    let offered = Kind::ALL
        .into_iter()
        .filter(|&kind| kind != Kind::NowPlaying);

    bar_menu(
        chrome_glyph(Icon::Add, BAR_ICON_SIZE, "Open tab", Tone::Quiet)
            .w(px(ADD_WIDTH))
            .h(px(STRIP_HEIGHT))
            .border_r_1()
            .border_color(move || theme().border),
    )
    .children(offered.map(row))
}

/// The body of the tab of `kind` under the group's header, showing while
/// its tab does. The library stays made while hidden, keeping its place;
/// the rest are made when their tab shows.
fn body(store: Store, kind: Kind) -> impl IntoElement {
    let shown = store.select(move |model| model.tabs.shows(kind));
    let body = view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .overflow_hidden()
        .style_when(move || !shown.get(), |style| style.hidden());
    match kind {
        Kind::Library => body.child(content(kind)),
        _ => body.child(when(move || shown.get(), move || content(kind))),
    }
}

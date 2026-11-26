use std::sync::Arc;

use nsh_base::{button, theme};
use nsh_core::geometry::Percentage;
use nsh_core::prelude::*;

use super::song_information::now_playing_panel;
use crate::app::lyrics::Shown;
use crate::app::store::{store, Store};
use crate::app::Message;
use crate::library::Lyrics;
use crate::theme as tokens;
use crate::ui::{caption, strip};

/// The title over the lyrics: 28 and its bottom line, as the library's
/// column titles are.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
const HORIZONTAL_PADDING: f32 = 10.0;
/// A line of the lyrics: 15pt, inset 16 across and 5 down; a line that
/// runs longer wraps.
const LINE_TEXT_SIZE: f32 = 15.0;
const LINE_PADDING_X: f32 = 16.0;
const LINE_PADDING_Y: f32 = 5.0;
const LINE_HEIGHT: f32 = 30.0;
/// The room above the first line and below the last.
const LIST_PADDING_Y: f32 = 12.0;
/// Where the line being sung is kept: this far down the view.
const CURRENT_LINE_AT: f32 = 0.4;
/// What stands for a line with no words in timed lyrics: a pause.
const PAUSE: &str = "\u{266a}";

/// The Now Playing tab: on the left what is playing, its artwork, name
/// and details as Song Information shows them; on the right its lyrics.
/// Timed lyrics follow the track, the line being sung lit and kept in
/// view; a press on a line goes there.
pub(super) fn now_playing() -> impl IntoElement {
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(now_playing_panel())
        .child(lyrics())
}

/// The lyrics' title, then what is known of them.
fn lyrics() -> impl IntoElement {
    let store = store();
    let shown = store.select(|model| model.lyrics.shown());
    let synced = computed(move || {
        shown.with(|shown| matches!(shown, Shown::Ready(lyrics) if lyrics.synced))
    });
    let border = move || theme().border;
    let title = |said: &'static str| {
        caption(said, HEADER_TEXT_SIZE)
            .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
    };

    let header = view()
        .h(px(HEADER_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(HORIZONTAL_PADDING))
        .bg(move || theme().card)
        .border_b_1()
        .border_color(border)
        .child(title("LYRICS").text_color(move || theme().muted_foreground))
        .child(when(synced, move || {
            title("SYNCED").text_color(tokens::text_faint)
        }));

    let notice = |said: &'static str| {
        strip(said)
            .border_b_1()
            .border_color(move || theme().border)
            .into_element()
    };
    let body = dynamic(move || {
        match shown.get() {
        Shown::Nothing => notice("Nothing is playing."),
        Shown::Reading => notice("Reading lyrics\u{2026}"),
        Shown::Missing => notice(
            "No lyrics for this track. Add them to its tags, or put an .lrc file of the same name beside it.",
        ),
        Shown::Ready(lyrics) => lines(store, lyrics).into_element(),
    }
    });

    view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .child(header)
        .child(body)
}

/// The lines in a list; timed ones follow the position.
fn lines(store: Store, lyrics: Arc<Lyrics>) -> impl IntoElement {
    let list = element_ref();
    let current = store.select(|model| model.lyrics.line_at(model.playback.position_ms()));
    if lyrics.synced {
        effect(move || {
            if let Some(index) = current.get() {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                list.scroll_to_index(index, Percentage(CURRENT_LINE_AT), px(0.0), true);
            }
        });
    }
    let count = lyrics.lines.len();
    virtual_list(
        move || count,
        move |at| line(store, Arc::clone(&lyrics), current, at),
    )
    .estimated_item_size(px(LINE_HEIGHT))
    .py(px(LIST_PADDING_Y))
    .flex_1()
    .element_ref(&list)
}

/// A line of the lyrics: the one being sung in the foreground, the rest
/// muted; plain lyrics all in the foreground.
fn line(
    store: Store,
    lyrics: Arc<Lyrics>,
    current: Memo<Option<usize>>,
    at: Position,
) -> impl IntoElement {
    let synced = lyrics.synced;
    let words = {
        let lyrics = Arc::clone(&lyrics);
        move || {
            let text = match lyrics.lines.get(at.index()) {
                Some(line) => line.text.as_str(),
                None => "",
            };
            let text = if text.is_empty() && synced {
                PAUSE
            } else {
                text
            };
            SharedString::from(text.to_owned())
        }
    };
    let lit = move || !synced || current.get() == Some(at.index());
    let words = text(words)
        .text_size(px(LINE_TEXT_SIZE))
        .text_color(move || {
            if lit() {
                theme().foreground
            } else {
                theme().muted_foreground
            }
        });

    if !synced {
        return view()
            .w_full()
            .min_h(px(LINE_HEIGHT))
            .justify_center()
            .px(px(LINE_PADDING_X))
            .py(px(LINE_PADDING_Y))
            .selectable(true)
            .child(words)
            .into_element();
    }
    button()
        .w_full()
        .min_h(px(LINE_HEIGHT))
        .justify_center()
        .px(px(LINE_PADDING_X))
        .py(px(LINE_PADDING_Y))
        .transition_colors()
        .hover(|s| s.bg(move || theme().card))
        .active(|s| s.bg(move || theme().border))
        .on_press(move || {
            if let Some(at_ms) = lyrics.lines.get(at.index()).and_then(|line| line.at_ms) {
                store.dispatch(Message::PositionChanged(at_ms));
            }
        })
        .child(words)
        .into_element()
}

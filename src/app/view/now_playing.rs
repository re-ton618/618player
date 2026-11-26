use nsh_base::{button, theme};
use nsh_core::prelude::*;

use super::song_information::now_playing_panel;
use crate::app::queue::Entry;
use crate::app::store::{store, Store};
use crate::app::Message;
use crate::library::Track;
use crate::theme as tokens;
use crate::ui::{caption, chrome_button, strip, Icon, Tone};

/// The column titles: 28 and their bottom line, as the library's are.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
/// A queue row, its bottom line included: a library row.
const ROW_HEIGHT: f32 = 32.0;
const CELL_TEXT_SIZE: f32 = 12.0;
const TITLE_PORTION: f32 = 5.0;
const ARTIST_PORTION: f32 = 3.0;
const TIME_WIDTH: f32 = 58.0;
const CELL_GAP: f32 = 12.0;
const HORIZONTAL_PADDING: f32 = 10.0;
/// The cells that move and remove a row: square, with small glyphs.
const ACTION_ICON_SIZE: f32 = 10.0;

/// What a queue row shows of its entry.
#[derive(Debug, Clone, PartialEq, Default)]
struct Row {
    key: u64,
    index: usize,
    title: SharedString,
    artist: SharedString,
    time: SharedString,
    /// The entry that plays, or would.
    current: bool,
    /// Its file is there: it will be played rather than stepped over.
    present: bool,
}

impl Row {
    fn of(index: usize, entry: Entry, track: Option<&Track>, current: bool) -> Self {
        Self {
            key: entry.key,
            index,
            title: track
                .map_or_else(|| "Unknown track".to_owned(), |track| track.title.clone())
                .into(),
            artist: track
                .and_then(|track| track.artist.clone())
                .unwrap_or_default()
                .into(),
            time: track
                .map(Track::formatted_duration)
                .unwrap_or_default()
                .into(),
            current,
            present: track.is_some_and(|track| track.present),
        }
    }
}

/// The Now Playing tab: on the left what is playing, its artwork, name
/// and details as Song Information shows them; on the right the queue in
/// the order it plays, the current track marked as in the library. A
/// press on a row plays it; its cells move it up or down and remove it.
pub(super) fn now_playing() -> impl IntoElement {
    let store = store();
    let count = store.select(|model| model.queue.entries().len());
    let empty = computed(move || count.get() == 0);
    let border = move || theme().border;

    let title = |said: &'static str| {
        caption(said, HEADER_TEXT_SIZE)
            .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
            .text_color(move || theme().muted_foreground)
    };
    // The titles sit over the cells they name: the row's own padding and
    // gaps, then the order and removal cells, each with its left line.
    let columns = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .gap(px(CELL_GAP))
        .px(px(HORIZONTAL_PADDING))
        .child(
            view()
                .h_full()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .flex_1()
                .flex_grow(TITLE_PORTION)
                .min_w(px(0.0))
                .child(title("TITLE"))
                .child(
                    caption(
                        move || SharedString::from(count.get().to_string()),
                        HEADER_TEXT_SIZE,
                    )
                    .tabular_nums()
                    .text_color(tokens::text_faint),
                ),
        )
        .child(
            view()
                .h_full()
                .justify_center()
                .flex_1()
                .flex_grow(ARTIST_PORTION)
                .min_w(px(0.0))
                .child(title("ARTIST")),
        )
        .child(
            view()
                .h_full()
                .justify_center()
                .items_end()
                .w(px(TIME_WIDTH))
                .flex_shrink_0()
                .child(title("TIME")),
        );
    let order = view()
        .w(px(ROW_HEIGHT * 2.0))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(border)
        .child(title("QUEUE"));
    let clear = chrome_button(
        Icon::Close,
        ACTION_ICON_SIZE,
        "Clear queue",
        Tone::Danger,
        move || store.dispatch(Message::QueueClearPressed),
    )
    .w(px(ROW_HEIGHT))
    .border_l_1()
    .border_color(border);

    let header = view()
        .h(px(HEADER_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_b_1()
        .border_color(border)
        .child(columns)
        .child(order)
        .child(clear);

    let list = virtual_list(move || count.get(), move |at| queue_row(store, at))
        .estimated_item_size(px(ROW_HEIGHT))
        .flex_1();

    let queue = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .child(header)
        .child(when(empty, move || {
            strip("Nothing is queued. Press a track in the library to play it from there.")
                .border_b_1()
                .border_color(move || theme().border)
        }))
        .child(list);

    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(now_playing_panel())
        .child(queue)
}

/// A cell of the list: it shows whichever entry it is given. The current
/// one is `card` with accent text; one whose file is gone is faint.
fn queue_row(store: Store, at: Position) -> impl IntoElement {
    let row = store.select(move |model| {
        let index = at.index();
        model
            .queued_track(index)
            .map(|(entry, track, current)| Row::of(index, entry, track, current))
    });
    let current = move || row.with(|row| row.as_ref().is_some_and(|row| row.current));
    let present = move || row.with(|row| row.as_ref().is_none_or(|row| row.present));
    let place = move || row.with_untracked(|row| row.as_ref().map(|row| (row.key, row.index)));
    let border = move || theme().border;

    let cell = move |read: fn(&Row) -> SharedString, leading: bool| {
        text(move || row.with(|row| row.as_ref().map(read).unwrap_or_default()))
            .text_size(px(CELL_TEXT_SIZE))
            .number_of_lines(1)
            .tabular_nums()
            .text_color(move || {
                if !present() {
                    tokens::text_faint()
                } else if current() {
                    theme().primary
                } else if leading {
                    theme().foreground
                } else {
                    theme().muted_foreground
                }
            })
    };
    let entry = button()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .gap(px(CELL_GAP))
        .px(px(HORIZONTAL_PADDING))
        .transition_colors()
        .hover(|s| s.bg(move || theme().card))
        .active(|s| s.bg(move || theme().border))
        .on_press(move || {
            if let Some((key, _)) = place() {
                store.dispatch(Message::QueueEntryPressed(key));
            }
        })
        .child(
            view()
                .h_full()
                .justify_center()
                .flex_1()
                .flex_grow(TITLE_PORTION)
                .min_w(px(0.0))
                .child(cell(|row| row.title.clone(), true)),
        )
        .child(
            view()
                .h_full()
                .justify_center()
                .flex_1()
                .flex_grow(ARTIST_PORTION)
                .min_w(px(0.0))
                .child(cell(|row| row.artist.clone(), false)),
        )
        .child(
            view()
                .h_full()
                .justify_center()
                .items_end()
                .w(px(TIME_WIDTH))
                .flex_shrink_0()
                .child(cell(|row| row.time.clone(), false)),
        );

    let action = move |icon, label, tone, message: fn(u64, usize) -> Message| {
        chrome_button(icon, ACTION_ICON_SIZE, label, tone, move || {
            if let Some((key, index)) = place() {
                store.dispatch(message(key, index));
            }
        })
        .w(px(ROW_HEIGHT))
        .border_l_1()
        .border_color(border)
    };

    view()
        .h(px(ROW_HEIGHT))
        .flex_row()
        .border_b_1()
        .border_color(border)
        .style_when(current, |s| s.bg(move || theme().card))
        .child(entry)
        .child(action(
            Icon::SortAscending,
            "Move up",
            Tone::Quiet,
            |key, index| Message::QueueMovePressed {
                key,
                to: index.saturating_sub(1),
            },
        ))
        .child(action(
            Icon::SortDescending,
            "Move down",
            Tone::Quiet,
            |key, index| Message::QueueMovePressed { key, to: index + 1 },
        ))
        .child(action(
            Icon::Close,
            "Remove from queue",
            Tone::Danger,
            |key, _| Message::QueueRemovePressed(key),
        ))
}

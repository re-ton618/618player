//! A listing of tracks: its column titles over a virtualized list, a row
//! a track. Which columns each [`Listing`] has is here; its rows and
//! their order are the model's.

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use super::format::timestamp;
use crate::app::Message;
use crate::app::listing::Listing;
use crate::app::store::{Store, store};
use crate::library::{SortColumn, SortDirection, Track};
use crate::ui::{Icon, caption, menu_row, subject_menu};

/// A track row, its bottom line included.
const ROW_HEIGHT: f32 = 32.0;
/// The column titles: 28 and their bottom line.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
const SORT_ICON_SIZE: f32 = 8.0;
const CELL_TEXT_SIZE: f32 = 12.0;
const CELL_GAP: f32 = 12.0;
const HORIZONTAL_PADDING: f32 = 10.0;

/// A column's width: a share of the free space, or fixed and right-aligned.
#[derive(Clone, Copy)]
enum Width {
    Portion(f32),
    Fixed(f32),
}

impl Width {
    fn apply(self, element: View) -> View {
        match self {
            Self::Portion(portion) => element.flex_1().flex_grow(portion).min_w(px(0.0)),
            Self::Fixed(width) => element.w(px(width)).flex_shrink_0().items_end(),
        }
    }
}

/// A column: its title, the order it is marked by (and a press on it
/// sorts by, where the listing sorts), its width, and what it shows of a
/// track.
#[derive(Clone, Copy)]
struct Column {
    title: &'static str,
    sort: SortColumn,
    width: Width,
    cell: fn(&Track) -> String,
}

const TITLE: Column = Column {
    title: "TITLE",
    sort: SortColumn::Title,
    width: Width::Portion(5.0),
    cell: |track| track.title.clone(),
};
const ARTIST: Column = Column {
    title: "ARTIST",
    sort: SortColumn::Artist,
    width: Width::Portion(3.0),
    cell: |track| track.artist.clone().unwrap_or_default(),
};
const ALBUM: Column = Column {
    title: "ALBUM",
    sort: SortColumn::Album,
    width: Width::Portion(4.0),
    cell: |track| track.album.clone().unwrap_or_default(),
};
const YEAR: Column = Column {
    title: "YEAR",
    sort: SortColumn::Year,
    width: Width::Fixed(58.0),
    cell: |track| track.year.map(|year| year.to_string()).unwrap_or_default(),
};
const TIME: Column = Column {
    title: "TIME",
    sort: SortColumn::Duration,
    width: Width::Fixed(58.0),
    cell: Track::formatted_duration,
};
/// Fits "2023-11-14 22:13 UTC".
const ADDED: Column = Column {
    title: "ADDED",
    sort: SortColumn::Added,
    width: Width::Fixed(140.0),
    cell: |track| timestamp(track.modified_ns).unwrap_or_default(),
};

const LIBRARY_COLUMNS: &[Column] = &[TITLE, ARTIST, ALBUM, YEAR, TIME];
const RECENTLY_ADDED_COLUMNS: &[Column] = &[TITLE, ARTIST, ALBUM, ADDED, TIME];

const fn columns(listing: Listing) -> &'static [Column] {
    match listing {
        Listing::Library => LIBRARY_COLUMNS,
        Listing::RecentlyAdded => RECENTLY_ADDED_COLUMNS,
    }
}

/// What a row shows of its track: a cell a column.
#[derive(Debug, Clone, PartialEq, Default)]
struct Row {
    id: i64,
    cells: Vec<SharedString>,
}

impl Row {
    fn of(track: &Track, columns: &[Column]) -> Self {
        Self {
            id: track.id,
            cells: columns
                .iter()
                .map(|column| (column.cell)(track).into())
                .collect(),
        }
    }
}

/// The column titles of `listing` over the virtualized list of its rows.
pub(super) fn track_list(listing: Listing) -> impl IntoElement {
    let store = store();
    let count = store.select(move |model| model.rows(listing).len());
    let list = virtual_list(move || count.get(), move |at| track_row(store, listing, at))
        .estimated_item_size(px(ROW_HEIGHT))
        .flex_1()
        .element_ref(&store.list(listing));

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(header(store, listing))
        .child(list)
}

/// The column titles: `card`, 10pt semibold and spaced, with their bottom
/// line, the order the list is in marked. A press on one sorts by it,
/// where the listing sorts.
fn header(store: Store, listing: Listing) -> impl IntoElement {
    let sort = store.select(move |model| model.sort_of(listing));
    let column = move |&Column {
                           title,
                           sort: by,
                           width,
                           ..
                       }: &Column| {
        let marker = move |direction, icon: Icon| {
            when(
                move || sort.get() == (by, direction),
                move || {
                    icon.view(SORT_ICON_SIZE)
                        .text_color(move || theme().muted_foreground)
                        .group_hover(|s| s.text_color(move || theme().foreground))
                },
            )
        };
        let content = view()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(
                caption(title, HEADER_TEXT_SIZE)
                    .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
                    .text_color(move || theme().muted_foreground)
                    .group_hover(|s| s.text_color(move || theme().foreground)),
            )
            .child(marker(SortDirection::Ascending, Icon::SortAscending))
            .child(marker(SortDirection::Descending, Icon::SortDescending));
        let title = if listing.sortable() {
            button()
                .group()
                .h_full()
                .justify_center()
                .accessibility_label(format!("Sort by {}", title.to_lowercase()))
                .on_press(move || store.dispatch(Message::SortChanged(by)))
                .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
                .child(content)
                .into_element()
        } else {
            content.into_element()
        };
        width.apply(view().h_full().justify_center().child(title))
    };

    view()
        .h(px(HEADER_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .gap(px(CELL_GAP))
        .px(px(HORIZONTAL_PADDING))
        .bg(move || theme().card)
        .border_b_1()
        .border_color(move || theme().border)
        .children(columns(listing).iter().map(column))
}

/// A cell of the list: it shows whichever row it is given. A line with
/// its bottom edge; the current track is `card` with accent text. A press
/// plays it; the secondary button opens the menu that queues it.
fn track_row(store: Store, listing: Listing, at: Position) -> impl IntoElement {
    let row = store.select(move |model| {
        model
            .listed_track(listing, at.index())
            .map(|track| Row::of(track, columns(listing)))
    });
    let current_id = store.select(|model| model.current_track().map(|track| track.id));
    let current = move || {
        let id = row.with(|row| row.as_ref().map(|row| row.id));
        id.is_some() && id == current_id.get()
    };
    let cell = move |index: usize, width: Width| {
        width.apply(
            view().h_full().justify_center().child(
                text(move || {
                    row.with(|row| {
                        row.as_ref()
                            .and_then(|row| row.cells.get(index).cloned())
                            .unwrap_or_default()
                    })
                })
                .text_size(px(CELL_TEXT_SIZE))
                .number_of_lines(1)
                .tabular_nums()
                .text_color(move || {
                    if current() {
                        theme().primary
                    } else if index == 0 {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
            ),
        )
    };

    // The track the cell shows when it is asked about, not when it was made.
    let ask = move |message: &dyn Fn(i64) -> Message| {
        if let Some(id) = row.with_untracked(|row| row.as_ref().map(|row| row.id)) {
            store.dispatch(message(id));
        }
    };

    let line = button()
        .h(px(ROW_HEIGHT))
        .flex_row()
        .gap(px(CELL_GAP))
        .px(px(HORIZONTAL_PADDING))
        .border_b_1()
        .border_color(move || theme().border)
        .transition_colors()
        .style_when(current, |s| s.bg(move || theme().card))
        .hover(|s| s.bg(move || theme().card))
        .active(|s| s.bg(move || theme().border))
        .on_press(move || ask(&|id| Message::TrackPressed(listing, id)))
        .children(
            columns(listing)
                .iter()
                .enumerate()
                .map(|(index, column)| cell(index, column.width)),
        );

    subject_menu(line)
        .child(menu_row("Play Next", move || {
            ask(&Message::PlayNextPressed)
        }))
        .child(menu_row("Add to Queue", move || {
            ask(&Message::AddToQueuePressed)
        }))
}

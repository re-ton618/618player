use nsh_base::{button, theme};
use nsh_core::prelude::*;

use crate::app::Message;
use crate::app::store::{Store, store};
use crate::library::{SortColumn, SortDirection, Track};
use crate::ui::{Icon, caption};

/// A track row, its bottom line included.
const ROW_HEIGHT: f32 = 32.0;
/// The column titles: 28 and their bottom line.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
const SORT_ICON_SIZE: f32 = 8.0;
const CELL_TEXT_SIZE: f32 = 12.0;
const TITLE_PORTION: f32 = 5.0;
const ARTIST_PORTION: f32 = 3.0;
const ALBUM_PORTION: f32 = 4.0;
const YEAR_WIDTH: f32 = 58.0;
const TIME_WIDTH: f32 = 58.0;
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

const COLUMNS: [(&str, SortColumn, Width); 5] = [
    ("TITLE", SortColumn::Title, Width::Portion(TITLE_PORTION)),
    ("ARTIST", SortColumn::Artist, Width::Portion(ARTIST_PORTION)),
    ("ALBUM", SortColumn::Album, Width::Portion(ALBUM_PORTION)),
    ("YEAR", SortColumn::Year, Width::Fixed(YEAR_WIDTH)),
    ("TIME", SortColumn::Duration, Width::Fixed(TIME_WIDTH)),
];

/// What a library row shows of its track.
#[derive(Debug, Clone, PartialEq, Default)]
struct Row {
    id: i64,
    cells: [SharedString; 5],
}

impl Row {
    fn of(track: &Track) -> Self {
        Self {
            id: track.id,
            cells: [
                track.title.clone().into(),
                track.artist.clone().unwrap_or_default().into(),
                track.album.clone().unwrap_or_default().into(),
                track
                    .year
                    .map_or_else(String::new, |year| year.to_string())
                    .into(),
                track.formatted_duration().into(),
            ],
        }
    }
}

/// The column titles over the virtualized list of the visible tracks.
pub(super) fn library() -> impl IntoElement {
    let store = store();
    let count = store.select(|model| model.visible_tracks.len());
    let list = virtual_list(move || count.get(), move |at| track_row(store, at))
        .estimated_item_size(px(ROW_HEIGHT))
        .flex_1()
        .element_ref(&store.library_list());

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(header(store))
        .child(list)
}

/// The sortable column titles: `card`, 10pt semibold and spaced, with
/// their bottom line.
fn header(store: Store) -> impl IntoElement {
    let sort = store.select(|model| (model.sort_column, model.sort_direction));
    let column = move |(label, column, width): (&'static str, SortColumn, Width)| {
        let marker = move |direction, icon: Icon| {
            when(
                move || sort.get() == (column, direction),
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
                caption(label, HEADER_TEXT_SIZE)
                    .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
                    .text_color(move || theme().muted_foreground)
                    .group_hover(|s| s.text_color(move || theme().foreground)),
            )
            .child(marker(SortDirection::Ascending, Icon::SortAscending))
            .child(marker(SortDirection::Descending, Icon::SortDescending));
        width.apply(
            view().h_full().justify_center().child(
                button()
                    .group()
                    .h_full()
                    .justify_center()
                    .accessibility_label(format!("Sort by {}", label.to_lowercase()))
                    .on_press(move || store.dispatch(Message::SortChanged(column)))
                    .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
                    .child(content),
            ),
        )
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
        .children(COLUMNS.map(column))
}

/// A cell of the list: it shows whichever row it is given. A line with
/// its bottom edge; the current track is `card` with accent text.
fn track_row(store: Store, at: Position) -> impl IntoElement {
    let row = store.select(move |model| model.visible_track(at.index()).map(Row::of));
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
                            .map(|row| row.cells[index].clone())
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

    button()
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
        .on_press(move || {
            if let Some(id) = row.with_untracked(|row| row.as_ref().map(|row| row.id)) {
                store.dispatch(Message::TrackPressed(id));
            }
        })
        .children(
            COLUMNS
                .iter()
                .enumerate()
                .map(|(index, &(_, _, width))| cell(index, width)),
        )
}

//! The Playlists tab: the column of playlists on the left, as the
//! Settings tab's categories are; on the right the chosen one's header
//! (the artwork of its first track, its name and what it holds) over its
//! tracks, listed as the library's are.

use nsh_base::{alert_dialog, button, theme, ButtonElement};
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use super::format::total_duration;
use super::track_list::track_list;
use crate::app::listing::Listing;
use crate::app::playlists::Summary;
use crate::app::store::{store, Store};
use crate::app::Message;
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{caption, chrome_button, menu_row, strip, subject_menu, Icon, Tone};

/// The playlist column, its right edge included: as wide as the Settings
/// tab's categories.
const COLUMN_WIDTH: f32 = 160.0;
/// The column's title, 28 and its bottom line, as section titles are.
const TITLE_HEIGHT: f32 = 28.0 + CHROME_BORDER_WIDTH;
const ADD_ICON_SIZE: f32 = 10.0;
/// A playlist of the column, 36 and its bottom line.
const ROW_HEIGHT: f32 = 36.0 + CHROME_BORDER_WIDTH;
const ROW_PADDING: f32 = 12.0;
const ROW_GAP: f32 = 8.0;
const ROW_TEXT_SIZE: f32 = 11.0;
const COUNT_TEXT_SIZE: f32 = 10.0;
/// The fill of the chosen playlist: 10% of the accent.
const CHOSEN_TINT: f32 = 0.1;
/// The header: the artwork square, as wide as the column, its right line
/// outside; the block beside it inset 16 across, 14 down.
const ARTWORK_SIZE: f32 = COLUMN_WIDTH - CHROME_BORDER_WIDTH;
const HEADER_PADDING_X: f32 = 16.0;
const HEADER_PADDING_Y: f32 = 14.0;
const NAME_TEXT_SIZE: f32 = 20.0;
const STAT_GAP: f32 = 28.0;
const STAT_TEXT_SIZE: f32 = 12.0;
const TAG_TEXT_SIZE: f32 = 9.0;
const TAG_LETTER_SPACING: f32 = 0.8;
const PLACEHOLDER_TEXT_SIZE: f32 = 10.0;

/// A playlist of the column: what its row shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Entry {
    id: i64,
    name: SharedString,
    count: usize,
}

/// The Playlists tab: the playlists in a column on the left, and on the
/// right the chosen one.
pub(super) fn playlists() -> impl IntoElement {
    let store = store();
    let any = store.select(|model| !model.playlists.list().is_empty());
    let empty = store.select(|model| {
        model
            .playlists
            .chosen()
            .is_some_and(|playlist| playlist.tracks.is_empty())
    });

    let page = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .child(dynamic(move || {
            if !any.get() {
                return line(strip(
                    "No playlists yet. Press + to make one, or right-click a track and choose Add to Playlist.",
                ))
                .into_element();
            }
            view()
                .flex_1()
                .min_h(px(0.0))
                .child(header(store))
                .child(dynamic(move || {
                    if empty.get() {
                        line(strip(
                            "This playlist is empty. Right-click a track and choose Add to Playlist to put it here.",
                        ))
                        .into_element()
                    } else {
                        track_list(Listing::Playlist).into_element()
                    }
                }))
                .into_element()
        }));

    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(column(store))
        .child(page)
}

/// A strip with the stack's line under it.
fn line(strip: View) -> View {
    strip.border_b_1().border_color(move || theme().border)
}

/// The column of playlists on `card`, under its title whose cell makes
/// one, with its right edge where it meets the page; it scrolls when there
/// are more than it can show.
fn column(store: Store) -> View {
    let border = move || theme().border;
    let entries = store.select(|model| {
        model
            .playlists
            .list()
            .iter()
            .map(|playlist| Entry {
                id: playlist.id,
                name: playlist.name.clone().into(),
                count: playlist.tracks.len(),
            })
            .collect::<Vec<_>>()
    });

    let title = view()
        .w_full()
        .h(px(TITLE_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .border_b_1()
        .border_color(border)
        .role(AccessibilityRole::Header)
        .label("Playlists")
        .child(
            view()
                .flex_1()
                .h_full()
                .px(px(ROW_PADDING))
                .justify_center()
                .child(
                    caption("PLAYLISTS", TAG_TEXT_SIZE + 1.0)
                        .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            chrome_button(
                Icon::Add,
                ADD_ICON_SIZE,
                "New playlist",
                Tone::Quiet,
                move || {
                    store.dispatch(Message::NewPlaylistPressed(Vec::new()));
                },
            )
            .w(px(TITLE_HEIGHT))
            .border_l_1()
            .border_color(border),
        );

    view()
        .w(px(COLUMN_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(move || theme().card)
        .border_r_1()
        .border_color(border)
        .child(title)
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .role(AccessibilityRole::Tablist)
                    .label("Playlists")
                    .child(each(
                        entries,
                        |entry: &Entry| entry.clone(),
                        move |entry, _| playlist_row(store, entry.get_untracked()),
                    )),
            ),
        )
}

/// A playlist: its name and how many tracks it holds; chosen, the accent
/// on a tint of it. The secondary button opens the menu that renames or
/// deletes it.
fn playlist_row(store: Store, entry: Entry) -> impl IntoElement {
    let Entry { id, name, count } = entry;
    let chosen =
        store.select(move |model| model.playlists.chosen().map(|playlist| playlist.id) == Some(id));
    let tone = move |hovered: bool| {
        if chosen.get() {
            theme().primary
        } else if hovered {
            theme().foreground
        } else {
            theme().muted_foreground
        }
    };

    let row: ButtonElement = button()
        .group()
        .w_full()
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(ROW_PADDING))
        .border_b_1()
        .border_color(move || theme().border)
        .transition_colors()
        .style_when(chosen, |s| {
            s.bg(move || theme().primary.opacity(CHOSEN_TINT))
        })
        .hover(|s| s.bg(move || theme().background))
        .active(|s| s.bg(move || theme().border))
        .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
        .selected(chosen)
        .accessibility_label(format!("{name}, {count} tracks"))
        .on_press(move || store.dispatch(Message::PlaylistChosen(id)))
        .child(
            text(name.clone())
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(ROW_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || tone(false))
                .group_hover(move |s| s.text_color(move || tone(true))),
        )
        .child(
            text(count.to_string())
                .text_size(px(COUNT_TEXT_SIZE))
                .tabular_nums()
                .text_color(tokens::text_faint),
        );

    subject_menu(row)
        .child(menu_row("Rename\u{2026}", move || {
            store.dispatch(Message::RenamePlaylistPressed(id));
        }))
        .child(menu_row("Delete\u{2026}", move || {
            confirm_delete(store, id, name.clone());
        }))
}

/// Asks, in the system's alert, whether to delete the playlist.
fn confirm_delete(store: Store, id: i64, name: SharedString) {
    let answer = alert_dialog()
        .title(format!("Delete \u{201c}{name}\u{201d}?"))
        .description("The playlist goes; its tracks stay in the library.")
        .cancel("cancel", "Cancel")
        .destructive("delete", "Delete")
        .show();
    spawn(async move {
        let deleted = answer
            .await
            .is_ok_and(|answer| answer.action.as_deref() == Some("delete"));
        if deleted {
            store.dispatch(Message::DeletePlaylistConfirmed(id));
        }
    });
}

/// The artwork of the playlist's first track, and beside it its name over
/// what it holds: how many tracks, the years they span and how long they
/// play. With its bottom line.
fn header(store: Store) -> View {
    let border = move || theme().border;
    let summary = store.select(|model| model.playlist_summary());
    let artwork = store.select(|model| model.playlists.art().cloned().unwrap_or_default());
    let shown = computed(move || artwork.with(|uri| !uri.is_empty()));
    let read = move |of: fn(&Summary) -> SharedString| {
        move || summary.with(|summary| summary.as_ref().map(of).unwrap_or_default())
    };

    let art = view()
        .w(px(ARTWORK_SIZE + CHROME_BORDER_WIDTH))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .border_r_1()
        .border_color(border)
        .child(
            when(shown, move || {
                image(move || ImageSrc::from(artwork.get()))
                    .w(px(ARTWORK_SIZE))
                    .h(px(ARTWORK_SIZE))
                    .object_fit(ResizeMode::Cover)
                    .label("Playlist artwork")
                    .into_element()
            })
            .otherwise(move || {
                text("No artwork")
                    .text_size(px(PLACEHOLDER_TEXT_SIZE))
                    .text_color(tokens::text_faint)
                    .into_element()
            }),
        );

    let identity = view().w_full().gap(px(4.0)).child(tag("PLAYLIST")).child(
        text(read(|summary| summary.name.clone()))
            .text_size(px(NAME_TEXT_SIZE))
            .font_weight(FontWeight::SEMIBOLD)
            .number_of_lines(1)
            .text_color(move || theme().foreground),
    );
    let stats = view()
        .w_full()
        .flex_row()
        .flex_wrap()
        .gap(px(STAT_GAP))
        .child(stat("TRACKS", read(|summary| tracks(summary).into())))
        .child(stat("YEARS", read(|summary| years(summary).into())))
        .child(stat(
            "DURATION",
            read(|summary| total_duration(summary.duration_ms).into()),
        ));

    view()
        .w_full()
        .h(px(ARTWORK_SIZE + CHROME_BORDER_WIDTH))
        .flex_shrink_0()
        .flex_row()
        .border_b_1()
        .border_color(border)
        .child(art)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .px(px(HEADER_PADDING_X))
                .py(px(HEADER_PADDING_Y))
                .justify_between()
                .selectable(true)
                .child(identity)
                .child(stats),
        )
}

/// A 9pt tag, semibold and spaced, muted.
fn tag(label: &'static str) -> Text {
    caption(label, TAG_TEXT_SIZE)
        .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
        .text_color(move || theme().muted_foreground)
}

/// A figure of the header: its tag over it.
fn stat(label: &'static str, value: impl Fn() -> SharedString + 'static) -> View {
    view()
        .gap(px(4.0))
        .role(AccessibilityRole::Text)
        .child(tag(label))
        .child(
            text(value)
                .text_size(px(STAT_TEXT_SIZE))
                .number_of_lines(1)
                .tabular_nums()
                .text_color(move || theme().foreground),
        )
}

/// "12", or "12 · 2 not found" when files of it are not there.
fn tracks(summary: &Summary) -> String {
    match summary.missing {
        0 => summary.tracks.to_string(),
        missing => format!("{} \u{b7} {missing} not found", summary.tracks),
    }
}

/// "1994 – 2021", "2021", or a dash when none of it says.
fn years(summary: &Summary) -> String {
    match summary.years {
        Some((first, last)) if first == last => first.to_string(),
        Some((first, last)) => format!("{first} \u{2013} {last}"),
        None => "\u{2014}".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(tracks: usize, missing: usize, years: Option<(u32, u32)>) -> Summary {
        Summary {
            name: "Mix".into(),
            tracks,
            missing,
            years,
            duration_ms: 0,
        }
    }

    #[test]
    fn the_header_figures_read_as_people_write_them() {
        assert_eq!(tracks(&summary(12, 0, None)), "12");
        assert_eq!(tracks(&summary(12, 2, None)), "12 \u{b7} 2 not found");
        assert_eq!(years(&summary(1, 0, Some((2021, 2021)))), "2021");
        assert_eq!(
            years(&summary(1, 0, Some((1994, 2021)))),
            "1994 \u{2013} 2021"
        );
        assert_eq!(years(&summary(1, 0, None)), "\u{2014}");
    }
}

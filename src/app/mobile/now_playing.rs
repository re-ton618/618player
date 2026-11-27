//! The phone's Now Playing sheet, under the tab group: no header, no
//! tabs. Shut, a bar of the artwork, the track's name and who made it, and
//! the cell that puts it in a playlist; open, the artwork as large as its
//! room lets it be over the name. A tap on the artwork blurs it under the
//! lyrics; a double tap on them takes them away.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::geometry::Percentage;
use nsh_core::prelude::*;

use crate::app::lyrics::Shown;
use crate::app::store::{store, Store};
use crate::app::view::Details;
use crate::app::Message;
use crate::library::Lyrics;
use crate::theme as tokens;
use crate::ui::{bar_menu, caption, chrome_glyph, menu_divider, menu_row, strip, Icon, Tone};

/// The shut sheet, its lines included.
pub(super) const BAR_HEIGHT: f32 = 64.0;
/// Its artwork fills it inside its lines.
const BAR_ARTWORK_SIZE: f32 = BAR_HEIGHT - 2.0;
const BAR_PADDING: f32 = 14.0;
const BAR_GAP: f32 = 2.0;
const BAR_TITLE_TEXT_SIZE: f32 = 15.0;
const BAR_ARTIST_TEXT_SIZE: f32 = 13.0;
/// The add to playlist cell: a touch target.
const ADD_WIDTH: f32 = 56.0;
const ADD_ICON_SIZE: f32 = 18.0;
const MENU_ROW_HEIGHT: f32 = 44.0;
/// The track's name, who made it, its album and its format: this tall,
/// its bottom line included.
const IDENTITY_HEIGHT: f32 = 116.0;
const IDENTITY_PADDING: f32 = 16.0;
const IDENTITY_GAP: f32 = 4.0;
const TITLE_TEXT_SIZE: f32 = 20.0;
const BYLINE_TEXT_SIZE: f32 = 15.0;
const TAG_TEXT_SIZE: f32 = 10.0;
const TAG_LETTER_SPACING: f32 = 0.8;
const PLACEHOLDER_TEXT_SIZE: f32 = 12.0;
/// The smallest the artwork is over the name; with less room it goes
/// beside it.
const SMALLEST_OVER: f32 = 140.0;
/// The most of the width the artwork takes beside the name.
const WIDEST_BESIDE: f32 = 0.45;
/// The artwork under the lyrics: blurred this much, and veiled by this
/// much of the background.
const LYRICS_BLUR: f32 = 24.0;
const LYRICS_VEIL: f32 = 0.6;
/// A line of the lyrics: 17pt, inset 20 across and 6 down.
const LINE_TEXT_SIZE: f32 = 17.0;
const LINE_PADDING_X: f32 = 20.0;
const LINE_PADDING_Y: f32 = 6.0;
const LINE_HEIGHT: f32 = 34.0;
const LIST_PADDING_Y: f32 = 16.0;
const NOTICE_TEXT_SIZE: f32 = 14.0;
/// Where the line being sung is kept: this far down the lyrics.
const CURRENT_LINE_AT: f32 = 0.4;
/// What stands for a line with no words in timed lyrics: a pause.
const PAUSE: &str = "\u{266a}";
/// Two taps this close are a double tap; closer than this, one tap told
/// twice (by a line of the lyrics and by the artwork under it).
const DOUBLE_TAP: Duration = Duration::from_millis(350);
const SAME_TAP: Duration = Duration::from_millis(40);

/// How the open sheet is laid out in the room it has.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fit {
    /// The artwork is beside the name, not over it.
    beside: bool,
    /// A side of the artwork's square.
    side: f32,
}

impl Fit {
    /// Over the name, the artwork is as wide as the sheet, or as tall as
    /// the name leaves it. Where that is too small to make out, it goes
    /// beside the name, as tall as the sheet.
    fn of(width: f32, height: f32) -> Self {
        let over = width.min(height - IDENTITY_HEIGHT);
        if over >= SMALLEST_OVER {
            return Self {
                beside: false,
                side: over,
            };
        }
        Self {
            beside: true,
            side: height.min(width * WIDEST_BESIDE).max(0.0),
        }
    }
}

/// The current track's artwork, empty without one.
fn artwork(store: Store) -> Memo<SharedString> {
    store.select(|model| {
        model
            .current_track()
            .and(model.playback.artwork_image())
            .cloned()
            .unwrap_or_default()
    })
}

/// The shut sheet: the artwork, the track's name and who made it, and the
/// add to playlist cell. A tap on the rest of it opens the sheet; `drag`
/// makes it the handle that drags the sheet open.
pub(super) fn bar(drag: impl Fn(View) -> View, open: impl Fn() + 'static) -> impl IntoElement {
    let store = store();
    let details = store.select(|model| model.current_track().map(Details::of));
    let track = store.select(|model| model.current_track().map(|track| track.id));
    let artwork = artwork(store);
    let has_artwork = computed(move || artwork.with(|uri| !uri.is_empty()));
    let border = move || theme().border;

    let art = view()
        .w(px(BAR_ARTWORK_SIZE + 1.0))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(border)
        .child(when(has_artwork, move || {
            image(move || ImageSrc::from(artwork.get()))
                .w(px(BAR_ARTWORK_SIZE))
                .h(px(BAR_ARTWORK_SIZE))
                .object_fit(ResizeMode::Cover)
                .label("Album artwork")
        }));

    let names = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .justify_center()
        .gap(px(BAR_GAP))
        .px(px(BAR_PADDING))
        .child(dynamic(move || match details.get() {
            Some(details) => {
                let artist = details
                    .artist
                    .clone()
                    .unwrap_or_else(|| SharedString::from("Unknown artist"));
                view()
                    .child(
                        text(details.title.clone())
                            .text_size(px(BAR_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .number_of_lines(1)
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        text(artist)
                            .text_size(px(BAR_ARTIST_TEXT_SIZE))
                            .number_of_lines(1)
                            .text_color(move || theme().muted_foreground),
                    )
                    .into_element()
            }
            None => text("Nothing is playing.")
                .text_size(px(BAR_ARTIST_TEXT_SIZE))
                .text_color(move || theme().muted_foreground)
                .into_element(),
        }));

    let opener = drag(
        view()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex_row()
            .role(AccessibilityRole::Button)
            .label("Show Now Playing")
            .on_press(open)
            .child(art)
            .child(names),
    );

    view()
        .h_full()
        .flex_row()
        .bg(move || theme().card)
        .child(opener)
        .child(when(
            move || track.get().is_some(),
            move || add_to_playlist(store, track),
        ))
}

/// The cell with its left edge, and its menu that puts the current track
/// in a new playlist or one there is.
fn add_to_playlist(store: Store, track: Memo<Option<i64>>) -> impl IntoElement {
    let playlists = store.select(|model| {
        model
            .playlists
            .list()
            .iter()
            .map(|playlist| (playlist.id, SharedString::from(playlist.name.clone())))
            .collect::<Vec<_>>()
    });
    // The track playing when a row is chosen, not when the menu was made.
    let ask = move |message: &dyn Fn(i64) -> Message| {
        if let Some(id) = track.get_untracked() {
            store.dispatch(message(id));
        }
    };

    bar_menu(
        chrome_glyph(Icon::Add, ADD_ICON_SIZE, "Add to playlist", Tone::Quiet)
            .w(px(ADD_WIDTH))
            .h_full()
            .border_l_1()
            .border_color(move || theme().border),
    )
    .child(
        menu_row("New Playlist\u{2026}", move || {
            ask(&|id| Message::NewPlaylistPressed(vec![id]))
        })
        .h(px(MENU_ROW_HEIGHT)),
    )
    .child(when(
        move || playlists.with(|list| !list.is_empty()),
        menu_divider,
    ))
    .child(each(
        playlists,
        |entry: &(i64, SharedString)| entry.clone(),
        move |entry, _| {
            let (playlist, name) = entry.get_untracked();
            menu_row(name, move || {
                ask(&|track| Message::AddToPlaylistPressed { playlist, track })
            })
            .h(px(MENU_ROW_HEIGHT))
        },
    ))
}

/// The open sheet, fitted to its room: tall, the artwork across it over
/// the name; short (dragged small, or the phone on its side), the artwork
/// beside the name. `drag` makes the name the handle that drags the sheet
/// shut. With nothing playing, the empty artwork and a strip that says so.
pub(super) fn now_playing(drag: impl Fn(View) -> View + Copy + 'static) -> impl IntoElement {
    let store = store();
    let details = store.select(|model| model.current_track().map(Details::of));
    let artwork = artwork(store);
    let room = signal((0.0_f32, 0.0_f32));
    let fit = computed(move || {
        let (width, height) = room.get();
        Fit::of(width, height)
    });
    let beside = computed(move || fit.get().beside);
    let border = move || theme().border;

    // A tap brings the lyrics over the artwork; a double tap on them takes
    // them away.
    let lyrics_shown = signal(false);
    let last_tap = signal(None::<Instant>);
    let tapped = move || {
        if !lyrics_shown.get_untracked() {
            lyrics_shown.set(true);
            last_tap.set(None);
            return;
        }
        let now = Instant::now();
        match last_tap
            .get_untracked()
            .map(|before| now.duration_since(before))
        {
            Some(since) if since < SAME_TAP => {}
            Some(since) if since <= DOUBLE_TAP => {
                lyrics_shown.set(false);
                last_tap.set(None);
            }
            _ => last_tap.set(Some(now)),
        }
    };

    let square = view()
        .overflow_hidden()
        .role(AccessibilityRole::Button)
        .label(move || {
            if lyrics_shown.get() {
                "Lyrics, double tap to hide"
            } else {
                "Album artwork, tap for lyrics"
            }
        })
        .on_press(tapped)
        .style_with(move |style| {
            let side = fit.get().side;
            style.w(px(side)).h(px(side))
        })
        .child(
            view()
                .size_full()
                .items_center()
                .justify_center()
                .style_when(
                    move || lyrics_shown.get(),
                    |style| style.blur(px(LYRICS_BLUR)),
                )
                .child(picture(artwork)),
        )
        .child(when(
            move || lyrics_shown.get(),
            move || lyrics_over(store, tapped),
        ));

    let art = view()
        .flex_shrink_0()
        .items_center()
        .border_color(border)
        .style_with(move |style| {
            let Fit { beside, side } = fit.get();
            if beside {
                style.w(px(side + 1.0)).h_full().border_r_1()
            } else {
                style.w_full().h(px(side + 1.0)).border_b_1()
            }
        })
        .child(square);

    let about = view()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .style_with(move |style| {
            if beside.get() {
                style.flex_1().h_full()
            } else {
                style.flex_1().w_full()
            }
        })
        .child(dynamic(move || match details.get() {
            Some(details) => drag(identity(&details)).into_element(),
            None => drag(
                strip("Nothing is playing.")
                    .border_b_1()
                    .border_color(border),
            )
            .into_element(),
        }));

    view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .on_layout(move |frame: &Bounds<Points>| {
            room.set((frame.size.width.0, frame.size.height.0));
        })
        .style_with(move |style| {
            if beside.get() {
                style.flex_row()
            } else {
                style.flex_col()
            }
        })
        .child(art)
        .child(about)
}

/// The artwork filling its square; a placeholder when there is none.
fn picture(artwork: Memo<SharedString>) -> impl IntoElement {
    let has = computed(move || artwork.with(|uri| !uri.is_empty()));
    when(has, move || {
        image(move || ImageSrc::from(artwork.get()))
            .size_full()
            .object_fit(ResizeMode::Cover)
            .label("Album artwork")
            .into_element()
    })
    .otherwise(move || {
        text("No artwork")
            .text_size(px(PLACEHOLDER_TEXT_SIZE))
            .text_color(tokens::text_faint)
            .into_element()
    })
}

/// The lyrics over the blurred artwork, veiled so they read: timed ones
/// follow the track, the line being sung lit and kept in view.
fn lyrics_over(store: Store, tapped: impl Fn() + Copy + 'static) -> impl IntoElement {
    let shown = store.select(|model| model.lyrics.shown());
    let notice = |said: &'static str| {
        view()
            .flex_1()
            .justify_center()
            .px(px(LINE_PADDING_X))
            .child(
                text(said)
                    .text_size(px(NOTICE_TEXT_SIZE))
                    .text_center()
                    .text_color(move || theme().foreground),
            )
            .into_element()
    };
    view()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bg(move || theme().background.opacity(LYRICS_VEIL))
        .child(dynamic(move || match shown.get() {
            Shown::Nothing => notice("Nothing is playing."),
            Shown::Reading => notice("Reading lyrics\u{2026}"),
            Shown::Missing => notice("No lyrics for this track."),
            Shown::Ready(lyrics) => lines(store, lyrics, tapped).into_element(),
        }))
}

/// The lines in a list. A tap on a line is a tap on the artwork (a double
/// tap hides them): the list keeps its taps from the square around it.
fn lines(
    store: Store,
    lyrics: Arc<Lyrics>,
    tapped: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let list = element_ref();
    let current = store.select(|model| model.lyrics.line_at(model.playback.position_ms()));
    let synced = lyrics.synced;
    if synced {
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
        move |at| {
            let lyrics = Arc::clone(&lyrics);
            let words = move || {
                let text = lyrics
                    .lines
                    .get(at.index())
                    .map_or("", |line| line.text.as_str());
                let text = if text.is_empty() && synced {
                    PAUSE
                } else {
                    text
                };
                SharedString::from(text.to_owned())
            };
            let lit = move || !synced || current.get() == Some(at.index());
            view()
                .w_full()
                .min_h(px(LINE_HEIGHT))
                .justify_center()
                .px(px(LINE_PADDING_X))
                .py(px(LINE_PADDING_Y))
                .on_press(tapped)
                .child(
                    text(words)
                        .text_size(px(LINE_TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_center()
                        .text_color(move || {
                            if lit() {
                                theme().foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                )
        },
    )
    .estimated_item_size(px(LINE_HEIGHT))
    .py(px(LIST_PADDING_Y))
    .flex_1()
    .element_ref(&list)
}

/// The track's name, who made it, its album and its format, a line each,
/// over its bottom line.
fn identity(details: &Details) -> View {
    let line =
        |value: SharedString, size: f32, weight: FontWeight, color: fn() -> AdaptiveColor| {
            text(value)
                .text_size(px(size))
                .font_weight(weight)
                .number_of_lines(1)
                .text_color(color)
        };
    let muted: fn() -> AdaptiveColor = || theme().muted_foreground;
    let artist = details
        .artist
        .clone()
        .unwrap_or_else(|| SharedString::from("Unknown artist"));
    let format = details.format.clone();

    view()
        .w_full()
        .h(px(IDENTITY_HEIGHT))
        .flex_shrink_0()
        .justify_center()
        .overflow_hidden()
        .px(px(IDENTITY_PADDING))
        .gap(px(IDENTITY_GAP))
        .border_b_1()
        .border_color(move || theme().border)
        .child(line(
            details.title.clone(),
            TITLE_TEXT_SIZE,
            FontWeight::SEMIBOLD,
            || theme().foreground,
        ))
        .child(line(artist, BYLINE_TEXT_SIZE, FontWeight::NORMAL, muted))
        .when_some(details.album.clone(), |lines, album| {
            lines.child(line(album, BYLINE_TEXT_SIZE, FontWeight::NORMAL, muted))
        })
        .when_some((!format.is_empty()).then_some(format), |lines, format| {
            lines.child(
                caption(format, TAG_TEXT_SIZE)
                    .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
                    .text_color(muted),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{Fit, IDENTITY_HEIGHT};

    #[test]
    fn the_artwork_is_as_large_as_the_room_lets_it_be() {
        // A phone held up: across the sheet, the name under it.
        let tall = Fit::of(400.0, 560.0);
        assert_eq!((tall.beside, tall.side), (false, 400.0));
        // Dragged shorter: it shrinks to what the name leaves it.
        let shorter = Fit::of(400.0, 400.0);
        assert_eq!(
            (shorter.beside, shorter.side),
            (false, 400.0 - IDENTITY_HEIGHT)
        );
        // Too short for that: beside the name, as tall as the sheet.
        let short = Fit::of(400.0, 150.0);
        assert_eq!((short.beside, short.side), (true, 150.0));
        // On its side: beside the name.
        let wide = Fit::of(900.0, 230.0);
        assert_eq!((wide.beside, wide.side), (true, 230.0));
        // Never more than its share of the width, nor less than nothing.
        assert_eq!(Fit::of(400.0, 250.0).side, 180.0);
        assert_eq!(Fit::of(0.0, 0.0).side, 0.0);
    }
}

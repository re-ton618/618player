use nsh_base::{SliderValue, slider, theme};
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use super::{BAR_CELL_WIDTH, BAR_ICON_SIZE, PLAYBACK_BAR_HEIGHT, volume_control};
use crate::app::Message;
use crate::app::playback::Transport;
use crate::app::store::{Store, store};
use crate::theme::CHROME_BORDER_WIDTH;
use crate::ui::{Icon, Tone, caption, chrome_button};

/// The artwork fills the bar's height inside its border.
const ARTWORK_SIZE: f32 = PLAYBACK_BAR_HEIGHT - 2.0 * CHROME_BORDER_WIDTH;
/// The leading group, and the trailing group sized to match so the
/// progress stays centered: artwork, its edge, three transport cells.
const SIDE_REGION_WIDTH: f32 = ARTWORK_SIZE + CHROME_BORDER_WIDTH + 3.0 * BAR_CELL_WIDTH;
const PROGRESS_PADDING: f32 = 10.0;
const PROGRESS_GAP: f32 = 10.0;
const PROGRESS_TRACK_HEIGHT: f32 = 3.0;
/// The progress takes presses and drags this far around its line.
const PROGRESS_HIT_HEIGHT: f32 = 16.0;
const PROGRESS_THUMB_WIDTH: f32 = 2.0;
const PROGRESS_THUMB_HEIGHT: f32 = 11.0;
/// What a drag or an arrow key moves the position by: a second.
const PROGRESS_STEP_MS: f64 = 1_000.0;
const TIME_TEXT_SIZE: f32 = 10.0;

/// The bar under the workspace: artwork and transport on the left, the
/// progress in the middle, the volume on the right. Leading cells own
/// their right edge, the volume its left.
pub(super) fn playback_bar() -> impl IntoElement {
    let store = store();
    let border = move || theme().border;
    let transport = move |icon, label, tone, message: Message| {
        chrome_button(icon, BAR_ICON_SIZE, label, tone, move || {
            store.dispatch(message.clone())
        })
        .w(px(BAR_CELL_WIDTH))
        .border_r_1()
        .border_color(border)
    };
    let playing = store.select(|model| model.playback.transport() == Transport::Playing);

    let left = view()
        .w(px(SIDE_REGION_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex_row()
        .child(artwork_cell())
        .child(transport(
            Icon::Previous,
            "Previous",
            Tone::Quiet,
            Message::PreviousPressed,
        ))
        .child(
            when(playing, move || {
                transport(
                    Icon::Pause,
                    "Pause",
                    Tone::Accent,
                    Message::PlayPausePressed,
                )
            })
            .otherwise(move || {
                transport(Icon::Play, "Play", Tone::Accent, Message::PlayPausePressed)
            }),
        )
        .child(transport(
            Icon::Next,
            "Next",
            Tone::Quiet,
            Message::NextPressed,
        ));

    let right = view()
        .w(px(SIDE_REGION_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex_row()
        .justify_end()
        .child(volume_control::volume_control(BAR_CELL_WIDTH));

    view()
        .h(px(PLAYBACK_BAR_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_1()
        .border_color(border)
        .child(left)
        .child(progress(store))
        .child(right)
}

/// The current track's position, a line the user drags to move it, and
/// its length.
fn progress(store: Store) -> impl IntoElement {
    let position = store.select(|model| model.playback.position_ms());
    let duration = store.select(|model| model.playback.duration_ms());
    let loaded = store.select(|model| {
        model.playback.transport() != Transport::Stopped && model.playback.duration_ms().is_some()
    });
    // Where a drag is, until it is let go: the player moves once, then.
    let dragged = signal(None::<u64>);
    let shown = move || dragged.get().unwrap_or_else(|| position.get());
    let milliseconds = |value: &SliderValue| match value {
        SliderValue::Single(value) => Some(value.max(0.0).round() as u64),
        SliderValue::Range(..) => None,
    };

    let muted = move || theme().muted_foreground;
    let line = slider()
        .min(0.0)
        .max(move || duration.get().map_or(1.0, |end| end.max(1) as f64))
        .step(PROGRESS_STEP_MS)
        .value(move || Some(SliderValue::Single(shown() as f64)))
        .disabled(move || !loaded.get())
        .on_change(move |value: &SliderValue| dragged.set(milliseconds(value)))
        .on_change_end(move |value: &SliderValue| {
            if let Some(position) = milliseconds(value) {
                store.dispatch(Message::PositionChanged(position));
            }
            dragged.set(None);
        })
        .accessibility_label("Position")
        .flex_1()
        .min_w(px(0.0))
        .track_style(|s| s.h(px(PROGRESS_HIT_HEIGHT)).justify_center())
        .indicator_style(|s| s.h(px(PROGRESS_TRACK_HEIGHT)).bg(move || theme().border))
        .range_style(|s| s.bg(move || theme().primary))
        .thumb_class("bg-primary hover:bg-foreground active:bg-foreground disabled:opacity-0")
        .thumb_style(|s| s.w(px(PROGRESS_THUMB_WIDTH)).h(px(PROGRESS_THUMB_HEIGHT)));

    view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .items_center()
        .gap(px(PROGRESS_GAP))
        .px(px(PROGRESS_PADDING))
        .child(
            caption(move || SharedString::from(clock(shown())), TIME_TEXT_SIZE)
                .text_color(muted)
                .tabular_nums(),
        )
        .child(line)
        .child(
            caption(
                move || {
                    duration
                        .get()
                        .map_or_else(|| SharedString::from("--:--"), |end| clock(end).into())
                },
                TIME_TEXT_SIZE,
            )
            .text_color(muted)
            .tabular_nums(),
        )
}

/// Milliseconds as a clock reads them: `m:ss`, hours in front once there
/// are any.
fn clock(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let (hours, minutes, seconds) = (seconds / 3_600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The current track's artwork, or an empty square, with its right edge.
fn artwork_cell() -> impl IntoElement {
    let store = store();
    let artwork = store.select(|model| {
        model
            .current_track()
            .and(model.playback.artwork_image())
            .cloned()
            .unwrap_or_default()
    });
    let shown = computed(move || artwork.with(|uri| !uri.is_empty()));
    view()
        .w(px(ARTWORK_SIZE + CHROME_BORDER_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(move || theme().background)
        .border_r_1()
        .border_color(move || theme().border)
        .child(when(shown, move || {
            image(move || ImageSrc::from(artwork.get()))
                .w(px(ARTWORK_SIZE))
                .h(px(ARTWORK_SIZE))
                .object_fit(ResizeMode::Cover)
                .label("Album artwork")
        }))
}

#[cfg(test)]
mod tests {
    use super::clock;

    #[test]
    fn clock_reads_minutes_and_hours() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(59_999), "0:59");
        assert_eq!(clock(61_000), "1:01");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}

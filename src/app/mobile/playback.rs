use nsh_base::theme;
use nsh_core::prelude::*;

use super::{BAR_CELL_SIZE, BAR_ICON_SIZE};
use crate::app::playback::Transport;
use crate::app::queue::Repeat;
use crate::app::store::store;
use crate::app::view::progress;
use crate::app::Message;
use crate::ui::{chrome_button, Icon, Tone};

/// The progress row and the controls row, each with its line.
const PROGRESS_ROW_HEIGHT: f32 = 44.0;
const CONTROLS_ROW_HEIGHT: f32 = 56.0;
/// The progress takes presses this far around its line: a fingertip.
const PROGRESS_HIT_HEIGHT: f32 = 32.0;
const TIME_TEXT_SIZE: f32 = 13.0;
/// Play and pause, a size up from the other controls.
const PLAY_ICON_SIZE: f32 = 24.0;

/// The bar under the workspace, two rows: the progress across the top,
/// and under it shuffle on the left, the transport in the middle, repeat
/// on the right. No artwork, no volume: the phone has its own keys.
pub(super) fn playback_bar() -> impl IntoElement {
    let store = store();
    let border = move || theme().border;
    let cell = move |icon, icon_size, label, tone, message: Message| {
        chrome_button(icon, icon_size, label, tone, move || {
            store.dispatch(message.clone())
        })
        .w(px(BAR_CELL_SIZE))
    };
    let playing = store.select(|model| model.playback.transport() == Transport::Playing);
    let shuffled = store.select(|model| model.queue.shuffled());
    let repeat = store.select(|model| model.queue.repeat());

    let progress_row = view()
        .h(px(PROGRESS_ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .border_b_1()
        .border_color(border)
        .child(progress(store, PROGRESS_HIT_HEIGHT, TIME_TEXT_SIZE));

    let shuffle = when(shuffled, move || {
        cell(
            Icon::Shuffle,
            BAR_ICON_SIZE,
            "Shuffle, on",
            Tone::Accent,
            Message::ShufflePressed,
        )
        .border_r_1()
        .border_color(border)
    })
    .otherwise(move || {
        cell(
            Icon::Shuffle,
            BAR_ICON_SIZE,
            "Shuffle, off",
            Tone::Quiet,
            Message::ShufflePressed,
        )
        .border_r_1()
        .border_color(border)
    });

    let transport = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .justify_center()
        .child(
            cell(
                Icon::Previous,
                BAR_ICON_SIZE,
                "Previous",
                Tone::Quiet,
                Message::PreviousPressed,
            )
            .border_l_1()
            .border_r_1()
            .border_color(border),
        )
        .child(
            when(playing, move || {
                cell(
                    Icon::Pause,
                    PLAY_ICON_SIZE,
                    "Pause",
                    Tone::Accent,
                    Message::PlayPausePressed,
                )
                .border_r_1()
                .border_color(border)
            })
            .otherwise(move || {
                cell(
                    Icon::Play,
                    PLAY_ICON_SIZE,
                    "Play",
                    Tone::Accent,
                    Message::PlayPausePressed,
                )
                .border_r_1()
                .border_color(border)
            }),
        )
        .child(
            cell(
                Icon::Next,
                BAR_ICON_SIZE,
                "Next",
                Tone::Quiet,
                Message::NextPressed,
            )
            .border_r_1()
            .border_color(border),
        );

    let repeat = dynamic(move || {
        let (icon, label, tone) = match repeat.get() {
            Repeat::Off => (Icon::Repeat, "Repeat, off", Tone::Quiet),
            Repeat::All => (Icon::Repeat, "Repeat, all", Tone::Accent),
            Repeat::One => (Icon::RepeatOne, "Repeat, one", Tone::Accent),
        };
        cell(icon, BAR_ICON_SIZE, label, tone, Message::RepeatPressed)
            .border_l_1()
            .border_color(border)
            .into_element()
    });

    let controls_row = view()
        .h(px(CONTROLS_ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .child(shuffle)
        .child(transport)
        .child(repeat);

    view()
        .flex_shrink_0()
        .bg(move || theme().card)
        .border_1()
        .border_color(border)
        .child(progress_row)
        .child(controls_row)
}

//! Playback: what the queue does (shuffle, repeat), the volume, when a
//! track counts as listened to, and whether it shows on Discord.

use nsh_base::theme;
use nsh_core::prelude::*;

use super::CELL_PADDING;
use crate::app::queue::Repeat;
use crate::app::store::Store;
use crate::app::Message;
use crate::theme::CHROME_BORDER_WIDTH;
use crate::ui::{self, label_column_row, section_title};

const VOLUME_TEXT_SIZE: f32 = 11.0;
/// A − or + cell of the volume: square to its row.
const STEPPER_WIDTH: f32 = ui::ROW_HEIGHT + CHROME_BORDER_WIDTH;
/// How far a press of − or + moves the volume, in percent.
const VOLUME_STEP: u8 = 5;
const MAX_VOLUME: u8 = 100;
/// How far a press of − or + moves the listening threshold, in percent.
const LISTEN_STEP: u8 = 5;
const MIN_LISTEN_PERCENT: u8 = 5;
const MAX_LISTEN_PERCENT: u8 = 100;

/// Playback: what the queue does (shuffle, repeat), the volume, and when
/// a track counts as listened to.
pub(super) fn page(store: Store) -> View {
    let border = move || theme().border;
    let shuffled = store.select(|model| model.queue.shuffled());
    let repeat = store.select(|model| model.queue.repeat());
    let volume = store.select(|model| model.playback.volume());
    let shown_volume =
        store.select(|model| SharedString::from(format!("{}%", model.playback.volume())));
    let listen_percent = store.select(|model| model.playback.listen_percent());
    let shown_listen_percent = store.select(|model| {
        SharedString::from(format!("{}% of the track", model.playback.listen_percent()))
    });

    let shuffle = move |label, on: bool| {
        ui::choice(
            label,
            move || shuffled.get() == on,
            move || store.dispatch(Message::ShuffleChosen(on)),
        )
        .border_l_1()
        .border_color(border)
    };
    let mode = move |label, mode: Repeat| {
        ui::choice(
            label,
            move || repeat.get() == mode,
            move || store.dispatch(Message::RepeatChosen(mode)),
        )
        .border_l_1()
        .border_color(border)
    };
    let step = move |label, change: fn(u8) -> u8| {
        ui::button(label, move || {
            store.dispatch(Message::VolumeChanged(change(volume.get_untracked())))
        })
        .w(px(STEPPER_WIDTH))
        .justify_center()
        .border_l_1()
        .border_color(border)
    };
    #[cfg(not(target_os = "android"))]
    let discord_presence = store.select(|model| model.discord_presence);
    #[cfg(not(target_os = "android"))]
    let presence = move |label, on: bool| {
        ui::choice(
            label,
            move || discord_presence.get() == on,
            move || store.dispatch(Message::DiscordPresenceChosen(on)),
        )
        .border_l_1()
        .border_color(border)
    };
    let listen_step = move |label, change: fn(u8) -> u8| {
        ui::button(label, move || {
            store.dispatch(Message::ListenPercentChosen(change(
                listen_percent.get_untracked(),
            )))
        })
        .w(px(STEPPER_WIDTH))
        .justify_center()
        .border_l_1()
        .border_color(border)
    };
    let value = move |shown: Memo<SharedString>| {
        view()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(CELL_PADDING))
            .justify_center()
            .border_l_1()
            .border_color(border)
            .child(text(shown).text_size(px(VOLUME_TEXT_SIZE)).tabular_nums())
    };

    let page = view()
        .w_full()
        .child(section_title("Queue"))
        .child(
            label_column_row("Shuffle")
                .child(shuffle("Off", false))
                .child(shuffle("On", true)),
        )
        .child(
            label_column_row("Repeat")
                .child(mode("Off", Repeat::Off))
                .child(mode("All", Repeat::All))
                .child(mode("One", Repeat::One)),
        )
        .child(section_title("Output"))
        .child(
            label_column_row("Volume")
                .child(value(shown_volume))
                .child(step("\u{2212}", |volume| {
                    volume.saturating_sub(VOLUME_STEP)
                }))
                .child(step("+", |volume| {
                    volume.saturating_add(VOLUME_STEP).min(MAX_VOLUME)
                })),
        )
        .child(section_title("Play count"))
        .child(
            label_column_row("Counts as played")
                .child(value(shown_listen_percent))
                .child(listen_step("\u{2212}", |percent| {
                    percent.saturating_sub(LISTEN_STEP).max(MIN_LISTEN_PERCENT)
                }))
                .child(listen_step("+", |percent| {
                    percent.saturating_add(LISTEN_STEP).min(MAX_LISTEN_PERCENT)
                })),
        );
    #[cfg(not(target_os = "android"))]
    let page = page.child(section_title("Discord")).child(
        label_column_row("Show what plays")
            .child(presence("Off", false))
            .child(presence("On", true)),
    );
    page
}

//! The system's media controls (Android's notification, lock screen and
//! headset buttons; Windows' flyout, lock screen and media keys,
//! [`crate::platform::media`]): they show what the model
//! says plays, and what the user asks of them comes back as the messages
//! the app's own buttons send.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use nsh_core::prelude::*;
use tokio::sync::mpsc::unbounded_channel;

use super::playback::Transport;
use super::{Message, Model, Store};
use crate::audio;
use crate::platform::media::{self, NowPlaying, Remote};

/// How far the position may be from where the system reckons it is (it
/// moves it on by itself while playing) before it is told again: a seek,
/// or a stall.
const DRIFT_MS: u64 = 1_500;

/// Shows what plays in the system's controls, and has them drive `store`.
/// `player` is the engine's ([`audio::Player::sender`]).
pub(super) fn bind(store: Store, player: std::sync::mpsc::Sender<audio::Command>) {
    let transport = store.select(|model| model.playback.transport());
    let (remotes, mut asked) = unbounded_channel();
    media::listen(move |remote| {
        if remote == Remote::Stop {
            // Straight to the engine: the store may have gone with the UI.
            player.send(audio::Command::Pause).ok();
        }
        remotes.send(remote).ok();
    });
    spawn(async move {
        while let Some(remote) = asked.recv().await {
            if let Some(message) = message(remote, transport.get_untracked()) {
                store.dispatch(message);
            }
            if remote == Remote::Stop {
                // Where it stopped goes into the session, as a closing window's.
                store.closing();
            }
        }
    });

    let shown = store.select(shown);
    let position = store.select(|model| model.playback.position_ms());
    // What the system was told last, where it was then, and when.
    let told: Rc<RefCell<Option<(NowPlaying, u64, Instant)>>> = Rc::default();
    effect(move || {
        let position = position.get();
        shown.with(|shown| {
            let mut told = told.borrow_mut();
            let Some(shown) = shown else {
                if told.take().is_some() {
                    media::show(None);
                }
                return;
            };
            let stale = told.as_ref().is_none_or(|(last, at, when)| {
                let reckoned = if last.playing {
                    at.saturating_add(u64::try_from(when.elapsed().as_millis()).unwrap_or(u64::MAX))
                } else {
                    *at
                };
                last != shown || reckoned.abs_diff(position) > DRIFT_MS
            });
            if stale {
                media::show(Some(&NowPlaying {
                    position_ms: position,
                    ..shown.clone()
                }));
                *told = Some((shown.clone(), position, Instant::now()));
            }
        });
    });
}

/// What the system shows of the model: the loaded track, its position
/// left at zero (it changes too often to compare).
fn shown(model: &Model) -> Option<NowPlaying> {
    let transport = model.playback.transport();
    if transport == Transport::Stopped {
        return None;
    }
    let track = model.current_track()?;
    let named = |value: &Option<String>| value.clone().filter(|name| !name.trim().is_empty());
    Some(NowPlaying {
        title: track.title.clone(),
        artist: named(&track.artist).or_else(|| named(&track.album_artist)),
        album: named(&track.album),
        path: track.path.clone(),
        duration_ms: model.playback.duration_ms(),
        position_ms: 0,
        playing: transport == Transport::Playing,
    })
}

/// The app's message for what the user asked, as playback is.
fn message(remote: Remote, transport: Transport) -> Option<Message> {
    let playing = transport == Transport::Playing;
    match remote {
        Remote::Toggle => Some(Message::PlayPausePressed),
        Remote::Play if !playing => Some(Message::PlayPausePressed),
        Remote::Pause | Remote::Stop if playing => Some(Message::PlayPausePressed),
        Remote::Play | Remote::Pause | Remote::Stop => None,
        Remote::Next => Some(Message::NextPressed),
        Remote::Previous => Some(Message::PreviousPressed),
        Remote::Seek(position_ms) => Some(Message::PositionChanged(position_ms)),
    }
}

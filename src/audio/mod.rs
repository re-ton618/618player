//! Playback: decodes one file at a time and plays it on the system's
//! default output. UI-free; the app drives it with [`Command`]s and learns
//! what happened from [`Event`]s.
//!
//! The engine runs on a thread of its own: it decodes (Symphonia), converts
//! to the output's layout and rate (Rubato) and feeds a ring that the
//! output's callback (CPAL) drains.

mod convert;
mod decoder;
mod engine;
mod output;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

/// What the app asks of the engine. A `ticket` names the load, the queued
/// file or the seek it comes with; every [`Event`] carries the latest one
/// of the file that is heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// Replaces what plays with the file at `path`, from its start.
    Load {
        ticket: u64,
        path: PathBuf,
    },
    /// Replaces what plays with the file at `path`, paused at `position`:
    /// where the last run left off. [`Command::Play`] starts it there.
    Cue {
        ticket: u64,
        path: PathBuf,
        position: Duration,
    },
    /// Has the file at `path` play on when the loaded one ends, with no
    /// gap between them. It replaces the file queued before, and goes
    /// with the next load.
    Queue {
        ticket: u64,
        path: PathBuf,
    },
    /// Takes the queued file back: the loaded one ends with nothing
    /// behind it.
    Unqueue,
    Play,
    Pause,
    /// Moves within the loaded file.
    Seek {
        ticket: u64,
        position: Duration,
    },
    /// The volume, in percent.
    Volume(u8),
    /// Drops the loaded file.
    Stop,
}

/// What the engine reports of the file that is heard: the one loaded or
/// queued with `ticket`, or sought in with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// The file opened or, queued, began to be heard as the one before it
    /// ended; its length, when the container tells it.
    Started {
        ticket: u64,
        duration: Option<Duration>,
    },
    /// Where playback is.
    Position { ticket: u64, position: Duration },
    /// The file played to its end, and nothing was queued behind it.
    Finished { ticket: u64 },
    /// The file cannot be played, or no output can be opened.
    Failed { ticket: u64, error: String },
}

/// The handle of the engine's thread, which ends when this is dropped.
pub(crate) struct Player {
    commands: mpsc::Sender<Command>,
}

impl Player {
    /// Starts the engine, which reports through `emit` until it returns
    /// false (nobody listens any more).
    pub(crate) fn spawn(emit: impl FnMut(Event) -> bool + Send + 'static) -> Self {
        let (commands, receiver) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("playback".to_owned())
            .spawn(move || engine::run(receiver, emit))
        {
            eprintln!("Playback thread failed to start: {error}");
        }
        Self { commands }
    }

    pub(crate) fn send(&self, command: Command) {
        // The engine only goes away with the app.
        let _ = self.commands.send(command);
    }

    /// Another way in, for a thread outside the app's: the system's media
    /// controls, which stop what plays when the app's UI went away with its
    /// store.
    #[cfg(target_os = "android")]
    pub(crate) fn sender(&self) -> mpsc::Sender<Command> {
        self.commands.clone()
    }
}

/// The layout of interleaved frames: how many a second, of how many samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Format {
    rate: u32,
    channels: usize,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use super::*;

    const PATIENCE: Duration = Duration::from_secs(10);

    /// The next event that `wanted` picks, within [`PATIENCE`].
    fn expect<T>(events: &Receiver<Event>, what: &str, wanted: impl Fn(&Event) -> Option<T>) -> T {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = events
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no {what} in time"));
            if let Event::Failed { error, .. } = &event {
                panic!("waiting for {what}: {error}");
            }
            if let Some(found) = wanted(&event) {
                return found;
            }
        }
    }

    /// Plays real files on the machine's output, at no volume:
    /// `PLAYER_SMOKE_FILES="a.flac;b.opus" cargo test -- --ignored smoke`.
    #[test]
    #[ignore = "needs an audio output and PLAYER_SMOKE_FILES"]
    fn smoke_plays_seeks_pauses_and_finishes() {
        let files = std::env::var("PLAYER_SMOKE_FILES").expect("PLAYER_SMOKE_FILES is set");
        for path in files.split(';').filter(|path| !path.is_empty()) {
            let (sender, events) = mpsc::channel();
            let player = Player::spawn(move |event| sender.send(event).is_ok());
            player.send(Command::Volume(0));
            player.send(Command::Load {
                ticket: 1,
                path: PathBuf::from(path),
            });

            let duration = expect(&events, "start", |event| match event {
                Event::Started {
                    ticket: 1,
                    duration,
                } => Some(*duration),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{path}: no duration"));
            expect(&events, "progress", |event| match event {
                Event::Position {
                    ticket: 1,
                    position,
                } => (*position >= Duration::from_millis(400)).then_some(()),
                _ => None,
            });

            // Paused, the position holds.
            player.send(Command::Pause);
            std::thread::sleep(Duration::from_millis(300));
            while events.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(500));
            assert!(events.try_recv().is_err(), "{path}: moved while paused");
            player.send(Command::Play);

            // A second before the end, then to the end.
            let target = duration.saturating_sub(Duration::from_secs(1));
            player.send(Command::Seek {
                ticket: 2,
                position: target,
            });
            let landed = expect(&events, "seek", |event| match event {
                Event::Position {
                    ticket: 2,
                    position,
                } => Some(*position),
                _ => None,
            });
            assert!(
                landed.abs_diff(target) < Duration::from_millis(250),
                "{path}: sought {target:?}, landed {landed:?}"
            );
            let sought = Instant::now();
            expect(&events, "finish", |event| {
                matches!(event, Event::Finished { ticket: 2 }).then_some(())
            });
            let took = sought.elapsed();
            assert!(
                took.abs_diff(Duration::from_secs(1)) < Duration::from_millis(600),
                "{path}: the last second took {took:?}"
            );

            // Back from the end, it plays on.
            player.send(Command::Seek {
                ticket: 3,
                position: Duration::ZERO,
            });
            expect(&events, "progress after the end", |event| match event {
                Event::Position {
                    ticket: 3,
                    position,
                } => (*position >= Duration::from_millis(300)).then_some(()),
                _ => None,
            });
            println!("{path}: ok, {duration:?}");
        }
    }

    /// As the smoke test above, for the first two files: the second,
    /// queued, starts as the first ends.
    #[test]
    #[ignore = "needs an audio output and PLAYER_SMOKE_FILES"]
    fn smoke_plays_a_queued_file_on() {
        let files = std::env::var("PLAYER_SMOKE_FILES").expect("PLAYER_SMOKE_FILES is set");
        let mut files = files.split(';').filter(|path| !path.is_empty());
        let (first, second) = (files.next().unwrap(), files.next().unwrap());
        let (sender, events) = mpsc::channel();
        let player = Player::spawn(move |event| sender.send(event).is_ok());
        player.send(Command::Volume(0));
        player.send(Command::Load {
            ticket: 1,
            path: PathBuf::from(first),
        });
        player.send(Command::Queue {
            ticket: 2,
            path: PathBuf::from(second),
        });
        let duration = expect(&events, "start", |event| match event {
            Event::Started {
                ticket: 1,
                duration,
            } => Some(*duration),
            _ => None,
        })
        .expect("the first file tells its length");

        // A second before the first file's end, the second is a second away.
        player.send(Command::Seek {
            ticket: 3,
            position: duration.saturating_sub(Duration::from_secs(1)),
        });
        expect(&events, "seek", |event| {
            matches!(event, Event::Position { ticket: 3, .. }).then_some(())
        });
        let sought = Instant::now();
        expect(&events, "the queued file's start", |event| match event {
            Event::Finished { .. } => panic!("the first file ended instead"),
            Event::Started { ticket: 2, .. } => Some(()),
            _ => None,
        });
        let took = sought.elapsed();
        assert!(
            took.abs_diff(Duration::from_secs(1)) < Duration::from_millis(300),
            "the last second took {took:?}"
        );
        expect(&events, "the queued file's progress", |event| match event {
            Event::Position {
                ticket: 2,
                position,
            } => (*position >= Duration::from_millis(400)).then_some(()),
            _ => None,
        });
    }

    /// As the smoke tests above, for the first file: cued, it waits where
    /// it was cued until it is played, and goes on from there.
    #[test]
    #[ignore = "needs an audio output and PLAYER_SMOKE_FILES"]
    fn smoke_cues_a_file_paused_at_a_position() {
        let files = std::env::var("PLAYER_SMOKE_FILES").expect("PLAYER_SMOKE_FILES is set");
        let path = files.split(';').find(|path| !path.is_empty()).unwrap();
        let (sender, events) = mpsc::channel();
        let player = Player::spawn(move |event| sender.send(event).is_ok());
        player.send(Command::Volume(0));
        let target = Duration::from_secs(2);
        player.send(Command::Cue {
            ticket: 1,
            path: PathBuf::from(path),
            position: target,
        });
        expect(&events, "start", |event| {
            matches!(event, Event::Started { ticket: 1, .. }).then_some(())
        });
        let landed = expect(&events, "the cued position", |event| match event {
            Event::Position {
                ticket: 1,
                position,
            } => Some(*position),
            _ => None,
        });
        assert!(
            landed.abs_diff(target) < Duration::from_millis(250),
            "cued {target:?}, landed {landed:?}"
        );

        // Cued, it does not move.
        std::thread::sleep(Duration::from_millis(800));
        assert!(events.try_recv().is_err(), "{path}: moved while cued");

        // Played, it goes on from there, at the pace of a clock.
        player.send(Command::Play);
        let played = Instant::now();
        let position = expect(&events, "progress", |event| match event {
            Event::Position {
                ticket: 1,
                position,
            } => (*position >= target + Duration::from_millis(600)).then_some(*position),
            _ => None,
        });
        let took = played.elapsed();
        assert!(
            (position - target).abs_diff(took) < Duration::from_millis(400),
            "{:?} of the file took {took:?}",
            position - target
        );
    }

    #[test]
    fn a_missing_file_fails_without_ending_the_engine() {
        let (sender, events) = mpsc::channel();
        let player = Player::spawn(move |event| sender.send(event).is_ok());
        for ticket in [1, 2] {
            player.send(Command::Load {
                ticket,
                path: PathBuf::from("/no/such/file.flac"),
            });
            let failed = events.recv_timeout(PATIENCE).expect("the load is answered");
            assert!(matches!(failed, Event::Failed { ticket: failed, .. } if failed == ticket));
        }
        // Cued or taken back with nothing loaded, it goes on as well.
        player.send(Command::Unqueue);
        player.send(Command::Cue {
            ticket: 3,
            path: PathBuf::from("/no/such/file.flac"),
            position: Duration::from_secs(1),
        });
        let failed = events.recv_timeout(PATIENCE).expect("the cue is answered");
        assert!(matches!(failed, Event::Failed { ticket: 3, .. }));
    }
}

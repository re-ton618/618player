use std::path::{Path, PathBuf};
use std::time::Duration;

use nsh_core::prelude::SharedString;

use crate::audio::{Command, Event};
use crate::library::Track;

/// Whether the current track sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Transport {
    /// Nothing is loaded in the player.
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// What the player reported that the queue has to answer.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Report {
    /// Nothing: the state took it, or it was about an older load or seek.
    Handled,
    /// The player went on to the track queued behind the current one,
    /// as the queue's entry of this key, which the caller now selects.
    Advanced(u64),
    /// The player went on to a track that was queued behind the current
    /// one and has been replaced or taken back since: the word did not
    /// reach it in time. The current track is over all the same, and what
    /// is heard is not what the queue says comes next.
    Strayed,
    /// The current track played to its end, with nothing queued behind it.
    Finished,
    /// The current track cannot be played.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CurrentTrack {
    track_id: i64,
    key: ArtworkKey,
    source: SourceFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceFingerprint {
    path: PathBuf,
    size: u64,
    modified_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArtworkKey {
    Album { directory: PathBuf, title: String },
    Track(PathBuf),
}

#[derive(Debug, Clone)]
enum Artwork {
    Empty,
    Reading {
        request_id: u64,
        key: ArtworkKey,
        source: SourceFingerprint,
    },
    Ready {
        key: ArtworkKey,
        source: SourceFingerprint,
        image: SharedString,
    },
    Missing {
        key: ArtworkKey,
        source: SourceFingerprint,
    },
}

impl Artwork {
    fn key(&self) -> Option<&ArtworkKey> {
        match self {
            Self::Empty => None,
            Self::Reading { key, .. } | Self::Ready { key, .. } | Self::Missing { key, .. } => {
                Some(key)
            }
        }
    }

    fn source(&self) -> Option<&SourceFingerprint> {
        match self {
            Self::Empty => None,
            Self::Reading { source, .. }
            | Self::Ready { source, .. }
            | Self::Missing { source, .. } => Some(source),
        }
    }

    fn set_key(&mut self, new_key: ArtworkKey) {
        match self {
            Self::Empty => {}
            Self::Reading { key, .. } | Self::Ready { key, .. } | Self::Missing { key, .. } => {
                *key = new_key
            }
        }
    }

    fn image(&self) -> Option<&SharedString> {
        match self {
            Self::Ready { image, .. } => Some(image),
            Self::Empty | Self::Reading { .. } | Self::Missing { .. } => None,
        }
    }
}

/// The share of a track, in percent, heard by default before it counts as
/// listened to.
pub(super) const DEFAULT_LISTEN_PERCENT: u8 = 10;
/// The most a report of the position may move on from the one before and
/// still be heard as playback: anything further is a jump.
const MAX_HEARD_STEP_MS: u64 = 5_000;

/// What is heard of the loaded track since it started, to tell when it is
/// listened to.
#[derive(Debug, Clone, Default)]
struct Listen {
    track_id: Option<i64>,
    heard_ms: u64,
    /// It was counted as listened to already.
    counted: bool,
}

impl Listen {
    fn of(track_id: i64) -> Self {
        Self {
            track_id: Some(track_id),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct State {
    current: Option<CurrentTrack>,
    last_request_id: u64,
    artwork: Artwork,
    volume: u8,
    transport: Transport,
    /// The current track's last load or seek; the player reports with it.
    ticket: u64,
    /// The last ticket given out.
    last_ticket: u64,
    /// The track the player plays on with when the current one ends.
    queued: Option<Queued>,
    /// The tickets of tracks that were queued behind the current one
    /// before that one.
    replaced: Vec<u64>,
    position_ms: u64,
    duration_ms: Option<u64>,
    listen: Listen,
    /// The share of a track, in percent, to hear before it counts as
    /// listened to.
    listen_percent: u8,
    /// The tracks listened to since the caller last took them.
    plays: Vec<i64>,
}

#[derive(Debug, Clone)]
struct Queued {
    ticket: u64,
    track_id: i64,
    /// The key of the queue's entry it was queued as.
    entry: u64,
    path: PathBuf,
    duration_ms: Option<u64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            current: None,
            last_request_id: 0,
            artwork: Artwork::Empty,
            volume: 100,
            transport: Transport::Stopped,
            ticket: 0,
            last_ticket: 0,
            queued: None,
            replaced: Vec::new(),
            position_ms: 0,
            duration_ms: None,
            listen: Listen::default(),
            listen_percent: DEFAULT_LISTEN_PERCENT,
            plays: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ArtworkRequest {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
}

pub(super) enum ArtworkEffect {
    None,
    Load {
        request: ArtworkRequest,
        error: Option<String>,
    },
    Missing {
        error: Option<String>,
    },
}

impl State {
    pub(super) fn select(&mut self, track: &Track) -> Option<ArtworkRequest> {
        let selected = CurrentTrack::from(track);
        let Some(previous) = self.current.as_ref() else {
            self.current = Some(selected);
            return Some(self.read_current());
        };

        let same_key = previous.key == selected.key;
        let same_track = previous.track_id == selected.track_id;
        let same_source = previous.source == selected.source;

        if same_key && (!same_track || same_source) {
            self.current = Some(selected);
            let current = self.current.as_ref().expect("current track was just set");

            return match &self.artwork {
                Artwork::Reading { .. } | Artwork::Ready { .. }
                    if self.artwork.key() == Some(&current.key) =>
                {
                    None
                }
                Artwork::Missing { key, source }
                    if key == &current.key && source == &current.source =>
                {
                    None
                }
                Artwork::Empty
                | Artwork::Reading { .. }
                | Artwork::Ready { .. }
                | Artwork::Missing { .. } => Some(self.read_current()),
            };
        }

        if same_track && same_source {
            self.current = Some(selected);
            let current = self.current.as_ref().expect("current track was just set");
            if self.artwork.source() == Some(&current.source) {
                self.artwork.set_key(current.key.clone());
                return None;
            }
        } else {
            self.current = Some(selected);
        }

        Some(self.read_current())
    }

    /// Forgets the current track, and has the player drop it.
    pub(super) fn clear(&mut self) -> Command {
        self.current = None;
        self.artwork = Artwork::Empty;
        self.duration_ms = None;
        self.stop()
    }

    pub(super) fn transport(&self) -> Transport {
        self.transport
    }

    /// Where the current track is, in milliseconds.
    pub(super) fn position_ms(&self) -> u64 {
        self.position_ms
    }

    /// The current track's length: the player's word, the library's until
    /// it has opened the file.
    pub(super) fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    /// Starts `track`, which the caller has selected, from its beginning.
    pub(super) fn play(&mut self, track: &Track) -> Command {
        self.transport = Transport::Playing;
        self.position_ms = 0;
        self.duration_ms = track.duration_ms;
        // The player drops what was queued behind the track before.
        self.queued = None;
        self.replaced.clear();
        self.listen = Listen::of(track.id);
        self.ticket = self.next_ticket();
        Command::Load {
            ticket: self.ticket,
            path: track.path.clone(),
        }
    }

    /// Loads `track`, which the caller has selected, paused at
    /// `position_ms`: where the last run left it.
    pub(super) fn cue(&mut self, track: &Track, position_ms: u64) -> Command {
        self.transport = Transport::Paused;
        self.duration_ms = track.duration_ms;
        self.position_ms = self
            .duration_ms
            .map_or(position_ms, |end| position_ms.min(end));
        self.queued = None;
        self.replaced.clear();
        self.listen = Listen::of(track.id);
        self.ticket = self.next_ticket();
        Command::Cue {
            ticket: self.ticket,
            path: track.path.clone(),
            position: Duration::from_millis(self.position_ms),
        }
    }

    /// Has the player go on with `track`, the queue's entry `entry`, when
    /// the loaded one ends, with no gap between them; nothing when
    /// stopped.
    pub(super) fn queue(&mut self, track: &Track, entry: u64) -> Option<Command> {
        if self.transport == Transport::Stopped {
            return None;
        }
        self.forget_queued();
        let ticket = self.next_ticket();
        self.queued = Some(Queued {
            ticket,
            track_id: track.id,
            entry,
            path: track.path.clone(),
            duration_ms: track.duration_ms,
        });
        Some(Command::Queue {
            ticket,
            path: track.path.clone(),
        })
    }

    /// Has the player end with the loaded track; nothing when none is
    /// queued behind it.
    pub(super) fn unqueue(&mut self) -> Option<Command> {
        self.queued.as_ref()?;
        self.forget_queued();
        Some(Command::Unqueue)
    }

    /// The queue's entry and the file the player has behind the loaded
    /// track.
    pub(super) fn queued(&self) -> Option<(u64, &Path)> {
        self.queued
            .as_ref()
            .map(|queued| (queued.entry, queued.path.as_path()))
    }

    /// The player may have gone on to what was queued before it hears
    /// otherwise: its ticket is kept to know that by.
    fn forget_queued(&mut self) {
        if let Some(queued) = self.queued.take() {
            self.replaced.push(queued.ticket);
        }
    }

    /// Pauses what plays and resumes what is paused; nothing when stopped,
    /// where the caller picks what to start.
    pub(super) fn toggle(&mut self) -> Option<Command> {
        match self.transport {
            Transport::Playing => {
                self.transport = Transport::Paused;
                Some(Command::Pause)
            }
            Transport::Paused => {
                self.transport = Transport::Playing;
                Some(Command::Play)
            }
            Transport::Stopped => None,
        }
    }

    /// Moves within the loaded track, at most to its end.
    pub(super) fn seek(&mut self, position_ms: u64) -> Option<Command> {
        if self.transport == Transport::Stopped {
            return None;
        }
        self.position_ms = self
            .duration_ms
            .map_or(position_ms, |end| position_ms.min(end));
        self.ticket = self.next_ticket();
        Some(Command::Seek {
            ticket: self.ticket,
            position: Duration::from_millis(self.position_ms),
        })
    }

    /// Has the player drop the track; it stays current.
    pub(super) fn stop(&mut self) -> Command {
        self.transport = Transport::Stopped;
        self.position_ms = 0;
        self.queued = None;
        self.replaced.clear();
        self.listen = Listen::default();
        // What the player still reports of the dropped track is stale.
        self.ticket = self.next_ticket();
        Command::Stop
    }

    /// Takes what the player reported.
    pub(super) fn report(&mut self, event: Event) -> Report {
        if let Event::Started { ticket, duration } = &event
            && let Some(queued) = self.queued.take_if(|queued| queued.ticket == *ticket)
        {
            // The track before played to its end.
            self.hear_rest();
            self.listen = Listen::of(queued.track_id);
            self.ticket = queued.ticket;
            self.replaced.clear();
            self.position_ms = 0;
            self.duration_ms = duration.map(millis).or(queued.duration_ms);
            return Report::Advanced(queued.entry);
        }
        if let Event::Started { ticket, .. } = &event
            && self.replaced.contains(ticket)
        {
            // Whatever is queued now is behind the file the player went
            // on to, not behind the current track.
            self.hear_rest();
            self.replaced.clear();
            return Report::Strayed;
        }
        match event {
            Event::Started { ticket, duration } if ticket == self.ticket => {
                if let Some(duration) = duration {
                    self.duration_ms = Some(millis(duration));
                }
                Report::Handled
            }
            Event::Position { ticket, position } if ticket == self.ticket => {
                let position_ms = millis(position);
                if position_ms > self.position_ms
                    && position_ms - self.position_ms <= MAX_HEARD_STEP_MS
                {
                    self.hear(position_ms - self.position_ms);
                }
                self.position_ms = position_ms;
                Report::Handled
            }
            Event::Finished { ticket } if ticket == self.ticket => {
                self.hear_rest();
                Report::Finished
            }
            Event::Failed { ticket, error } if ticket == self.ticket => Report::Failed(error),
            Event::Started { .. }
            | Event::Position { .. }
            | Event::Finished { .. }
            | Event::Failed { .. } => Report::Handled,
        }
    }

    /// The loaded track was heard for `heard_ms` more: once that makes the
    /// share of it the user set, it is listened to.
    fn hear(&mut self, heard_ms: u64) {
        let listen = &mut self.listen;
        listen.heard_ms = listen.heard_ms.saturating_add(heard_ms);
        let (Some(track_id), Some(duration_ms)) = (listen.track_id, self.duration_ms) else {
            return;
        };
        let due = duration_ms.saturating_mul(u64::from(self.listen_percent)) / 100;
        if !listen.counted && duration_ms > 0 && listen.heard_ms >= due {
            listen.counted = true;
            self.plays.push(track_id);
        }
    }

    /// The loaded track ended: what was left of it after the last report
    /// of the position was heard too.
    fn hear_rest(&mut self) {
        let rest = self
            .duration_ms
            .map_or(0, |end| end.saturating_sub(self.position_ms));
        if rest <= MAX_HEARD_STEP_MS {
            self.hear(rest);
        }
    }

    /// The tracks listened to since this was asked last, in the order
    /// they were.
    pub(super) fn take_plays(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.plays)
    }

    /// The share of a track, in percent, to hear before it counts as
    /// listened to.
    pub(super) fn listen_percent(&self) -> u8 {
        self.listen_percent
    }

    pub(super) fn set_listen_percent(&mut self, percent: u8) {
        self.listen_percent = percent.clamp(1, 100);
    }

    fn next_ticket(&mut self) -> u64 {
        self.last_ticket += 1;
        self.last_ticket
    }

    #[cfg(test)]
    pub(super) fn current_track_id(&self) -> Option<i64> {
        self.current.as_ref().map(|current| current.track_id)
    }

    /// The current track's artwork, as a `data:` URI.
    pub(super) fn artwork_image(&self) -> Option<&SharedString> {
        self.artwork.image()
    }

    pub(super) fn volume(&self) -> u8 {
        self.volume
    }

    pub(super) fn set_volume(&mut self, volume: u8) {
        let volume = volume.min(100);
        if self.volume != volume {
            self.volume = volume;
        }
    }

    pub(super) fn loaded(
        &mut self,
        request_id: u64,
        result: Result<Option<SharedString>, String>,
    ) -> ArtworkEffect {
        let Artwork::Reading {
            request_id: active_request_id,
            key,
            source,
        } = &self.artwork
        else {
            return ArtworkEffect::None;
        };
        if request_id != *active_request_id {
            return ArtworkEffect::None;
        }

        let key = key.clone();
        let source = source.clone();
        match result {
            Ok(Some(image)) => {
                self.artwork = Artwork::Ready { key, source, image };
                ArtworkEffect::None
            }
            Ok(None) => self.failed(key, source, None),
            Err(error) => self.failed(key, source, Some(error)),
        }
    }

    fn read_current(&mut self) -> ArtworkRequest {
        let current = self
            .current
            .as_ref()
            .expect("artwork reads need a current track");
        let key = current.key.clone();
        let source = current.source.clone();
        self.start_read(key, source)
    }

    fn start_read(&mut self, key: ArtworkKey, source: SourceFingerprint) -> ArtworkRequest {
        self.last_request_id = self
            .last_request_id
            .checked_add(1)
            .expect("artwork request id overflowed");
        let request_id = self.last_request_id;
        let path = source.path.clone();
        self.artwork = Artwork::Reading {
            request_id,
            key,
            source,
        };
        ArtworkRequest { request_id, path }
    }

    fn failed(
        &mut self,
        key: ArtworkKey,
        source: SourceFingerprint,
        error: Option<String>,
    ) -> ArtworkEffect {
        let retry_source = self.current.as_ref().and_then(|current| {
            (current.key == key && current.source != source)
                .then(|| (current.key.clone(), current.source.clone()))
        });

        if let Some((retry_key, retry_source)) = retry_source {
            let request = self.start_read(retry_key, retry_source);
            ArtworkEffect::Load { request, error }
        } else {
            self.artwork = Artwork::Missing { key, source };
            ArtworkEffect::Missing { error }
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl From<&Track> for CurrentTrack {
    fn from(track: &Track) -> Self {
        let source = SourceFingerprint {
            path: track.path.clone(),
            size: track.size,
            modified_ns: track.modified_ns,
        };
        let key = if track.album_key().is_empty() {
            ArtworkKey::Track(track.path.clone())
        } else {
            ArtworkKey::Album {
                directory: track
                    .path
                    .parent()
                    .unwrap_or(track.path.as_path())
                    .to_path_buf(),
                title: track.album_key().to_owned(),
            }
        };

        Self {
            track_id: track.id,
            key,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::library::test_track;

    use super::*;

    fn album_track(id: i64, file: &str, album: &str) -> Track {
        test_track(
            id,
            PathBuf::from(format!("/music/{album}/{file}.flac")),
            Some(album),
        )
    }

    #[test]
    fn transport_follows_play_pause_seek_and_stop() {
        let track = album_track(1, "one", "album");
        let mut playback = State::default();
        assert_eq!(playback.toggle(), None);
        assert_eq!(playback.seek(1_000), None);

        let _ = playback.select(&track);
        let Command::Load { ticket, path } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        assert_eq!(path, track.path);
        assert_eq!(playback.transport(), Transport::Playing);

        assert_eq!(playback.toggle(), Some(Command::Pause));
        assert_eq!(playback.transport(), Transport::Paused);
        assert_eq!(playback.toggle(), Some(Command::Play));
        assert_eq!(playback.transport(), Transport::Playing);

        let started = Event::Started {
            ticket,
            duration: Some(Duration::from_secs(60)),
        };
        assert_eq!(playback.report(started), Report::Handled);
        assert_eq!(playback.duration_ms(), Some(60_000));

        // A seek is held to the track, and shows at once.
        let Some(Command::Seek {
            ticket: seek_ticket,
            position,
        }) = playback.seek(90_000)
        else {
            panic!("a loaded track seeks");
        };
        assert_eq!(position, Duration::from_secs(60));
        assert_eq!(playback.position_ms(), 60_000);
        assert!(seek_ticket > ticket);

        assert_eq!(playback.stop(), Command::Stop);
        assert_eq!(playback.transport(), Transport::Stopped);
        assert_eq!(playback.position_ms(), 0);
        assert_eq!(playback.current_track_id(), Some(1));
    }

    #[test]
    fn reports_of_an_older_load_or_seek_are_dropped() {
        let track = album_track(1, "one", "album");
        let mut playback = State::default();
        let _ = playback.select(&track);
        let Command::Load { ticket: first, .. } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        let Some(Command::Seek { ticket: second, .. }) = playback.seek(5_000) else {
            panic!("a loaded track seeks");
        };

        let stale = Event::Position {
            ticket: first,
            position: Duration::from_secs(1),
        };
        assert_eq!(playback.report(stale), Report::Handled);
        assert_eq!(playback.position_ms(), 5_000);
        assert_eq!(
            playback.report(Event::Finished { ticket: first }),
            Report::Handled
        );

        let fresh = Event::Position {
            ticket: second,
            position: Duration::from_secs(6),
        };
        assert_eq!(playback.report(fresh), Report::Handled);
        assert_eq!(playback.position_ms(), 6_000);
        assert_eq!(
            playback.report(Event::Finished { ticket: second }),
            Report::Finished
        );
        let failed = Event::Failed {
            ticket: second,
            error: "broken".to_owned(),
        };
        assert_eq!(playback.report(failed), Report::Failed("broken".to_owned()));
    }

    #[test]
    fn the_player_going_on_to_the_queued_track_makes_it_the_reported_one() {
        let (one, two) = (
            album_track(1, "one", "album"),
            album_track(2, "two", "album"),
        );
        let mut playback = State::default();
        assert_eq!(playback.queue(&two, 9), None);
        assert_eq!(playback.unqueue(), None);

        let _ = playback.select(&one);
        let Command::Load { ticket: first, .. } = playback.play(&one) else {
            panic!("playing loads the track");
        };
        let Some(Command::Queue {
            ticket: second,
            path,
        }) = playback.queue(&two, 9)
        else {
            panic!("a playing track takes one behind it");
        };
        assert_eq!(path, two.path);
        assert_eq!(playback.queued(), Some((9, two.path.as_path())));

        // The current track still reports, and a seek in it keeps the queue.
        let position = Event::Position {
            ticket: first,
            position: Duration::from_secs(3),
        };
        assert_eq!(playback.report(position), Report::Handled);
        assert_eq!(playback.position_ms(), 3_000);
        let Some(Command::Seek { ticket: sought, .. }) = playback.seek(9_000) else {
            panic!("a loaded track seeks");
        };

        let started = Event::Started {
            ticket: second,
            duration: Some(Duration::from_secs(7)),
        };
        assert_eq!(playback.report(started.clone()), Report::Advanced(9));
        assert_eq!(playback.queued(), None);
        assert_eq!(playback.position_ms(), 0);
        assert_eq!(playback.duration_ms(), Some(7_000));
        // The track before it is over, whatever still arrives of it.
        assert_eq!(
            playback.report(Event::Finished { ticket: sought }),
            Report::Handled
        );
        assert_eq!(playback.report(started), Report::Handled);
        assert_eq!(
            playback.report(Event::Finished { ticket: second }),
            Report::Finished
        );
    }

    /// Reports the position of `ticket` at every second from `from` to `to`.
    fn heard(playback: &mut State, ticket: u64, from: u64, to: u64) {
        for second in from..=to {
            let position = Event::Position {
                ticket,
                position: Duration::from_secs(second),
            };
            assert_eq!(playback.report(position), Report::Handled);
        }
    }

    #[test]
    fn a_track_counts_as_listened_to_once_when_its_share_is_heard() {
        let track = album_track(1, "one", "album");
        let mut playback = State::default();
        let _ = playback.select(&track);
        let Command::Load { ticket, .. } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        let started = Event::Started {
            ticket,
            duration: Some(Duration::from_secs(100)),
        };
        assert_eq!(playback.report(started), Report::Handled);

        heard(&mut playback, ticket, 1, 9);
        assert_eq!(playback.take_plays(), Vec::<i64>::new());
        heard(&mut playback, ticket, 10, 10);
        assert_eq!(playback.take_plays(), [1]);
        heard(&mut playback, ticket, 11, 40);
        assert_eq!(playback.take_plays(), Vec::<i64>::new());

        // Played again, it counts again.
        let Command::Load { ticket, .. } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        let started = Event::Started {
            ticket,
            duration: Some(Duration::from_secs(100)),
        };
        assert_eq!(playback.report(started), Report::Handled);
        heard(&mut playback, ticket, 1, 10);
        assert_eq!(playback.take_plays(), [1]);
    }

    #[test]
    fn what_is_skipped_by_a_seek_or_a_jump_is_not_heard() {
        let track = album_track(1, "one", "album");
        let mut playback = State::default();
        playback.set_listen_percent(50);
        let _ = playback.select(&track);
        let Command::Load { ticket, .. } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        let started = Event::Started {
            ticket,
            duration: Some(Duration::from_secs(100)),
        };
        assert_eq!(playback.report(started), Report::Handled);
        heard(&mut playback, ticket, 1, 10);

        let Some(Command::Seek { ticket, .. }) = playback.seek(80_000) else {
            panic!("a loaded track seeks");
        };
        heard(&mut playback, ticket, 81, 99);
        // A report far from the one before is a jump, not playback.
        heard(&mut playback, ticket, 70, 70);
        heard(&mut playback, ticket, 90, 90);
        assert_eq!(
            playback.report(Event::Finished { ticket }),
            Report::Finished
        );
        assert_eq!(playback.take_plays(), Vec::<i64>::new());

        // Nothing is counted of a track that is stopped, nor of one whose
        // length is not known.
        let Command::Load { ticket, .. } = playback.play(&track) else {
            panic!("playing loads the track");
        };
        playback.duration_ms = None;
        heard(&mut playback, ticket, 1, 60);
        let _ = playback.stop();
        heard(&mut playback, ticket, 61, 70);
        assert_eq!(playback.take_plays(), Vec::<i64>::new());
    }

    #[test]
    fn a_track_heard_to_its_end_counts_before_the_next_one_goes_on() {
        let (one, two) = (
            album_track(1, "one", "album"),
            album_track(2, "two", "album"),
        );
        let mut playback = State::default();
        playback.set_listen_percent(100);
        let _ = playback.select(&one);
        let Command::Load { ticket: first, .. } = playback.play(&one) else {
            panic!("playing loads the track");
        };
        let started = Event::Started {
            ticket: first,
            duration: Some(Duration::from_millis(5_500)),
        };
        assert_eq!(playback.report(started), Report::Handled);
        let Some(Command::Queue { ticket: second, .. }) = playback.queue(&two, 9) else {
            panic!("a playing track takes one behind it");
        };
        heard(&mut playback, first, 1, 5);
        assert_eq!(playback.take_plays(), Vec::<i64>::new());

        let started = Event::Started {
            ticket: second,
            duration: Some(Duration::from_secs(4)),
        };
        assert_eq!(playback.report(started), Report::Advanced(9));
        assert_eq!(playback.take_plays(), [1]);
        heard(&mut playback, second, 1, 3);
        assert_eq!(
            playback.report(Event::Finished { ticket: second }),
            Report::Finished
        );
        assert_eq!(playback.take_plays(), [2]);
    }

    #[test]
    fn the_listening_threshold_is_a_percent() {
        let mut playback = State::default();
        assert_eq!(playback.listen_percent(), DEFAULT_LISTEN_PERCENT);
        playback.set_listen_percent(0);
        assert_eq!(playback.listen_percent(), 1);
        playback.set_listen_percent(250);
        assert_eq!(playback.listen_percent(), 100);
    }

    #[test]
    fn a_new_load_or_a_stop_forgets_the_queued_track() {
        let (one, two) = (
            album_track(1, "one", "album"),
            album_track(2, "two", "album"),
        );
        for stop in [false, true] {
            let mut playback = State::default();
            let _ = playback.select(&one);
            let _ = playback.play(&one);
            let Some(Command::Queue { ticket, .. }) = playback.queue(&two, 9) else {
                panic!("a playing track takes one behind it");
            };
            let _ = if stop {
                playback.stop()
            } else {
                playback.play(&one)
            };

            let started = Event::Started {
                ticket,
                duration: None,
            };
            assert_eq!(playback.report(started), Report::Handled);
        }
    }

    #[test]
    fn the_player_going_on_to_a_track_that_was_replaced_or_taken_back_is_told() {
        let (one, two, three) = (
            album_track(1, "one", "album"),
            album_track(2, "two", "album"),
            album_track(3, "three", "album"),
        );
        let started = |ticket| Event::Started {
            ticket,
            duration: None,
        };

        // Replaced: the player had gone on to the one queued before.
        let mut playback = State::default();
        let _ = playback.select(&one);
        let _ = playback.play(&one);
        let Some(Command::Queue { ticket: old, .. }) = playback.queue(&two, 8) else {
            panic!("a playing track takes one behind it");
        };
        let Some(Command::Queue { ticket: new, .. }) = playback.queue(&three, 9) else {
            panic!("a queued track is replaced");
        };
        assert_eq!(playback.report(started(old)), Report::Strayed);
        // Told once: what is queued now is no longer behind the current
        // track, and the caller loads what it wants.
        assert_eq!(playback.report(started(old)), Report::Handled);
        let _ = playback.play(&three);
        assert_eq!(playback.report(started(new)), Report::Handled);

        // Taken back.
        let mut playback = State::default();
        let _ = playback.select(&one);
        let _ = playback.play(&one);
        let Some(Command::Queue { ticket: old, .. }) = playback.queue(&two, 8) else {
            panic!("a playing track takes one behind it");
        };
        assert_eq!(playback.unqueue(), Some(Command::Unqueue));
        assert_eq!(playback.queued(), None);
        assert_eq!(playback.unqueue(), None);
        assert_eq!(playback.report(started(old)), Report::Strayed);

        // The one that is queued is gone on to as ever.
        let mut playback = State::default();
        let _ = playback.select(&one);
        let _ = playback.play(&one);
        let _ = playback.queue(&two, 8);
        let Some(Command::Queue { ticket: new, .. }) = playback.queue(&three, 9) else {
            panic!("a queued track is replaced");
        };
        assert_eq!(playback.report(started(new)), Report::Advanced(9));
    }

    #[test]
    fn a_cued_track_is_paused_at_its_position_and_plays_from_there() {
        let mut track = album_track(1, "one", "album");
        track.duration_ms = Some(60_000);
        let mut playback = State::default();
        let _ = playback.select(&track);

        let Command::Cue {
            ticket,
            path,
            position,
        } = playback.cue(&track, 90_000)
        else {
            panic!("cueing loads the track paused");
        };

        assert_eq!(path, track.path);
        // Held to the track, as a seek is.
        assert_eq!(position, Duration::from_secs(60));
        assert_eq!(playback.transport(), Transport::Paused);
        assert_eq!(playback.position_ms(), 60_000);
        let moved = Event::Position {
            ticket,
            position: Duration::from_secs(59),
        };
        assert_eq!(playback.report(moved), Report::Handled);
        assert_eq!(playback.position_ms(), 59_000);
        assert!(playback.queue(&track, 2).is_some());
        assert_eq!(playback.toggle(), Some(Command::Play));
        assert_eq!(playback.transport(), Transport::Playing);
    }

    #[test]
    fn volume_clamps_to_percent_range() {
        let mut state = State::default();
        assert_eq!(state.volume(), 100);

        state.set_volume(42);
        assert_eq!(state.volume(), 42);

        state.set_volume(255);
        assert_eq!(state.volume(), 100);
    }

    #[test]
    fn selection_sets_current_id_before_returning_read_request() {
        let track = album_track(7, "track", "album");
        let mut state = State::default();

        let request = state.select(&track).expect("first selection must read");

        assert_eq!(state.current_track_id(), Some(7));
        assert_eq!(request.path, track.path);
        assert!(matches!(state.artwork, Artwork::Reading { .. }));
    }

    #[test]
    fn same_album_siblings_reuse_in_flight_and_ready_artwork() {
        let first = album_track(1, "first", "album");
        let second = album_track(2, "second", "album");
        let third = album_track(3, "third", "album");
        let mut state = State::default();

        let request = state.select(&first).expect("first selection must read");
        assert!(state.select(&second).is_none());
        assert_eq!(state.current_track_id(), Some(2));

        let image = SharedString::from("data:image/png;base64,AQID");
        assert!(matches!(
            state.loaded(request.request_id, Ok(Some(image.clone()))),
            ArtworkEffect::None
        ));
        assert_eq!(state.artwork_image(), Some(&image));

        assert!(state.select(&third).is_none());
        assert_eq!(state.current_track_id(), Some(3));
        assert_eq!(state.artwork_image(), Some(&image));
    }

    #[test]
    fn stale_result_cannot_replace_different_album_request() {
        let first = album_track(1, "first", "first-album");
        let second = album_track(2, "second", "second-album");
        let mut state = State::default();

        let first_request = state.select(&first).expect("first selection must read");
        let second_request = state.select(&second).expect("new album must read");
        assert!(second_request.request_id > first_request.request_id);

        let stale = SharedString::from("data:image/png;base64,AQ==");
        assert!(matches!(
            state.loaded(first_request.request_id, Ok(Some(stale))),
            ArtworkEffect::None
        ));
        assert!(state.artwork_image().is_none());
        assert_eq!(state.current_track_id(), Some(2));

        let current = SharedString::from("data:image/png;base64,Ag==");
        assert!(matches!(
            state.loaded(second_request.request_id, Ok(Some(current.clone()))),
            ArtworkEffect::None
        ));
        assert_eq!(state.artwork_image(), Some(&current));
    }

    #[test]
    fn failed_source_retries_current_sibling_only_once() {
        let first = album_track(1, "first", "album");
        let second = album_track(2, "second", "album");
        let mut state = State::default();

        let first_request = state.select(&first).expect("first selection must read");
        assert!(state.select(&second).is_none());

        let retry = match state.loaded(first_request.request_id, Ok(None)) {
            ArtworkEffect::Load { request, error } => {
                assert!(error.is_none());
                request
            }
            _ => panic!("missing sibling art must retry the current source"),
        };
        assert_eq!(retry.path, second.path);
        assert!(retry.request_id > first_request.request_id);
        assert!(state.select(&second).is_none());

        assert!(matches!(
            state.loaded(retry.request_id, Err("invalid image".into())),
            ArtworkEffect::Missing { error: Some(_) }
        ));
        assert!(state.select(&second).is_none());
    }

    #[test]
    fn metadata_rekey_preserves_art_from_the_same_source() {
        let path = PathBuf::from("/music/album/track.flac");
        let before = test_track(1, path.clone(), Some("old-album"));
        let after = test_track(1, path, Some("new-album"));
        let mut state = State::default();

        let request = state.select(&before).expect("first selection must read");
        let image = SharedString::from("data:image/png;base64,AQ==");
        assert!(matches!(
            state.loaded(request.request_id, Ok(Some(image.clone()))),
            ArtworkEffect::None
        ));

        assert!(state.select(&after).is_none());
        assert_eq!(state.artwork_image(), Some(&image));
        assert_eq!(
            state.artwork.key(),
            state.current.as_ref().map(|current| &current.key)
        );
    }

    #[test]
    fn metadata_rekey_does_not_relabel_sibling_art() {
        let first = album_track(1, "first", "old-album");
        let sibling_before = album_track(2, "second", "old-album");
        let sibling_after = test_track(2, sibling_before.path.clone(), Some("new-album"));
        let mut state = State::default();

        let request = state.select(&first).expect("first selection must read");
        assert!(state.select(&sibling_before).is_none());
        assert!(matches!(
            state.loaded(
                request.request_id,
                Ok(Some(SharedString::from("data:image/png;base64,AQ==")))
            ),
            ArtworkEffect::None
        ));

        let reread = state
            .select(&sibling_after)
            .expect("rekeyed sibling must read its own source");
        assert_eq!(reread.path, sibling_after.path);
        assert!(matches!(state.artwork, Artwork::Reading { .. }));
    }
}

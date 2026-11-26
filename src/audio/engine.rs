//! The engine's thread: takes the app's commands, keeps the output's ring
//! full of the loaded file's frames, and reports where playback is.

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use super::convert::Converter;
use super::decoder::Decoder;
use super::output::{Feed, Output};
use super::{Command, Event, Format};

/// How often the ring is topped up while it drains.
const FILL_INTERVAL: Duration = Duration::from_millis(20);
/// The least change of position worth reporting.
const POSITION_STEP: Duration = Duration::from_millis(200);
/// What a silent output is given to fade out before its stream rests.
const REST_DELAY: Duration = Duration::from_millis(250);
/// An output that breaks again this soon after it was opened is given up.
const REOPEN_INTERVAL: Duration = Duration::from_secs(1);

pub(super) fn run(commands: Receiver<Command>, emit: impl FnMut(Event) -> bool) {
    let mut engine = Engine {
        emit,
        listening: true,
        output: None,
        session: None,
        playing: false,
        gain: 1.0,
        rest_at: None,
        opened_at: None,
    };

    while engine.listening {
        let command = match engine.wait() {
            Some(wait) if wait.is_zero() => match commands.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            },
            Some(wait) => match commands.recv_timeout(wait) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            },
        };

        // Everything asked so far. A seek that another seek or a load
        // follows is skipped: dragging the position asks for many.
        let pending: Vec<Command> = command.into_iter().chain(commands.try_iter()).collect();
        let last_move = pending.iter().rposition(|command| {
            matches!(
                command,
                Command::Seek { .. } | Command::Load { .. } | Command::Cue { .. }
            )
        });
        for (index, command) in pending.into_iter().enumerate() {
            let superseded = matches!(command, Command::Seek { .. })
                && last_move.is_some_and(|last| index < last);
            if !superseded {
                engine.handle(command);
            }
        }

        engine.advance();
    }
}

struct Engine<E> {
    emit: E,
    /// Somebody still takes the events.
    listening: bool,
    /// Opened with the first file, and again when it breaks.
    output: Option<Output>,
    session: Option<Session>,
    playing: bool,
    gain: f32,
    /// When the silent output's stream may rest.
    rest_at: Option<Instant>,
    /// When the output was last opened.
    opened_at: Option<Instant>,
}

/// The file that is heard, and the feed its frames go out in. It stays
/// loaded at its end, so that a seek back into it plays on.
struct Session {
    ticket: u64,
    decoder: Decoder,
    /// The decoder reached the end; the feed may still hold frames.
    decoded: bool,
    /// Where in the file the feed's frame `origin` is from.
    base: Duration,
    origin: u64,
    /// The end was reported.
    finished: bool,
    reported: Option<Duration>,
    /// The file that plays on in the same feed when this one ends.
    next: Option<Next>,

    feed: Feed,
    /// Brings frames to the output's layout; made for the layout of the
    /// first ones, and again when a file comes in another.
    converter: Option<Converter>,
    /// Converted samples the ring had no room for, from `queued_from` on.
    queued: Vec<f32>,
    queued_from: usize,
    /// The frames converted for this feed so far.
    fed: u64,
}

/// A file queued behind the one that is heard.
struct Next {
    ticket: u64,
    decoder: Decoder,
    /// The feed's frame its first one is, once it has begun to be decoded.
    start: Option<u64>,
    decoded: bool,
}

impl<E: FnMut(Event) -> bool> Engine<E> {
    fn emit(&mut self, event: Event) {
        if !(self.emit)(event) {
            self.listening = false;
        }
    }

    /// How long nothing needs doing without a command: forever (`None`)
    /// when nothing plays.
    fn wait(&self) -> Option<Duration> {
        let rest = self
            .rest_at
            .map(|at| at.saturating_duration_since(Instant::now()));
        let draining = self.playing
            && self
                .session
                .as_ref()
                .is_some_and(|session| !session.finished);
        if draining { Some(FILL_INTERVAL) } else { rest }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load { ticket, path } => match self.load(ticket, &path) {
                Ok(duration) => {
                    self.emit(Event::Started { ticket, duration });
                    self.set_playing(true);
                }
                Err(error) => {
                    self.silence();
                    self.emit(Event::Failed { ticket, error });
                }
            },
            Command::Cue {
                ticket,
                path,
                position,
            } => match self.load(ticket, &path) {
                Ok(duration) => {
                    self.emit(Event::Started { ticket, duration });
                    self.seek(ticket, position);
                    // Loaded and silent until it is played.
                    self.set_playing(false);
                }
                Err(error) => {
                    self.silence();
                    self.emit(Event::Failed { ticket, error });
                }
            },
            Command::Queue { ticket, path } => self.queue(ticket, &path),
            Command::Unqueue => self.unqueue(),
            Command::Play => {
                if self.session.is_some() {
                    self.set_playing(true);
                }
            }
            Command::Pause => self.set_playing(false),
            Command::Seek { ticket, position } => {
                self.seek(ticket, position);
            }
            Command::Volume(percent) => {
                // Squared: closer to how loud a setting sounds. Full volume
                // leaves the samples as they are.
                let level = f32::from(percent.min(100)) / 100.0;
                self.gain = level * level;
                if let Some(output) = &self.output {
                    output.set_gain(self.gain);
                }
            }
            Command::Stop => {
                self.session = None;
                self.silence();
            }
        }
    }

    /// Replaces the loaded file, and tells the new one's length.
    fn load(&mut self, ticket: u64, path: &Path) -> Result<Option<Duration>, String> {
        self.session = None;
        let decoder = Decoder::open(path)?;
        let duration = decoder.duration();
        let feed = self.ensure_output()?.begin();
        self.session = Some(Session::new(ticket, decoder, feed));
        Ok(duration)
    }

    /// Has the file at `path` play on when the loaded one ends. One that
    /// does not open is left out: the loaded file then just ends.
    fn queue(&mut self, ticket: u64, path: &Path) {
        if self.session.as_ref().is_none_or(|session| session.finished) {
            return;
        }
        self.unqueue();
        let next = match Decoder::open(path) {
            Ok(decoder) => Some(Next {
                ticket,
                decoder,
                start: None,
                decoded: false,
            }),
            Err(error) => {
                eprintln!("Queued file failed to open: {error}");
                None
            }
        };
        if let Some(session) = &mut self.session {
            session.next = next;
        }
    }

    /// Takes the queued file back: the loaded one ends with nothing
    /// behind it.
    fn unqueue(&mut self) {
        if let (Some(session), Some(output)) = (&mut self.session, &self.output) {
            session.unqueue(output.format(), || output.begin());
        }
    }

    /// Moves within the loaded file; false when it could not.
    fn seek(&mut self, ticket: u64, position: Duration) -> bool {
        let (Some(session), Some(output)) = (&mut self.session, &self.output) else {
            return false;
        };
        session.seek(ticket, position, || output.begin())
    }

    /// Fills the ring, reports the position and the end, and looks after
    /// the output.
    fn advance(&mut self) {
        if self.output.as_ref().is_some_and(Output::failed) {
            self.reopen_output();
        }

        if let (Some(session), Some(output)) = (&mut self.session, &self.output) {
            let events = session.advance(output.format());
            for event in events.into_iter().flatten() {
                self.emit(event);
            }
        }

        if let (Some(at), Some(output)) = (self.rest_at, &self.output)
            && at <= Instant::now()
        {
            output.rest();
            self.rest_at = None;
        }
    }

    /// The output, opened when there is none or it broke.
    fn ensure_output(&mut self) -> Result<&Output, String> {
        if self.output.as_ref().is_none_or(Output::failed) {
            self.output = None;
            let output = Output::open()?;
            output.set_gain(self.gain);
            self.opened_at = Some(Instant::now());
            self.output = Some(output);
        }
        self.output
            .as_ref()
            .ok_or_else(|| "no audio output".to_owned())
    }

    /// Opens the output again after it broke, and has the loaded file go
    /// on from where it was heard.
    fn reopen_output(&mut self) {
        let position = self
            .session
            .as_ref()
            .zip(self.output.as_ref())
            .map(|(session, output)| session.position(output.format()));
        self.output = None;
        let Some(session) = &mut self.session else {
            return;
        };
        // The new output may differ in rate and channels.
        session.converter = None;
        let ticket = session.ticket;

        let unstable = self
            .opened_at
            .is_some_and(|at| at.elapsed() < REOPEN_INTERVAL);
        let reopened = if unstable {
            Err("audio output keeps failing".to_owned())
        } else {
            self.ensure_output().map(|_| ())
        };
        let resumed = reopened.and_then(|()| {
            let position = position.unwrap_or_default();
            if self.seek(ticket, position) {
                Ok(())
            } else {
                Err("playback could not resume on the new output".to_owned())
            }
        });
        match resumed {
            Ok(()) => {
                if let Some(output) = &self.output {
                    output.set_playing(self.playing);
                }
            }
            Err(error) => {
                self.session = None;
                self.silence();
                self.emit(Event::Failed { ticket, error });
            }
        }
    }

    fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.rest_at = (!playing).then(|| Instant::now() + REST_DELAY);
        if let Some(output) = &self.output {
            output.set_playing(playing);
        }
    }

    /// Nothing is loaded: the feed is dropped and the output goes quiet.
    fn silence(&mut self) {
        self.set_playing(false);
        if let Some(output) = &self.output {
            drop(output.begin());
        }
    }
}

impl Session {
    fn new(ticket: u64, decoder: Decoder, feed: Feed) -> Self {
        Self {
            ticket,
            decoder,
            decoded: false,
            base: Duration::ZERO,
            origin: 0,
            finished: false,
            reported: None,
            next: None,
            feed,
            converter: None,
            queued: Vec::new(),
            queued_from: 0,
            fed: 0,
        }
    }

    /// The feed's frame the queued file starts at, once it is in the feed.
    fn next_start(&self) -> Option<u64> {
        self.next.as_ref().and_then(|next| next.start)
    }

    /// Where in the file the output is; `format` is the output's.
    fn position(&self, format: Format) -> Duration {
        let played = self.feed.played_frames().saturating_sub(self.origin);
        let position = self.base + Duration::from_secs_f64(played as f64 / f64::from(format.rate));
        self.decoder
            .duration()
            .map_or(position, |end| position.min(end))
    }

    /// Moves within the file, in a feed of its own (`begin` makes it):
    /// what was queued of the old one is not heard. False when it could
    /// not, and nothing changed.
    fn seek(&mut self, ticket: u64, position: Duration, begin: impl FnOnce() -> Feed) -> bool {
        self.ticket = ticket;
        let landed = match self.decoder.seek(position) {
            Ok(landed) => {
                self.decoded = false;
                landed
            }
            // Past the last frame: the file is over.
            Err(_) if self.decoder.duration().is_some_and(|end| position >= end) => {
                self.decoded = true;
                position
            }
            Err(error) => {
                eprintln!("Seek failed: {error}");
                return false;
            }
        };
        if let Some(converter) = &mut self.converter {
            converter.reset();
        }
        self.queued.clear();
        self.queued_from = 0;
        self.fed = 0;
        self.feed = begin();
        self.base = landed;
        self.origin = 0;
        self.finished = false;
        self.reported = None;

        // What the queued file had in the old feed went with it.
        if let Some(next) = &mut self.next
            && next.start.take().is_some()
        {
            next.decoded = false;
            if let Err(error) = next.decoder.restart() {
                eprintln!("Queued file failed to open: {error}");
                self.next = None;
            }
        }
        true
    }

    /// Takes the queued file back. It may be in the feed already, behind
    /// the file that is heard: the feed (`begin` makes one) then starts
    /// again from what is heard. `format` is the output's.
    fn unqueue(&mut self, format: Format, begin: impl FnOnce() -> Feed) {
        if self.next_start().is_some() {
            let (heard, position) = (self.ticket, self.position(format));
            self.seek(heard, position, begin);
        }
        self.next = None;
    }

    /// Tops the ring up, then tells what happened: the queued file began
    /// to be heard, the position moved, the last frame was played.
    fn advance(&mut self, format: Format) -> [Option<Event>; 3] {
        self.fill(format);

        let started = self.hand_over().then(|| Event::Started {
            ticket: self.ticket,
            duration: self.decoder.duration(),
        });
        let position = self.position(format);
        let moved = self
            .reported
            .is_none_or(|reported| position.abs_diff(reported) >= POSITION_STEP);
        let ended = self.decoded && self.next.is_none() && !self.finished && self.is_played_out();
        let position_event = (moved || ended).then(|| {
            self.reported = Some(position);
            Event::Position {
                ticket: self.ticket,
                position,
            }
        });
        let finished_event = ended.then(|| {
            self.finished = true;
            Event::Finished {
                ticket: self.ticket,
            }
        });
        [started, position_event, finished_event]
    }

    /// Makes the queued file the one that is heard, once the output has
    /// played up to its first frame.
    fn hand_over(&mut self) -> bool {
        let played = self.feed.played_frames();
        let Some(start) = self.next_start().filter(|&start| played >= start) else {
            return false;
        };
        let Some(next) = self.next.take() else {
            return false;
        };
        self.ticket = next.ticket;
        self.decoder = next.decoder;
        self.decoded = next.decoded;
        self.base = Duration::ZERO;
        self.origin = start;
        self.reported = None;
        true
    }

    fn is_played_out(&self) -> bool {
        self.queued_from >= self.queued.len() && self.feed.is_drained()
    }

    /// Decodes into the ring until it is full: the file, then the queued
    /// one right behind its last frame.
    fn fill(&mut self, format: Format) {
        loop {
            if self.queued_from < self.queued.len() {
                self.queued_from += self.feed.push(&self.queued[self.queued_from..]);
                if self.queued_from < self.queued.len() {
                    // The ring is full.
                    return;
                }
                self.queued.clear();
                self.queued_from = 0;
            }

            let (decoder, queued) = if !self.decoded {
                (&mut self.decoder, false)
            } else {
                match &mut self.next {
                    Some(next) if !next.decoded => {
                        if next.start.is_none() {
                            // Behind what the converter still holds of
                            // the file before it.
                            let pending = self.converter.as_ref().map_or(0, Converter::pending);
                            next.start = Some(self.fed + pending);
                        }
                        (&mut next.decoder, true)
                    }
                    _ => {
                        // Nothing is left to decode. What the converter
                        // kept for a queued file that never came is due.
                        let converter = self.converter.as_mut();
                        if let Some(converter) = converter.filter(|held| held.pending() > 0) {
                            converter.finish(&mut self.queued);
                            self.fed += (self.queued.len() / format.channels) as u64;
                            continue;
                        }
                        return;
                    }
                }
            };

            let ended = match decoder.next() {
                Ok(Some((from, samples))) => {
                    let mut converted = true;
                    if self
                        .converter
                        .as_ref()
                        .is_none_or(|converter| converter.from() != from)
                    {
                        // Another layout: what the converter holds of the
                        // frames before these comes out first.
                        if let Some(converter) = &mut self.converter {
                            converter.finish(&mut self.queued);
                        }
                        self.converter = Converter::new(from, format)
                            .inspect_err(|error| eprintln!("Playback stopped: {error}"))
                            .ok();
                        converted = self.converter.is_some();
                    }
                    if let Some(converter) = &mut self.converter {
                        converter.push(samples, &mut self.queued);
                    }
                    !converted
                }
                Ok(None) => true,
                Err(error) => {
                    eprintln!("Playback stopped: {error}");
                    true
                }
            };
            if ended {
                match &mut self.next {
                    Some(next) if queued => next.decoded = true,
                    _ => self.decoded = true,
                }
                // Nothing follows in this feed: the converter gives what
                // it holds. Before a queued file it keeps it, so that the
                // two run on as one.
                if (queued || self.next.is_none())
                    && let Some(converter) = &mut self.converter
                {
                    converter.finish(&mut self.queued);
                }
            }
            self.fed += (self.queued.len() / format.channels) as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::output::Source;
    use super::*;

    const RATE: u32 = 44_100;
    const PITCH: f64 = 440.0;
    /// The frames of each of the two files the tone is cut into: not a
    /// whole number of its waves, so the cut is mid-wave.
    const HALF: usize = 30_000;

    /// A 440 Hz tone at half scale, as 16-bit samples.
    fn tone(frames: usize, rate: u32) -> Vec<i16> {
        (0..frames)
            .map(|frame| {
                let phase = frame as f64 * PITCH * std::f64::consts::TAU / f64::from(rate);
                (phase.sin() * 0.5 * 32_767.0).round() as i16
            })
            .collect()
    }

    /// Writes `samples` as a mono 16-bit WAV file.
    fn wav(path: &Path, rate: u32, samples: &[i16]) {
        let size = samples.len() as u32 * 2;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * 2).to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        bytes.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        std::fs::write(path, bytes).unwrap();
    }

    /// The tone cut into two files, and the folder that holds them.
    fn two_halves() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<i16>) {
        let folder = tempfile::tempdir().unwrap();
        let (first, second) = (folder.path().join("1.wav"), folder.path().join("2.wav"));
        let whole = tone(HALF * 2, RATE);
        wav(&first, RATE, &whole[..HALF]);
        wav(&second, RATE, &whole[HALF..]);
        (folder, first, second, whole)
    }

    /// A session of `first` with `second` queued, feeding an output of
    /// `format` that the test stands in for.
    fn session(first: &Path, second: &Path, format: Format) -> (Session, Source) {
        let (feed, source) = Feed::new(format);
        let mut session = Session::new(1, Decoder::open(first).unwrap(), feed);
        session.next = Some(Next {
            ticket: 2,
            decoder: Decoder::open(second).unwrap(),
            start: None,
            decoded: false,
        });
        (session, source)
    }

    /// Plays the session to its end: what the output took, and what the
    /// session reported.
    fn play_out(
        session: &mut Session,
        source: &mut Source,
        format: Format,
    ) -> (Vec<f32>, Vec<Event>) {
        let (mut heard, mut events) = (Vec::new(), Vec::new());
        for _ in 0..1_000 {
            events.extend(session.advance(format).into_iter().flatten());
            if matches!(events.last(), Some(Event::Finished { .. })) {
                return (heard, events);
            }
            source.take(&mut heard, format.channels);
        }
        panic!("the session never finished: {events:?}");
    }

    fn assert_same(heard: &[f32], samples: &[i16]) {
        assert_eq!(heard.len(), samples.len());
        for (frame, (heard, sample)) in heard.iter().zip(samples).enumerate() {
            let expected = f32::from(*sample) / 32_768.0;
            assert!(
                (heard - expected).abs() < 1e-6,
                "frame {frame}: {heard} vs {expected}"
            );
        }
    }

    #[test]
    fn a_queued_file_plays_on_without_a_gap_or_a_changed_sample() {
        let (_folder, first, second, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (mut session, mut source) = session(&first, &second, format);

        let (heard, events) = play_out(&mut session, &mut source, format);

        assert_same(&heard, &whole);
        assert_eq!(session.origin, HALF as u64);
        // The first file never ends for the app: the second starts.
        let milestones: Vec<&Event> = events
            .iter()
            .filter(|event| !matches!(event, Event::Position { .. }))
            .collect();
        assert!(matches!(
            milestones[..],
            [
                Event::Started { ticket: 2, duration: Some(duration) },
                Event::Finished { ticket: 2 }
            ] if duration.abs_diff(Duration::from_secs_f64(HALF as f64 / f64::from(RATE))) < Duration::from_millis(1)
        ));
        assert!(matches!(
            events.first(),
            Some(Event::Position { ticket: 1, position }) if position.is_zero()
        ));
    }

    #[test]
    fn a_queued_file_runs_through_the_resampler_as_one_with_the_file_before() {
        let (_folder, first, second, _) = two_halves();
        let format = Format {
            rate: 48_000,
            channels: 2,
        };
        let (mut session, mut source) = session(&first, &second, format);

        let (heard, events) = play_out(&mut session, &mut source, format);

        let frames = (HALF * 2 * 48_000).div_ceil(RATE as usize);
        assert_eq!(heard.len(), frames * 2);
        let seam = HALF * 48_000 / RATE as usize;
        assert!(session.origin.abs_diff(seam as u64) <= 1);
        assert!(events.contains(&Event::Finished { ticket: 2 }));

        // Around the seam the tone goes on: as loud, and with no step
        // larger than its own.
        let around: Vec<f32> = heard[(seam - 2_000) * 2..(seam + 2_000) * 2]
            .chunks(2)
            .map(|frame| frame[0])
            .collect();
        let largest_step = around
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        let own_step = (0.5 * PITCH * std::f64::consts::TAU / 48_000.0) as f32;
        assert!(
            largest_step < own_step * 1.01,
            "{largest_step} vs {own_step}"
        );
        let level =
            (around.iter().map(|sample| sample * sample).sum::<f32>() / around.len() as f32).sqrt();
        assert!((level - 0.5 / 2.0_f32.sqrt()).abs() < 0.002, "{level}");
    }

    #[test]
    fn a_queued_file_in_another_layout_follows_at_its_own_length() {
        let (folder, first, _, whole) = two_halves();
        let slow = folder.path().join("slow.wav");
        wav(&slow, RATE / 2, &tone(HALF / 2, RATE / 2));
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (mut session, mut source) = session(&first, &slow, format);

        let (heard, events) = play_out(&mut session, &mut source, format);

        assert_eq!(heard.len(), HALF * 2);
        assert_same(&heard[..HALF], &whole[..HALF]);
        assert_eq!(session.origin, HALF as u64);
        assert!(events.contains(&Event::Finished { ticket: 2 }));
    }

    #[test]
    fn a_seek_takes_the_queued_file_back_out_of_the_feed() {
        let (_folder, first, second, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (mut session, _source) = session(&first, &second, format);
        // Both files fit the ring: the queued one is in it already.
        let _ = session.advance(format);
        assert_eq!(session.next_start(), Some(HALF as u64));

        let (feed, mut source) = Feed::new(format);
        assert!(session.seek(3, Duration::from_millis(500), || feed));
        assert_eq!(session.next_start(), None);
        let (heard, events) = play_out(&mut session, &mut source, format);

        let from = RATE as usize / 2;
        assert_same(&heard, &whole[from..]);
        assert!(matches!(
            events.first(),
            Some(Event::Position { ticket: 3, position }) if *position == Duration::from_millis(500)
        ));
        assert!(events.contains(&Event::Finished { ticket: 2 }));
    }

    #[test]
    fn a_queued_file_taken_back_leaves_the_loaded_one_to_end_alone() {
        let (_folder, first, second, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (mut session, _source) = session(&first, &second, format);
        // Both files fit the ring: the queued one is in it already.
        let _ = session.advance(format);
        assert_eq!(session.next_start(), Some(HALF as u64));

        let (feed, mut source) = Feed::new(format);
        session.unqueue(format, || feed);
        assert!(session.next.is_none());
        let (heard, events) = play_out(&mut session, &mut source, format);

        // Nothing was heard yet: the first file plays whole, and ends.
        assert_same(&heard, &whole[..HALF]);
        assert_eq!(events.last(), Some(&Event::Finished { ticket: 1 }));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Started { .. }))
        );

        // Queued again after it was taken back, a file plays on as ever.
        let (feed, mut source) = Feed::new(format);
        let mut session = Session::new(1, Decoder::open(&first).unwrap(), feed);
        session.unqueue(format, || unreachable!("nothing is queued"));
        session.next = Some(Next {
            ticket: 2,
            decoder: Decoder::open(&second).unwrap(),
            start: None,
            decoded: false,
        });
        let (heard, _) = play_out(&mut session, &mut source, format);
        assert_same(&heard, &whole);
    }

    #[test]
    fn a_file_queued_behind_itself_repeats_without_a_gap() {
        let (_folder, first, _, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (mut session, mut source) = session(&first, &first, format);

        let (heard, events) = play_out(&mut session, &mut source, format);

        assert_eq!(heard.len(), HALF * 2);
        assert_same(&heard[..HALF], &whole[..HALF]);
        assert_same(&heard[HALF..], &whole[..HALF]);
        assert!(events.contains(&Event::Finished { ticket: 2 }));
    }

    #[test]
    fn a_cued_file_waits_at_its_position_and_plays_on_from_there() {
        let (_folder, first, second, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        // What the engine does for `Command::Cue`: a load, then a seek.
        let (feed, _unheard) = Feed::new(format);
        let mut session = Session::new(4, Decoder::open(&first).unwrap(), feed);
        let (feed, mut source) = Feed::new(format);
        assert!(session.seek(4, Duration::from_millis(250), || feed));

        // Paused, nothing takes from the feed: however often the engine
        // looks, the position holds and nothing ends.
        let mut events = Vec::new();
        for _ in 0..5 {
            events.extend(session.advance(format).into_iter().flatten());
        }
        assert_eq!(
            events,
            [Event::Position {
                ticket: 4,
                position: Duration::from_millis(250)
            }]
        );

        // The file that follows is queued behind it as behind any other.
        session.next = Some(Next {
            ticket: 5,
            decoder: Decoder::open(&second).unwrap(),
            start: None,
            decoded: false,
        });
        let (heard, events) = play_out(&mut session, &mut source, format);

        let from = RATE as usize / 4;
        assert_same(&heard, &whole[from..]);
        assert!(events.contains(&Event::Finished { ticket: 5 }));
    }

    #[test]
    fn a_file_with_nothing_queued_finishes() {
        let (_folder, first, _, whole) = two_halves();
        let format = Format {
            rate: RATE,
            channels: 1,
        };
        let (feed, mut source) = Feed::new(format);
        let mut session = Session::new(7, Decoder::open(&first).unwrap(), feed);

        let (heard, events) = play_out(&mut session, &mut source, format);

        assert_same(&heard, &whole[..HALF]);
        assert_eq!(events.last(), Some(&Event::Finished { ticket: 7 }));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Started { .. }))
        );
    }
}

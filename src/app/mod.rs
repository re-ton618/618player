mod input;
mod playback;
mod store;
mod tab_store;
mod tabs;
mod view;

use std::collections::HashMap;

use nsh_core::prelude::SharedString;

use crate::audio;
use crate::library::{LibraryEvent, SortColumn, SortDirection, Track};

pub(crate) use store::Store;
pub(crate) use view::root;

/// Tracks in a row the player may fail on before the queue gives up.
const MAX_PLAYBACK_FAILURES: u8 = 5;
/// How far into a track "previous" starts it again instead of leaving it.
const RESTART_AFTER_MS: u64 = 3_000;

/// The player's state, free of any UI: `update` changes it, and the store
/// carries out the [`Effect`]s it returns.
#[derive(Clone)]
pub(crate) struct Model {
    tracks: Vec<Track>,
    track_positions: HashMap<i64, usize>,
    visible_tracks: Vec<usize>,
    search_query: String,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    tabs: tabs::State,
    playback: playback::State,
    /// The tracks that play one after another: the library as it was
    /// listed when one of them was started.
    queue: Vec<i64>,
    /// Tracks in a row the player failed on.
    playback_failures: u8,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            track_positions: HashMap::new(),
            visible_tracks: Vec::new(),
            search_query: String::new(),
            sort_column: SortColumn::Title,
            sort_direction: SortDirection::Ascending,
            tabs: tabs::State::default(),
            playback: playback::State::default(),
            queue: Vec::new(),
            playback_failures: 0,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    LibraryEvent(LibraryEvent),
    SearchChanged(String),
    SortChanged(SortColumn),
    TrackPressed(i64),
    ArtworkLoaded {
        request_id: u64,
        result: Result<Option<SharedString>, String>,
    },
    PlayPausePressed,
    PreviousPressed,
    NextPressed,
    /// The user moved the position, to this many milliseconds.
    PositionChanged(u64),
    Player(audio::Event),
    FileMenuPressed,
    VolumeChanged(u8),
    Tabs(tabs::Message),
}

/// What an update asks of the world outside the model.
#[derive(Debug, PartialEq, Eq)]
enum Effect {
    LoadArtwork(playback::ArtworkRequest),
    ScrollLibraryToTop,
    Player(audio::Command),
}

fn update(model: &mut Model, message: Message) -> Vec<Effect> {
    match message {
        Message::LibraryEvent(event) => match event {
            LibraryEvent::Cached(tracks) | LibraryEvent::Reconciled(tracks) => {
                return model.set_tracks(tracks);
            }
            LibraryEvent::MetadataBatch(updates) => {
                let current_track_id = model.playback.current_track_id();
                let mut current_track_updated = false;

                for update in updates {
                    let update_id = update.id;
                    if let Some(&position) = model.track_positions.get(&update_id) {
                        model.tracks[position].apply(update);
                        current_track_updated |= Some(update_id) == current_track_id;
                    }
                }

                if current_track_updated {
                    return model.reconcile_current_track();
                }
            }
            LibraryEvent::Complete => model.rebuild_projection(),
            LibraryEvent::Failed(error) => eprintln!("Library indexing failed: {error}"),
        },
        Message::SearchChanged(query) => {
            if model.search_query == query {
                return Vec::new();
            }
            model.search_query = query;
            model.rebuild_projection();
            return vec![Effect::ScrollLibraryToTop];
        }
        Message::SortChanged(column) => {
            if model.sort_column == column {
                model.sort_direction = model.sort_direction.toggled();
            } else {
                model.sort_column = column;
                model.sort_direction = SortDirection::Ascending;
            }
            model.rebuild_projection();
            return vec![Effect::ScrollLibraryToTop];
        }
        Message::TrackPressed(track_id) => {
            let Some(&position) = model.track_positions.get(&track_id) else {
                return Vec::new();
            };
            model.queue_visible_tracks();
            return model.play(position);
        }
        Message::ArtworkLoaded { request_id, result } => {
            return match model.playback.loaded(request_id, result) {
                playback::ArtworkEffect::None => Vec::new(),
                playback::ArtworkEffect::Load { request, error } => {
                    if let Some(error) = error {
                        eprintln!("Album artwork failed: {error}");
                    }
                    vec![Effect::LoadArtwork(request)]
                }
                playback::ArtworkEffect::Missing { error } => {
                    if let Some(error) = error {
                        eprintln!("Album artwork failed: {error}");
                    }
                    Vec::new()
                }
            };
        }
        Message::PlayPausePressed => {
            if let Some(command) = model.playback.toggle() {
                return vec![Effect::Player(command)];
            }
            // Stopped: the current track again, else the list from its top.
            let position = model.current_position().or_else(|| {
                model.queue_visible_tracks();
                model.visible_tracks.first().copied()
            });
            if let Some(position) = position {
                return model.play(position);
            }
        }
        Message::PreviousPressed => {
            let previous = model.queued_neighbor(Neighbor::Previous);
            let stopped = model.playback.transport() == playback::Transport::Stopped;
            let restart = model.playback.position_ms() > RESTART_AFTER_MS || previous.is_none();
            if !stopped && restart {
                return model
                    .playback
                    .seek(0)
                    .map(Effect::Player)
                    .into_iter()
                    .collect();
            }
            if let Some(position) = previous {
                return model.play(position);
            }
        }
        Message::NextPressed => {
            if let Some(position) = model.queued_neighbor(Neighbor::Next) {
                return model.play(position);
            }
        }
        Message::PositionChanged(position_ms) => {
            return model
                .playback
                .seek(position_ms)
                .map(Effect::Player)
                .into_iter()
                .collect();
        }
        Message::Player(event) => match model.playback.report(event) {
            playback::Report::Handled => {}
            playback::Report::Advanced(track_id) => {
                model.playback_failures = 0;
                return model.follow(track_id);
            }
            playback::Report::Finished => {
                model.playback_failures = 0;
                return model.play_next_or_stop();
            }
            playback::Report::Failed(error) => {
                eprintln!("Playback failed: {error}");
                model.playback_failures += 1;
                if model.playback_failures >= MAX_PLAYBACK_FAILURES {
                    model.playback_failures = 0;
                    return vec![Effect::Player(model.playback.stop())];
                }
                // An unplayable file does not end the queue.
                return model.play_next_or_stop();
            }
        },
        Message::FileMenuPressed => {}
        Message::VolumeChanged(volume) => {
            model.playback.set_volume(volume);
            return vec![Effect::Player(audio::Command::Volume(
                model.playback.volume(),
            ))];
        }
        Message::Tabs(message) => {
            let outcome = model.tabs.update(message);
            if outcome.persist
                && let Err(error) = tab_store::save(&model.tabs)
            {
                eprintln!("Workspace save failed: {error}");
            }
        }
    }

    Vec::new()
}

/// A side of the current track in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Neighbor {
    Previous,
    Next,
}

impl Model {
    /// The model at start: the workspace's tabs as they were saved.
    fn restored() -> Self {
        let mut model = Self::default();
        match tab_store::load() {
            Ok(Some(tabs)) => model.tabs = tabs,
            Ok(None) => {}
            Err(error) => eprintln!("Workspace restore failed: {error}"),
        }
        model
    }

    fn set_tracks(&mut self, tracks: Vec<Track>) -> Vec<Effect> {
        self.tracks = tracks;
        self.track_positions = self
            .tracks
            .iter()
            .enumerate()
            .map(|(position, track)| (track.id, position))
            .collect();
        self.rebuild_projection();
        self.reconcile_current_track()
    }

    /// Follows the current track through a change of the library: its
    /// artwork is read again when it differs, and a track that is gone
    /// stops.
    fn reconcile_current_track(&mut self) -> Vec<Effect> {
        let Some(track_id) = self.playback.current_track_id() else {
            return Vec::new();
        };
        let Some(&position) = self.track_positions.get(&track_id) else {
            return vec![Effect::Player(self.playback.clear())];
        };

        self.playback
            .select(&self.tracks[position])
            .map(Effect::LoadArtwork)
            .into_iter()
            .collect()
    }

    /// Makes the track at `position` current and starts it, with the
    /// queue's next one behind it.
    fn play(&mut self, position: usize) -> Vec<Effect> {
        let track = &self.tracks[position];
        let artwork = self.playback.select(track);
        let load = self.playback.play(track);
        artwork
            .map(Effect::LoadArtwork)
            .into_iter()
            .chain([Effect::Player(load)])
            .chain(self.queue_next())
            .collect()
    }

    /// Makes current the track the player went on to by itself, and puts
    /// the queue's next one behind it.
    fn follow(&mut self, track_id: i64) -> Vec<Effect> {
        let Some(&position) = self.track_positions.get(&track_id) else {
            // The library lost it meanwhile: on to one it still has.
            return self.play_next_or_stop();
        };
        let artwork = self.playback.select(&self.tracks[position]);
        artwork
            .map(Effect::LoadArtwork)
            .into_iter()
            .chain(self.queue_next())
            .collect()
    }

    /// Has the player go on, gaplessly, with the track after the current
    /// one when that ends.
    fn queue_next(&mut self) -> Option<Effect> {
        let position = self.queued_neighbor(Neighbor::Next)?;
        self.playback
            .queue(&self.tracks[position])
            .map(Effect::Player)
    }

    /// Goes on to the queue's next track; after its last, playback stops.
    fn play_next_or_stop(&mut self) -> Vec<Effect> {
        match self.queued_neighbor(Neighbor::Next) {
            Some(position) => self.play(position),
            None => vec![Effect::Player(self.playback.stop())],
        }
    }

    /// Queues the library as it is listed now, searched and sorted.
    fn queue_visible_tracks(&mut self) {
        self.playback_failures = 0;
        self.queue = self
            .visible_tracks
            .iter()
            .map(|&position| self.tracks[position].id)
            .collect();
    }

    /// The nearest track of the queue on that side of the current one that
    /// the library still has.
    fn queued_neighbor(&self, neighbor: Neighbor) -> Option<usize> {
        let current = self.playback.current_track_id()?;
        let at = self.queue.iter().position(|&id| id == current)?;
        let in_library = |id: &i64| self.track_positions.get(id).copied();
        match neighbor {
            Neighbor::Previous => self.queue[..at].iter().rev().find_map(in_library),
            Neighbor::Next => self.queue[at + 1..].iter().find_map(in_library),
        }
    }

    /// Where the current track is in `tracks`.
    fn current_position(&self) -> Option<usize> {
        self.playback
            .current_track_id()
            .and_then(|track_id| self.track_positions.get(&track_id).copied())
    }

    pub(crate) fn current_track(&self) -> Option<&Track> {
        self.current_position()
            .map(|position| &self.tracks[position])
    }

    /// The track shown in row `row` of the library, as searched and sorted.
    fn visible_track(&self, row: usize) -> Option<&Track> {
        self.visible_tracks
            .get(row)
            .map(|&position| &self.tracks[position])
    }

    fn rebuild_projection(&mut self) {
        self.visible_tracks = crate::library::matching_track_indices(
            &self.tracks,
            &self.search_query,
            self.sort_column,
            self.sort_direction,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::library::test_track;

    use super::*;

    /// Three tracks of one album, listed by title: 1, 2, 3.
    fn app_with_three_tracks() -> Model {
        let tracks = ["a", "b", "c"]
            .iter()
            .zip(1..)
            .map(|(name, id)| {
                test_track(
                    id,
                    PathBuf::from(format!("/music/album/{name}.flac")),
                    Some("album"),
                )
            })
            .collect();
        let mut app = Model::default();
        let _ = app.set_tracks(tracks);
        app
    }

    /// The ticket and the path of the load among `effects`.
    fn loaded(effects: &[Effect]) -> (u64, PathBuf) {
        effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Player(audio::Command::Load { ticket, path }) => {
                    Some((*ticket, path.clone()))
                }
                _ => None,
            })
            .expect("a track was loaded")
    }

    fn current_id(app: &Model) -> Option<i64> {
        app.current_track().map(|track| track.id)
    }

    #[test]
    fn unknown_track_press_preserves_current_track() {
        let track = test_track(1, PathBuf::from("/music/album/current.flac"), Some("album"));
        let mut app = Model::default();
        assert!(app.set_tracks(vec![track]).is_empty());
        let _ = update(&mut app, Message::TrackPressed(1));
        assert_eq!(current_id(&app), Some(1));

        assert!(update(&mut app, Message::TrackPressed(999)).is_empty());

        assert_eq!(current_id(&app), Some(1));
    }

    #[test]
    fn library_replacement_without_selected_id_clears_current_track() {
        let selected = test_track(
            1,
            PathBuf::from("/music/first/selected.flac"),
            Some("first"),
        );
        let replacement = test_track(
            2,
            PathBuf::from("/music/second/replacement.flac"),
            Some("second"),
        );
        let mut app = Model::default();
        assert!(app.set_tracks(vec![selected]).is_empty());
        let _ = update(&mut app, Message::TrackPressed(1));
        assert!(app.current_track().is_some());

        let effects = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Reconciled(vec![replacement])),
        );

        assert!(app.current_track().is_none());
        assert_eq!(effects, [Effect::Player(audio::Command::Stop)]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
    }

    #[test]
    fn a_pressed_track_plays_and_reads_its_artwork() {
        let mut app = app_with_three_tracks();

        let effects = update(&mut app, Message::TrackPressed(2));

        assert!(matches!(effects[0], Effect::LoadArtwork(_)));
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
        assert_eq!(app.playback.transport(), playback::Transport::Playing);
        assert_eq!(current_id(&app), Some(2));
    }

    #[test]
    fn play_pause_starts_the_list_then_pauses_and_resumes() {
        let mut app = app_with_three_tracks();

        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/a.flac"));

        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(effects, [Effect::Player(audio::Command::Pause)]);
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(effects, [Effect::Player(audio::Command::Play)]);
    }

    #[test]
    fn next_and_previous_walk_the_queue_as_it_was_listed() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::TrackPressed(2));
        // A search after the press does not change what comes next.
        let _ = update(&mut app, Message::SearchChanged("a".to_owned()));

        let effects = update(&mut app, Message::NextPressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/c.flac"));
        assert!(update(&mut app, Message::NextPressed).is_empty());
        assert_eq!(current_id(&app), Some(3));

        let effects = update(&mut app, Message::PreviousPressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
    }

    #[test]
    fn previous_restarts_a_track_that_is_under_way_or_first() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(2));
        let (ticket, _) = loaded(&effects);
        let position = audio::Event::Position {
            ticket,
            position: Duration::from_secs(10),
        };
        let _ = update(&mut app, Message::Player(position));

        let effects = update(&mut app, Message::PreviousPressed);
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { position, .. })] if position.is_zero()
        ));
        assert_eq!(current_id(&app), Some(2));

        let _ = update(&mut app, Message::PreviousPressed);
        assert_eq!(current_id(&app), Some(1));
        let effects = update(&mut app, Message::PreviousPressed);
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { .. })]
        ));
        assert_eq!(current_id(&app), Some(1));
    }

    #[test]
    fn a_finished_track_goes_on_to_the_next_and_the_last_one_stops() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(2));
        let (ticket, _) = loaded(&effects);

        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        let (ticket, path) = loaded(&effects);
        assert_eq!(path, PathBuf::from("/music/album/c.flac"));

        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        assert_eq!(effects, [Effect::Player(audio::Command::Stop)]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
        assert_eq!(current_id(&app), Some(3));

        // Play starts the track it stopped on again.
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/c.flac"));
    }

    /// The ticket and the path of the file queued among `effects`.
    fn queued(effects: &[Effect]) -> Option<(u64, PathBuf)> {
        effects.iter().find_map(|effect| match effect {
            Effect::Player(audio::Command::Queue { ticket, path }) => Some((*ticket, path.clone())),
            _ => None,
        })
    }

    #[test]
    fn a_started_track_has_the_next_queued_and_follows_the_player_to_it() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(1));
        let (ticket, path) = queued(&effects).expect("the next track is queued");
        assert_eq!(path, PathBuf::from("/music/album/b.flac"));

        // The player went on by itself: nothing is loaded again.
        let started = audio::Event::Started {
            ticket,
            duration: Some(Duration::from_secs(3)),
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(2));
        assert_eq!(app.playback.position_ms(), 0);
        assert_eq!(app.playback.duration_ms(), Some(3_000));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Player(audio::Command::Load { .. })))
        );
        let (ticket, path) = queued(&effects).expect("the track after it is queued");
        assert_eq!(path, PathBuf::from("/music/album/c.flac"));

        // Behind the last track nothing is queued, and its end stops.
        let started = audio::Event::Started {
            ticket,
            duration: None,
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(3));
        assert!(queued(&effects).is_none());
        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        assert_eq!(effects, [Effect::Player(audio::Command::Stop)]);
    }

    #[test]
    fn an_unplayable_track_is_skipped() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(1));
        let (ticket, _) = loaded(&effects);
        let failed = audio::Event::Failed {
            ticket,
            error: "unsupported file".to_owned(),
        };

        let effects = update(&mut app, Message::Player(failed));

        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
    }

    #[test]
    fn volume_and_position_reach_the_player() {
        let mut app = app_with_three_tracks();
        assert!(update(&mut app, Message::PositionChanged(1_000)).is_empty());

        let effects = update(&mut app, Message::VolumeChanged(40));
        assert_eq!(effects, [Effect::Player(audio::Command::Volume(40))]);

        let _ = update(&mut app, Message::TrackPressed(1));
        let effects = update(&mut app, Message::PositionChanged(1_000));
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { position, .. })]
                if position == Duration::from_secs(1)
        ));
        assert_eq!(app.playback.position_ms(), 1_000);
    }
}

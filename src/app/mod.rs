mod input;
mod playback;
mod store;
mod tab_store;
mod tabs;
mod view;

use std::collections::HashMap;

use nsh_core::prelude::SharedString;

use crate::library::{LibraryEvent, SortColumn, SortDirection, Track};

pub(crate) use store::Store;
pub(crate) use view::root;

/// The player's state, free of any UI: `update` changes it, and the store
/// carries out the [`Effect`] it returns.
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
    PlaybackControlPressed,
    FileMenuPressed,
    VolumeChanged(u8),
    Tabs(tabs::Message),
}

/// What an update asks of the world outside the model.
#[derive(Debug, PartialEq, Eq)]
enum Effect {
    None,
    LoadArtwork(playback::ArtworkRequest),
    ScrollLibraryToTop,
}

fn update(model: &mut Model, message: Message) -> Effect {
    match message {
        Message::LibraryEvent(event) => match event {
            LibraryEvent::Cached(tracks) | LibraryEvent::Reconciled(tracks) => {
                if let Some(request) = model.set_tracks(tracks) {
                    return Effect::LoadArtwork(request);
                }
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

                if current_track_updated && let Some(request) = model.reconcile_current_track() {
                    return Effect::LoadArtwork(request);
                }
            }
            LibraryEvent::Complete => model.rebuild_projection(),
            LibraryEvent::Failed(error) => eprintln!("Library indexing failed: {error}"),
        },
        Message::SearchChanged(query) => {
            if model.search_query == query {
                return Effect::None;
            }
            model.search_query = query;
            model.rebuild_projection();
            return Effect::ScrollLibraryToTop;
        }
        Message::SortChanged(column) => {
            if model.sort_column == column {
                model.sort_direction = model.sort_direction.toggled();
            } else {
                model.sort_column = column;
                model.sort_direction = SortDirection::Ascending;
            }
            model.rebuild_projection();
            return Effect::ScrollLibraryToTop;
        }
        Message::TrackPressed(track_id) => {
            let Some(&position) = model.track_positions.get(&track_id) else {
                return Effect::None;
            };
            if let Some(request) = model.playback.select(&model.tracks[position]) {
                return Effect::LoadArtwork(request);
            }
        }
        Message::ArtworkLoaded { request_id, result } => {
            return match model.playback.loaded(request_id, result) {
                playback::ArtworkEffect::None => Effect::None,
                playback::ArtworkEffect::Load { request, error } => {
                    if let Some(error) = error {
                        eprintln!("Album artwork failed: {error}");
                    }
                    Effect::LoadArtwork(request)
                }
                playback::ArtworkEffect::Missing { error } => {
                    if let Some(error) = error {
                        eprintln!("Album artwork failed: {error}");
                    }
                    Effect::None
                }
            };
        }
        Message::PlaybackControlPressed | Message::FileMenuPressed => {}
        Message::VolumeChanged(volume) => model.playback.set_volume(volume),
        Message::Tabs(message) => {
            let outcome = model.tabs.update(message);
            if outcome.persist
                && let Err(error) = tab_store::save(&model.tabs)
            {
                eprintln!("Workspace save failed: {error}");
            }
        }
    }

    Effect::None
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

    fn set_tracks(&mut self, tracks: Vec<Track>) -> Option<playback::ArtworkRequest> {
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

    fn reconcile_current_track(&mut self) -> Option<playback::ArtworkRequest> {
        let track_id = self.playback.current_track_id()?;
        let Some(&position) = self.track_positions.get(&track_id) else {
            self.playback.clear();
            return None;
        };

        self.playback.select(&self.tracks[position])
    }

    pub(crate) fn current_track(&self) -> Option<&Track> {
        self.playback
            .current_track_id()
            .and_then(|track_id| self.track_positions.get(&track_id))
            .map(|&position| &self.tracks[position])
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

    use crate::library::test_track;

    use super::*;

    #[test]
    fn unknown_track_press_preserves_current_track() {
        let track = test_track(1, PathBuf::from("/music/album/current.flac"), Some("album"));
        let mut app = Model::default();
        assert!(app.set_tracks(vec![track]).is_none());
        let _ = update(&mut app, Message::TrackPressed(1));
        assert_eq!(app.current_track().map(|track| track.id), Some(1));

        let _ = update(&mut app, Message::TrackPressed(999));

        assert_eq!(app.current_track().map(|track| track.id), Some(1));
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
        assert!(app.set_tracks(vec![selected]).is_none());
        let _ = update(&mut app, Message::TrackPressed(1));
        assert!(app.current_track().is_some());

        let _ = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Reconciled(vec![replacement])),
        );

        assert!(app.current_track().is_none());
    }
}

//! Playlists as the app holds them: the library keeps them, and tells
//! them whole after every change ([`State::set`]). Which one the
//! Playlists tab shows, the artwork it shows of it, and the prompt that
//! names one are the app's own.

use std::path::{Path, PathBuf};

use nsh_core::prelude::SharedString;

use crate::library::{Playlist, Track};

/// Where the naming prompt's name goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Purpose {
    /// A new playlist, holding these tracks.
    Create(Vec<i64>),
    /// The playlist of this id, renamed.
    Rename(i64),
}

/// The prompt that asks for a playlist's name, and what is typed in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Prompt {
    pub(super) purpose: Purpose,
    pub(super) name: String,
}

impl Prompt {
    pub(super) const fn title(&self) -> &'static str {
        match self.purpose {
            Purpose::Create(_) => "New Playlist",
            Purpose::Rename(_) => "Rename Playlist",
        }
    }

    pub(super) const fn confirm_label(&self) -> &'static str {
        match self.purpose {
            Purpose::Create(_) => "Create",
            Purpose::Rename(_) => "Rename",
        }
    }
}

/// A read of the artwork of a playlist's first track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ArtworkRequest {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
}

/// What the Playlists tab's header tells of a playlist, of the tracks of
/// it that are there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Summary {
    pub(super) name: SharedString,
    pub(super) tracks: usize,
    /// Tracks of it whose files are not there.
    pub(super) missing: usize,
    /// The earliest and the latest year of its music.
    pub(super) years: Option<(u32, u32)>,
    pub(super) duration_ms: u64,
}

impl Summary {
    /// `track` tells of a track of the playlist: none when the library
    /// does not know it.
    pub(super) fn of<'a>(playlist: &Playlist, track: impl Fn(i64) -> Option<&'a Track>) -> Self {
        let mut summary = Self {
            name: playlist.name.clone().into(),
            tracks: 0,
            missing: 0,
            years: None,
            duration_ms: 0,
        };
        for track in playlist.tracks.iter().map(|&id| track(id)) {
            let Some(track) = track.filter(|track| track.present) else {
                summary.missing += 1;
                continue;
            };
            summary.tracks += 1;
            summary.duration_ms += track.duration_ms.unwrap_or(0);
            if let Some(year) = track.year {
                summary.years = Some(match summary.years {
                    Some((first, last)) => (first.min(year), last.max(year)),
                    None => (year, year),
                });
            }
        }
        summary
    }
}

#[derive(Clone, Default)]
pub(super) struct State {
    /// By name, as the library tells them.
    list: Vec<Playlist>,
    /// The library told them at least once.
    loaded: bool,
    /// The one the tab shows; while it is none or gone, the first.
    chosen: Option<i64>,
    pub(super) prompt: Option<Prompt>,
    /// The file the shown artwork is of, and the artwork once it is read.
    art_path: Option<PathBuf>,
    art: Option<SharedString>,
    art_request: u64,
}

impl State {
    pub(super) fn list(&self) -> &[Playlist] {
        &self.list
    }

    pub(super) fn find(&self, id: i64) -> Option<&Playlist> {
        self.list.iter().find(|playlist| playlist.id == id)
    }

    /// The playlist the tab shows.
    pub(super) fn chosen(&self) -> Option<&Playlist> {
        self.chosen
            .and_then(|id| self.find(id))
            .or_else(|| self.list.first())
    }

    /// Takes the playlists as the library has them now. One that is new
    /// since it last told them is the one the user just made: it is shown.
    pub(super) fn set(&mut self, list: Vec<Playlist>) {
        let made = self
            .loaded
            .then(|| {
                list.iter()
                    .find(|playlist| self.find(playlist.id).is_none())
                    .map(|playlist| playlist.id)
            })
            .flatten();
        self.list = list;
        self.loaded = true;
        if made.is_some() {
            self.chosen = made;
        }
    }

    /// Shows the playlist of `id`; false when it already was, or is not.
    pub(super) fn choose(&mut self, id: i64) -> bool {
        if self.find(id).is_none() || self.chosen().map(|playlist| playlist.id) == Some(id) {
            return false;
        }
        self.chosen = Some(id);
        true
    }

    /// The artwork shown is to be of the file at `path` (or none): a read
    /// of it when it is another's.
    pub(super) fn show_art(&mut self, path: Option<&Path>) -> Option<ArtworkRequest> {
        if self.art_path.as_deref() == path {
            return None;
        }
        self.art_path = path.map(Path::to_path_buf);
        self.art = None;
        self.art_request += 1;
        self.art_path.clone().map(|path| ArtworkRequest {
            request_id: self.art_request,
            path,
        })
    }

    /// A read came back; one asked before the last is of a file no longer
    /// shown.
    pub(super) fn art_loaded(
        &mut self,
        request_id: u64,
        result: Result<Option<SharedString>, String>,
    ) {
        if request_id != self.art_request {
            return;
        }
        match result {
            Ok(image) => self.art = image,
            Err(error) => eprintln!("Playlist artwork failed: {error}"),
        }
    }

    pub(super) fn art(&self) -> Option<&SharedString> {
        self.art.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::library::test_track;

    fn playlist(id: i64, name: &str, tracks: &[i64]) -> Playlist {
        Playlist {
            id,
            name: name.to_owned(),
            tracks: tracks.to_vec(),
        }
    }

    #[test]
    fn the_first_is_shown_until_one_is_chosen_and_a_new_one_is_shown_once_made() {
        let mut state = State::default();
        assert!(state.chosen().is_none());
        state.set(vec![playlist(1, "A", &[]), playlist(2, "B", &[])]);
        assert_eq!(state.chosen().map(|p| p.id), Some(1));
        assert!(!state.choose(1));
        assert!(!state.choose(9));
        assert!(state.choose(2));

        state.set(vec![
            playlist(3, "0 New", &[]),
            playlist(1, "A", &[]),
            playlist(2, "B", &[]),
        ]);
        assert_eq!(state.chosen().map(|p| p.id), Some(3));
        // The chosen one deleted: the first again.
        state.set(vec![playlist(1, "A", &[]), playlist(2, "B", &[])]);
        assert_eq!(state.chosen().map(|p| p.id), Some(1));
    }

    #[test]
    fn artwork_is_read_once_per_file_and_an_older_read_is_dropped() {
        let mut state = State::default();
        let first = state.show_art(Some(Path::new("/a.flac"))).unwrap();
        assert!(state.show_art(Some(Path::new("/a.flac"))).is_none());
        let second = state.show_art(Some(Path::new("/b.flac"))).unwrap();
        state.art_loaded(first.request_id, Ok(Some("old".into())));
        assert!(state.art().is_none());
        state.art_loaded(second.request_id, Ok(Some("new".into())));
        assert_eq!(state.art().map(|art| art.as_ref()), Some("new"));
        assert!(state.show_art(None).is_none());
        assert!(state.art().is_none());
    }

    #[test]
    fn a_summary_counts_what_is_there_and_spans_its_years() {
        let mut tracks: Vec<Track> = (1..=4)
            .map(|id| test_track(id, PathBuf::from(format!("/{id}.flac")), None))
            .collect();
        for (track, (year, duration)) in tracks.iter_mut().zip([
            (Some(2001), 60_000),
            (None, 30_000),
            (Some(1994), 90_000),
            (Some(2020), 1),
        ]) {
            track.year = year;
            track.duration_ms = Some(duration);
        }
        tracks[3].present = false;
        let summary = Summary::of(&playlist(1, "Mix", &[1, 2, 3, 4, 99]), |id| {
            tracks.iter().find(|track| track.id == id)
        });
        assert_eq!(
            summary,
            Summary {
                name: "Mix".into(),
                tracks: 3,
                missing: 2,
                years: Some((1994, 2001)),
                duration_ms: 180_000,
            }
        );
    }
}

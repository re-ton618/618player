//! The lyrics the Now Playing tab shows: of the current track, read once
//! per file off the UI thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::library::Lyrics;

/// A read of the lyrics of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Request {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
}

/// What is known of the current track's lyrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// Nothing is playing.
    Nothing,
    Reading,
    /// The track has none, or they could not be read.
    Missing,
    Ready(Arc<Lyrics>),
}

#[derive(Debug, Clone, Default)]
pub(super) struct State {
    path: Option<PathBuf>,
    lyrics: Option<Arc<Lyrics>>,
    reading: bool,
    request: u64,
}

impl State {
    /// The lyrics are to be of the file at `path` (or none): a read of it
    /// when it is another's.
    pub(super) fn show(&mut self, path: Option<&Path>) -> Option<Request> {
        if self.path.as_deref() == path {
            return None;
        }
        self.path = path.map(Path::to_path_buf);
        self.lyrics = None;
        self.request += 1;
        self.reading = self.path.is_some();
        self.path.clone().map(|path| Request {
            request_id: self.request,
            path,
        })
    }

    /// A read came back; one asked before the last is of another file.
    pub(super) fn loaded(&mut self, request_id: u64, result: Result<Option<Lyrics>, String>) {
        if request_id != self.request {
            return;
        }
        self.reading = false;
        match result {
            Ok(lyrics) => self.lyrics = lyrics.map(Arc::new),
            Err(error) => eprintln!("Lyrics failed: {error}"),
        }
    }

    pub(super) fn shown(&self) -> Shown {
        match (&self.path, &self.lyrics) {
            (None, _) => Shown::Nothing,
            (Some(_), Some(lyrics)) => Shown::Ready(Arc::clone(lyrics)),
            (Some(_), None) if self.reading => Shown::Reading,
            (Some(_), None) => Shown::Missing,
        }
    }

    /// The line sung at `position_ms`, of synced lyrics.
    pub(super) fn line_at(&self, position_ms: u64) -> Option<usize> {
        self.lyrics.as_ref()?.line_at(position_ms)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Shown, State};
    use crate::library::parse_lyrics;

    #[test]
    fn lyrics_are_read_once_per_file_and_an_older_read_is_dropped() {
        let mut state = State::default();
        assert_eq!(state.shown(), Shown::Nothing);
        let first = state.show(Some(Path::new("/a.flac"))).unwrap();
        assert!(state.show(Some(Path::new("/a.flac"))).is_none());
        assert_eq!(state.shown(), Shown::Reading);
        let second = state.show(Some(Path::new("/b.flac"))).unwrap();
        state.loaded(first.request_id, Ok(parse_lyrics("old")));
        assert_eq!(state.shown(), Shown::Reading);
        state.loaded(second.request_id, Ok(None));
        assert_eq!(state.shown(), Shown::Missing);
        let third = state.show(Some(Path::new("/c.flac"))).unwrap();
        state.loaded(third.request_id, Ok(parse_lyrics("[00:01.00]new")));
        assert!(matches!(state.shown(), Shown::Ready(lyrics) if lyrics.synced));
        assert_eq!(state.line_at(1_500), Some(0));
        assert!(state.show(None).is_none());
        assert_eq!(state.shown(), Shown::Nothing);
    }
}

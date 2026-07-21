use std::path::PathBuf;

use iced::widget::image;

use crate::library::Track;

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

#[derive(Debug)]
enum Artwork {
    Empty,
    Reading {
        request_id: u64,
        key: ArtworkKey,
        source: SourceFingerprint,
    },
    Allocating {
        request_id: u64,
        key: ArtworkKey,
        source: SourceFingerprint,
        handle: image::Handle,
    },
    Ready {
        key: ArtworkKey,
        source: SourceFingerprint,
        allocation: image::Allocation,
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
            Self::Reading { key, .. }
            | Self::Allocating { key, .. }
            | Self::Ready { key, .. }
            | Self::Missing { key, .. } => Some(key),
        }
    }

    fn source(&self) -> Option<&SourceFingerprint> {
        match self {
            Self::Empty => None,
            Self::Reading { source, .. }
            | Self::Allocating { source, .. }
            | Self::Ready { source, .. }
            | Self::Missing { source, .. } => Some(source),
        }
    }

    fn set_key(&mut self, new_key: ArtworkKey) {
        match self {
            Self::Empty => {}
            Self::Reading { key, .. }
            | Self::Allocating { key, .. }
            | Self::Ready { key, .. }
            | Self::Missing { key, .. } => *key = new_key,
        }
    }

    fn handle(&self) -> Option<&image::Handle> {
        match self {
            Self::Allocating { handle, .. } => Some(handle),
            Self::Ready { allocation, .. } => Some(allocation.handle()),
            Self::Empty | Self::Reading { .. } | Self::Missing { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct State {
    current: Option<CurrentTrack>,
    last_request_id: u64,
    artwork: Artwork,
}

impl Default for State {
    fn default() -> Self {
        Self {
            current: None,
            last_request_id: 0,
            artwork: Artwork::Empty,
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
    Allocate(image::Handle),
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
                Artwork::Reading { .. } | Artwork::Allocating { .. } | Artwork::Ready { .. }
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
                | Artwork::Allocating { .. }
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

    pub(super) fn clear(&mut self) {
        self.current = None;
        self.artwork = Artwork::Empty;
    }

    pub(super) fn current_track_id(&self) -> Option<i64> {
        self.current.as_ref().map(|current| current.track_id)
    }

    pub(super) fn artwork_handle(&self) -> Option<&image::Handle> {
        self.artwork.handle()
    }

    pub(super) fn loaded(
        &mut self,
        request_id: u64,
        result: Result<Option<image::Handle>, String>,
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
            Ok(Some(handle)) => {
                self.artwork = Artwork::Allocating {
                    request_id,
                    key,
                    source,
                    handle: handle.clone(),
                };
                ArtworkEffect::Allocate(handle)
            }
            Ok(None) => self.failed(key, source, None),
            Err(error) => self.failed(key, source, Some(error)),
        }
    }

    pub(super) fn allocated(
        &mut self,
        request_id: u64,
        result: Result<image::Allocation, image::Error>,
    ) -> ArtworkEffect {
        let Artwork::Allocating {
            request_id: active_request_id,
            key,
            source,
            ..
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
            Ok(allocation) => {
                self.artwork = Artwork::Ready {
                    key,
                    source,
                    allocation,
                };
                ArtworkEffect::None
            }
            Err(error) => self.failed(key, source, Some(error.to_string())),
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
    fn selection_sets_current_id_before_returning_read_request() {
        let track = album_track(7, "track", "album");
        let mut state = State::default();

        let request = state.select(&track).expect("first selection must read");

        assert_eq!(state.current_track_id(), Some(7));
        assert_eq!(request.path, track.path);
        assert!(matches!(state.artwork, Artwork::Reading { .. }));
    }

    #[test]
    fn same_album_siblings_reuse_in_flight_and_allocating_handle() {
        let first = album_track(1, "first", "album");
        let second = album_track(2, "second", "album");
        let third = album_track(3, "third", "album");
        let mut state = State::default();

        let request = state.select(&first).expect("first selection must read");
        assert!(state.select(&second).is_none());
        assert_eq!(state.current_track_id(), Some(2));

        let handle = image::Handle::from_bytes(vec![1, 2, 3]);
        let handle_id = handle.id();
        assert!(matches!(
            state.loaded(request.request_id, Ok(Some(handle))),
            ArtworkEffect::Allocate(_)
        ));
        assert_eq!(
            state.artwork_handle().map(image::Handle::id),
            Some(handle_id)
        );

        assert!(state.select(&third).is_none());
        assert_eq!(state.current_track_id(), Some(3));
        assert_eq!(
            state.artwork_handle().map(image::Handle::id),
            Some(handle_id)
        );
    }

    #[test]
    fn stale_result_cannot_replace_different_album_request() {
        let first = album_track(1, "first", "first-album");
        let second = album_track(2, "second", "second-album");
        let mut state = State::default();

        let first_request = state.select(&first).expect("first selection must read");
        let second_request = state.select(&second).expect("new album must read");
        assert!(second_request.request_id > first_request.request_id);

        let stale = image::Handle::from_bytes(vec![1]);
        assert!(matches!(
            state.loaded(first_request.request_id, Ok(Some(stale))),
            ArtworkEffect::None
        ));
        assert!(state.artwork_handle().is_none());
        assert_eq!(state.current_track_id(), Some(2));

        let current = image::Handle::from_bytes(vec![2]);
        let current_id = current.id();
        assert!(matches!(
            state.loaded(second_request.request_id, Ok(Some(current))),
            ArtworkEffect::Allocate(_)
        ));
        assert_eq!(
            state.artwork_handle().map(image::Handle::id),
            Some(current_id)
        );
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
        let handle = image::Handle::from_bytes(vec![1]);
        let handle_id = handle.id();
        assert!(matches!(
            state.loaded(request.request_id, Ok(Some(handle))),
            ArtworkEffect::Allocate(_)
        ));

        assert!(state.select(&after).is_none());
        assert_eq!(
            state.artwork_handle().map(image::Handle::id),
            Some(handle_id)
        );
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
                Ok(Some(image::Handle::from_bytes(vec![1])))
            ),
            ArtworkEffect::Allocate(_)
        ));

        let reread = state
            .select(&sibling_after)
            .expect("rekeyed sibling must read its own source");
        assert_eq!(reread.path, sibling_after.path);
        assert!(matches!(state.artwork, Artwork::Reading { .. }));
    }
}

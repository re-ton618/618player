use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataState {
    Pending,
    Ready,
    Fallback,
    Failed,
}

impl MetadataState {
    pub(super) fn as_i64(self) -> i64 {
        match self {
            Self::Pending => 0,
            Self::Ready => 1,
            Self::Fallback => 2,
            Self::Failed => 3,
        }
    }

    pub(super) fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Ready,
            2 => Self::Fallback,
            3 => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Track {
    pub(crate) id: i64,
    pub(crate) path: PathBuf,
    pub(crate) relative_path: PathBuf,
    pub(crate) size: u64,
    pub(crate) modified_ns: i64,
    pub(crate) file_identity: Option<String>,
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) year: Option<u32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) track_number: Option<u32>,
    pub(crate) disc_number: Option<u32>,
    pub(crate) metadata_state: MetadataState,
    pub(crate) metadata_error: Option<String>,
    search_key: String,
    title_key: String,
    artist_key: String,
    album_key: String,
}

impl Track {
    pub(super) fn fallback(
        id: i64,
        path: PathBuf,
        relative_path: PathBuf,
        size: u64,
        modified_ns: i64,
        file_identity: Option<String>,
        metadata_state: MetadataState,
    ) -> Self {
        let title = fallback_title(&path);
        let mut track = Self {
            id,
            path,
            relative_path,
            size,
            modified_ns,
            file_identity,
            title,
            artist: None,
            album: None,
            year: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            metadata_state,
            metadata_error: None,
            search_key: String::new(),
            title_key: String::new(),
            artist_key: String::new(),
            album_key: String::new(),
        };
        track.rebuild_keys();
        track
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_cache(
        id: i64,
        path: PathBuf,
        relative_path: PathBuf,
        size: u64,
        modified_ns: i64,
        file_identity: Option<String>,
        title: String,
        artist: Option<String>,
        album: Option<String>,
        year: Option<u32>,
        duration_ms: Option<u64>,
        track_number: Option<u32>,
        disc_number: Option<u32>,
        metadata_state: MetadataState,
        metadata_error: Option<String>,
    ) -> Self {
        let mut track = Self {
            id,
            path,
            relative_path,
            size,
            modified_ns,
            file_identity,
            title,
            artist,
            album,
            year,
            duration_ms,
            track_number,
            disc_number,
            metadata_state,
            metadata_error,
            search_key: String::new(),
            title_key: String::new(),
            artist_key: String::new(),
            album_key: String::new(),
        };
        track.rebuild_keys();
        track
    }

    pub(crate) fn apply(&mut self, update: TrackUpdate) {
        debug_assert_eq!(self.id, update.id);
        self.title = update.title;
        self.artist = update.artist;
        self.album = update.album;
        self.year = update.year;
        self.duration_ms = update.duration_ms;
        self.track_number = update.track_number;
        self.disc_number = update.disc_number;
        self.metadata_state = update.metadata_state;
        self.metadata_error = update.metadata_error;
        self.rebuild_keys();
    }

    pub(crate) fn matches_fingerprint(&self, size: u64, modified_ns: i64) -> bool {
        self.size == size && self.modified_ns == modified_ns
    }

    pub(super) fn relocate(
        &mut self,
        path: PathBuf,
        relative_path: PathBuf,
        file_identity: Option<String>,
    ) {
        let title_uses_filename = self.title == fallback_title(&self.path);
        self.path = path;
        self.relative_path = relative_path;
        self.file_identity = file_identity;
        if title_uses_filename {
            self.title = fallback_title(&self.path);
        }
        self.rebuild_keys();
    }

    pub(crate) fn search_key(&self) -> &str {
        &self.search_key
    }

    pub(crate) fn title_key(&self) -> &str {
        &self.title_key
    }

    pub(crate) fn artist_key(&self) -> &str {
        &self.artist_key
    }

    pub(crate) fn album_key(&self) -> &str {
        &self.album_key
    }

    pub(crate) fn formatted_duration(&self) -> String {
        let Some(milliseconds) = self.duration_ms else {
            return String::new();
        };
        let seconds = milliseconds / 1_000;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }

    fn rebuild_keys(&mut self) {
        self.title_key = self.title.to_lowercase();
        self.artist_key = self.artist.as_deref().unwrap_or_default().to_lowercase();
        self.album_key = self.album.as_deref().unwrap_or_default().to_lowercase();
        self.search_key = format!(
            "{}\n{}\n{}\n{}",
            self.title_key,
            self.artist_key,
            self.album_key,
            self.relative_path.to_string_lossy().to_lowercase()
        );
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TrackUpdate {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) year: Option<u32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) track_number: Option<u32>,
    pub(crate) disc_number: Option<u32>,
    pub(crate) metadata_state: MetadataState,
    pub(crate) metadata_error: Option<String>,
}

pub(super) fn fallback_title(path: &Path) -> String {
    path.file_stem().or_else(|| path.file_name()).map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_title_uses_file_stem() {
        assert_eq!(
            fallback_title(Path::new("Artist/Track Name.flac")),
            "Track Name"
        );
    }

    #[test]
    fn search_key_contains_metadata_and_path() {
        let track = Track::from_cache(
            1,
            PathBuf::from("/music/Artist/Album/file.mp3"),
            PathBuf::from("Artist/Album/file.mp3"),
            10,
            20,
            None,
            "Title".into(),
            Some("Artist".into()),
            Some("Album".into()),
            Some(2020),
            Some(1000),
            None,
            None,
            MetadataState::Ready,
            None,
        );

        assert!(track.search_key().contains("title"));
        assert!(track.search_key().contains("artist/album/file.mp3"));
    }

    #[test]
    fn duration_is_formatted_without_allocating_ui_state() {
        let mut track = Track::fallback(
            1,
            PathBuf::from("/music/file.mp3"),
            PathBuf::from("file.mp3"),
            1,
            1,
            None,
            MetadataState::Ready,
        );
        track.duration_ms = Some(245_500);

        assert_eq!(track.formatted_duration(), "4:05");
    }
}

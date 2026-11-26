use std::path::{Path, PathBuf};

/// What an album without an album artist of its own is filed under, once
/// its tracks disagree about who made it.
pub(crate) const VARIOUS_ARTISTS: &str = "Various Artists";

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

/// The ReplayGain tags of a file, as they are written: gains in dB, peaks
/// as a share of full scale. Only stored for now; nothing applies them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ReplayGain {
    pub(crate) track_gain_db: Option<f64>,
    pub(crate) track_peak: Option<f64>,
    pub(crate) album_gain_db: Option<f64>,
    pub(crate) album_peak: Option<f64>,
}

/// What a file's tags and its stream say of it.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Metadata {
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) album_artist: Option<String>,
    pub(crate) genre: Option<String>,
    pub(crate) composer: Option<String>,
    pub(crate) year: Option<u32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) track_number: Option<u32>,
    pub(crate) track_total: Option<u32>,
    pub(crate) disc_number: Option<u32>,
    pub(crate) disc_total: Option<u32>,
    /// The file is flagged as part of a compilation.
    pub(crate) compilation: bool,
    /// The MusicBrainz release the file is tagged as part of.
    pub(crate) release_id: Option<String>,
    pub(crate) sample_rate: Option<u32>,
    pub(crate) bit_depth: Option<u8>,
    pub(crate) channels: Option<u8>,
    pub(crate) codec: Option<String>,
    pub(crate) bitrate_kbps: Option<u32>,
    pub(crate) replay_gain: ReplayGain,
}

/// Where a track's file is, and what told it apart when it was last seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRecord {
    pub(crate) id: i64,
    /// The library folder the file is under; none once that was removed.
    pub(crate) folder_id: Option<i64>,
    pub(crate) path: PathBuf,
    pub(crate) relative_path: PathBuf,
    pub(crate) size: u64,
    pub(crate) modified_ns: i64,
    pub(crate) file_identity: Option<String>,
    /// The file was there at the last scan. An absent track keeps its row
    /// and its id, for the playlists and the queue that name it.
    pub(crate) present: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Track {
    pub(crate) id: i64,
    pub(crate) folder_id: Option<i64>,
    pub(crate) path: PathBuf,
    pub(crate) relative_path: PathBuf,
    pub(crate) size: u64,
    pub(crate) modified_ns: i64,
    pub(crate) file_identity: Option<String>,
    pub(crate) present: bool,
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) album_artist: Option<String>,
    pub(crate) genre: Option<String>,
    pub(crate) composer: Option<String>,
    pub(crate) year: Option<u32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) track_number: Option<u32>,
    pub(crate) track_total: Option<u32>,
    pub(crate) disc_number: Option<u32>,
    pub(crate) disc_total: Option<u32>,
    pub(crate) compilation: bool,
    pub(crate) release_id: Option<String>,
    pub(crate) sample_rate: Option<u32>,
    pub(crate) bit_depth: Option<u8>,
    pub(crate) channels: Option<u8>,
    pub(crate) codec: Option<String>,
    pub(crate) bitrate_kbps: Option<u32>,
    pub(crate) replay_gain: ReplayGain,
    pub(crate) metadata_state: MetadataState,
    pub(crate) metadata_error: Option<String>,
    /// How many times the track was listened to: played for at least the
    /// share of it the user set.
    pub(crate) play_count: u32,
    search_key: String,
    title_key: String,
    artist_key: String,
    album_key: String,
    album_identity: String,
}

impl Track {
    /// A track known by its file alone, titled after it.
    pub(super) fn fallback(file: FileRecord, metadata_state: MetadataState) -> Self {
        let metadata = Metadata {
            title: fallback_title(&file.path),
            ..Metadata::default()
        };
        Self::new(file, metadata, metadata_state, None)
    }

    pub(super) fn new(
        file: FileRecord,
        metadata: Metadata,
        metadata_state: MetadataState,
        metadata_error: Option<String>,
    ) -> Self {
        let mut track = Self {
            id: file.id,
            folder_id: file.folder_id,
            path: file.path,
            relative_path: file.relative_path,
            size: file.size,
            modified_ns: file.modified_ns,
            file_identity: file.file_identity,
            present: file.present,
            title: String::new(),
            artist: None,
            album: None,
            album_artist: None,
            genre: None,
            composer: None,
            year: None,
            duration_ms: None,
            track_number: None,
            track_total: None,
            disc_number: None,
            disc_total: None,
            compilation: false,
            release_id: None,
            sample_rate: None,
            bit_depth: None,
            channels: None,
            codec: None,
            bitrate_kbps: None,
            replay_gain: ReplayGain::default(),
            metadata_state,
            metadata_error,
            play_count: 0,
            search_key: String::new(),
            title_key: String::new(),
            artist_key: String::new(),
            album_key: String::new(),
            album_identity: String::new(),
        };
        track.set_metadata(metadata);
        track
    }

    pub(crate) fn apply(&mut self, update: TrackUpdate) {
        debug_assert_eq!(self.id, update.id);
        self.metadata_state = update.metadata_state;
        self.metadata_error = update.metadata_error;
        self.set_metadata(update.metadata);
    }

    fn set_metadata(&mut self, metadata: Metadata) {
        self.title = metadata.title;
        self.artist = metadata.artist;
        self.album = metadata.album;
        self.album_artist = metadata.album_artist;
        self.genre = metadata.genre;
        self.composer = metadata.composer;
        self.year = metadata.year;
        self.duration_ms = metadata.duration_ms;
        self.track_number = metadata.track_number;
        self.track_total = metadata.track_total;
        self.disc_number = metadata.disc_number;
        self.disc_total = metadata.disc_total;
        self.compilation = metadata.compilation;
        self.release_id = metadata.release_id;
        self.sample_rate = metadata.sample_rate;
        self.bit_depth = metadata.bit_depth;
        self.channels = metadata.channels;
        self.codec = metadata.codec;
        self.bitrate_kbps = metadata.bitrate_kbps;
        self.replay_gain = metadata.replay_gain;
        self.rebuild_keys();
    }

    pub(crate) fn matches_fingerprint(&self, size: u64, modified_ns: i64) -> bool {
        self.size == size && self.modified_ns == modified_ns
    }

    pub(super) fn relocate(&mut self, path: PathBuf, relative_path: PathBuf) {
        let title_uses_filename = self.title == fallback_title(&self.path);
        self.path = path;
        self.relative_path = relative_path;
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

    /// Names the album the track is of: equal for two tracks of one
    /// album, empty for a track of none. [`album_identity`] tells how it
    /// is derived.
    pub(crate) fn album_identity(&self) -> &str {
        &self.album_identity
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
        self.album_identity = album_identity(self);
        let lowered = |value: &Option<String>| value.as_deref().unwrap_or_default().to_lowercase();
        self.search_key = [
            self.title_key.as_str(),
            &self.artist_key,
            &self.album_key,
            &lowered(&self.album_artist),
            &lowered(&self.genre),
            &lowered(&self.composer),
            &self.relative_path.to_string_lossy().to_lowercase(),
        ]
        .join("\n");
    }
}

/// The album a track is of, told from its own tags and folder, so that no
/// other track has to be looked at. The first that applies:
///
/// 1. no album title: no album;
/// 2. a MusicBrainz release id: that release;
/// 3. an album artist: that artist's album of that title;
/// 4. the compilation flag: the compilation of that title;
/// 5. otherwise the album of that title in the track's folder (the folder
///    above a disc folder such as "CD 2"), whoever its tracks are by.
///
/// The last rule is what keeps an untagged compilation one album rather
/// than one per track artist, and two same-titled albums apart.
fn album_identity(track: &Track) -> String {
    if track.album_key.is_empty() {
        return String::new();
    }
    if let Some(release) = &track.release_id {
        return format!("release\n{}", release.to_lowercase());
    }
    if let Some(artist) = &track.album_artist {
        return format!("artist\n{}\n{}", artist.to_lowercase(), track.album_key);
    }
    if track.compilation {
        return format!("compilation\n{}", track.album_key);
    }
    format!(
        "folder\n{}\n{}",
        album_folder(&track.path).to_string_lossy().to_lowercase(),
        track.album_key
    )
}

/// The folder an album's files are in: the file's own, or the one above
/// it when that is a disc of a set.
fn album_folder(path: &Path) -> &Path {
    let Some(folder) = path.parent() else {
        return path;
    };
    let is_disc = folder
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_disc_folder);
    match folder.parent() {
        Some(above) if is_disc => above,
        _ => folder,
    }
}

/// "CD1", "Disc 2", "disk-03".
fn is_disc_folder(name: &str) -> bool {
    let name = name.trim().to_lowercase();
    ["cd", "disc", "disk"].iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|number| {
            let number = number.trim_start_matches([' ', '-', '_', '.']);
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

#[derive(Debug, Clone)]
pub(crate) struct TrackUpdate {
    pub(crate) id: i64,
    pub(crate) metadata: Metadata,
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
pub(crate) mod tests {
    use super::*;

    /// A present track at "/music/{relative}".
    pub(crate) fn track_at(id: i64, relative: &str, metadata: Metadata) -> Track {
        Track::new(
            FileRecord {
                id,
                folder_id: Some(1),
                path: PathBuf::from(format!("/music/{relative}")),
                relative_path: PathBuf::from(relative),
                size: 1,
                modified_ns: 1,
                file_identity: None,
                present: true,
            },
            metadata,
            MetadataState::Ready,
            None,
        )
    }

    fn of_album(relative: &str, album: &str, change: impl FnOnce(&mut Metadata)) -> Track {
        let mut metadata = Metadata {
            title: "Title".into(),
            album: Some(album.into()),
            ..Metadata::default()
        };
        change(&mut metadata);
        track_at(1, relative, metadata)
    }

    #[test]
    fn fallback_title_uses_file_stem() {
        assert_eq!(
            fallback_title(Path::new("Artist/Track Name.flac")),
            "Track Name"
        );
    }

    #[test]
    fn search_key_contains_metadata_and_path() {
        let track = track_at(
            1,
            "Artist/Album/file.mp3",
            Metadata {
                title: "Title".into(),
                artist: Some("Artist".into()),
                album: Some("Album".into()),
                album_artist: Some("Ensemble".into()),
                genre: Some("Baroque".into()),
                composer: Some("Telemann".into()),
                ..Metadata::default()
            },
        );

        for part in [
            "title",
            "ensemble",
            "baroque",
            "telemann",
            "artist/album/file.mp3",
        ] {
            assert!(track.search_key().contains(part), "{part}");
        }
    }

    #[test]
    fn duration_is_formatted_without_allocating_ui_state() {
        let mut track = track_at(1, "file.mp3", Metadata::default());
        track.duration_ms = Some(245_500);

        assert_eq!(track.formatted_duration(), "4:05");
    }

    #[test]
    fn an_album_is_told_by_release_then_album_artist_then_flag_then_folder() {
        let untitled = track_at(1, "a/1.flac", Metadata::default());
        assert_eq!(untitled.album_identity(), "");

        // An album artist joins folders and keeps same-titled albums apart.
        let by = |relative: &str, artist: &str| {
            of_album(relative, "Greatest Hits", |metadata| {
                metadata.album_artist = Some(artist.into());
            })
        };
        assert_eq!(
            by("x/1.flac", "Queen").album_identity(),
            by("y/2.flac", "QUEEN").album_identity()
        );
        assert_ne!(
            by("x/1.flac", "Queen").album_identity(),
            by("x/2.flac", "ABBA").album_identity()
        );

        // A release id tells two editions of one title by one artist apart.
        let edition = |release: &str| {
            of_album("x/1.flac", "Greatest Hits", |metadata| {
                metadata.album_artist = Some("Queen".into());
                metadata.release_id = Some(release.into());
            })
        };
        assert_ne!(
            edition("1981").album_identity(),
            edition("2011").album_identity()
        );

        // Flagged tracks are one compilation wherever they are.
        let flagged =
            |relative: &str| of_album(relative, "Now 1", |metadata| metadata.compilation = true);
        assert_eq!(
            flagged("x/1.flac").album_identity(),
            flagged("y/2.flac").album_identity()
        );

        // With none of these the folder decides, whoever the artist is.
        let plain = |relative: &str, artist: &str| {
            of_album(relative, "Live", |metadata| {
                metadata.artist = Some(artist.into());
            })
        };
        assert_eq!(
            plain("Mix/1.flac", "One").album_identity(),
            plain("Mix/2.flac", "Two").album_identity()
        );
        assert_ne!(
            plain("Mix/1.flac", "One").album_identity(),
            plain("Other/1.flac", "One").album_identity()
        );
    }

    #[test]
    fn disc_folders_belong_to_the_album_above_them() {
        let disc = |relative: &str| of_album(relative, "Box", |_| {});
        assert_eq!(
            disc("Box/CD1/1.flac").album_identity(),
            disc("Box/Disc 2/1.flac").album_identity()
        );
        assert_ne!(
            disc("Box/CD1/1.flac").album_identity(),
            disc("Box/Bonus/1.flac").album_identity()
        );
        for name in ["cd1", "CD 12", "Disc-2", "disk_03"] {
            assert!(is_disc_folder(name), "{name}");
        }
        for name in ["CD", "Discovery", "cd one", "1"] {
            assert!(!is_disc_folder(name), "{name}");
        }
    }
}

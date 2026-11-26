//! The library database, and the picture of it that scans are reconciled
//! against: every track it has, by id, by path and by file identity.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row, params};

use super::metadata::MetadataJob;
use super::migrations;
use super::scanner::{DiscoveredTrack, is_within, path_key, supports_embedded_metadata};
use super::track::{FileRecord, Metadata, MetadataState, ReplayGain, Track, TrackUpdate};

pub(super) type CacheResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// The setting that says the first run's folder was added: a user who
/// removes every folder is not given the Music folder again.
const FOLDERS_SEEDED: &str = "folders_seeded";

/// What a scan changed of the library.
#[derive(Default)]
pub(super) struct Reconciled {
    /// Every track that is new, moved, changed, back or gone absent.
    pub(super) changed: Vec<Track>,
    /// The tracks whose tags are due to be read.
    pub(super) jobs: Vec<i64>,
}

pub(super) struct Cache {
    connection: Connection,
    tracks: HashMap<i64, Track>,
    /// By [`path_key`], absent tracks included: a file that comes back
    /// where it was is the track it was.
    by_path: HashMap<PathBuf, i64>,
    by_identity: HashMap<String, Vec<i64>>,
}

impl Cache {
    /// The user's library, with the system's Music folder in it on the
    /// first run.
    pub(super) fn open() -> CacheResult<Self> {
        let directory = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("618player");
        fs::create_dir_all(&directory)?;
        let mut cache = Self::open_at(&directory.join("library.sqlite3"))?;
        cache.seed(dirs::audio_dir())?;
        Ok(cache)
    }

    pub(super) fn open_at(path: &Path) -> CacheResult<Self> {
        let mut connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;
        migrations::migrate(&mut connection)?;
        let mut cache = Self {
            connection,
            tracks: HashMap::new(),
            by_path: HashMap::new(),
            by_identity: HashMap::new(),
        };
        cache.load()?;
        Ok(cache)
    }

    /// Makes `folder` the library's first, unless a first was made before.
    pub(super) fn seed(&mut self, folder: Option<PathBuf>) -> CacheResult<()> {
        let seeded: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?1)",
            [FOLDERS_SEEDED],
            |row| row.get(0),
        )?;
        if seeded {
            return Ok(());
        }
        if let Some(folder) = folder {
            self.add_folder(&folder)?;
        }
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, '1')",
            [FOLDERS_SEEDED],
        )?;
        Ok(())
    }

    /// The library's folders, in the order they were added.
    pub(super) fn folders(&self) -> CacheResult<Vec<(i64, PathBuf)>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, path FROM folders ORDER BY id")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, read_path(row, 1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Adds `path` to the library and gives its id; nothing when a folder
    /// of the library holds it already. Folders it holds itself are taken
    /// up into it: their tracks are found again under it, as they were.
    pub(super) fn add_folder(&mut self, path: &Path) -> CacheResult<Option<i64>> {
        let folders = self.folders()?;
        if folders.iter().any(|(_, folder)| is_within(path, folder)) {
            return Ok(None);
        }
        let transaction = self.connection.transaction()?;
        for (inner, _) in folders.iter().filter(|(_, folder)| is_within(folder, path)) {
            transaction.execute("DELETE FROM folders WHERE id = ?1", [inner])?;
            for track in self.tracks.values_mut() {
                if track.folder_id == Some(*inner) {
                    track.folder_id = None;
                }
            }
        }
        let id = transaction.query_row(
            "INSERT INTO folders (path) VALUES (?1) RETURNING id",
            [path_to_bytes(path)],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(Some(id))
    }

    /// Takes a folder out of the library. Its tracks stay in the database,
    /// absent, so that what names them still does if it is added again.
    pub(super) fn remove_folder(&mut self, id: i64) -> CacheResult<Vec<Track>> {
        let transaction = self.connection.transaction()?;
        transaction.execute("UPDATE tracks SET present = 0 WHERE folder_id = ?1", [id])?;
        // The tracks' `folder_id` goes with the row (`ON DELETE SET NULL`).
        transaction.execute("DELETE FROM folders WHERE id = ?1", [id])?;
        transaction.commit()?;

        let mut removed = Vec::new();
        for track in self.tracks.values_mut() {
            if track.folder_id == Some(id) {
                track.folder_id = None;
                if track.present {
                    track.present = false;
                    removed.push(track.clone());
                }
            }
        }
        removed.sort_unstable_by_key(|track| track.id);
        Ok(removed)
    }

    /// Every track the database has, absent ones too.
    pub(super) fn tracks(&self) -> Vec<Track> {
        let mut tracks: Vec<_> = self.tracks.values().cloned().collect();
        tracks.sort_unstable_by_key(|track| track.id);
        tracks
    }

    /// The reading of `id`'s tags, when it is still due.
    pub(super) fn pending_job(&self, id: i64) -> Option<MetadataJob> {
        self.tracks
            .get(&id)
            .filter(|track| track.present && track.metadata_state == MetadataState::Pending)
            .map(|track| MetadataJob {
                id,
                path: track.path.clone(),
            })
    }

    /// Takes what a scan found under `directories` of the folder `root`:
    /// the files there are (`discovered`), and by their absence the ones
    /// that are gone.
    ///
    /// A file is the track that was at its path; failing that the one of
    /// its file identity whose own file is no longer where it was (a
    /// rename or a move); failing that the only one gone of its size and
    /// time (a move to another volume, which gives a new identity); and
    /// otherwise a new track. A track nothing claims goes absent and keeps
    /// its row.
    pub(super) fn reconcile(
        &mut self,
        folder_id: i64,
        directories: &[PathBuf],
        discovered: Vec<DiscoveredTrack>,
    ) -> CacheResult<Reconciled> {
        let discovered_keys: HashSet<PathBuf> =
            discovered.iter().map(|file| path_key(&file.path)).collect();
        let directory_keys: Vec<PathBuf> = directories.iter().map(|path| path_key(path)).collect();
        // The tracks the scan should have found, and did not.
        let mut missing: HashSet<i64> = self
            .by_path
            .iter()
            .filter(|(key, id)| {
                self.tracks[id].present
                    && !discovered_keys.contains(*key)
                    && directory_keys
                        .iter()
                        .any(|directory| key.starts_with(directory))
            })
            .map(|(_, &id)| id)
            .collect();

        let mut touched = Vec::new();
        let mut jobs = Vec::new();
        let mut claimed = HashSet::new();
        let mut unknown = Vec::new();
        for file in discovered {
            match self.by_path.get(&path_key(&file.path)) {
                Some(&id) => {
                    claimed.insert(id);
                    self.refresh(id, file, folder_id, &mut touched, &mut jobs);
                }
                None => unknown.push(file),
            }
        }

        // Known by identity: a file that was renamed or moved.
        let mut strangers = Vec::new();
        for file in unknown {
            let moved = file
                .file_identity
                .as_ref()
                .and_then(|identity| self.by_identity.get(identity))
                .and_then(|ids| {
                    only(ids.iter().copied().filter(|id| {
                        let track = &self.tracks[id];
                        if claimed.contains(id) {
                            false
                        } else if missing.contains(id) {
                            true
                        } else if !track.present {
                            // Gone for a while: an inode may have been
                            // given to another file since.
                            track.matches_fingerprint(file.size, file.modified_ns)
                        } else {
                            // Seen elsewhere at its last scan: moved from
                            // there, unless that is a hard link to it.
                            !discovered_keys.contains(&path_key(&track.path))
                                && !track.path.exists()
                        }
                    }))
                });
            match moved {
                Some(id) => {
                    claimed.insert(id);
                    missing.remove(&id);
                    self.refresh(id, file, folder_id, &mut touched, &mut jobs);
                }
                None => strangers.push(file),
            }
        }

        // Known by size and time alone: only when one file and one track
        // that is gone agree, and no other does.
        let mut new_files = Vec::new();
        if !strangers.is_empty() {
            let mut gone: HashMap<(u64, i64), Vec<i64>> = HashMap::new();
            for track in self.tracks.values() {
                let is_gone = missing.contains(&track.id) || !track.present;
                if is_gone && track.size > 0 && !claimed.contains(&track.id) {
                    gone.entry((track.size, track.modified_ns))
                        .or_default()
                        .push(track.id);
                }
            }
            let mut found: HashMap<(u64, i64), usize> = HashMap::new();
            for file in &strangers {
                *found.entry((file.size, file.modified_ns)).or_default() += 1;
            }
            for file in strangers {
                let fingerprint = (file.size, file.modified_ns);
                let moved = (found[&fingerprint] == 1)
                    .then(|| gone.get(&fingerprint))
                    .flatten()
                    .and_then(|ids| only(ids.iter().copied()));
                match moved {
                    Some(id) => {
                        missing.remove(&id);
                        self.refresh(id, file, folder_id, &mut touched, &mut jobs);
                    }
                    None => new_files.push(file),
                }
            }
        }

        for &id in &missing {
            if let Some(track) = self.tracks.get_mut(&id) {
                track.present = false;
                touched.push(id);
            }
        }

        let mut added: Vec<Track> = new_files
            .into_iter()
            .map(|file| {
                let state = if supports_embedded_metadata(&file.path) {
                    MetadataState::Pending
                } else {
                    MetadataState::Fallback
                };
                Track::fallback(
                    FileRecord {
                        id: 0,
                        folder_id: Some(folder_id),
                        path: file.path,
                        relative_path: file.relative_path,
                        size: file.size,
                        modified_ns: file.modified_ns,
                        file_identity: file.file_identity,
                        present: true,
                    },
                    state,
                )
            })
            .collect();

        let written = write_records(&mut self.connection, &self.tracks, &touched, &mut added);
        if let Err(error) = written {
            // Nothing was written: the picture is the database's again.
            self.load()?;
            return Err(error.into());
        }

        let mut changed: Vec<Track> = touched.iter().map(|id| self.tracks[id].clone()).collect();
        for track in added {
            if track.metadata_state == MetadataState::Pending {
                jobs.push(track.id);
            }
            self.by_path.insert(path_key(&track.path), track.id);
            if let Some(identity) = &track.file_identity {
                self.by_identity
                    .entry(identity.clone())
                    .or_default()
                    .push(track.id);
            }
            changed.push(track.clone());
            self.tracks.insert(track.id, track);
        }
        Ok(Reconciled { changed, jobs })
    }

    /// Brings track `id` to what `file` says of it: where it is, that it is
    /// there, and that it is to be read again when the file changed.
    fn refresh(
        &mut self,
        id: i64,
        file: DiscoveredTrack,
        folder_id: i64,
        touched: &mut Vec<i64>,
        jobs: &mut Vec<i64>,
    ) {
        let track = self.tracks.get_mut(&id).expect("indexed tracks exist");
        let moved = track.path != file.path || track.relative_path != file.relative_path;
        // Another file at the path, though it is as large and as old.
        let replaced = track.path == file.path
            && file.file_identity.is_some()
            && track.file_identity.is_some()
            && track.file_identity != file.file_identity;
        let reread = !track.matches_fingerprint(file.size, file.modified_ns) || replaced;
        // A scan that could not tell the identity does not forget it.
        let identity = file.file_identity.or_else(|| track.file_identity.clone());
        let same = !reread
            && !moved
            && track.present
            && track.folder_id == Some(folder_id)
            && track.file_identity == identity;

        if !same {
            if track.file_identity != identity {
                if let Some(old) = &track.file_identity
                    && let Some(ids) = self.by_identity.get_mut(old)
                {
                    ids.retain(|&other| other != id);
                }
                if let Some(new) = &identity {
                    self.by_identity.entry(new.clone()).or_default().push(id);
                }
                track.file_identity = identity;
            }
            if moved {
                self.by_path.remove(&path_key(&track.path));
                self.by_path.insert(path_key(&file.path), id);
                track.relocate(file.path, file.relative_path);
            }
            track.folder_id = Some(folder_id);
            track.present = true;
            track.size = file.size;
            track.modified_ns = file.modified_ns;
            if reread {
                track.metadata_state = if supports_embedded_metadata(&track.path) {
                    MetadataState::Pending
                } else {
                    MetadataState::Fallback
                };
                track.metadata_error = None;
            }
            touched.push(id);
        }
        if track.metadata_state == MetadataState::Pending {
            jobs.push(id);
        }
    }

    /// Marks absent every present track that is under none of `roots`:
    /// what a folder that is no longer the library's left behind.
    pub(super) fn mark_absent_outside(&mut self, roots: &[PathBuf]) -> CacheResult<Vec<Track>> {
        let roots: Vec<PathBuf> = roots.iter().map(|root| path_key(root)).collect();
        let stray: Vec<i64> = self
            .by_path
            .iter()
            .filter(|(key, id)| {
                self.tracks[id].present && !roots.iter().any(|root| key.starts_with(root))
            })
            .map(|(_, &id)| id)
            .collect();
        for id in &stray {
            if let Some(track) = self.tracks.get_mut(id) {
                track.present = false;
            }
        }
        if let Err(error) = write_records(&mut self.connection, &self.tracks, &stray, &mut []) {
            self.load()?;
            return Err(error.into());
        }
        let mut gone: Vec<Track> = stray.iter().map(|id| self.tracks[id].clone()).collect();
        gone.sort_unstable_by_key(|track| track.id);
        Ok(gone)
    }

    pub(super) fn store_updates(&mut self, updates: &[TrackUpdate]) -> CacheResult<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut statement = transaction.prepare(
                "UPDATE tracks SET
                    title = ?2, artist = ?3, album = ?4, album_artist = ?5, genre = ?6,
                    composer = ?7, year = ?8, duration_ms = ?9, track_number = ?10,
                    track_total = ?11, disc_number = ?12, disc_total = ?13, compilation = ?14,
                    release_id = ?15, sample_rate = ?16, bit_depth = ?17, channels = ?18,
                    codec = ?19, bitrate_kbps = ?20, track_gain_db = ?21, track_peak = ?22,
                    album_gain_db = ?23, album_peak = ?24, metadata_state = ?25,
                    metadata_error = ?26
                 WHERE id = ?1",
            )?;
            for update in updates {
                let metadata = &update.metadata;
                statement.execute(params![
                    update.id,
                    metadata.title,
                    metadata.artist,
                    metadata.album,
                    metadata.album_artist,
                    metadata.genre,
                    metadata.composer,
                    metadata.year,
                    metadata.duration_ms.map(to_i64),
                    metadata.track_number,
                    metadata.track_total,
                    metadata.disc_number,
                    metadata.disc_total,
                    metadata.compilation,
                    metadata.release_id,
                    metadata.sample_rate,
                    metadata.bit_depth,
                    metadata.channels,
                    metadata.codec,
                    metadata.bitrate_kbps,
                    metadata.replay_gain.track_gain_db,
                    metadata.replay_gain.track_peak,
                    metadata.replay_gain.album_gain_db,
                    metadata.replay_gain.album_peak,
                    update.metadata_state.as_i64(),
                    update.metadata_error,
                ])?;
            }
        }
        transaction.commit()?;
        for update in updates {
            if let Some(track) = self.tracks.get_mut(&update.id) {
                track.apply(update.clone());
            }
        }
        Ok(())
    }

    /// Reads the picture from the database.
    fn load(&mut self) -> CacheResult<()> {
        let mut statement = self.connection.prepare(
            "SELECT id, folder_id, path, relative_path, size, modified_ns, file_identity,
                    present, title, artist, album, album_artist, genre, composer, year,
                    duration_ms, track_number, track_total, disc_number, disc_total,
                    compilation, release_id, sample_rate, bit_depth, channels, codec,
                    bitrate_kbps, track_gain_db, track_peak, album_gain_db, album_peak,
                    metadata_state, metadata_error
             FROM tracks",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Track::new(
                FileRecord {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    path: read_path(row, 2)?,
                    relative_path: read_path(row, 3)?,
                    size: from_i64(row.get(4)?),
                    modified_ns: row.get(5)?,
                    file_identity: row.get(6)?,
                    present: row.get(7)?,
                },
                Metadata {
                    title: row.get(8)?,
                    artist: row.get(9)?,
                    album: row.get(10)?,
                    album_artist: row.get(11)?,
                    genre: row.get(12)?,
                    composer: row.get(13)?,
                    year: row.get(14)?,
                    duration_ms: row.get::<_, Option<i64>>(15)?.map(from_i64),
                    track_number: row.get(16)?,
                    track_total: row.get(17)?,
                    disc_number: row.get(18)?,
                    disc_total: row.get(19)?,
                    compilation: row.get(20)?,
                    release_id: row.get(21)?,
                    sample_rate: row.get(22)?,
                    bit_depth: row.get(23)?,
                    channels: row.get(24)?,
                    codec: row.get(25)?,
                    bitrate_kbps: row.get(26)?,
                    replay_gain: ReplayGain {
                        track_gain_db: row.get(27)?,
                        track_peak: row.get(28)?,
                        album_gain_db: row.get(29)?,
                        album_peak: row.get(30)?,
                    },
                },
                MetadataState::from_i64(row.get(31)?),
                row.get(32)?,
            ))
        })?;

        let (mut tracks, mut by_path) = (HashMap::new(), HashMap::new());
        let mut by_identity: HashMap<String, Vec<i64>> = HashMap::new();
        for track in rows {
            let track = track?;
            by_path.insert(path_key(&track.path), track.id);
            if let Some(identity) = &track.file_identity {
                by_identity
                    .entry(identity.clone())
                    .or_default()
                    .push(track.id);
            }
            tracks.insert(track.id, track);
        }
        drop(statement);
        (self.tracks, self.by_path, self.by_identity) = (tracks, by_path, by_identity);
        Ok(())
    }
}

/// The one item of `items`, when there is exactly one.
fn only(mut items: impl Iterator<Item = i64>) -> Option<i64> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

/// Writes the file records of `touched` and inserts `added`, giving those
/// their ids, all or nothing.
fn write_records(
    connection: &mut Connection,
    tracks: &HashMap<i64, Track>,
    touched: &[i64],
    added: &mut [Track],
) -> rusqlite::Result<()> {
    if touched.is_empty() && added.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    {
        let mut update = transaction.prepare(
            "UPDATE tracks SET
                path = ?2, relative_path = ?3, folder_id = ?4, size = ?5, modified_ns = ?6,
                file_identity = ?7, present = ?8, title = ?9, metadata_state = ?10,
                metadata_error = ?11
             WHERE id = ?1",
        )?;
        for track in touched.iter().filter_map(|id| tracks.get(id)) {
            update.execute(params![
                track.id,
                path_to_bytes(&track.path),
                path_to_bytes(&track.relative_path),
                track.folder_id,
                to_i64(track.size),
                track.modified_ns,
                track.file_identity,
                track.present,
                track.title,
                track.metadata_state.as_i64(),
                track.metadata_error,
            ])?;
        }
        let mut insert = transaction.prepare(
            "INSERT INTO tracks (
                path, relative_path, folder_id, size, modified_ns, file_identity, present,
                title, metadata_state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)
             RETURNING id",
        )?;
        for track in added.iter_mut() {
            track.id = insert.query_row(
                params![
                    path_to_bytes(&track.path),
                    path_to_bytes(&track.relative_path),
                    track.folder_id,
                    to_i64(track.size),
                    track.modified_ns,
                    track.file_identity,
                    track.title,
                    track.metadata_state.as_i64(),
                ],
                |row| row.get(0),
            )?;
        }
    }
    transaction.commit()
}

fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn from_i64(value: i64) -> u64 {
    value.max(0) as u64
}

fn read_path(row: &Row<'_>, index: usize) -> rusqlite::Result<PathBuf> {
    Ok(match row.get_ref(index)? {
        ValueRef::Blob(bytes) => PathBuf::from(bytes_to_os_string(bytes)),
        ValueRef::Text(bytes) => PathBuf::from(String::from_utf8_lossy(bytes).into_owned()),
        _ => unreachable!("path columns only contain text or blobs"),
    })
}

#[cfg(unix)]
fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(bytes.to_vec())
}

#[cfg(windows)]
fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    use std::os::windows::ffi::OsStringExt;

    let wide: Vec<_> = bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    OsString::from_wide(&wide)
}

#[cfg(not(any(unix, windows)))]
fn path_to_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

#[cfg(not(any(unix, windows)))]
fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use tempfile::{TempDir, tempdir};

    use super::*;

    /// A library of one folder, which is the temporary directory itself.
    fn library() -> CacheResult<(TempDir, Cache, i64)> {
        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let folder = cache.add_folder(directory.path())?.expect("a new folder");
        Ok((directory, cache, folder))
    }

    fn file(root: &Path, name: &str, size: u64, identity: Option<&str>) -> DiscoveredTrack {
        DiscoveredTrack {
            path: root.join(name),
            relative_path: PathBuf::from(name),
            size,
            modified_ns: size as i64 * 2,
            file_identity: identity.map(str::to_owned),
        }
    }

    fn scan(
        cache: &mut Cache,
        folder: i64,
        root: &Path,
        files: Vec<DiscoveredTrack>,
    ) -> Reconciled {
        cache
            .reconcile(folder, &[root.to_path_buf()], files)
            .expect("the scan is taken")
    }

    fn ready(id: i64, title: &str) -> TrackUpdate {
        TrackUpdate {
            id,
            metadata: Metadata {
                title: title.into(),
                artist: Some("Cached artist".into()),
                duration_ms: Some(1000),
                ..Metadata::default()
            },
            metadata_state: MetadataState::Ready,
            metadata_error: None,
        }
    }

    #[test]
    fn unchanged_files_reuse_cached_metadata() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, None)],
        );
        assert_eq!(first.jobs.len(), 1);
        cache.store_updates(&[ready(first.changed[0].id, "Cached title")])?;

        let second = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, None)],
        );

        assert!(second.jobs.is_empty());
        assert!(second.changed.is_empty());
        // Opened again, the database tells the same.
        drop(cache);
        let cache = Cache::open_at(&root.join("library.sqlite3"))?;
        let tracks = cache.tracks();
        assert_eq!(tracks[0].title, "Cached title");
        assert_eq!(tracks[0].artist.as_deref(), Some("Cached artist"));
        assert_eq!(tracks[0].folder_id, Some(folder));
        Ok(())
    }

    #[test]
    fn every_stored_field_round_trips() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let id = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "a.flac", 10, None)],
        )
        .changed[0]
            .id;
        let metadata = Metadata {
            title: "Title".into(),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            album_artist: Some("Album artist".into()),
            genre: Some("Genre".into()),
            composer: Some("Composer".into()),
            year: Some(1977),
            duration_ms: Some(123_456),
            track_number: Some(2),
            track_total: Some(9),
            disc_number: Some(1),
            disc_total: Some(2),
            compilation: true,
            release_id: Some("release".into()),
            sample_rate: Some(96_000),
            bit_depth: Some(24),
            channels: Some(2),
            codec: Some("FLAC".into()),
            bitrate_kbps: Some(2_900),
            replay_gain: ReplayGain {
                track_gain_db: Some(-6.5),
                track_peak: Some(0.98),
                album_gain_db: Some(-7.25),
                album_peak: Some(1.0),
            },
        };
        cache.store_updates(&[TrackUpdate {
            id,
            metadata: metadata.clone(),
            metadata_state: MetadataState::Ready,
            metadata_error: None,
        }])?;

        drop(cache);
        let cache = Cache::open_at(&root.join("library.sqlite3"))?;
        let track = &cache.tracks()[0];

        assert_eq!(track.album_artist, metadata.album_artist);
        assert_eq!(track.genre, metadata.genre);
        assert_eq!(track.composer, metadata.composer);
        assert_eq!((track.track_total, track.disc_total), (Some(9), Some(2)));
        assert!(track.compilation);
        assert_eq!(track.release_id, metadata.release_id);
        assert_eq!(
            (track.sample_rate, track.bit_depth, track.channels),
            (Some(96_000), Some(24), Some(2))
        );
        assert_eq!(track.codec.as_deref(), Some("FLAC"));
        assert_eq!(track.bitrate_kbps, Some(2_900));
        assert_eq!(track.replay_gain, metadata.replay_gain);
        assert_eq!(track.duration_ms, Some(123_456));
        Ok(())
    }

    #[test]
    fn wma_tracks_are_cached_without_parse_jobs() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();

        let scan = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.wma", 10, None)],
        );

        assert!(scan.jobs.is_empty());
        assert_eq!(scan.changed[0].metadata_state, MetadataState::Fallback);
        Ok(())
    }

    #[test]
    fn changed_files_are_queued_and_missing_files_go_absent_keeping_their_rows() -> CacheResult<()>
    {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, None)],
        );
        let id = first.changed[0].id;
        cache.store_updates(&[ready(id, "Song")])?;
        cache
            .connection
            .execute("INSERT INTO playlists (name) VALUES ('Keep me')", [])?;
        let playlist_id = cache.connection.last_insert_rowid();
        cache.connection.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position)
             VALUES (?1, ?2, 0)",
            [playlist_id, id],
        )?;

        let changed = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 11, None)],
        );
        assert_eq!(changed.jobs, [id]);
        assert_eq!(changed.changed[0].id, id);
        cache.store_updates(&[ready(id, "Song")])?;

        let removed = scan(&mut cache, folder, root, Vec::new());
        assert_eq!(removed.changed.len(), 1);
        assert!(!removed.changed[0].present);
        assert!(removed.jobs.is_empty());
        assert_eq!(
            cache.connection.query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = ?1",
                [id],
                |row| row.get::<_, u32>(0),
            )?,
            1
        );

        // Back where it was, it is the track it was, with what it showed.
        let back = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 11, None)],
        );
        assert_eq!(back.changed.len(), 1);
        assert_eq!(back.changed[0].id, id);
        assert!(back.changed[0].present);
        assert_eq!(back.changed[0].title, "Song");
        assert!(back.jobs.is_empty());
        Ok(())
    }

    #[test]
    fn filesystem_identity_preserves_track_id_across_rename() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "old.wma", 10, Some("device:file"))],
        );
        let original_id = first.changed[0].id;

        let renamed = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "new.wma", 10, Some("device:file"))],
        );

        assert_eq!(renamed.changed.len(), 1);
        assert_eq!(renamed.changed[0].id, original_id);
        assert_eq!(renamed.changed[0].path, root.join("new.wma"));
        assert_eq!(renamed.changed[0].title, "new");
        assert!(renamed.changed[0].present);
        drop(cache);
        let cache = Cache::open_at(&root.join("library.sqlite3"))?;
        assert_eq!(cache.tracks().len(), 1);
        assert_eq!(cache.tracks()[0].id, original_id);
        Ok(())
    }

    #[test]
    fn a_retagged_and_renamed_file_keeps_its_id_and_is_read_again() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let id = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "old.flac", 10, Some("volume:1"))],
        )
        .changed[0]
            .id;
        cache.store_updates(&[ready(id, "Old tags")])?;

        let renamed = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "new.flac", 12, Some("volume:1"))],
        );

        assert_eq!(renamed.changed[0].id, id);
        assert_eq!(renamed.jobs, [id]);
        Ok(())
    }

    #[test]
    fn a_rename_seen_in_two_scans_keeps_the_id() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let (from, to) = (root.join("from"), root.join("to"));
        let at = |folder: &Path| DiscoveredTrack {
            path: folder.join("song.flac"),
            relative_path: PathBuf::from("song.flac"),
            size: 10,
            modified_ns: 20,
            file_identity: Some("volume:1".into()),
        };
        let id = cache
            .reconcile(folder, std::slice::from_ref(&from), vec![at(&from)])?
            .changed[0]
            .id;

        // The folder it left is scanned first: it is gone...
        let left = cache.reconcile(folder, std::slice::from_ref(&from), Vec::new())?;
        assert!(!left.changed[0].present);
        // ...and then found where it went.
        let arrived = cache.reconcile(folder, std::slice::from_ref(&to), vec![at(&to)])?;

        assert_eq!(arrived.changed.len(), 1);
        assert_eq!(arrived.changed[0].id, id);
        assert!(arrived.changed[0].present);
        assert_eq!(cache.tracks().len(), 1);
        Ok(())
    }

    #[test]
    fn a_hard_link_to_a_present_file_is_a_track_of_its_own() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        fs::write(root.join("song.flac"), [1])?;
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.flac", 10, Some("volume:1"))],
        );

        let linked = scan(
            &mut cache,
            folder,
            root,
            vec![
                file(root, "link.flac", 10, Some("volume:1")),
                file(root, "song.flac", 10, Some("volume:1")),
            ],
        );

        assert_eq!(linked.changed.len(), 1);
        assert_ne!(linked.changed[0].id, first.changed[0].id);
        assert_eq!(cache.tracks().len(), 2);
        Ok(())
    }

    #[test]
    fn a_file_moved_to_another_volume_is_known_by_its_size_and_time() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let before = scan(
            &mut cache,
            folder,
            root,
            vec![
                file(root, "a.flac", 10, Some("old-volume:1")),
                file(root, "b.flac", 30, Some("old-volume:2")),
                file(root, "c.flac", 30, Some("old-volume:3")),
            ],
        );
        let id = |name: &str| {
            before
                .changed
                .iter()
                .find(|track| track.path.ends_with(name))
                .map(|track| track.id)
                .unwrap()
        };
        let read: Vec<TrackUpdate> = before
            .changed
            .iter()
            .map(|track| ready(track.id, "Read"))
            .collect();
        cache.store_updates(&read)?;

        let after = scan(
            &mut cache,
            folder,
            root,
            vec![
                file(root, "moved-a.flac", 10, Some("new-volume:7")),
                file(root, "moved-b.flac", 30, Some("new-volume:8")),
                file(root, "moved-c.flac", 30, Some("new-volume:9")),
            ],
        );

        let moved = |name: &str| {
            after
                .changed
                .iter()
                .find(|track| track.path.ends_with(name))
                .unwrap()
        };
        assert_eq!(moved("moved-a.flac").id, id("a.flac"));
        assert!(after.jobs.iter().all(|job| *job != id("a.flac")));
        // Two of one size and time cannot be told apart: both are new.
        for name in ["moved-b.flac", "moved-c.flac"] {
            assert!(moved(name).id != id("b.flac") && moved(name).id != id("c.flac"));
        }
        Ok(())
    }

    #[test]
    fn atomic_replacement_at_same_path_reuses_the_track_row() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, Some("device:old-inode"))],
        );
        cache.store_updates(&[ready(first.changed[0].id, "Song")])?;

        let replacement = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, Some("device:new-inode"))],
        );

        assert_eq!(replacement.changed[0].id, first.changed[0].id);
        assert_eq!(replacement.jobs.len(), 1);
        Ok(())
    }

    #[test]
    fn a_scan_of_one_folder_leaves_the_tracks_of_the_others_alone() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let in_folder = |folder: &str, name: &str| DiscoveredTrack {
            path: root.join(folder).join(name),
            relative_path: PathBuf::from(folder).join(name),
            size: 10,
            modified_ns: 20,
            file_identity: None,
        };
        scan(
            &mut cache,
            folder,
            root,
            vec![in_folder("a", "1.flac"), in_folder("ab", "1.flac")],
        );

        // "a" is empty now; "ab" only starts with the same letters.
        let rescan = cache.reconcile(folder, &[root.join("a")], Vec::new())?;

        assert_eq!(rescan.changed.len(), 1);
        assert_eq!(rescan.changed[0].path, root.join("a").join("1.flac"));
        assert_eq!(
            cache.tracks().iter().filter(|track| track.present).count(),
            1
        );
        Ok(())
    }

    #[test]
    fn removing_a_folder_hides_its_tracks_and_adding_it_again_brings_them_back() -> CacheResult<()>
    {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let id = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, None)],
        )
        .changed[0]
            .id;
        cache.store_updates(&[ready(id, "Song")])?;

        let removed = cache.remove_folder(folder)?;
        assert_eq!(removed.len(), 1);
        assert!(!removed[0].present);
        assert!(cache.folders()?.is_empty());
        assert_eq!(cache.tracks().len(), 1);

        let again = cache.add_folder(root)?.expect("the folder is new again");
        let back = scan(
            &mut cache,
            again,
            root,
            vec![file(root, "song.mp3", 10, None)],
        );
        assert_eq!(back.changed[0].id, id);
        assert_eq!(back.changed[0].title, "Song");
        assert_eq!(back.changed[0].folder_id, Some(again));
        assert!(back.jobs.is_empty());
        Ok(())
    }

    #[test]
    fn folders_do_not_nest_and_the_first_run_is_seeded_once() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        // Inside one that is there: covered already.
        assert_eq!(cache.add_folder(&root.join("inner"))?, None);
        assert_eq!(cache.add_folder(root)?, None);

        // Around one that is there: it takes that one up.
        let outer = root.parent().expect("a temporary directory has a parent");
        let around = cache.add_folder(outer)?.expect("the outer folder is new");
        assert_eq!(cache.folders()?, [(around, outer.to_path_buf())]);
        assert!(
            cache
                .tracks()
                .iter()
                .all(|track| track.folder_id != Some(folder))
        );

        // Seeding happens once: an emptied library stays empty.
        cache.remove_folder(around)?;
        cache.seed(Some(root.to_path_buf()))?;
        assert_eq!(cache.folders()?.len(), 1);
        let seeded = cache.folders()?[0].0;
        cache.remove_folder(seeded)?;
        cache.seed(Some(root.to_path_buf()))?;
        assert!(cache.folders()?.is_empty());
        Ok(())
    }

    #[test]
    fn tracks_under_no_folder_go_absent() -> CacheResult<()> {
        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.mp3", 10, None)],
        );

        assert!(cache.mark_absent_outside(&[root.to_path_buf()])?.is_empty());
        let gone = cache.mark_absent_outside(&[root.join("elsewhere")])?;

        assert_eq!(gone.len(), 1);
        assert!(!gone[0].present);
        Ok(())
    }

    #[test]
    fn a_database_of_the_unversioned_build_keeps_its_ids_through_the_first_scan() -> CacheResult<()>
    {
        let directory = tempdir()?;
        let root = directory.path();
        let database = root.join("library.sqlite3");
        {
            let connection = Connection::open(&database)?;
            connection.execute_batch(migrations::tests::UNVERSIONED_SCHEMA)?;
            connection.execute(
                "INSERT INTO tracks (id, path, relative_path, size, modified_ns, title, artist,
                                     metadata_state)
                 VALUES (41, ?1, ?2, 10, 20, 'Tagged title', 'Artist', 1)",
                params![
                    path_to_bytes(&root.join("song.flac")),
                    path_to_bytes(Path::new("song.flac"))
                ],
            )?;
        }

        let mut cache = Cache::open_at(&database)?;
        cache.seed(Some(root.to_path_buf()))?;
        let folder = cache.folders()?[0].0;
        let first = scan(
            &mut cache,
            folder,
            root,
            vec![file(root, "song.flac", 10, Some("volume:1"))],
        );

        // The same row, now of the folder and known by its identity, shown
        // as it was until its tags are read for the new columns.
        assert_eq!(first.changed.len(), 1);
        let track = &first.changed[0];
        assert_eq!(track.id, 41);
        assert_eq!(track.title, "Tagged title");
        assert_eq!(track.folder_id, Some(folder));
        assert_eq!(track.file_identity.as_deref(), Some("volume:1"));
        assert_eq!(first.jobs, [41]);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_round_trip_losslessly() -> CacheResult<()> {
        use std::os::unix::ffi::OsStringExt;

        let (directory, mut cache, folder) = library()?;
        let root = directory.path();
        let file_name = OsString::from_vec(b"song-\xFF.mp3".to_vec());
        let audio_path = root.join(&file_name);
        let scan = cache.reconcile(
            folder,
            &[root.to_path_buf()],
            vec![DiscoveredTrack {
                path: audio_path.clone(),
                relative_path: PathBuf::from(file_name),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:non-utf8".into()),
            }],
        )?;

        assert_eq!(scan.changed[0].path, audio_path);
        drop(cache);
        let cache = Cache::open_at(&root.join("library.sqlite3"))?;
        assert_eq!(cache.tracks()[0].path, audio_path);
        Ok(())
    }
}

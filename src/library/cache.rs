use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row, params};

use super::metadata::MetadataJob;
use super::scanner::{DiscoveredTrack, supports_embedded_metadata};
use super::track::{MetadataState, Track, TrackUpdate};

pub(super) type CacheResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub(super) struct PreparedScan {
    pub(super) tracks: Vec<Track>,
    pub(super) metadata_jobs: Vec<MetadataJob>,
}

pub(super) struct Cache {
    connection: Connection,
}

impl Cache {
    pub(super) fn open() -> CacheResult<Self> {
        let directory = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("618player");
        fs::create_dir_all(&directory)?;
        Self::open_at(&directory.join("library.sqlite3"))
    }

    fn open_at(path: &Path) -> CacheResult<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS tracks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 relative_path TEXT NOT NULL,
                 size INTEGER NOT NULL,
                 modified_ns INTEGER NOT NULL,
                 file_identity TEXT,
                 present INTEGER NOT NULL DEFAULT 1,
                 title TEXT NOT NULL,
                 artist TEXT,
                 album TEXT,
                 year INTEGER,
                 duration_ms INTEGER,
                 track_number INTEGER,
                 disc_number INTEGER,
                 metadata_state INTEGER NOT NULL,
                 metadata_error TEXT
             );

             CREATE TABLE IF NOT EXISTS playlists (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                 updated_at INTEGER NOT NULL DEFAULT (unixepoch())
             );

             CREATE TABLE IF NOT EXISTS playlist_tracks (
                 playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                 track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (playlist_id, position)
             );

             CREATE INDEX IF NOT EXISTS playlist_tracks_track_id
                  ON playlist_tracks(track_id);",
        )?;
        let has_file_identity = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'file_identity'
             )",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if !has_file_identity {
            connection.execute("ALTER TABLE tracks ADD COLUMN file_identity TEXT", [])?;
        }
        let has_present = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'present'
             )",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if !has_present {
            connection.execute(
                "ALTER TABLE tracks ADD COLUMN present INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        Ok(Self { connection })
    }

    pub(super) fn load_tracks(&self) -> CacheResult<Vec<Track>> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, relative_path, size, modified_ns, file_identity,
                    title, artist, album, year, duration_ms, track_number, disc_number,
                    metadata_state, metadata_error
             FROM tracks
             WHERE present = 1",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Track::from_cache(
                row.get(0)?,
                read_path(row, 1)?,
                read_path(row, 2)?,
                from_i64(row.get(3)?),
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get::<_, Option<i64>>(10)?.map(from_i64),
                row.get(11)?,
                row.get(12)?,
                MetadataState::from_i64(row.get(13)?),
                row.get(14)?,
            ))
        })?;

        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(super) fn prepare_scan(
        &mut self,
        discovered: Vec<DiscoveredTrack>,
        cached: &[Track],
    ) -> CacheResult<PreparedScan> {
        let cached_by_path: HashMap<_, _> = cached
            .iter()
            .map(|track| (track.path.clone(), track))
            .collect();
        let mut cached_by_identity = HashMap::new();
        let mut duplicate_identities = HashSet::new();
        for track in cached {
            let Some(identity) = track.file_identity.as_deref() else {
                continue;
            };
            if cached_by_identity.insert(identity, track).is_some() {
                duplicate_identities.insert(identity);
            }
        }
        let discovered_by_path: HashMap<_, _> = discovered
            .iter()
            .map(|track| (track.path.clone(), track.file_identity.clone()))
            .collect();
        let transaction = self.connection.transaction()?;
        let mut tracks = Vec::with_capacity(discovered.len());
        let mut metadata_jobs = Vec::new();
        let mut seen_ids = HashSet::with_capacity(discovered.len());

        for file in discovered {
            let path_match = cached_by_path.get(&file.path).copied();
            let identity_match = file
                .file_identity
                .as_deref()
                .filter(|identity| !duplicate_identities.contains(identity))
                .and_then(|identity| cached_by_identity.get(identity).copied())
                .filter(|track| !seen_ids.contains(&track.id))
                .filter(|track| {
                    discovered_by_path
                        .get(&track.path)
                        .is_none_or(|identity| identity != &file.file_identity)
                });
            let cached_track = path_match.or(identity_match);

            if let Some(cached_track) = cached_track {
                let path_changed = cached_track.path != file.path
                    || cached_track.relative_path != file.relative_path;
                let identity_changed = cached_track.file_identity != file.file_identity;
                if cached_track.matches_fingerprint(file.size, file.modified_ns)
                    && !identity_changed
                    && cached_track.metadata_state != MetadataState::Pending
                {
                    let mut track = cached_track.clone();
                    track.relocate(file.path, file.relative_path, file.file_identity);
                    if path_changed || identity_changed {
                        update_file_record(&transaction, &track)?;
                    }
                    seen_ids.insert(track.id);
                    tracks.push(track);
                    continue;
                }

                let mut track = cached_track.clone();
                track.relocate(file.path, file.relative_path, file.file_identity);
                track.size = file.size;
                track.modified_ns = file.modified_ns;
                track.metadata_state = if supports_embedded_metadata(&track.path) {
                    MetadataState::Pending
                } else {
                    MetadataState::Fallback
                };
                track.metadata_error = None;
                update_file_record(&transaction, &track)?;
                if track.metadata_state == MetadataState::Pending {
                    metadata_jobs.push(MetadataJob {
                        id: track.id,
                        path: track.path.clone(),
                    });
                }
                seen_ids.insert(track.id);
                tracks.push(track);
                continue;
            }

            let state = if supports_embedded_metadata(&file.path) {
                MetadataState::Pending
            } else {
                MetadataState::Fallback
            };
            let mut track = Track::fallback(
                0,
                file.path,
                file.relative_path,
                file.size,
                file.modified_ns,
                file.file_identity,
                state,
            );
            track.id = transaction.query_row(
                "INSERT INTO tracks (
                    path, relative_path, size, modified_ns, file_identity, present,
                    title, metadata_state
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                    relative_path = excluded.relative_path,
                    size = excluded.size,
                    modified_ns = excluded.modified_ns,
                    file_identity = excluded.file_identity,
                    present = 1,
                    title = excluded.title,
                    artist = NULL,
                    album = NULL,
                    year = NULL,
                    duration_ms = NULL,
                    track_number = NULL,
                    disc_number = NULL,
                    metadata_state = excluded.metadata_state,
                    metadata_error = NULL
                 RETURNING id",
                params![
                    path_to_bytes(&track.path),
                    path_to_bytes(&track.relative_path),
                    to_i64(track.size),
                    track.modified_ns,
                    track.file_identity,
                    track.title,
                    track.metadata_state.as_i64(),
                ],
                |row| row.get(0),
            )?;
            if state == MetadataState::Pending {
                metadata_jobs.push(MetadataJob {
                    id: track.id,
                    path: track.path.clone(),
                });
            }
            seen_ids.insert(track.id);
            tracks.push(track);
        }

        for track in cached {
            if !seen_ids.contains(&track.id) {
                transaction.execute("UPDATE tracks SET present = 0 WHERE id = ?1", [track.id])?;
            }
        }

        transaction.commit()?;
        Ok(PreparedScan {
            tracks,
            metadata_jobs,
        })
    }

    pub(super) fn store_updates(&mut self, updates: &[TrackUpdate]) -> CacheResult<()> {
        let transaction = self.connection.transaction()?;
        for update in updates {
            transaction.execute(
                "UPDATE tracks SET
                    title = ?2,
                    artist = ?3,
                    album = ?4,
                    year = ?5,
                    duration_ms = ?6,
                    track_number = ?7,
                    disc_number = ?8,
                    metadata_state = ?9,
                    metadata_error = ?10
                 WHERE id = ?1",
                params![
                    update.id,
                    update.title,
                    update.artist,
                    update.album,
                    update.year,
                    update.duration_ms.map(to_i64),
                    update.track_number,
                    update.disc_number,
                    update.metadata_state.as_i64(),
                    update.metadata_error,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn update_file_record(connection: &Connection, track: &Track) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE tracks SET
            path = ?2,
            relative_path = ?3,
            size = ?4,
            modified_ns = ?5,
            file_identity = ?6,
            present = 1,
            metadata_state = ?7,
            metadata_error = NULL
         WHERE id = ?1",
        params![
            track.id,
            path_to_bytes(&track.path),
            path_to_bytes(&track.relative_path),
            to_i64(track.size),
            track.modified_ns,
            track.file_identity,
            track.metadata_state.as_i64(),
        ],
    )?;
    Ok(())
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
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn unchanged_files_reuse_cached_metadata() -> CacheResult<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let audio_path = directory.path().join("song.mp3");
        let mut cache = Cache::open_at(&path)?;
        let first = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path.clone(),
                relative_path: PathBuf::from("song.mp3"),
                size: 10,
                modified_ns: 20,
                file_identity: None,
            }],
            &[],
        )?;
        assert_eq!(first.metadata_jobs.len(), 1);

        let update = TrackUpdate {
            id: first.tracks[0].id,
            title: "Cached title".into(),
            artist: Some("Cached artist".into()),
            album: None,
            year: None,
            duration_ms: Some(1000),
            track_number: None,
            disc_number: None,
            metadata_state: MetadataState::Ready,
            metadata_error: None,
        };
        cache.store_updates(&[update])?;
        let cached = cache.load_tracks()?;
        let second = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path,
                relative_path: PathBuf::from("song.mp3"),
                size: 10,
                modified_ns: 20,
                file_identity: None,
            }],
            &cached,
        )?;

        assert!(second.metadata_jobs.is_empty());
        assert_eq!(second.tracks[0].title, "Cached title");
        assert_eq!(second.tracks[0].artist.as_deref(), Some("Cached artist"));
        Ok(())
    }

    #[test]
    fn wma_tracks_are_cached_without_parse_jobs() -> CacheResult<()> {
        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let scan = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: directory.path().join("song.wma"),
                relative_path: PathBuf::from("song.wma"),
                size: 10,
                modified_ns: 20,
                file_identity: None,
            }],
            &[],
        )?;

        assert!(scan.metadata_jobs.is_empty());
        assert_eq!(scan.tracks[0].metadata_state, MetadataState::Fallback);
        Ok(())
    }

    #[test]
    fn changed_files_are_queued_and_missing_files_are_removed() -> CacheResult<()> {
        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let audio_path = directory.path().join("song.mp3");
        let first = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path.clone(),
                relative_path: PathBuf::from("song.mp3"),
                size: 10,
                modified_ns: 20,
                file_identity: None,
            }],
            &[],
        )?;
        let cached = first.tracks;
        cache
            .connection
            .execute("INSERT INTO playlists (name) VALUES ('Keep me')", [])?;
        let playlist_id = cache.connection.last_insert_rowid();
        cache.connection.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position)
             VALUES (?1, ?2, 0)",
            [playlist_id, cached[0].id],
        )?;

        let changed = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path,
                relative_path: PathBuf::from("song.mp3"),
                size: 11,
                modified_ns: 21,
                file_identity: None,
            }],
            &cached,
        )?;
        assert_eq!(changed.metadata_jobs.len(), 1);
        assert_eq!(changed.tracks[0].id, cached[0].id);

        let removed = cache.prepare_scan(Vec::new(), &changed.tracks)?;
        assert!(removed.tracks.is_empty());
        assert!(cache.load_tracks()?.is_empty());
        assert_eq!(
            cache.connection.query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = ?1",
                [cached[0].id],
                |row| row.get::<_, u32>(0),
            )?,
            1
        );
        Ok(())
    }

    #[test]
    fn filesystem_identity_preserves_track_id_across_rename() -> CacheResult<()> {
        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let original_path = directory.path().join("old.wma");
        let first = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: original_path,
                relative_path: PathBuf::from("old.wma"),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:file".into()),
            }],
            &[],
        )?;
        let original_id = first.tracks[0].id;

        let renamed_path = directory.path().join("new.wma");
        let renamed = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: renamed_path.clone(),
                relative_path: PathBuf::from("new.wma"),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:file".into()),
            }],
            &first.tracks,
        )?;

        assert_eq!(renamed.tracks[0].id, original_id);
        assert_eq!(renamed.tracks[0].path, renamed_path);
        assert_eq!(renamed.tracks[0].title, "new");
        assert_eq!(cache.load_tracks()?[0].id, original_id);
        Ok(())
    }

    #[test]
    fn atomic_replacement_at_same_path_reuses_the_track_row() -> CacheResult<()> {
        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let audio_path = directory.path().join("song.mp3");
        let first = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path.clone(),
                relative_path: PathBuf::from("song.mp3"),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:old-inode".into()),
            }],
            &[],
        )?;

        let replacement = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path,
                relative_path: PathBuf::from("song.mp3"),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:new-inode".into()),
            }],
            &first.tracks,
        )?;

        assert_eq!(replacement.tracks[0].id, first.tracks[0].id);
        assert_eq!(replacement.metadata_jobs.len(), 1);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_round_trip_losslessly() -> CacheResult<()> {
        use std::os::unix::ffi::OsStringExt;

        let directory = tempdir()?;
        let mut cache = Cache::open_at(&directory.path().join("library.sqlite3"))?;
        let file_name = OsString::from_vec(b"song-\xFF.mp3".to_vec());
        let audio_path = directory.path().join(&file_name);
        let scan = cache.prepare_scan(
            vec![DiscoveredTrack {
                path: audio_path.clone(),
                relative_path: PathBuf::from(file_name),
                size: 10,
                modified_ns: 20,
                file_identity: Some("device:non-utf8".into()),
            }],
            &[],
        )?;

        assert_eq!(scan.tracks[0].path, audio_path);
        assert_eq!(cache.load_tracks()?[0].path, audio_path);
        Ok(())
    }
}

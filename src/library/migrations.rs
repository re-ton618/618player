//! The library database's schema, as a list of steps. `PRAGMA user_version`
//! counts the steps a database has had; opening one runs the rest, each in
//! a transaction with its count, so a step that fails leaves the database
//! at the step before.
//!
//! A step is never changed once a build with it has shipped: what comes
//! after is another step.

use rusqlite::{Connection, Transaction};

type Migration = fn(&Transaction<'_>) -> rusqlite::Result<()>;

const MIGRATIONS: &[Migration] = &[baseline, library_model, play_counts];

/// Brings `connection`'s schema up to this build's. A database a newer
/// build has stepped further is refused rather than guessed at.
pub(super) fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let version = version as usize;
    if version > MIGRATIONS.len() {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "the library database is of schema {version}, newer than this build's {}",
            MIGRATIONS.len()
        )));
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
        let transaction = connection.transaction()?;
        migration(&transaction)?;
        transaction.pragma_update(None, "user_version", index as u32 + 1)?;
        transaction.commit()?;
    }
    Ok(())
}

/// Step 1: the schema the builds before versioning made in place, whose
/// databases all say version 0. One of those has its tables already,
/// the oldest without the two columns added since; a new one has nothing.
fn baseline(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS tracks (
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
    for (column, definition) in [
        ("file_identity", "TEXT"),
        ("present", "INTEGER NOT NULL DEFAULT 1"),
    ] {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1)",
            [column],
            |row| row.get(0),
        )?;
        if !exists {
            transaction.execute_batch(&format!(
                "ALTER TABLE tracks ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    Ok(())
}

/// Step 2: the folders the library is made of, and what an audiophile's
/// library tells of a track. Tracks that were read before are read once
/// more for the new columns, keeping what they show until then.
fn library_model(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "CREATE TABLE folders (
             id INTEGER PRIMARY KEY,
             path TEXT NOT NULL UNIQUE
         );

         CREATE TABLE settings (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );

         ALTER TABLE tracks ADD COLUMN folder_id INTEGER
             REFERENCES folders(id) ON DELETE SET NULL;
         ALTER TABLE tracks ADD COLUMN album_artist TEXT;
         ALTER TABLE tracks ADD COLUMN genre TEXT;
         ALTER TABLE tracks ADD COLUMN composer TEXT;
         ALTER TABLE tracks ADD COLUMN track_total INTEGER;
         ALTER TABLE tracks ADD COLUMN disc_total INTEGER;
         ALTER TABLE tracks ADD COLUMN compilation INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE tracks ADD COLUMN release_id TEXT;
         ALTER TABLE tracks ADD COLUMN sample_rate INTEGER;
         ALTER TABLE tracks ADD COLUMN bit_depth INTEGER;
         ALTER TABLE tracks ADD COLUMN channels INTEGER;
         ALTER TABLE tracks ADD COLUMN codec TEXT;
         ALTER TABLE tracks ADD COLUMN bitrate_kbps INTEGER;
         ALTER TABLE tracks ADD COLUMN track_gain_db REAL;
         ALTER TABLE tracks ADD COLUMN track_peak REAL;
         ALTER TABLE tracks ADD COLUMN album_gain_db REAL;
         ALTER TABLE tracks ADD COLUMN album_peak REAL;

         CREATE INDEX tracks_folder_id ON tracks(folder_id);

         UPDATE tracks SET metadata_state = 0 WHERE metadata_state IN (1, 3);",
    )
}

/// Step 3: how many times each track was listened to.
fn play_counts(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction
        .execute_batch("ALTER TABLE tracks ADD COLUMN play_count INTEGER NOT NULL DEFAULT 0;")
}

#[cfg(test)]
pub(super) mod tests {
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::*;

    /// What the last build before versioning ran on every open, word for
    /// word: the schema its databases have.
    pub(in crate::library) const UNVERSIONED_SCHEMA: &str = "
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
             ON playlist_tracks(track_id);";

    fn version(connection: &Connection) -> usize {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap() as usize
    }

    fn columns(connection: &Connection, table: &str) -> Vec<String> {
        connection
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .unwrap()
            .query_map([table], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_database_of_the_unversioned_build_upgrades_in_place_keeping_its_ids() {
        let folder = tempdir().unwrap();
        let path = folder.path().join("library.sqlite3");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch("PRAGMA foreign_keys = ON;")
                .unwrap();
            connection.execute_batch(UNVERSIONED_SCHEMA).unwrap();
            connection
                .execute_batch(
                    "INSERT INTO tracks (id, path, relative_path, size, modified_ns, title,
                                         artist, album, year, duration_ms, track_number,
                                         disc_number, metadata_state, present)
                     VALUES (7, '/music/a.flac', 'a.flac', 10, 20, 'A', 'Artist', 'Album',
                             1999, 1000, 1, 1, 1, 1),
                            (42, '/music/b.wma', 'b.wma', 11, 21, 'B', NULL, NULL,
                             NULL, NULL, NULL, NULL, 2, 1),
                            (99, '/music/gone.flac', 'gone.flac', 12, 22, 'Gone', NULL, NULL,
                             NULL, NULL, NULL, NULL, 3, 0);
                     INSERT INTO playlists (id, name) VALUES (1, 'Kept');
                     INSERT INTO playlist_tracks (playlist_id, track_id, position)
                     VALUES (1, 42, 0), (1, 7, 1), (1, 99, 2);",
                )
                .unwrap();
            assert_eq!(version(&connection), 0);
        }

        let mut connection = Connection::open(&path).unwrap();
        migrate(&mut connection).unwrap();

        assert_eq!(version(&connection), MIGRATIONS.len());
        let tracks: Vec<(i64, String, Option<String>, i64, bool)> = connection
            .prepare("SELECT id, title, artist, metadata_state, present FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // Every id and what it showed is kept. Read tracks are due to be
        // read again for the new columns; a WMA's file-name title is not.
        assert_eq!(
            tracks,
            [
                (7, "A".to_owned(), Some("Artist".to_owned()), 0, true),
                (42, "B".to_owned(), None, 2, true),
                (99, "Gone".to_owned(), None, 0, false),
            ]
        );
        let playlist: Vec<i64> = connection
            .prepare("SELECT track_id FROM playlist_tracks ORDER BY position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(playlist, [42, 7, 99]);
        let track_columns = columns(&connection, "tracks");
        for column in [
            "folder_id",
            "album_artist",
            "genre",
            "composer",
            "track_total",
            "disc_total",
            "compilation",
            "release_id",
            "sample_rate",
            "bit_depth",
            "channels",
            "codec",
            "bitrate_kbps",
            "track_gain_db",
            "track_peak",
            "album_gain_db",
            "album_peak",
        ] {
            assert!(track_columns.iter().any(|name| name == column), "{column}");
        }
        assert_eq!(columns(&connection, "folders"), ["id", "path"]);

        // Opened again, nothing is left to run and nothing changes.
        migrate(&mut connection).unwrap();
        assert_eq!(version(&connection), MIGRATIONS.len());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM tracks", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn the_oldest_unversioned_database_gains_its_two_missing_columns() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE tracks (
                     id INTEGER PRIMARY KEY,
                     path TEXT NOT NULL UNIQUE,
                     relative_path TEXT NOT NULL,
                     size INTEGER NOT NULL,
                     modified_ns INTEGER NOT NULL,
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
                 INSERT INTO tracks (id, path, relative_path, size, modified_ns, title,
                                     metadata_state)
                 VALUES (5, '/music/a.flac', 'a.flac', 1, 2, 'A', 1);",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let (identity, present): (Option<String>, bool) = connection
            .query_row(
                "SELECT file_identity, present FROM tracks WHERE id = 5",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((identity, present), (None, true));
    }

    #[test]
    fn a_new_database_gets_every_step_and_a_newer_one_is_refused() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(version(&connection), MIGRATIONS.len());
        assert!(columns(&connection, "tracks").contains(&"album_artist".to_owned()));

        connection
            .pragma_update(None, "user_version", MIGRATIONS.len() as u32 + 1)
            .unwrap();
        assert!(migrate(&mut connection).is_err());
    }
}

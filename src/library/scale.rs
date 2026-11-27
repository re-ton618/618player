//! How the library holds up at 50,000 tracks: measurements that print
//! what they took rather than checks that pass or fail.
//!
//! `cargo test --lib scale -- --ignored --nocapture`

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::cache::Cache;
use super::catalog::Catalog;
use super::filter::{matching_track_indices, SortColumn, SortDirection};
use super::scanner::{self, DiscoveredTrack};
use super::track::{FileRecord, Metadata, MetadataState, Track, TrackUpdate};

const TRACKS: usize = 50_000;
const ALBUM_SIZE: usize = 12;
const ALBUMS_PER_ARTIST: usize = 10;

fn relative_path(index: usize) -> PathBuf {
    let album = index / ALBUM_SIZE;
    PathBuf::from(format!("Artist {}", album / ALBUMS_PER_ARTIST))
        .join(format!("Album {album}"))
        .join(format!("{:02} Title {index}.flac", index % ALBUM_SIZE + 1))
}

fn metadata(index: usize) -> Metadata {
    let album = index / ALBUM_SIZE;
    let artist = format!("Artist {}", album / ALBUMS_PER_ARTIST);
    let compilation = album % ALBUMS_PER_ARTIST == ALBUMS_PER_ARTIST - 1;
    Metadata {
        title: format!("Title {index}"),
        artist: Some(if compilation {
            format!("Guest {}", index % 97)
        } else {
            artist.clone()
        }),
        album: Some(format!("Album {album}")),
        album_artist: Some(if compilation {
            super::track::VARIOUS_ARTISTS.to_owned()
        } else {
            artist
        }),
        genre: Some(format!("Genre {}", album % 23)),
        composer: album
            .is_multiple_of(5)
            .then(|| format!("Composer {}", album % 41)),
        year: Some(1960 + (album % 60) as u32),
        duration_ms: Some(120_000 + (index as u64 * 7_919) % 240_000),
        track_number: Some((index % ALBUM_SIZE) as u32 + 1),
        track_total: Some(ALBUM_SIZE as u32),
        disc_number: Some(1),
        disc_total: Some(1),
        compilation,
        release_id: None,
        sample_rate: Some(if album.is_multiple_of(4) {
            96_000
        } else {
            44_100
        }),
        bit_depth: Some(if album.is_multiple_of(4) { 24 } else { 16 }),
        channels: Some(2),
        codec: Some("FLAC".to_owned()),
        bitrate_kbps: Some(900 + (index % 600) as u32),
        replay_gain: super::track::ReplayGain {
            track_gain_db: Some(-6.0 - (index % 40) as f64 / 10.0),
            track_peak: Some(0.98),
            album_gain_db: Some(-7.5),
            album_peak: Some(1.0),
        },
    }
}

/// The `index`th track of the made-up library, as the model holds it.
pub(super) fn generated_track(index: usize) -> Track {
    let relative_path = relative_path(index);
    Track::new(
        FileRecord {
            id: index as i64 + 1,
            folder_id: Some(1),
            path: Path::new("/music").join(&relative_path),
            relative_path,
            size: 20_000_000 + index as u64,
            modified_ns: index as i64,
            file_identity: Some(format!("volume:{index:x}")),
            present: true,
        },
        metadata(index),
        MetadataState::Ready,
        None,
    )
}

fn discovered(root: &Path, index: usize) -> DiscoveredTrack {
    let relative_path = relative_path(index);
    DiscoveredTrack {
        path: root.join(&relative_path),
        relative_path,
        size: 20_000_000 + index as u64,
        modified_ns: index as i64,
        file_identity: Some(format!("volume:{index:x}")),
    }
}

fn timed<T>(what: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    println!("{what}: {:?}", started.elapsed());
    result
}

#[test]
#[ignore = "a measurement, not a check"]
fn scale_of_the_database_at_50_000_tracks() {
    let home = tempfile::tempdir().unwrap();
    let database = home.path().join("library.sqlite3");
    let root = home.path().join("music");
    let files = || (0..TRACKS).map(|index| discovered(&root, index)).collect();

    let mut cache = Cache::open_at(&database).unwrap();
    let folder = cache.add_folder(&root).unwrap().unwrap();
    let imported = timed("database: first import of 50,000 files", || {
        cache
            .reconcile(folder, std::slice::from_ref(&root), files())
            .unwrap()
    });
    assert_eq!(imported.jobs.len(), TRACKS);
    let updates: Vec<TrackUpdate> = imported
        .changed
        .iter()
        .enumerate()
        .map(|(index, track)| TrackUpdate {
            id: track.id,
            metadata: metadata(index),
            metadata_state: MetadataState::Ready,
            metadata_error: None,
        })
        .collect();
    timed(
        "database: storing 50,000 tracks' tags, 64 at a time",
        || {
            for batch in updates.chunks(64) {
                cache.store_updates(batch).unwrap();
            }
        },
    );
    drop(cache);
    println!(
        "database: {} KiB on disk",
        std::fs::metadata(&database).unwrap().len() / 1024
    );

    let mut cache = timed(
        "database: opened and all 50,000 tracks read (launch)",
        || Cache::open_at(&database).unwrap(),
    );
    let tracks = timed("database: the tracks handed to the app", || cache.tracks());
    assert_eq!(tracks.len(), TRACKS);
    let unchanged = timed(
        "database: launch reconcile, nothing changed on disk",
        || {
            cache
                .reconcile(folder, std::slice::from_ref(&root), files())
                .unwrap()
        },
    );
    assert!(unchanged.changed.is_empty() && unchanged.jobs.is_empty());

    let album = root.join("Artist 0").join("Album 3");
    let mut one_album: Vec<DiscoveredTrack> =
        (36..48).map(|index| discovered(&root, index)).collect();
    one_album[0].size += 1;
    let rescanned = timed(
        "database: one album folder rescanned, one file changed",
        || {
            cache
                .reconcile(folder, std::slice::from_ref(&album), one_album)
                .unwrap()
        },
    );
    assert_eq!(rescanned.changed.len(), 1);

    let mut renamed: Vec<DiscoveredTrack> = files();
    for file in renamed.iter_mut().take(5_000) {
        file.path.set_extension("moved.flac");
        file.relative_path.set_extension("moved.flac");
    }
    let moved = timed(
        "database: 5,000 files renamed, found again by identity",
        || {
            cache
                .reconcile(folder, std::slice::from_ref(&root), renamed)
                .unwrap()
        },
    );
    assert_eq!(moved.changed.len(), 5_000);
    assert!(moved.jobs.len() <= 1);
    assert_eq!(cache.tracks().len(), TRACKS);
}

#[test]
#[ignore = "a measurement, not a check"]
fn scale_of_search_sort_and_catalog_at_50_000_tracks() {
    let tracks: Vec<Track> = (0..TRACKS).map(generated_track).collect();
    for (what, query) in [
        ("everything", ""),
        ("a letter", "a"),
        ("an album", "album 3141"),
        ("a genre", "genre 7"),
        ("nothing", "zzzz"),
    ] {
        let found = timed(&format!("search: {what}, sorted by title"), || {
            matching_track_indices(&tracks, query, SortColumn::Title, SortDirection::Ascending)
        });
        println!("    {} tracks", found.len());
    }
    for column in [
        SortColumn::Title,
        SortColumn::Artist,
        SortColumn::Album,
        SortColumn::Year,
        SortColumn::Duration,
    ] {
        timed(&format!("sort: all 50,000 by {column:?}"), || {
            matching_track_indices(&tracks, "", column, SortDirection::Descending)
        });
    }
    let catalog = timed("catalog: albums and artists of 50,000 tracks", || {
        Catalog::of(&tracks)
    });
    println!(
        "    {} albums, {} album artists",
        catalog.albums().len(),
        catalog.artists().len()
    );
}

#[test]
#[ignore = "a measurement, not a check"]
fn scale_of_a_scan_of_50_000_files() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("music");
    timed("disk: writing 50,000 empty files", || {
        for index in 0..TRACKS {
            let path = root.join(relative_path(index));
            if index.is_multiple_of(ALBUM_SIZE) {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            }
            std::fs::write(path, []).unwrap();
        }
    });
    let found = timed("disk: walking 50,000 files, with their identities", || {
        scanner::scan(&root, &root)
    });
    assert_eq!(found.len(), TRACKS);
    assert!(found.iter().all(|file| file.file_identity.is_some()));
    let album = root.join("Artist 0").join("Album 3");
    let found = timed("disk: walking one album folder", || {
        scanner::scan(&root, &album)
    });
    assert_eq!(found.len(), ALBUM_SIZE);
}

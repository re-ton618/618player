//! The music library: the folders the user made it of, the tracks found
//! under them, and the albums and artists those are of. UI-free; the app
//! drives it with [`LibraryCommand`]s and follows it by [`LibraryEvent`]s.
//!
//! It runs on a thread of its own ([`Library`]), which owns the database,
//! reads the tags and watches the folders, so that none of it holds the
//! UI thread.

mod artwork;
mod cache;
mod catalog;
mod filter;
mod metadata;
mod migrations;
#[cfg(test)]
mod scale;
mod scanner;
mod service;
mod track;
mod watcher;

use std::path::PathBuf;

pub(crate) use artwork::load_embedded_data_uri;
pub(crate) use catalog::Catalog;
pub(crate) use filter::{SortColumn, SortDirection, matching_in_order, matching_track_indices};
pub(crate) use service::Library;
pub(crate) use track::{MetadataState, Track, TrackUpdate};

/// A folder the library is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub(crate) id: i64,
    pub(crate) path: PathBuf,
    /// The folder is there: not on a drive that is unplugged, nor deleted.
    pub(crate) available: bool,
}

/// A list of tracks the user named, in the order they were added. A track
/// is in it once; one that goes absent stays in it, to be there again
/// when its file is back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Playlist {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) tracks: Vec<i64>,
}

/// What the app asks of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LibraryCommand {
    /// Makes the folder part of the library, and scans it.
    AddFolder(PathBuf),
    /// Takes the folder of this id out of the library. Its tracks go
    /// absent and keep their ids.
    RemoveFolder(i64),
    /// Makes a playlist of this name, holding these tracks.
    CreatePlaylist {
        name: String,
        tracks: Vec<i64>,
    },
    RenamePlaylist {
        playlist: i64,
        name: String,
    },
    DeletePlaylist(i64),
    /// Adds the tracks to the end of the playlist, those it holds already
    /// left where they are.
    AddToPlaylist {
        playlist: i64,
        tracks: Vec<i64>,
    },
    RemoveFromPlaylist {
        playlist: i64,
        track: i64,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum LibraryEvent {
    /// Every track the database has, as the last run left them: there at
    /// once, before any folder is looked at. Absent tracks are among them.
    Loaded(Vec<Track>),
    /// The library's folders, whenever one is added, removed, lost or back.
    Folders(Vec<Folder>),
    /// Tracks that are new, moved, changed, back or gone absent since.
    Changed(Vec<Track>),
    MetadataBatch(Vec<TrackUpdate>),
    /// Every playlist, by name: at once, and whenever one changes.
    Playlists(Vec<Playlist>),
    /// The library is as the disk is, and every tag is read; until the
    /// next change.
    Complete,
    Failed(String),
}

/// A made-up library of `count` tagged tracks: albums of twelve, ten
/// albums an artist, every tenth album a compilation.
#[cfg(test)]
pub(crate) fn test_library(count: usize) -> Vec<Track> {
    (0..count).map(scale::generated_track).collect()
}

#[cfg(test)]
pub(crate) fn test_track(id: i64, path: PathBuf, album: Option<&str>) -> Track {
    Track::new(
        track::FileRecord {
            id,
            folder_id: Some(1),
            path: path.clone(),
            relative_path: path,
            size: 1,
            modified_ns: 1,
            file_identity: None,
            present: true,
        },
        track::Metadata {
            // Sorted by title, they are in the order of their ids.
            title: format!("Track {id:03}"),
            album: album.map(str::to_owned),
            ..track::Metadata::default()
        },
        track::MetadataState::Ready,
        None,
    )
}

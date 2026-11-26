//! What changes on disk under the library's folders while the app runs:
//! the system's notifications ([`Watcher`]), gathered into the folders to
//! scan again once they have gone quiet ([`Pending`]).

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use super::scanner::is_supported_audio;

/// How long nothing more has to change before what changed is scanned: a
/// copy or a tagger's save is many notifications in a row.
pub(super) const QUIET: Duration = Duration::from_millis(500);
/// The longest a change waits while more keep coming: a long copy shows
/// up as it goes, in steps.
pub(super) const MAX_WAIT: Duration = Duration::from_secs(3);
/// More changed folders than this are scanned as their whole library
/// folders, in one walk.
const MAX_DIRECTORIES: usize = 64;

/// What the system told of the watched folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Change {
    /// The folders in which something changed.
    Directories(Vec<PathBuf>),
    /// Notifications were lost, or the watch broke: anything may have.
    Everything,
}

/// Watches library folders, each with everything under it.
pub(super) struct Watcher {
    inner: Option<RecommendedWatcher>,
    watched: HashSet<PathBuf>,
}

impl Watcher {
    /// A watcher that tells `report` of every change, on a thread of the
    /// system's. Without one (the system refused) the library only
    /// follows the disk at launch.
    pub(super) fn new(report: impl Fn(Change) + Send + 'static) -> Self {
        let handler = move |result: notify::Result<notify::Event>| match result {
            Ok(event) if event.need_rescan() => report(Change::Everything),
            // Reading a file changes nothing.
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => {}
            Ok(event) => {
                let directories: Vec<PathBuf> = event
                    .paths
                    .iter()
                    .filter_map(|path| changed_directory(path))
                    .collect();
                if !directories.is_empty() {
                    report(Change::Directories(directories));
                }
            }
            Err(error) => {
                eprintln!("Library watch failed: {error}");
                report(Change::Everything);
            }
        };
        let inner = notify::recommended_watcher(handler)
            .inspect_err(|error| eprintln!("Library folders are not watched: {error}"))
            .ok();
        Self {
            inner,
            watched: HashSet::new(),
        }
    }

    /// Watches `root`, unless it is watched already; false when it could
    /// not (it is not there).
    pub(super) fn watch(&mut self, root: &Path) -> bool {
        if self.watched.contains(root) {
            return true;
        }
        let Some(inner) = &mut self.inner else {
            return false;
        };
        match inner.watch(root, RecursiveMode::Recursive) {
            Ok(()) => {
                self.watched.insert(root.to_path_buf());
                true
            }
            Err(error) => {
                eprintln!("{} is not watched: {error}", root.display());
                false
            }
        }
    }

    pub(super) fn unwatch(&mut self, root: &Path) {
        if self.watched.remove(root)
            && let Some(inner) = &mut self.inner
        {
            // A folder that is gone took its watch with it.
            let _ = inner.unwatch(root);
        }
    }
}

/// The folder to scan again for a change at `path`: an audio file's own,
/// a folder itself. A path that is gone may have been a folder of tracks.
/// Other files (covers, a tagger's temporary files) change nothing.
fn changed_directory(path: &Path) -> Option<PathBuf> {
    if is_supported_audio(path) && !path.is_dir() {
        return path.parent().map(Path::to_path_buf);
    }
    (path.is_dir() || !path.exists()).then(|| path.to_path_buf())
}

/// What is to be scanned once the changes have gone quiet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dirty {
    Everything,
    /// Folders, none inside another.
    Directories(Vec<PathBuf>),
}

/// The changes that wait to be scanned: debounced, so that a burst of
/// thousands of them is one scan of the folders they were in.
#[derive(Debug, Default)]
pub(super) struct Pending {
    directories: BTreeSet<PathBuf>,
    everything: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Pending {
    pub(super) fn note(&mut self, change: Change, now: Instant) {
        match change {
            Change::Directories(directories) => self.directories.extend(directories),
            Change::Everything => self.everything = true,
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// When the waiting changes are to be scanned: [`QUIET`] after the
    /// last, but no later than [`MAX_WAIT`] after the first. None when
    /// nothing waits.
    pub(super) fn due(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + QUIET).min(first + MAX_WAIT))
    }

    /// What waits, which then no longer does.
    pub(super) fn take(&mut self) -> Option<Dirty> {
        self.first.take()?;
        self.last = None;
        let directories = std::mem::take(&mut self.directories);
        if std::mem::take(&mut self.everything) {
            return Some(Dirty::Everything);
        }
        // Sorted, a folder comes right before what is inside it.
        let mut outermost: Vec<PathBuf> = Vec::new();
        for directory in directories {
            if !outermost
                .last()
                .is_some_and(|last| directory.starts_with(last))
            {
                outermost.push(directory);
            }
        }
        Some(if outermost.len() > MAX_DIRECTORIES {
            Dirty::Everything
        } else {
            Dirty::Directories(outermost)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn directories(paths: &[&str]) -> Change {
        Change::Directories(paths.iter().map(PathBuf::from).collect())
    }

    #[test]
    fn changes_wait_until_quiet_but_not_forever() {
        let start = Instant::now();
        let mut pending = Pending::default();
        assert_eq!(pending.due(), None);
        assert_eq!(pending.take(), None);

        pending.note(directories(&["/music/a"]), start);
        assert_eq!(pending.due(), Some(start + QUIET));

        // Each further change puts the scan off...
        let later = start + Duration::from_millis(400);
        pending.note(directories(&["/music/b"]), later);
        assert_eq!(pending.due(), Some(later + QUIET));

        // ...up to the longest wait from the first.
        let much_later = start + MAX_WAIT - Duration::from_millis(10);
        pending.note(directories(&["/music/c"]), much_later);
        assert_eq!(pending.due(), Some(start + MAX_WAIT));

        assert!(pending.take().is_some());
        assert_eq!(pending.due(), None);
    }

    #[test]
    fn a_burst_is_one_scan_of_the_outermost_folders() {
        let now = Instant::now();
        let mut pending = Pending::default();
        for _ in 0..1_000 {
            pending.note(
                directories(&["/music/a/disc 1", "/music/b", "/music/a", "/music/ab"]),
                now,
            );
        }

        assert_eq!(
            pending.take(),
            Some(Dirty::Directories(vec![
                PathBuf::from("/music/a"),
                PathBuf::from("/music/ab"),
                PathBuf::from("/music/b"),
            ]))
        );
        assert_eq!(pending.take(), None);
    }

    #[test]
    fn too_many_folders_and_lost_notifications_scan_everything() {
        let now = Instant::now();
        let mut pending = Pending::default();
        for album in 0..=MAX_DIRECTORIES {
            pending.note(
                Change::Directories(vec![PathBuf::from(format!("/music/{album}"))]),
                now,
            );
        }
        assert_eq!(pending.take(), Some(Dirty::Everything));

        pending.note(directories(&["/music/a"]), now);
        pending.note(Change::Everything, now);
        assert_eq!(pending.take(), Some(Dirty::Everything));
    }

    #[test]
    fn a_change_is_of_the_folder_it_happened_in() {
        let folder = tempfile::tempdir().unwrap();
        let album = folder.path().join("album");
        std::fs::create_dir(&album).unwrap();
        std::fs::write(album.join("cover.jpg"), []).unwrap();

        assert_eq!(
            changed_directory(&album.join("1.flac")),
            Some(album.clone())
        );
        assert_eq!(changed_directory(&album), Some(album.clone()));
        assert_eq!(changed_directory(&album.join("cover.jpg")), None);
        // Gone: it may have been a folder.
        let gone = folder.path().join("gone");
        assert_eq!(changed_directory(&gone), Some(gone));
    }

    #[test]
    fn the_system_tells_of_a_file_written_under_a_watched_folder() {
        let folder = tempfile::tempdir().unwrap();
        let album = folder.path().join("album");
        std::fs::create_dir(&album).unwrap();
        let (sender, changes) = mpsc::channel();
        let mut watcher = Watcher::new(move |change| {
            let _ = sender.send(change);
        });
        assert!(watcher.watch(folder.path()));
        assert!(!watcher.watch(&folder.path().join("missing")));

        std::fs::write(album.join("1.flac"), [1]).unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let change = changes
                .recv_timeout(left)
                .expect("the write is told of in time");
            if let Change::Directories(directories) = change
                && directories
                    .iter()
                    .any(|directory| directory.ends_with("album"))
            {
                break;
            }
        }
        watcher.unwatch(folder.path());
    }
}

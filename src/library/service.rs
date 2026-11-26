//! The library's thread: reconciles the database with the folders at
//! launch, then follows them: a change on disk is scanned once it has
//! gone quiet, and only where it happened.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use super::cache::{Cache, CacheResult};
use super::metadata::{self, MetadataJob};
use super::scanner::{self, is_within};
use super::watcher::{Change, Dirty, Pending, Watcher};
use super::{Folder, LibraryCommand, LibraryEvent, TrackUpdate};

const METADATA_BATCH_SIZE: usize = 64;
const MAX_METADATA_WORKERS: usize = 4;
/// How often an idle library looks whether its folders are still there:
/// a drive that is unplugged or back tells nobody.
const FOLDER_CHECK_INTERVAL: Duration = Duration::from_secs(10);

enum Input {
    Command(LibraryCommand),
    Disk(Change),
    Stop,
}

/// The handle of the library's thread, which ends when this is dropped.
pub(crate) struct Library {
    inputs: Sender<Input>,
    #[cfg(test)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Library {
    /// Starts the user's library, which reports through `emit` until it
    /// returns false (nobody listens any more).
    pub(crate) fn spawn(emit: impl FnMut(LibraryEvent) -> bool + Send + 'static) -> Self {
        Self::spawn_with(Cache::open, emit)
    }

    fn spawn_with(
        open: impl FnOnce() -> CacheResult<Cache> + Send + 'static,
        emit: impl FnMut(LibraryEvent) -> bool + Send + 'static,
    ) -> Self {
        let (inputs, receiver) = mpsc::channel();
        let disk = inputs.clone();
        let thread = std::thread::Builder::new()
            .name("library".to_owned())
            .spawn(move || run(open, &receiver, disk, emit))
            .inspect_err(|error| eprintln!("Library thread failed to start: {error}"))
            .ok();
        #[cfg(not(test))]
        let _ = thread;
        Self {
            inputs,
            #[cfg(test)]
            thread,
        }
    }

    pub(crate) fn send(&self, command: LibraryCommand) {
        // The library only goes away with the app.
        let _ = self.inputs.send(Input::Command(command));
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // The watcher holds a sender too: the channel alone never closes.
        let _ = self.inputs.send(Input::Stop);
    }
}

fn run(
    open: impl FnOnce() -> CacheResult<Cache>,
    inputs: &Receiver<Input>,
    disk: Sender<Input>,
    mut emit: impl FnMut(LibraryEvent) -> bool,
) {
    let cache = match open() {
        Ok(cache) => cache,
        Err(error) => {
            emit(LibraryEvent::Failed(error.to_string()));
            return;
        }
    };
    let watcher = Watcher::new(move |change| {
        let _ = disk.send(Input::Disk(change));
    });
    let mut service = Service {
        cache,
        folders: Vec::new(),
        watcher,
        pending: Pending::default(),
        jobs: VecDeque::new(),
        unsettled: true,
        emit,
        listening: true,
    };

    service.emit(LibraryEvent::Loaded(service.cache.tracks()));
    service.refresh_playlists();
    service.refresh_folders();
    service.scan_everything();
    service.settle();

    while service.listening {
        // With tags left to read nothing is waited for.
        let wait = if !service.jobs.is_empty() {
            Duration::ZERO
        } else if let Some(due) = service.pending.due() {
            due.saturating_duration_since(Instant::now())
        } else {
            FOLDER_CHECK_INTERVAL
        };
        let mut input = match inputs.recv_timeout(wait) {
            Ok(input) => Some(input),
            Err(RecvTimeoutError::Timeout) => {
                if service.jobs.is_empty() && service.pending.due().is_none() {
                    service.check_folders();
                }
                None
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Everything asked and told so far, before any work.
        while let Some(taken) = input {
            match taken {
                Input::Command(command) => service.handle(command),
                Input::Disk(change) => service.pending.note(change, Instant::now()),
                Input::Stop => return,
            }
            input = inputs.try_recv().ok();
        }

        if service
            .pending
            .due()
            .is_some_and(|due| due <= Instant::now())
            && let Some(dirty) = service.pending.take()
        {
            service.rescan(dirty);
        }
        service.read_batch();
        service.settle();
    }
}

struct Service<E> {
    cache: Cache,
    folders: Vec<Folder>,
    watcher: Watcher,
    /// Changes on disk that wait to be scanned.
    pending: Pending,
    /// Tracks whose tags are to be read, a batch at a time between
    /// whatever else is asked.
    jobs: VecDeque<i64>,
    /// Something changed since the library was last said to be complete.
    unsettled: bool,
    emit: E,
    /// Somebody still takes the events.
    listening: bool,
}

impl<E: FnMut(LibraryEvent) -> bool> Service<E> {
    fn emit(&mut self, event: LibraryEvent) {
        if !(self.emit)(event) {
            self.listening = false;
        }
    }

    fn fail(&mut self, error: impl ToString) {
        self.emit(LibraryEvent::Failed(error.to_string()));
    }

    fn handle(&mut self, command: LibraryCommand) {
        match command {
            LibraryCommand::AddFolder(path) => match self.cache.add_folder(&path) {
                Ok(Some(id)) => {
                    self.refresh_folders();
                    self.scan(id, &path, std::slice::from_ref(&path));
                }
                // A folder of the library holds it already.
                Ok(None) => {}
                Err(error) => self.fail(error),
            },
            LibraryCommand::RemoveFolder(id) => match self.cache.remove_folder(id) {
                Ok(removed) => {
                    self.refresh_folders();
                    self.changed(removed);
                }
                Err(error) => self.fail(error),
            },
            LibraryCommand::CreatePlaylist { name, tracks } => {
                let done = self.cache.create_playlist(&name, &tracks).map(drop);
                self.playlists_changed(done);
            }
            LibraryCommand::RenamePlaylist { playlist, name } => {
                let done = self.cache.rename_playlist(playlist, &name);
                self.playlists_changed(done);
            }
            LibraryCommand::DeletePlaylist(playlist) => {
                let done = self.cache.delete_playlist(playlist);
                self.playlists_changed(done);
            }
            LibraryCommand::AddToPlaylist { playlist, tracks } => {
                let done = self.cache.add_to_playlist(playlist, &tracks);
                self.playlists_changed(done);
            }
            LibraryCommand::RemoveFromPlaylist { playlist, track } => {
                let done = self.cache.remove_from_playlist(playlist, track);
                self.playlists_changed(done);
            }
            LibraryCommand::RecordPlay(track) => {
                if let Err(error) = self.cache.record_play(track) {
                    self.fail(error);
                }
            }
        }
    }

    /// Tells the app the playlists as they are after a change to them,
    /// and what went wrong with it.
    fn playlists_changed(&mut self, done: CacheResult<()>) {
        if let Err(error) = done {
            self.fail(error);
        }
        self.refresh_playlists();
    }

    fn refresh_playlists(&mut self) {
        match self.cache.playlists() {
            Ok(playlists) => self.emit(LibraryEvent::Playlists(playlists)),
            Err(error) => self.fail(error),
        }
    }

    /// Reads the folders from the database, watches those that are there
    /// and tells the app when anything about them differs.
    fn refresh_folders(&mut self) {
        let folders: Vec<Folder> = match self.cache.folders() {
            Ok(folders) => folders
                .into_iter()
                .map(|(id, path)| Folder {
                    id,
                    available: path.is_dir(),
                    path,
                })
                .collect(),
            Err(error) => return self.fail(error),
        };
        for old in &self.folders {
            if !folders
                .iter()
                .any(|folder| folder.available && folder.path == old.path)
            {
                self.watcher.unwatch(&old.path);
            }
        }
        for folder in folders.iter().filter(|folder| folder.available) {
            self.watcher.watch(&folder.path);
        }
        if folders != self.folders {
            self.folders = folders.clone();
            self.unsettled = true;
            self.emit(LibraryEvent::Folders(folders));
        }
    }

    /// Scans the folders that came or went since they were last looked at.
    fn check_folders(&mut self) {
        let flipped: Vec<Folder> = self
            .folders
            .iter()
            .filter(|folder| folder.path.is_dir() != folder.available)
            .cloned()
            .collect();
        if flipped.is_empty() {
            return;
        }
        for folder in flipped {
            self.scan(folder.id, &folder.path, std::slice::from_ref(&folder.path));
        }
    }

    /// Reconciles every folder with the disk, and what is under none of
    /// them any more with that.
    fn scan_everything(&mut self) {
        for folder in self.folders.clone() {
            self.scan(folder.id, &folder.path, std::slice::from_ref(&folder.path));
        }
        let roots: Vec<PathBuf> = self
            .folders
            .iter()
            .map(|folder| folder.path.clone())
            .collect();
        match self.cache.mark_absent_outside(&roots) {
            Ok(gone) => self.changed(gone),
            Err(error) => self.fail(error),
        }
    }

    /// Scans what changed, each folder in the library folder it is in.
    fn rescan(&mut self, dirty: Dirty) {
        let directories = match dirty {
            Dirty::Everything => {
                self.refresh_folders();
                return self.scan_everything();
            }
            Dirty::Directories(directories) => directories,
        };
        for folder in self.folders.clone() {
            // A change above the folder may have been to the folder.
            if directories
                .iter()
                .any(|directory| is_within(&folder.path, directory))
            {
                self.scan(folder.id, &folder.path, std::slice::from_ref(&folder.path));
                continue;
            }
            let inside: Vec<PathBuf> = directories
                .iter()
                .filter(|directory| is_within(directory, &folder.path))
                .cloned()
                .collect();
            if !inside.is_empty() {
                self.scan(folder.id, &folder.path, &inside);
            }
        }
    }

    /// Reconciles `directories` of the folder at `root` with the disk. A
    /// folder that is not there holds nothing: its tracks go absent, and
    /// are the tracks they were once it is back.
    fn scan(&mut self, folder_id: i64, root: &std::path::Path, directories: &[PathBuf]) {
        let available = root.is_dir();
        if self
            .folders
            .iter()
            .any(|folder| folder.id == folder_id && folder.available != available)
        {
            // Lost or back since it was last looked at.
            self.refresh_folders();
        }
        let whole = [root.to_path_buf()];
        let (directories, discovered) = if available {
            let discovered = directories
                .iter()
                .flat_map(|directory| scanner::scan(root, directory))
                .collect();
            (directories, discovered)
        } else {
            (&whole[..], Vec::new())
        };
        match self.cache.reconcile(folder_id, directories, discovered) {
            Ok(reconciled) => {
                self.jobs.extend(reconciled.jobs);
                self.changed(reconciled.changed);
            }
            Err(error) => self.fail(error),
        }
    }

    fn changed(&mut self, tracks: Vec<super::Track>) {
        if !tracks.is_empty() {
            self.unsettled = true;
            self.emit(LibraryEvent::Changed(tracks));
        }
    }

    /// Reads the tags of the next tracks that are due.
    fn read_batch(&mut self) {
        let mut ids: Vec<i64> = Vec::with_capacity(METADATA_BATCH_SIZE);
        while ids.len() < METADATA_BATCH_SIZE
            && let Some(id) = self.jobs.pop_front()
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        // A track that went absent or was read meanwhile is no longer due.
        let jobs: Vec<MetadataJob> = ids
            .into_iter()
            .filter_map(|id| self.cache.pending_job(id))
            .collect();
        if jobs.is_empty() {
            return;
        }
        let updates = extract_batch(&jobs);
        match self.cache.store_updates(&updates) {
            Ok(()) => {
                self.unsettled = true;
                self.emit(LibraryEvent::MetadataBatch(updates));
            }
            Err(error) => self.fail(error),
        }
    }

    /// Tells the app once nothing is left to do.
    fn settle(&mut self) {
        if self.unsettled && self.jobs.is_empty() {
            self.unsettled = false;
            self.emit(LibraryEvent::Complete);
        }
    }
}

fn extract_batch(jobs: &[MetadataJob]) -> Vec<TrackUpdate> {
    if jobs.len() <= 1 {
        return jobs.iter().map(metadata::extract).collect();
    }

    let worker_count = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_METADATA_WORKERS)
        .min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let updates = Mutex::new(Vec::with_capacity(jobs.len()));

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    let update = metadata::extract(job);
                    updates
                        .lock()
                        .expect("metadata update lock poisoned")
                        .push(update);
                }
            });
        }
    });

    let mut updates = updates.into_inner().expect("metadata update lock poisoned");
    updates.sort_unstable_by_key(|update| update.id);
    updates
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::library::Track;
    use crate::library::metadata::tests::write_wav;
    use crate::library::track::MetadataState;

    const PATIENCE: Duration = Duration::from_secs(20);

    /// A library on a thread, and what it has told so far.
    struct Following {
        library: Option<Library>,
        events: Receiver<LibraryEvent>,
        tracks: HashMap<i64, Track>,
        folders: Vec<Folder>,
        playlists: Vec<super::super::Playlist>,
        /// How many times it told of changed tracks.
        changes: usize,
    }

    impl Following {
        /// Starts a library whose database is in `home`, with `music` as
        /// its first folder on its first run.
        fn start(home: &Path, music: &Path) -> Self {
            let (database, music) = (home.join("library.sqlite3"), music.to_path_buf());
            let (sender, events) = mpsc::channel();
            let library = Library::spawn_with(
                move || {
                    let mut cache = Cache::open_at(&database)?;
                    cache.seed(Some(music))?;
                    Ok(cache)
                },
                move |event| sender.send(event).is_ok(),
            );
            Self {
                library: Some(library),
                events,
                tracks: HashMap::new(),
                folders: Vec::new(),
                playlists: Vec::new(),
                changes: 0,
            }
        }

        fn send(&self, command: LibraryCommand) {
            self.library.as_ref().unwrap().send(command);
        }

        /// Takes events until the library is complete and `done` holds.
        fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + PATIENCE;
            let mut complete = false;
            while !(complete && done(self)) {
                let left = deadline.saturating_duration_since(Instant::now());
                let event = self
                    .events
                    .recv_timeout(left)
                    .unwrap_or_else(|_| panic!("{what}: not in time"));
                complete = matches!(event, LibraryEvent::Complete);
                match event {
                    LibraryEvent::Loaded(tracks) => {
                        self.tracks = tracks.into_iter().map(|track| (track.id, track)).collect();
                    }
                    LibraryEvent::Changed(tracks) => {
                        self.changes += 1;
                        for track in tracks {
                            self.tracks.insert(track.id, track);
                        }
                    }
                    LibraryEvent::MetadataBatch(updates) => {
                        for update in updates {
                            if let Some(track) = self.tracks.get_mut(&update.id) {
                                track.apply(update);
                            }
                        }
                    }
                    LibraryEvent::Folders(folders) => self.folders = folders,
                    LibraryEvent::Playlists(playlists) => self.playlists = playlists,
                    LibraryEvent::Complete => {}
                    LibraryEvent::Failed(error) => panic!("{what}: {error}"),
                }
            }
        }

        /// Takes events until the playlists it told last are `expected`:
        /// a change to them is told without the library going unsettled.
        fn until_playlists(&mut self, what: &str, expected: &[(&str, Vec<i64>)]) {
            let deadline = Instant::now() + PATIENCE;
            let told = |library: &Self| {
                library
                    .playlists
                    .iter()
                    .map(|playlist| (playlist.name.as_str(), playlist.tracks.clone()))
                    .eq(expected.iter().cloned())
            };
            while !told(self) {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.events.recv_timeout(left) {
                    Ok(LibraryEvent::Playlists(playlists)) => self.playlists = playlists,
                    Ok(LibraryEvent::Failed(error)) => panic!("{what}: {error}"),
                    Ok(_) => {}
                    Err(_) => panic!("{what}: not in time"),
                }
            }
        }

        fn present(&self) -> Vec<&Track> {
            let mut tracks: Vec<&Track> =
                self.tracks.values().filter(|track| track.present).collect();
            tracks.sort_unstable_by_key(|track| track.id);
            tracks
        }

        fn named(&self, name: &str) -> Option<&Track> {
            self.tracks
                .values()
                .find(|track| track.path.file_name().is_some_and(|file| file == name))
        }

        /// Ends the library's thread, and with it its hold on the database.
        fn stop(&mut self) {
            if let Some(mut library) = self.library.take() {
                let thread = library.thread.take();
                drop(library);
                if let Some(thread) = thread {
                    thread.join().unwrap();
                }
            }
        }
    }

    /// A home for the database and, beside it, a music folder of two
    /// albums of two tracks.
    fn music() -> (TempDir, PathBuf) {
        let home = tempdir().unwrap();
        let music = home.path().join("music");
        for album in ["first", "second"] {
            fs::create_dir_all(music.join(album)).unwrap();
            for track in ["1.wav", "2.wav"] {
                write_wav(&music.join(album).join(track), 44_100, 4_410);
            }
        }
        (home, music)
    }

    #[test]
    fn playlists_are_told_at_once_and_after_every_change_to_them() {
        let (home, music) = music();
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);
        let ids: Vec<i64> = library.present().iter().map(|track| track.id).collect();

        library.send(LibraryCommand::CreatePlaylist {
            name: "Mix".into(),
            tracks: vec![ids[2]],
        });
        library.until_playlists("made", &[("Mix", vec![ids[2]])]);
        let id = library.playlists[0].id;
        library.send(LibraryCommand::AddToPlaylist {
            playlist: id,
            tracks: vec![ids[0], ids[2]],
        });
        library.until_playlists("added to", &[("Mix", vec![ids[2], ids[0]])]);
        library.send(LibraryCommand::RemoveFromPlaylist {
            playlist: id,
            track: ids[2],
        });
        library.send(LibraryCommand::RenamePlaylist {
            playlist: id,
            name: "Kept".into(),
        });
        library.until_playlists("renamed", &[("Kept", vec![ids[0]])]);

        // Told at once on the next run.
        library.stop();
        let mut library = Following::start(home.path(), &music);
        library.until_playlists("the next run", &[("Kept", vec![ids[0]])]);
        library.send(LibraryCommand::DeletePlaylist(id));
        library.until_playlists("deleted", &[]);
    }

    #[test]
    fn the_library_follows_files_added_renamed_changed_and_deleted_while_it_runs() {
        let (home, music) = music();
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);
        assert!(
            library
                .present()
                .iter()
                .all(|track| track.metadata_state == MetadataState::Ready
                    && track.sample_rate == Some(44_100)
                    && track.duration_ms == Some(100))
        );
        assert_eq!(
            library.folders,
            [Folder {
                id: library.folders[0].id,
                path: music.clone(),
                available: true
            }]
        );

        // Added.
        write_wav(&music.join("first").join("3.wav"), 48_000, 4_800);
        library.until("the added file", |library| {
            library
                .named("3.wav")
                .is_some_and(|track| track.sample_rate == Some(48_000))
        });
        assert_eq!(library.present().len(), 5);

        // Renamed, and moved to another folder: the same track.
        let id = library.named("3.wav").unwrap().id;
        fs::rename(
            music.join("first").join("3.wav"),
            music.join("second").join("three.wav"),
        )
        .unwrap();
        library.until("the renamed file", |library| {
            library.named("three.wav").is_some()
        });
        let renamed = library.named("three.wav").unwrap();
        assert_eq!(renamed.id, id);
        assert!(renamed.present);
        assert_eq!(renamed.path, music.join("second").join("three.wav"));
        assert_eq!(library.present().len(), 5);

        // Changed: read again.
        write_wav(&music.join("second").join("three.wav"), 96_000, 19_200);
        library.until("the changed file", |library| {
            library.tracks[&id].sample_rate == Some(96_000)
        });
        assert_eq!(library.tracks[&id].duration_ms, Some(200));

        // Deleted: absent, and kept.
        fs::remove_file(music.join("second").join("three.wav")).unwrap();
        library.until("the deleted file", |library| !library.tracks[&id].present);
        assert_eq!(library.present().len(), 4);

        // A whole folder renamed: its tracks keep their ids.
        let before: Vec<i64> = library
            .present()
            .iter()
            .filter(|track| track.path.starts_with(music.join("first")))
            .map(|track| track.id)
            .collect();
        fs::rename(music.join("first"), music.join("renamed")).unwrap();
        library.until("the renamed folder", |library| {
            library
                .present()
                .iter()
                .filter(|track| track.path.starts_with(music.join("renamed")))
                .count()
                == 2
        });
        let after: Vec<i64> = library
            .present()
            .iter()
            .filter(|track| track.path.starts_with(music.join("renamed")))
            .map(|track| track.id)
            .collect();
        assert_eq!(after, before);
        assert_eq!(library.present().len(), 4);
        library.stop();
    }

    #[test]
    fn what_changed_while_the_app_was_closed_is_reconciled_at_launch() {
        let (home, music) = music();
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);
        let moved = library
            .present()
            .iter()
            .find(|track| track.path == music.join("first").join("1.wav"))
            .map(|track| track.id)
            .unwrap();
        let deleted = library
            .present()
            .iter()
            .find(|track| track.path == music.join("first").join("2.wav"))
            .map(|track| track.id)
            .unwrap();
        library.stop();

        fs::rename(
            music.join("first").join("1.wav"),
            music.join("second").join("moved.wav"),
        )
        .unwrap();
        fs::remove_file(music.join("first").join("2.wav")).unwrap();
        write_wav(&music.join("new.wav"), 44_100, 441);

        let mut library = Following::start(home.path(), &music);
        // What the last run knew is there at once...
        let first = library.events.recv_timeout(PATIENCE).unwrap();
        let LibraryEvent::Loaded(cached) = first else {
            panic!("the cached library comes first: {first:?}");
        };
        assert_eq!(cached.iter().filter(|track| track.present).count(), 4);
        library.tracks = cached.into_iter().map(|track| (track.id, track)).collect();
        // ...and then what happened since.
        library.until("the launch scan", |library| {
            library
                .named("new.wav")
                .is_some_and(|track| track.metadata_state == MetadataState::Ready)
        });

        assert_eq!(
            library.tracks[&moved].path,
            music.join("second").join("moved.wav")
        );
        assert!(library.tracks[&moved].present);
        assert!(!library.tracks[&deleted].present);
        assert_eq!(library.present().len(), 4);
        assert_eq!(library.tracks.len(), 5);
        library.stop();
    }

    #[test]
    fn folders_are_added_and_removed_while_it_runs() {
        let (home, music) = music();
        let other = home.path().join("other");
        fs::create_dir(&other).unwrap();
        write_wav(&other.join("elsewhere.wav"), 44_100, 441);
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);

        library.send(LibraryCommand::AddFolder(other.clone()));
        library.until("the added folder", |library| {
            library
                .named("elsewhere.wav")
                .is_some_and(|track| track.metadata_state == MetadataState::Ready)
        });
        assert_eq!(library.folders.len(), 2);
        let id = library.named("elsewhere.wav").unwrap().id;

        // The new folder is followed as the first is.
        write_wav(&other.join("later.wav"), 44_100, 441);
        library.until("a file in the added folder", |library| {
            library.named("later.wav").is_some()
        });

        let folder = library
            .folders
            .iter()
            .find(|folder| folder.path == other)
            .map(|folder| folder.id)
            .unwrap();
        library.send(LibraryCommand::RemoveFolder(folder));
        library.until("the removed folder", |library| library.present().len() == 4);
        assert_eq!(library.folders.len(), 1);
        assert!(!library.tracks[&id].present);

        // Added again, its tracks are the tracks they were.
        library.send(LibraryCommand::AddFolder(other.clone()));
        library.until("the folder added again", |library| {
            library.present().len() == 6
        });
        assert_eq!(library.named("elsewhere.wav").unwrap().id, id);
        library.stop();

        // Both folders are the library's at the next launch.
        let mut library = Following::start(home.path(), &music);
        library.until("the next launch", |library| library.folders.len() == 2);
        assert_eq!(library.present().len(), 6);
        library.stop();
    }

    #[test]
    fn a_folder_that_goes_away_takes_its_tracks_absent_until_it_is_back() {
        let (home, music) = music();
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);
        let ids: Vec<i64> = library.present().iter().map(|track| track.id).collect();

        // As an unplugged drive: the whole folder is not there.
        let parked = home.path().join("unplugged");
        fs::rename(&music, &parked).unwrap();
        library.until("the lost folder", |library| {
            library.present().is_empty() && !library.folders[0].available
        });
        assert_eq!(library.tracks.len(), 4);

        fs::rename(&parked, &music).unwrap();
        library.until("the folder back", |library| {
            library.present().len() == 4 && library.folders[0].available
        });
        let back: Vec<i64> = library.present().iter().map(|track| track.id).collect();
        assert_eq!(back, ids);
        library.stop();
    }

    #[test]
    fn a_burst_of_new_files_is_taken_in_a_few_scans_not_one_each() {
        let (home, music) = music();
        let mut library = Following::start(home.path(), &music);
        library.until("the first scan", |library| library.present().len() == 4);
        let changes_before = library.changes;

        for album in 0..20 {
            let folder = music.join(format!("burst {album}"));
            fs::create_dir(&folder).unwrap();
            for track in 0..25 {
                write_wav(&folder.join(format!("{track}.wav")), 44_100, 44);
            }
        }
        library.until("the burst", |library| {
            library.present().len() == 504
                && library
                    .present()
                    .iter()
                    .all(|track| track.metadata_state == MetadataState::Ready)
        });

        let scans = library.changes - changes_before;
        assert!(scans <= 10, "500 files took {scans} scans");
        library.stop();
    }
}

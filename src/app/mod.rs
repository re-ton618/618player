mod input;
mod listening;
mod listing;
mod playback;
mod playlists;
mod queue;
mod session;
mod settings_tab;
mod store;
mod tab_store;
mod tabs;
mod view;

use std::collections::HashMap;
use std::path::PathBuf;

use nsh_core::prelude::SharedString;

use crate::audio;
use crate::library::{
    Catalog, Folder, LibraryCommand, LibraryEvent, SortColumn, SortDirection, Track,
};

use listing::Listing;
pub(crate) use store::Store;
pub(crate) use view::root;

/// Tracks in a row the player may fail on before the queue gives up.
const MAX_PLAYBACK_FAILURES: u8 = 5;
/// How far into a track "previous" starts it again instead of leaving it.
const RESTART_AFTER_MS: u64 = 3_000;
/// How far the position moves before it is saved again while a track
/// plays: what a crash can lose of it. A pause, a change of track and
/// quitting save where it is at once.
const POSITION_SAVE_STEP_MS: u64 = 15_000;

/// The player's state, free of any UI: `update` changes it, and the store
/// carries out the [`Effect`]s it returns.
#[derive(Clone)]
pub(crate) struct Model {
    /// Every track the library knows, the absent ones too: what names
    /// them (the queue) can still tell what they were.
    tracks: Vec<Track>,
    track_positions: HashMap<i64, usize>,
    /// Where in `tracks` each listing's rows are: the present tracks,
    /// searched, in its order.
    listed: [Vec<usize>; Listing::COUNT],
    /// The folders the library is made of.
    folders: Vec<Folder>,
    /// The albums and album artists of the present tracks.
    catalog: Catalog,
    search_query: String,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    tabs: tabs::State,
    /// The category the Settings tab shows.
    settings_category: settings_tab::Category,
    /// The theme the app is drawn in.
    theme: crate::theme::Id,
    playback: playback::State,
    /// The tracks that play one after another, and which of them is
    /// current.
    queue: queue::State,
    /// Tracks in a row the player failed on.
    playback_failures: u8,
    /// Where in the current track the last run was, until the library has
    /// told what it knows and the track can be loaded there.
    resume_at: Option<u64>,
    /// What of the session was last given out to be saved.
    saved: Saved,
    /// The user's playlists, and what the Playlists tab shows of them.
    playlists: playlists::State,
}

/// What tells that the session differs from the one that was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Saved {
    queue_revision: u64,
    volume: u8,
    position_ms: u64,
    playing: bool,
}

impl Default for Model {
    fn default() -> Self {
        let mut model = Self {
            tracks: Vec::new(),
            track_positions: HashMap::new(),
            listed: Default::default(),
            folders: Vec::new(),
            catalog: Catalog::default(),
            search_query: String::new(),
            sort_column: SortColumn::Title,
            sort_direction: SortDirection::Ascending,
            tabs: tabs::State::default(),
            settings_category: settings_tab::Category::default(),
            theme: crate::theme::Id::default(),
            playback: playback::State::default(),
            queue: queue::State::default(),
            playback_failures: 0,
            resume_at: None,
            saved: Saved::default(),
            playlists: playlists::State::default(),
        };
        model.saved = model.unsaved();
        model
    }
}

#[derive(Debug, Clone)]
enum Message {
    LibraryEvent(LibraryEvent),
    SearchChanged(String),
    SortChanged(SortColumn),
    /// A row of a listing was pressed: its track plays, with the list as
    /// it is shown as the queue.
    TrackPressed(Listing, i64),
    /// The track is to play right after the current one.
    PlayNextPressed(i64),
    /// The track is to play after everything queued.
    AddToQueuePressed(i64),
    /// A row of the queue was pressed, the entry of this key: it plays.
    QueueEntryPressed(u64),
    QueueRemovePressed(u64),
    /// The entry of `key` is to go to place `to` of the queue.
    QueueMovePressed {
        key: u64,
        to: usize,
    },
    QueueClearPressed,
    ShufflePressed,
    /// Shuffle was chosen on or off (the Settings tab).
    ShuffleChosen(bool),
    /// Repeat goes on to its next mode: off, all, one.
    RepeatPressed,
    /// A repeat mode was chosen (the Settings tab).
    RepeatChosen(queue::Repeat),
    ArtworkLoaded {
        request_id: u64,
        result: Result<Option<SharedString>, String>,
    },
    PlayPausePressed,
    PreviousPressed,
    NextPressed,
    /// The user moved the position, to this many milliseconds.
    PositionChanged(u64),
    Player(audio::Event),
    /// The user asked to add a folder to the library.
    AddFolderPressed,
    /// The folder the user chose to add.
    FolderPicked(PathBuf),
    /// The user took the folder of this id out of the library.
    RemoveFolderPressed(i64),
    VolumeChanged(u8),
    Tabs(tabs::Message),
    /// The Settings tab is to show this category.
    SettingsCategoryPressed(settings_tab::Category),
    /// A theme was chosen (the Settings tab).
    ThemeChosen(crate::theme::Id),
    /// The share of a track, in percent, to hear before it counts as
    /// listened to (the Settings tab).
    ListenPercentChosen(u8),
    /// The Playlists tab is to show the playlist of this id.
    PlaylistChosen(i64),
    /// The naming prompt is to ask for a new playlist's name; the
    /// playlist will hold these tracks.
    NewPlaylistPressed(Vec<i64>),
    /// The naming prompt is to ask for a new name for this playlist.
    RenamePlaylistPressed(i64),
    /// The user said to delete the playlist of this id, and was sure.
    DeletePlaylistConfirmed(i64),
    AddToPlaylistPressed {
        playlist: i64,
        track: i64,
    },
    /// The track is to leave the playlist the Playlists tab shows.
    RemoveFromPlaylistPressed(i64),
    PromptNameChanged(String),
    PromptConfirmed,
    PromptCancelled,
    PlaylistArtworkLoaded {
        request_id: u64,
        result: Result<Option<SharedString>, String>,
    },
    /// The window is about to close.
    Closing,
}

/// What an update asks of the world outside the model.
#[derive(Debug, PartialEq, Eq)]
enum Effect {
    LoadArtwork(playback::ArtworkRequest),
    /// Reads the artwork the Playlists tab shows.
    LoadPlaylistArtwork(playlists::ArtworkRequest),
    ScrollToTop(Listing),
    Player(audio::Command),
    /// Shows the system's folder dialog; the choice comes back as
    /// [`Message::FolderPicked`].
    PickFolder,
    Library(LibraryCommand),
    /// Saves the session for the next launch, soon and off the UI thread.
    SaveSession(session::Session),
    /// Saves the session before the app goes: waited for.
    FlushSession(session::Session),
    /// Draws the app in this theme, and keeps it for the next launch.
    ApplyTheme(crate::theme::Id),
    /// Keeps the listening threshold, in percent, for the next launch.
    SaveListenPercent(u8),
}

/// Takes `message`, and with it keeps two promises whatever it was: the
/// player has queued behind the loaded track what the queue says comes
/// next (or nothing), which is what makes every transition gapless; and a
/// session that differs from the saved one is given out to be saved.
fn update(model: &mut Model, message: Message) -> Vec<Effect> {
    let closing = matches!(message, Message::Closing);
    let mut effects = model.handle(message);
    effects.extend(model.record_plays());
    effects.extend(model.sync_queued());
    effects.extend(model.sync_playlist_artwork());
    effects.extend(model.save_session(closing));
    effects
}

impl Model {
    fn handle(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::LibraryEvent(event) => match event {
                LibraryEvent::Loaded(tracks) => return self.set_tracks(tracks),
                LibraryEvent::Changed(tracks) => return self.merge_tracks(tracks),
                LibraryEvent::Folders(folders) => self.folders = folders,
                LibraryEvent::MetadataBatch(updates) => {
                    let current = self.queue.current().map(|entry| entry.track_id);
                    let mut current_track_updated = false;

                    for update in updates {
                        let update_id = update.id;
                        if let Some(&position) = self.track_positions.get(&update_id) {
                            self.tracks[position].apply(update);
                            current_track_updated |= Some(update_id) == current;
                        }
                    }

                    if current_track_updated {
                        return self.reconcile_current_track();
                    }
                }
                LibraryEvent::Complete => {
                    self.rebuild_projection();
                    self.catalog = Catalog::of(&self.tracks);
                }
                LibraryEvent::Playlists(list) => {
                    let shown = self.playlists.chosen().map(|playlist| playlist.id);
                    self.playlists.set(list);
                    self.rebuild_playlist_rows();
                    if self.playlists.chosen().map(|playlist| playlist.id) != shown {
                        return vec![Effect::ScrollToTop(Listing::Playlist)];
                    }
                }
                LibraryEvent::Failed(error) => eprintln!("Library failed: {error}"),
            },
            Message::SearchChanged(query) => {
                if self.search_query == query {
                    return Vec::new();
                }
                self.search_query = query;
                self.rebuild_projection();
                return Listing::ALL.map(Effect::ScrollToTop).into();
            }
            Message::SortChanged(column) => {
                if self.sort_column == column {
                    self.sort_direction = self.sort_direction.toggled();
                } else {
                    self.sort_column = column;
                    self.sort_direction = SortDirection::Ascending;
                }
                self.rebuild_projection();
                return vec![Effect::ScrollToTop(Listing::Library)];
            }
            Message::TrackPressed(listing, track_id) => {
                let listed = self.listed_tracks(listing);
                return self.play_listed(listed, track_id);
            }
            Message::PlayNextPressed(track_id) => {
                if self.track_positions.contains_key(&track_id) {
                    self.queue.play_next(track_id);
                }
            }
            Message::AddToQueuePressed(track_id) => {
                if self.track_positions.contains_key(&track_id) {
                    self.queue.add(track_id);
                }
            }
            Message::QueueEntryPressed(key) => {
                let entry = self
                    .queue
                    .index_of(key)
                    .filter(|&index| self.can_play(self.queue.entries()[index].track_id));
                if let Some(index) = entry {
                    self.playback_failures = 0;
                    self.queue.go_to(index);
                    return self.play_current();
                }
            }
            Message::QueueRemovePressed(key) => {
                if self.queue.remove(key) == queue::Removed::Current {
                    return self.leave_removed_track();
                }
            }
            Message::QueueMovePressed { key, to } => self.queue.move_to(key, to),
            Message::QueueClearPressed => {
                // What is loaded stays, as the whole of the queue.
                let loaded = self.playback.transport() != playback::Transport::Stopped;
                self.queue.clear(loaded);
                if !loaded {
                    return vec![Effect::Player(self.playback.clear())];
                }
            }
            Message::ShufflePressed => self.queue.set_shuffle(!self.queue.shuffled()),
            Message::ShuffleChosen(on) => {
                if on != self.queue.shuffled() {
                    self.queue.set_shuffle(on);
                }
            }
            Message::RepeatPressed => self.queue.set_repeat(self.queue.repeat().next()),
            Message::RepeatChosen(mode) => self.queue.set_repeat(mode),
            Message::ArtworkLoaded { request_id, result } => {
                return match self.playback.loaded(request_id, result) {
                    playback::ArtworkEffect::None => Vec::new(),
                    playback::ArtworkEffect::Load { request, error } => {
                        if let Some(error) = error {
                            eprintln!("Album artwork failed: {error}");
                        }
                        vec![Effect::LoadArtwork(request)]
                    }
                    playback::ArtworkEffect::Missing { error } => {
                        if let Some(error) = error {
                            eprintln!("Album artwork failed: {error}");
                        }
                        Vec::new()
                    }
                };
            }
            Message::PlayPausePressed => {
                if let Some(command) = self.playback.toggle() {
                    return vec![Effect::Player(command)];
                }
                // Stopped: the current track again, else the queue from
                // where it is, else the list from its top.
                self.playback_failures = 0;
                if self.queue.is_empty() {
                    self.queue.replace(self.listed_tracks(Listing::Library), 0);
                }
                return self.play_from_current();
            }
            Message::PreviousPressed => {
                let previous = self.queue.preceding(|id| self.can_play(id));
                let stopped = self.playback.transport() == playback::Transport::Stopped;
                let restart = self.playback.position_ms() > RESTART_AFTER_MS || previous.is_none();
                if !stopped && restart {
                    return self
                        .playback
                        .seek(0)
                        .map(Effect::Player)
                        .into_iter()
                        .collect();
                }
                if let Some(index) = previous {
                    self.queue.go_to(index);
                    return self.play_current();
                }
            }
            Message::NextPressed => {
                if let Some(index) = self.queue.following(|id| self.can_play(id)) {
                    self.queue.go_to(index);
                    return self.play_current();
                }
            }
            Message::PositionChanged(position_ms) => {
                return self
                    .playback
                    .seek(position_ms)
                    .map(Effect::Player)
                    .into_iter()
                    .collect();
            }
            Message::Player(event) => match self.playback.report(event) {
                playback::Report::Handled => {}
                playback::Report::Advanced(key) => {
                    self.playback_failures = 0;
                    if let Some(index) = self.queue.index_of(key) {
                        self.queue.go_to(index);
                    }
                    return self.show_current();
                }
                // Either way the current track is over, and the player
                // does not have behind it what comes next.
                playback::Report::Finished | playback::Report::Strayed => {
                    self.playback_failures = 0;
                    return match self.queue.upcoming(|id| self.can_play(id)) {
                        Some(index) => {
                            self.queue.go_to(index);
                            self.play_current()
                        }
                        None => vec![Effect::Player(self.playback.stop())],
                    };
                }
                playback::Report::Failed(error) => {
                    eprintln!("Playback failed: {error}");
                    // A track that was only loaded (as at launch) starts
                    // nothing else by failing.
                    if self.playback.transport() == playback::Transport::Paused {
                        return vec![Effect::Player(self.playback.stop())];
                    }
                    self.playback_failures += 1;
                    if self.playback_failures >= MAX_PLAYBACK_FAILURES {
                        self.playback_failures = 0;
                        return vec![Effect::Player(self.playback.stop())];
                    }
                    // An unplayable file does not end the queue.
                    return self.play_following_or_stop();
                }
            },
            Message::AddFolderPressed => return vec![Effect::PickFolder],
            Message::FolderPicked(path) => {
                return vec![Effect::Library(LibraryCommand::AddFolder(path))];
            }
            Message::RemoveFolderPressed(id) => {
                return vec![Effect::Library(LibraryCommand::RemoveFolder(id))];
            }
            Message::VolumeChanged(volume) => {
                self.playback.set_volume(volume);
                return vec![Effect::Player(audio::Command::Volume(
                    self.playback.volume(),
                ))];
            }
            Message::Tabs(message) => {
                let outcome = self.tabs.update(message);
                if outcome.persist
                    && let Err(error) = tab_store::save(&self.tabs)
                {
                    eprintln!("Workspace save failed: {error}");
                }
            }
            Message::SettingsCategoryPressed(category) => self.settings_category = category,
            Message::ThemeChosen(theme) => {
                if self.theme != theme {
                    self.theme = theme;
                    return vec![Effect::ApplyTheme(theme)];
                }
            }
            Message::ListenPercentChosen(percent) => {
                let before = self.playback.listen_percent();
                self.playback.set_listen_percent(percent);
                if self.playback.listen_percent() != before {
                    return vec![Effect::SaveListenPercent(self.playback.listen_percent())];
                }
            }
            Message::PlaylistChosen(id) => {
                if self.playlists.choose(id) {
                    self.rebuild_playlist_rows();
                    return vec![Effect::ScrollToTop(Listing::Playlist)];
                }
            }
            Message::NewPlaylistPressed(tracks) => {
                self.playlists.prompt = Some(playlists::Prompt {
                    purpose: playlists::Purpose::Create(tracks),
                    name: String::new(),
                });
            }
            Message::RenamePlaylistPressed(id) => {
                if let Some(playlist) = self.playlists.find(id) {
                    self.playlists.prompt = Some(playlists::Prompt {
                        purpose: playlists::Purpose::Rename(id),
                        name: playlist.name.clone(),
                    });
                }
            }
            Message::DeletePlaylistConfirmed(playlist) => {
                return vec![Effect::Library(LibraryCommand::DeletePlaylist(playlist))];
            }
            Message::AddToPlaylistPressed { playlist, track } => {
                if self.track_positions.contains_key(&track) {
                    return vec![Effect::Library(LibraryCommand::AddToPlaylist {
                        playlist,
                        tracks: vec![track],
                    })];
                }
            }
            Message::RemoveFromPlaylistPressed(track) => {
                if let Some(playlist) = self.playlists.chosen() {
                    return vec![Effect::Library(LibraryCommand::RemoveFromPlaylist {
                        playlist: playlist.id,
                        track,
                    })];
                }
            }
            Message::PromptNameChanged(name) => {
                if let Some(prompt) = &mut self.playlists.prompt {
                    prompt.name = name;
                }
            }
            Message::PromptConfirmed => return self.confirm_prompt(),
            Message::PromptCancelled => self.playlists.prompt = None,
            Message::PlaylistArtworkLoaded { request_id, result } => {
                self.playlists.art_loaded(request_id, result);
            }
            // The session is given out by `update`.
            Message::Closing => {}
        }

        Vec::new()
    }

    /// The model at start: the workspace's tabs and the session as they
    /// were saved.
    fn restored() -> Self {
        let mut model = Self {
            theme: crate::theme::saved(),
            ..Self::default()
        };
        model.playback.set_listen_percent(listening::saved());
        match tab_store::load() {
            Ok(Some(tabs)) => model.tabs = tabs,
            Ok(None) => {}
            Err(error) => eprintln!("Workspace restore failed: {error}"),
        }
        // Shuffles differ from one run to the next.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |since| since.as_nanos() as u64);
        match session::load() {
            Ok(Some(session)) => model.restore(session, seed),
            Ok(None) => model.queue = queue::State::seeded(seed),
            Err(error) => {
                eprintln!("Session restore failed: {error}");
                model.queue = queue::State::seeded(seed);
            }
        }
        model.saved = model.unsaved();
        model
    }

    /// Takes up a saved session. Its current track is loaded once the
    /// library has told what it knows ([`Self::set_tracks`]).
    fn restore(&mut self, session: session::Session, seed: u64) {
        self.playback.set_volume(session.volume);
        self.queue = queue::State::restored(
            session.order,
            session.unshuffled,
            session.current,
            session.repeat,
            seed,
        );
        self.resume_at = self.queue.current().map(|_| session.position_ms);
        self.saved = self.unsaved();
    }

    /// The session as it would be saved now.
    fn session(&self) -> session::Session {
        let ids = |entries: &[queue::Entry]| entries.iter().map(|entry| entry.track_id).collect();
        session::Session {
            volume: self.playback.volume(),
            repeat: self.queue.repeat(),
            order: ids(self.queue.entries()),
            unshuffled: self.queue.unshuffled().map(ids),
            current: self.queue.current_index(),
            position_ms: self.unsaved().position_ms,
        }
    }

    fn unsaved(&self) -> Saved {
        Saved {
            queue_revision: self.queue.revision(),
            volume: self.playback.volume(),
            // Until the track is loaded where the last run left it, that
            // is still where it is.
            position_ms: self
                .resume_at
                .unwrap_or_else(|| self.playback.position_ms()),
            playing: self.playback.transport() == playback::Transport::Playing,
        }
    }

    /// Gives the session out to be saved when it differs from the saved
    /// one by more than a position that is under way: the queue, the
    /// current track, shuffle, repeat or the volume changed, playback
    /// halted somewhere else, or moved [`POSITION_SAVE_STEP_MS`] on.
    /// `closing` saves whatever differs, and waits for it.
    fn save_session(&mut self, closing: bool) -> Option<Effect> {
        let now = self.unsaved();
        let halted = self.saved.playing && !now.playing;
        let moved = now.position_ms.abs_diff(self.saved.position_ms);
        let due = now.queue_revision != self.saved.queue_revision
            || now.volume != self.saved.volume
            || moved >= POSITION_SAVE_STEP_MS
            || (halted && moved > 0);
        self.saved.playing = now.playing;
        if closing {
            self.saved = now;
            return Some(Effect::FlushSession(self.session()));
        }
        due.then(|| {
            self.saved = now;
            Effect::SaveSession(self.session())
        })
    }

    /// Counts the tracks the player has had listened to, here and in the
    /// library.
    fn record_plays(&mut self) -> Vec<Effect> {
        let effects: Vec<Effect> = self
            .playback
            .take_plays()
            .into_iter()
            .map(|track_id| {
                if let Some(&position) = self.track_positions.get(&track_id) {
                    let track = &mut self.tracks[position];
                    track.play_count = track.play_count.saturating_add(1);
                }
                Effect::Library(LibraryCommand::RecordPlay(track_id))
            })
            .collect();
        if !effects.is_empty() {
            self.rebuild_listing(Listing::MostPlayed);
        }
        effects
    }

    /// The naming prompt's name, when it has one, makes the playlist or
    /// renames it, and the prompt goes.
    fn confirm_prompt(&mut self) -> Vec<Effect> {
        let Some(prompt) = self.playlists.prompt.take() else {
            return Vec::new();
        };
        let name = prompt.name.trim().to_owned();
        if name.is_empty() {
            self.playlists.prompt = Some(prompt);
            return Vec::new();
        }
        let command = match prompt.purpose {
            playlists::Purpose::Create(tracks) => LibraryCommand::CreatePlaylist { name, tracks },
            playlists::Purpose::Rename(playlist) => {
                LibraryCommand::RenamePlaylist { playlist, name }
            }
        };
        vec![Effect::Library(command)]
    }

    /// Has the Playlists tab's artwork be of the first track of its
    /// playlist that is there.
    fn sync_playlist_artwork(&mut self) -> Option<Effect> {
        let first = self
            .playlists
            .chosen()
            .and_then(|playlist| playlist.tracks.iter().find_map(|&id| self.playable(id)))
            .map(|position| self.tracks[position].path.clone());
        self.playlists
            .show_art(first.as_deref())
            .map(Effect::LoadPlaylistArtwork)
    }

    /// What the Playlists tab's header tells of its playlist.
    fn playlist_summary(&self) -> Option<playlists::Summary> {
        self.playlists.chosen().map(|playlist| {
            playlists::Summary::of(playlist, |id| {
                self.track_positions
                    .get(&id)
                    .map(|&position| &self.tracks[position])
            })
        })
    }

    /// Has the player hold, behind the loaded track, the one the queue
    /// says comes next, or nothing when none does or nothing is loaded.
    fn sync_queued(&mut self) -> Option<Effect> {
        let loaded = self.playback.transport() != playback::Transport::Stopped;
        let upcoming = loaded
            .then(|| self.queue.upcoming(|id| self.can_play(id)))
            .flatten()
            .map(|index| self.queue.entries()[index]);
        let command = match upcoming {
            Some(entry) => {
                let track = &self.tracks[self.track_positions[&entry.track_id]];
                if self.playback.queued() == Some((entry.key, track.path.as_path())) {
                    return None;
                }
                self.playback.queue(track, entry.key)
            }
            None => self.playback.unqueue(),
        };
        command.map(Effect::Player)
    }

    fn set_tracks(&mut self, tracks: Vec<Track>) -> Vec<Effect> {
        self.tracks = tracks;
        self.track_positions = self
            .tracks
            .iter()
            .enumerate()
            .map(|(position, track)| (track.id, position))
            .collect();
        self.rebuild_projection();
        self.catalog = Catalog::of(&self.tracks);

        // The track the last run ended on, loaded where it was and paused.
        if let Some(position_ms) = self.resume_at.take() {
            let Some(position) = self.current_position() else {
                return Vec::new();
            };
            let track = &self.tracks[position];
            let artwork = self.playback.select(track);
            let cue = self.playback.cue(track, position_ms);
            return artwork
                .map(Effect::LoadArtwork)
                .into_iter()
                .chain([Effect::Player(cue)])
                .collect();
        }
        self.reconcile_current_track()
    }

    /// Takes tracks that are new or differ from what the library told
    /// before: moved, changed, back or gone absent.
    fn merge_tracks(&mut self, tracks: Vec<Track>) -> Vec<Effect> {
        for mut track in tracks {
            match self.track_positions.get(&track.id) {
                Some(&position) => {
                    // Counts only grow: a change told before a listen
                    // reached the library does not take it back.
                    track.play_count = track.play_count.max(self.tracks[position].play_count);
                    self.tracks[position] = track;
                }
                None => {
                    self.track_positions.insert(track.id, self.tracks.len());
                    self.tracks.push(track);
                }
            }
        }
        self.rebuild_projection();
        self.catalog = Catalog::of(&self.tracks);
        self.reconcile_current_track()
    }

    /// Where the track of this id is in `tracks`, when its file is there.
    fn playable(&self, track_id: i64) -> Option<usize> {
        self.track_positions
            .get(&track_id)
            .copied()
            .filter(|&position| self.tracks[position].present)
    }

    fn can_play(&self, track_id: i64) -> bool {
        self.playable(track_id).is_some()
    }

    /// Follows the current track through a change of the library: its
    /// artwork is read again when it differs, and a track that is gone
    /// is left for the next one when it was playing, else dropped by the
    /// player. It keeps its place in the queue either way.
    fn reconcile_current_track(&mut self) -> Vec<Effect> {
        if self.queue.current().is_none() || self.current_position().is_some() {
            return self.show_current();
        }
        match self.playback.transport() {
            playback::Transport::Playing => self.play_following_or_stop(),
            playback::Transport::Paused => vec![Effect::Player(self.playback.stop())],
            playback::Transport::Stopped => Vec::new(),
        }
    }

    /// Has the playback bar show the queue's current track: its artwork
    /// is read when it is another's.
    fn show_current(&mut self) -> Vec<Effect> {
        let Some(position) = self.current_position() else {
            return Vec::new();
        };
        self.playback
            .select(&self.tracks[position])
            .map(Effect::LoadArtwork)
            .into_iter()
            .collect()
    }

    /// Starts the queue's current track, which can be played.
    fn play_current(&mut self) -> Vec<Effect> {
        let Some(position) = self.current_position() else {
            return Vec::new();
        };
        let track = &self.tracks[position];
        let artwork = self.playback.select(track);
        let load = self.playback.play(track);
        artwork
            .map(Effect::LoadArtwork)
            .into_iter()
            .chain([Effect::Player(load)])
            .collect()
    }

    /// Starts the queue's current track, or the next after it that can be
    /// played; with none, nothing.
    fn play_from_current(&mut self) -> Vec<Effect> {
        if self.current_position().is_none() {
            let Some(index) = self.queue.following(|id| self.can_play(id)) else {
                return Vec::new();
            };
            self.queue.go_to(index);
        }
        self.play_current()
    }

    /// Moves on from a track that cannot be played to the next that can;
    /// after the last, playback stops.
    fn play_following_or_stop(&mut self) -> Vec<Effect> {
        match self.queue.following(|id| self.can_play(id)) {
            Some(index) => {
                self.queue.go_to(index);
                self.play_current()
            }
            None => vec![Effect::Player(self.playback.stop())],
        }
    }

    /// The current entry was taken out of the queue, which made the one
    /// behind it current: that one plays on when the removed one was
    /// playing, and otherwise waits to be played.
    fn leave_removed_track(&mut self) -> Vec<Effect> {
        if self.queue.current().is_none() {
            return vec![Effect::Player(self.playback.clear())];
        }
        if self.playback.transport() == playback::Transport::Playing {
            let effects = self.play_from_current();
            if !effects.is_empty() {
                return effects;
            }
        }
        let stop = Effect::Player(self.playback.stop());
        std::iter::once(stop).chain(self.show_current()).collect()
    }

    /// The ids of the tracks of `listing`, as it is shown now.
    fn listed_tracks(&self, listing: Listing) -> Vec<i64> {
        self.rows(listing)
            .iter()
            .map(|&position| self.tracks[position].id)
            .collect()
    }

    /// The track of `track_id` plays, with `listed` as the queue.
    fn play_listed(&mut self, mut listed: Vec<i64>, track_id: i64) -> Vec<Effect> {
        if self.playable(track_id).is_none() {
            return Vec::new();
        }
        let start = listed
            .iter()
            .position(|&id| id == track_id)
            .unwrap_or_else(|| {
                // Pressed as the list changed under it.
                listed.insert(0, track_id);
                0
            });
        self.playback_failures = 0;
        self.queue.replace(listed, start);
        self.play_current()
    }

    /// Where the queue's current track is in `tracks`, when its file is
    /// there.
    fn current_position(&self) -> Option<usize> {
        self.queue
            .current()
            .and_then(|entry| self.playable(entry.track_id))
    }

    pub(crate) fn current_track(&self) -> Option<&Track> {
        self.current_position()
            .map(|position| &self.tracks[position])
    }

    /// Where in `tracks` the rows of `listing` are.
    fn rows(&self, listing: Listing) -> &[usize] {
        &self.listed[listing.index()]
    }

    /// The track shown in row `row` of `listing`.
    fn listed_track(&self, listing: Listing, row: usize) -> Option<&Track> {
        self.rows(listing)
            .get(row)
            .map(|&position| &self.tracks[position])
    }

    /// The order `listing` is in: the library's is the user's choice; a
    /// playlist is in its own (none).
    fn sort_of(&self, listing: Listing) -> Option<(SortColumn, SortDirection)> {
        match listing {
            Listing::Library => Some((self.sort_column, self.sort_direction)),
            Listing::RecentlyAdded => Some((SortColumn::Added, SortDirection::Descending)),
            Listing::MostPlayed => Some((SortColumn::Plays, SortDirection::Descending)),
            Listing::Playlist => None,
        }
    }

    /// The entry at place `index` of the queue and its track, when the
    /// library knows it; `true` for the current entry.
    fn queued_track(&self, index: usize) -> Option<(queue::Entry, Option<&Track>, bool)> {
        let entry = *self.queue.entries().get(index)?;
        let track = self
            .track_positions
            .get(&entry.track_id)
            .map(|&position| &self.tracks[position]);
        Some((entry, track, self.queue.current_index() == Some(index)))
    }

    fn rebuild_projection(&mut self) {
        self.listed = Listing::ALL.map(|listing| self.listing_rows(listing));
    }

    fn rebuild_listing(&mut self, listing: Listing) {
        self.listed[listing.index()] = self.listing_rows(listing);
    }

    /// Where in `tracks` the rows of `listing` are, as it is to be shown.
    fn listing_rows(&self, listing: Listing) -> Vec<usize> {
        let Some((column, direction)) = self.sort_of(listing) else {
            return self.playlist_rows();
        };
        let mut rows = crate::library::matching_track_indices(
            &self.tracks,
            &self.search_query,
            column,
            direction,
        );
        if listing == Listing::MostPlayed {
            // Only what was listened to.
            rows.retain(|&position| self.tracks[position].play_count > 0);
        }
        rows
    }

    fn rebuild_playlist_rows(&mut self) {
        self.listed[Listing::Playlist.index()] = self.playlist_rows();
    }

    /// Where in `tracks` the shown playlist's tracks that are there and
    /// match the search are, in its order.
    fn playlist_rows(&self) -> Vec<usize> {
        let Some(playlist) = self.playlists.chosen() else {
            return Vec::new();
        };
        crate::library::matching_in_order(
            &self.tracks,
            playlist
                .tracks
                .iter()
                .filter_map(|id| self.track_positions.get(id).copied()),
            &self.search_query,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use crate::library::test_track;

    use super::*;

    /// Three tracks of one album, listed by title: 1, 2, 3.
    fn app_with_three_tracks() -> Model {
        app_with_tracks(3)
    }

    /// Tracks 1 to `count` of one album at "/music/album/{id}.flac",
    /// listed in that order. The first three are also "a", "b" and "c".
    fn app_with_tracks(count: i64) -> Model {
        let mut app = Model::default();
        let _ = app.set_tracks((1..=count).map(album_track).collect());
        app
    }

    fn album_track(id: i64) -> Track {
        test_track(id, track_path(id), Some("album"))
    }

    fn track_path(id: i64) -> PathBuf {
        let name = match id {
            1 => "a".to_owned(),
            2 => "b".to_owned(),
            3 => "c".to_owned(),
            _ => id.to_string(),
        };
        PathBuf::from(format!("/music/album/{name}.flac"))
    }

    /// The ticket and the path of the load among `effects`.
    fn loaded(effects: &[Effect]) -> (u64, PathBuf) {
        effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Player(audio::Command::Load { ticket, path }) => {
                    Some((*ticket, path.clone()))
                }
                _ => None,
            })
            .expect("a track was loaded")
    }

    /// What `effects` ask of the player.
    fn commands(effects: &[Effect]) -> Vec<&audio::Command> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Player(command) => Some(command),
                _ => None,
            })
            .collect()
    }

    fn current_id(app: &Model) -> Option<i64> {
        app.current_track().map(|track| track.id)
    }

    fn queued_ids(app: &Model) -> Vec<i64> {
        app.queue
            .entries()
            .iter()
            .map(|entry| entry.track_id)
            .collect()
    }

    fn key_of(app: &Model, track_id: i64) -> u64 {
        app.queue
            .entries()
            .iter()
            .find(|entry| entry.track_id == track_id)
            .map(|entry| entry.key)
            .expect("the track is queued")
    }

    /// Presses the queue's row of `track_id`.
    fn press_entry(app: &mut Model, track_id: i64) -> Vec<Effect> {
        let key = key_of(app, track_id);
        update(app, Message::QueueEntryPressed(key))
    }

    /// Takes `track_id` out of the queue.
    fn remove_entry(app: &mut Model, track_id: i64) -> Vec<Effect> {
        let key = key_of(app, track_id);
        update(app, Message::QueueRemovePressed(key))
    }

    fn absent(app: &Model, track_id: i64) -> Message {
        let mut track = app.tracks[app.track_positions[&track_id]].clone();
        track.present = false;
        Message::LibraryEvent(LibraryEvent::Changed(vec![track]))
    }

    #[test]
    fn unknown_track_press_preserves_current_track() {
        let mut app = app_with_tracks(1);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        assert_eq!(current_id(&app), Some(1));

        assert!(update(&mut app, Message::TrackPressed(Listing::Library, 999)).is_empty());

        assert_eq!(current_id(&app), Some(1));
    }

    // Was `library_replacement_without_selected_id_clears_current_track`:
    // the library no longer replaces its list, it tells of a file that
    // is gone as an absent track, and the queue moves on from it.
    #[test]
    fn a_playing_track_that_goes_absent_is_left_for_the_next_or_stops() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));

        let gone = absent(&app, 2);
        let effects = update(&mut app, gone);
        assert_eq!(loaded(&effects).1, track_path(3));
        assert_eq!(current_id(&app), Some(3));
        // It keeps its place: back on disk, it is in the queue as before.
        assert_eq!(queued_ids(&app), [1, 2, 3]);

        // With nothing left to play, playback stops.
        let gone = absent(&app, 3);
        let effects = update(&mut app, gone);
        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert!(app.current_track().is_none());
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
    }

    #[test]
    fn library_changes_are_merged_and_absent_tracks_leave_the_list() {
        let mut app = app_with_three_tracks();
        let mut gone = app.tracks[0].clone();
        gone.present = false;
        let added = album_track(4);

        let effects = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Changed(vec![gone, added])),
        );

        assert!(effects.is_empty());
        let listed: Vec<i64> = (0..app.rows(Listing::Library).len())
            .filter_map(|row| {
                app.listed_track(Listing::Library, row)
                    .map(|track| track.id)
            })
            .collect();
        assert_eq!(listed, [2, 3, 4]);
        // The absent track is kept for what names it, and cannot be played.
        assert_eq!(app.tracks.len(), 4);
        assert_eq!(app.catalog.track_count(), 3);
        assert!(update(&mut app, Message::TrackPressed(Listing::Library, 1)).is_empty());
    }

    #[test]
    fn recently_added_lists_the_last_added_first_and_queues_that_list() {
        let mut app = app_with_three_tracks();
        for (track, modified_ns) in app.tracks.iter_mut().zip([20, 10, 30]) {
            track.modified_ns = modified_ns;
        }
        app.rebuild_projection();

        let recent = Listing::RecentlyAdded;
        let listed: Vec<i64> = (0..app.rows(recent).len())
            .filter_map(|row| app.listed_track(recent, row).map(|track| track.id))
            .collect();
        assert_eq!(listed, [3, 1, 2]);

        let _ = update(&mut app, Message::TrackPressed(recent, 1));
        assert_eq!(queued_ids(&app), [3, 1, 2]);
        assert_eq!(current_id(&app), Some(1));
    }

    #[test]
    fn most_played_lists_what_was_listened_to_the_most_often_first() {
        let mut app = app_with_three_tracks();
        for (track, plays) in app.tracks.iter_mut().zip([2, 0, 5]) {
            track.play_count = plays;
        }
        app.rebuild_projection();

        let most = Listing::MostPlayed;
        let listed = |app: &Model| -> Vec<i64> {
            (0..app.rows(most).len())
                .filter_map(|row| app.listed_track(most, row).map(|track| track.id))
                .collect()
        };
        assert_eq!(listed(&app), [3, 1]);

        // A stale change from the library keeps the count.
        let mut stale = app.tracks[app.track_positions[&1]].clone();
        stale.play_count = 0;
        let changed = Message::LibraryEvent(LibraryEvent::Changed(vec![stale]));
        let _ = update(&mut app, changed);
        assert_eq!(listed(&app), [3, 1]);

        // A track listened to for the first time joins the list at once.
        let (ticket, _) = loaded(&update(
            &mut app,
            Message::TrackPressed(Listing::Library, 2),
        ));
        let started = audio::Event::Started {
            ticket,
            duration: Some(Duration::from_secs(10)),
        };
        let _ = update(&mut app, Message::Player(started));
        let position = audio::Event::Position {
            ticket,
            position: Duration::from_secs(1),
        };
        let _ = update(&mut app, Message::Player(position));
        assert_eq!(listed(&app), [3, 1, 2]);

        // Played from the list, the list is the queue.
        let _ = update(&mut app, Message::TrackPressed(most, 1));
        assert_eq!(queued_ids(&app), [3, 1, 2]);
        assert_eq!(current_id(&app), Some(1));
    }

    #[test]
    fn folders_are_asked_for_picked_and_removed_through_the_library() {
        let mut app = Model::default();
        let folder = PathBuf::from("/music/more");

        assert_eq!(
            update(&mut app, Message::AddFolderPressed),
            [Effect::PickFolder]
        );
        assert_eq!(
            update(&mut app, Message::FolderPicked(folder.clone())),
            [Effect::Library(LibraryCommand::AddFolder(folder.clone()))]
        );
        assert_eq!(
            update(&mut app, Message::RemoveFolderPressed(3)),
            [Effect::Library(LibraryCommand::RemoveFolder(3))]
        );

        let folders = vec![Folder {
            id: 3,
            path: folder,
            available: false,
        }];
        let told = Message::LibraryEvent(LibraryEvent::Folders(folders.clone()));
        assert!(update(&mut app, told).is_empty());
        assert_eq!(app.folders, folders);
    }

    /// How long the model takes over a library of 50,000 tracks:
    /// `cargo test --lib scale -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn scale_of_the_model_at_50_000_tracks() {
        use std::time::Instant;

        let tracks = crate::library::test_library(50_000);
        let mut app = Model::default();
        let timed = |what: &str, app: &mut Model, message: Message| {
            let started = Instant::now();
            let _ = update(app, message);
            println!("model: {what}: {:?}", started.elapsed());
        };

        let loaded = Message::LibraryEvent(LibraryEvent::Loaded(tracks.clone()));
        timed(
            "the cached library arrives (list and catalog)",
            &mut app,
            loaded,
        );
        assert_eq!(app.rows(Listing::Library).len(), 50_000);
        for (what, query) in [
            ("a letter", "a"),
            ("a word", "album 31"),
            ("no match", "zzzz"),
        ] {
            timed(
                &format!("search for {what}"),
                &mut app,
                Message::SearchChanged(query.to_owned()),
            );
        }
        timed(
            "search cleared",
            &mut app,
            Message::SearchChanged(String::new()),
        );
        for column in [
            SortColumn::Album,
            SortColumn::Artist,
            SortColumn::Year,
            SortColumn::Duration,
            SortColumn::Title,
        ] {
            timed(
                &format!("sort by {column:?}"),
                &mut app,
                Message::SortChanged(column),
            );
        }
        let changed = Message::LibraryEvent(LibraryEvent::Changed(tracks[..1_000].to_vec()));
        timed("1,000 tracks change at once", &mut app, changed);
        timed(
            "a row is pressed (a queue of 50,000, and its session)",
            &mut app,
            Message::TrackPressed(Listing::Library, tracks[25_000].id),
        );
        timed("next", &mut app, Message::NextPressed);
        timed("shuffle on", &mut app, Message::ShufflePressed);
        timed(
            "play next",
            &mut app,
            Message::PlayNextPressed(tracks[7].id),
        );
        let key = app.queue.entries()[40_000].key;
        timed("remove", &mut app, Message::QueueRemovePressed(key));
        timed("shuffle off", &mut app, Message::ShufflePressed);
        let tick = audio::Event::Position {
            ticket: 0,
            position: Duration::from_secs(1),
        };
        timed("a position report", &mut app, Message::Player(tick));
    }

    #[test]
    fn a_pressed_track_plays_and_reads_its_artwork() {
        let mut app = app_with_three_tracks();

        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 2));

        assert!(matches!(effects[0], Effect::LoadArtwork(_)));
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
        assert_eq!(app.playback.transport(), playback::Transport::Playing);
        assert_eq!(current_id(&app), Some(2));
    }

    #[test]
    fn play_pause_starts_the_list_then_pauses_and_resumes() {
        let mut app = app_with_three_tracks();

        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/a.flac"));

        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(effects, [Effect::Player(audio::Command::Pause)]);
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(effects, [Effect::Player(audio::Command::Play)]);
    }

    #[test]
    fn next_and_previous_walk_the_queue_as_it_was_listed() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));
        // A search after the press does not change what comes next.
        let _ = update(&mut app, Message::SearchChanged("a".to_owned()));

        let effects = update(&mut app, Message::NextPressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/c.flac"));
        assert!(update(&mut app, Message::NextPressed).is_empty());
        assert_eq!(current_id(&app), Some(3));

        let effects = update(&mut app, Message::PreviousPressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
    }

    #[test]
    fn previous_restarts_a_track_that_is_under_way_or_first() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 2));
        let (ticket, _) = loaded(&effects);
        let position = audio::Event::Position {
            ticket,
            position: Duration::from_secs(10),
        };
        let _ = update(&mut app, Message::Player(position));

        let effects = update(&mut app, Message::PreviousPressed);
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { position, .. })] if position.is_zero()
        ));
        assert_eq!(current_id(&app), Some(2));

        let _ = update(&mut app, Message::PreviousPressed);
        assert_eq!(current_id(&app), Some(1));
        let effects = update(&mut app, Message::PreviousPressed);
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { .. })]
        ));
        assert_eq!(current_id(&app), Some(1));
    }

    #[test]
    fn a_finished_track_goes_on_to_the_next_and_the_last_one_stops() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 2));
        let (ticket, _) = loaded(&effects);

        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        let (ticket, path) = loaded(&effects);
        assert_eq!(path, PathBuf::from("/music/album/c.flac"));

        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        assert_eq!(effects, [Effect::Player(audio::Command::Stop)]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
        assert_eq!(current_id(&app), Some(3));

        // Play starts the track it stopped on again.
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/c.flac"));
    }

    /// The ticket and the path of the file queued among `effects`.
    fn queued(effects: &[Effect]) -> Option<(u64, PathBuf)> {
        effects.iter().find_map(|effect| match effect {
            Effect::Player(audio::Command::Queue { ticket, path }) => Some((*ticket, path.clone())),
            _ => None,
        })
    }

    #[test]
    fn a_started_track_has_the_next_queued_and_follows_the_player_to_it() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let (ticket, path) = queued(&effects).expect("the next track is queued");
        assert_eq!(path, PathBuf::from("/music/album/b.flac"));

        // The player went on by itself: nothing is loaded again.
        let started = audio::Event::Started {
            ticket,
            duration: Some(Duration::from_secs(3)),
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(2));
        assert_eq!(app.playback.position_ms(), 0);
        assert_eq!(app.playback.duration_ms(), Some(3_000));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Player(audio::Command::Load { .. })))
        );
        let (ticket, path) = queued(&effects).expect("the track after it is queued");
        assert_eq!(path, PathBuf::from("/music/album/c.flac"));

        // Behind the last track nothing is queued, and its end stops.
        let started = audio::Event::Started {
            ticket,
            duration: None,
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(3));
        assert!(queued(&effects).is_none());
        let effects = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        assert_eq!(effects, [Effect::Player(audio::Command::Stop)]);
    }

    #[test]
    fn an_unplayable_track_is_skipped() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let (ticket, _) = loaded(&effects);
        let failed = audio::Event::Failed {
            ticket,
            error: "unsupported file".to_owned(),
        };

        let effects = update(&mut app, Message::Player(failed));

        assert_eq!(loaded(&effects).1, PathBuf::from("/music/album/b.flac"));
    }

    #[test]
    fn five_tracks_failing_in_a_row_give_up_even_when_all_repeat() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::RepeatPressed);
        let mut effects = update(&mut app, Message::TrackPressed(Listing::Library, 1));

        for attempt in 1..=MAX_PLAYBACK_FAILURES {
            let failed = audio::Event::Failed {
                ticket: loaded(&effects).0,
                error: "unsupported file".to_owned(),
            };
            effects = update(&mut app, Message::Player(failed));
            if attempt < MAX_PLAYBACK_FAILURES {
                // Round the end of the queue to its start, as all repeat.
                let next = [2, 3, 1, 2][usize::from(attempt) - 1];
                assert_eq!(loaded(&effects).1, track_path(next));
            }
        }

        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
    }

    // Changed: the volume is part of the session now, so its change is
    // also given out to be saved.
    #[test]
    fn volume_and_position_reach_the_player() {
        let mut app = app_with_three_tracks();
        assert!(update(&mut app, Message::PositionChanged(1_000)).is_empty());

        let effects = update(&mut app, Message::VolumeChanged(40));
        assert_eq!(commands(&effects), [&audio::Command::Volume(40)]);
        assert!(matches!(
            effects[..],
            [_, Effect::SaveSession(session::Session { volume: 40, .. })]
        ));

        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let effects = update(&mut app, Message::PositionChanged(1_000));
        assert!(matches!(
            effects[..],
            [Effect::Player(audio::Command::Seek { position, .. })]
                if position == Duration::from_secs(1)
        ));
        assert_eq!(app.playback.position_ms(), 1_000);
    }

    #[test]
    fn a_track_heard_for_its_share_is_counted_here_and_in_the_library() {
        let mut app = app_with_three_tracks();
        let effects = update(&mut app, Message::ListenPercentChosen(50));
        assert_eq!(effects, [Effect::SaveListenPercent(50)]);
        assert!(update(&mut app, Message::ListenPercentChosen(50)).is_empty());

        let (ticket, _) = loaded(&update(
            &mut app,
            Message::TrackPressed(Listing::Library, 1),
        ));
        let started = audio::Event::Started {
            ticket,
            duration: Some(Duration::from_secs(10)),
        };
        let _ = update(&mut app, Message::Player(started));
        let mut recorded = Vec::new();
        for second in 1..=8 {
            let position = audio::Event::Position {
                ticket,
                position: Duration::from_secs(second),
            };
            recorded.extend(
                update(&mut app, Message::Player(position))
                    .into_iter()
                    .filter(|effect| matches!(effect, Effect::Library(_))),
            );
        }
        assert_eq!(recorded, [Effect::Library(LibraryCommand::RecordPlay(1))]);
        assert_eq!(app.tracks[app.track_positions[&1]].play_count, 1);
        assert_eq!(app.tracks[app.track_positions[&2]].play_count, 0);
    }

    #[test]
    fn play_next_and_add_to_end_queue_behind_the_current_track_and_the_last() {
        let mut app = app_with_tracks(5);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));

        // Behind the current track: it is what the player has next now.
        let effects = update(&mut app, Message::PlayNextPressed(5));
        assert_eq!(queued(&effects).unwrap().1, track_path(5));
        let effects = update(&mut app, Message::AddToQueuePressed(1));
        assert!(queued(&effects).is_none());
        assert_eq!(queued_ids(&app), [1, 2, 5, 3, 4, 5, 1]);
        assert_eq!(current_id(&app), Some(2));

        // Unknown tracks are not queued.
        let _ = update(&mut app, Message::PlayNextPressed(999));
        let _ = update(&mut app, Message::AddToQueuePressed(999));
        assert_eq!(queued_ids(&app).len(), 7);

        // Into an empty queue: queued, and played from there.
        let mut app = app_with_tracks(5);
        assert!(commands(&update(&mut app, Message::AddToQueuePressed(4))).is_empty());
        let _ = update(&mut app, Message::PlayNextPressed(3));
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, track_path(3));
        assert_eq!(queued(&effects).unwrap().1, track_path(4));
    }

    #[test]
    fn a_pressed_queue_entry_plays_and_the_queue_stays_as_it_is() {
        let mut app = app_with_tracks(4);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));

        let effects = press_entry(&mut app, 3);

        assert_eq!(loaded(&effects).1, track_path(3));
        assert_eq!(queued(&effects).unwrap().1, track_path(4));
        assert_eq!(queued_ids(&app), [1, 2, 3, 4]);
        assert!(update(&mut app, Message::QueueEntryPressed(999)).is_empty());
    }

    #[test]
    fn removing_and_moving_entries_keeps_what_plays_and_updates_what_is_next() {
        let mut app = app_with_tracks(4);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));

        // The one that was next: the one behind it is now.
        let effects = remove_entry(&mut app, 3);
        assert_eq!(commands(&effects).len(), 1);
        assert_eq!(queued(&effects).unwrap().1, track_path(4));
        assert_eq!(current_id(&app), Some(2));

        // Moved in front of the next one.
        let key = key_of(&app, 1);
        let effects = update(&mut app, Message::QueueMovePressed { key, to: 1 });
        assert_eq!(queued(&effects).unwrap().1, track_path(1));
        assert_eq!(queued_ids(&app), [2, 1, 4]);
        assert_eq!(current_id(&app), Some(2));

        // The playing one: the one behind it plays on.
        let effects = remove_entry(&mut app, 2);
        assert_eq!(loaded(&effects).1, track_path(1));
        assert_eq!(queued(&effects).unwrap().1, track_path(4));

        // The last one while it plays: nothing is left to play.
        let _ = update(&mut app, Message::NextPressed);
        let effects = remove_entry(&mut app, 4);
        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert!(app.current_track().is_none());
        assert_eq!(queued_ids(&app), [1]);
        // Play starts what is left.
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, track_path(1));
    }

    #[test]
    fn removing_the_paused_track_leaves_the_next_one_waiting() {
        let mut app = app_with_tracks(3);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let _ = update(&mut app, Message::PlayPausePressed);

        let effects = remove_entry(&mut app, 1);

        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
        assert_eq!(current_id(&app), Some(2));
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, track_path(2));
    }

    #[test]
    fn clearing_keeps_the_loaded_track_and_empties_a_stopped_queue() {
        let mut app = app_with_tracks(3);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));

        let effects = update(&mut app, Message::QueueClearPressed);
        assert_eq!(commands(&effects), [&audio::Command::Unqueue]);
        assert_eq!(queued_ids(&app), [2]);
        assert_eq!(app.playback.transport(), playback::Transport::Playing);

        // At its end playback stops; cleared again, nothing is current.
        let ticket = loaded(&press_entry(&mut app, 2)).0;
        let _ = update(&mut app, Message::Player(audio::Event::Finished { ticket }));
        let effects = update(&mut app, Message::QueueClearPressed);
        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert!(app.queue.is_empty());
        assert!(app.current_track().is_none());

        // Play then starts the list from its top, as on a first launch.
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(loaded(&effects).1, track_path(1));
    }

    #[test]
    fn shuffle_keeps_the_playing_track_and_returns_to_the_order_at_it() {
        let mut app = app_with_tracks(40);
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 5));
        assert_eq!(queued(&effects).unwrap().1, track_path(6));

        // On: nothing is loaded again, and the next is another track.
        let effects = update(&mut app, Message::ShufflePressed);
        assert!(app.queue.shuffled());
        assert_eq!(current_id(&app), Some(5));
        let [audio::Command::Queue { path, .. }] = commands(&effects)[..] else {
            panic!("only the next track changes: {effects:?}");
        };
        let next = queued_ids(&app)[5];
        assert_eq!(*path, track_path(next));
        assert_ne!(queued_ids(&app), (1..=40).collect::<Vec<i64>>());

        // On through the shuffle, by the player and by the user.
        let started = audio::Event::Started {
            ticket: queued(&effects).unwrap().0,
            duration: None,
        };
        let _ = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(next));
        let effects = update(&mut app, Message::NextPressed);
        let heard = queued_ids(&app)[6];
        assert_eq!(loaded(&effects).1, track_path(heard));

        // Off: the original order, at the track that plays, which goes on.
        let effects = update(&mut app, Message::ShufflePressed);
        assert!(!app.queue.shuffled());
        assert_eq!(queued_ids(&app), (1..=40).collect::<Vec<i64>>());
        assert_eq!(current_id(&app), Some(heard));
        let after = if heard == 40 { None } else { Some(heard + 1) };
        assert_eq!(
            queued(&effects).map(|(_, path)| path),
            after.map(track_path)
        );
        assert!(
            commands(&effects)
                .iter()
                .all(|command| !matches!(command, audio::Command::Load { .. }))
        );
    }

    #[test]
    fn repeat_all_wraps_gaplessly_and_for_next_and_previous() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 3));

        // All: the first track is queued behind the last.
        let effects = update(&mut app, Message::RepeatPressed);
        assert_eq!(app.queue.repeat(), queue::Repeat::All);
        let (ticket, path) = queued(&effects).expect("the first track is queued");
        assert_eq!(path, track_path(1));
        let started = audio::Event::Started {
            ticket,
            duration: None,
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(1));
        assert_eq!(queued(&effects).unwrap().1, track_path(2));

        let effects = update(&mut app, Message::PreviousPressed);
        assert_eq!(loaded(&effects).1, track_path(3));
        let effects = update(&mut app, Message::NextPressed);
        assert_eq!(loaded(&effects).1, track_path(1));
    }

    #[test]
    fn repeat_one_queues_the_track_behind_itself_until_the_user_moves_on() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 2));
        let _ = update(&mut app, Message::RepeatPressed);

        let effects = update(&mut app, Message::RepeatPressed);
        assert_eq!(app.queue.repeat(), queue::Repeat::One);
        let (ticket, path) = queued(&effects).expect("the track is queued behind itself");
        assert_eq!(path, track_path(2));

        // Round it goes, gaplessly, and is queued behind itself again.
        let started = audio::Event::Started {
            ticket,
            duration: None,
        };
        let effects = update(&mut app, Message::Player(started));
        assert_eq!(current_id(&app), Some(2));
        assert_eq!(app.playback.position_ms(), 0);
        let (again, path) = queued(&effects).expect("and again");
        assert_eq!(path, track_path(2));
        assert_ne!(again, ticket);

        // Next moves on, and the track it moves to repeats.
        let effects = update(&mut app, Message::NextPressed);
        assert_eq!(loaded(&effects).1, track_path(3));
        assert_eq!(queued(&effects).unwrap().1, track_path(3));

        // Off: nothing is behind the last track.
        let effects = update(&mut app, Message::RepeatPressed);
        assert_eq!(app.queue.repeat(), queue::Repeat::Off);
        assert_eq!(commands(&effects), [&audio::Command::Unqueue]);
    }

    #[test]
    fn a_player_that_went_on_to_a_replaced_track_is_put_right() {
        let mut app = app_with_tracks(4);
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let stale = queued(&effects).unwrap().0;

        // Track 4 is put in front of 2, but the player had gone on to 2.
        let _ = update(&mut app, Message::PlayNextPressed(4));
        let started = audio::Event::Started {
            ticket: stale,
            duration: None,
        };
        let effects = update(&mut app, Message::Player(started));

        // Track 1 is over: what the queue says comes next is loaded.
        assert_eq!(loaded(&effects).1, track_path(4));
        assert_eq!(current_id(&app), Some(4));
        assert_eq!(queued(&effects).unwrap().1, track_path(2));
    }

    #[test]
    fn the_next_track_follows_the_library() {
        let mut app = app_with_tracks(4);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));

        // Gone: the one behind it is next.
        let gone = absent(&app, 2);
        let effects = update(&mut app, gone);
        assert_eq!(queued(&effects).unwrap().1, track_path(3));

        // Renamed: queued again where it is now.
        let mut renamed = app.tracks[app.track_positions[&3]].clone();
        renamed.path = PathBuf::from("/music/album/renamed.flac");
        let moved = Message::LibraryEvent(LibraryEvent::Changed(vec![renamed]));
        let effects = update(&mut app, moved);
        assert_eq!(
            queued(&effects).unwrap().1,
            Path::new("/music/album/renamed.flac")
        );

        // Back: next again, in its old place.
        let back = Message::LibraryEvent(LibraryEvent::Changed(vec![album_track(2)]));
        let effects = update(&mut app, back);
        assert_eq!(queued(&effects).unwrap().1, track_path(2));
        assert_eq!(queued_ids(&app), [1, 2, 3, 4]);
    }

    #[test]
    fn the_session_is_saved_on_changes_and_not_on_every_position() {
        let mut app = app_with_three_tracks();
        let saved = |effects: &[Effect]| {
            effects.iter().find_map(|effect| match effect {
                Effect::SaveSession(session) => Some(session.clone()),
                _ => None,
            })
        };
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 2));
        let (ticket, _) = loaded(&effects);
        let session = saved(&effects).expect("a new queue is saved");
        assert_eq!((session.order, session.current), (vec![1, 2, 3], Some(1)));
        let tick = |app: &mut Model, seconds: u64| {
            let position = audio::Event::Position {
                ticket,
                position: Duration::from_secs(seconds),
            };
            update(app, Message::Player(position))
        };

        // The position as it moves: not until it is a step on.
        for seconds in 1..15 {
            assert!(saved(&tick(&mut app, seconds)).is_none(), "at {seconds} s");
        }
        assert_eq!(saved(&tick(&mut app, 15)).unwrap().position_ms, 15_000);
        assert!(saved(&tick(&mut app, 20)).is_none());

        // Where it is paused, and not again when nothing moved.
        let effects = update(&mut app, Message::PlayPausePressed);
        assert_eq!(saved(&effects).unwrap().position_ms, 20_000);
        assert!(saved(&update(&mut app, Message::PlayPausePressed)).is_none());
        assert!(saved(&update(&mut app, Message::PlayPausePressed)).is_none());

        // Shuffle, repeat and the volume.
        assert!(saved(&update(&mut app, Message::ShufflePressed)).is_some());
        let session = saved(&update(&mut app, Message::RepeatPressed)).unwrap();
        assert_eq!(session.repeat, queue::Repeat::All);
        assert!(session.unshuffled.is_some());
        assert_eq!(
            saved(&update(&mut app, Message::VolumeChanged(30)))
                .unwrap()
                .volume,
            30
        );
        // What changes nothing of it saves nothing.
        assert!(saved(&update(&mut app, Message::SearchChanged("b".into()))).is_none());

        // Closing saves where it is, and waits for it.
        let _ = update(&mut app, Message::PlayPausePressed);
        let _ = tick(&mut app, 23);
        let effects = update(&mut app, Message::Closing);
        assert!(matches!(
            effects[..],
            [Effect::FlushSession(session::Session {
                position_ms: 23_000,
                ..
            })]
        ));
    }

    #[test]
    fn the_next_launch_is_paused_where_the_last_run_was() {
        let mut app = app_with_tracks(6);
        let effects = update(&mut app, Message::TrackPressed(Listing::Library, 3));
        let position = audio::Event::Position {
            ticket: loaded(&effects).0,
            position: Duration::from_secs(42),
        };
        let _ = update(&mut app, Message::Player(position));
        let _ = update(&mut app, Message::ShufflePressed);
        let _ = update(&mut app, Message::RepeatPressed);
        let _ = update(&mut app, Message::VolumeChanged(25));
        let order = queued_ids(&app);
        let effects = update(&mut app, Message::Closing);
        let [Effect::FlushSession(session)] = &effects[..] else {
            panic!("closing saves the session");
        };

        // The next launch: the session first, then the library.
        let mut next = Model::default();
        next.restore(session.clone(), 99);
        assert_eq!(next.playback.volume(), 25);
        assert_eq!(queued_ids(&next), order);
        // Before the library tells anything, nothing of it is lost.
        assert!(update(&mut next, Message::ShufflePressed).iter().all(
            |effect| !matches!(effect, Effect::SaveSession(session) if session.position_ms != 42_000)
        ));
        let _ = update(&mut next, Message::ShufflePressed);

        let library = (1..=6).map(album_track).collect();
        let effects = update(
            &mut next,
            Message::LibraryEvent(LibraryEvent::Loaded(library)),
        );

        // Loaded where it was, paused, with the next track behind it.
        let cued = commands(&effects)
            .into_iter()
            .find_map(|command| match command {
                audio::Command::Cue { path, position, .. } => Some((path.clone(), *position)),
                _ => None,
            })
            .expect("the current track is cued");
        assert_eq!(cued, (track_path(3), Duration::from_secs(42)));
        assert_eq!(next.playback.transport(), playback::Transport::Paused);
        assert_eq!(next.playback.position_ms(), 42_000);
        assert_eq!(current_id(&next), Some(3));
        assert!(next.queue.shuffled());
        assert_eq!(next.queue.repeat(), queue::Repeat::All);
        let behind = queued_ids(&next)[next.queue.current_index().unwrap() + 1];
        assert_eq!(queued(&effects).unwrap().1, track_path(behind));
        assert!(
            commands(&effects)
                .iter()
                .all(|command| !matches!(command, audio::Command::Load { .. }))
        );
        // Play goes on from there.
        let effects = update(&mut next, Message::PlayPausePressed);
        assert_eq!(effects, [Effect::Player(audio::Command::Play)]);
    }

    #[test]
    fn a_launch_whose_current_track_is_gone_or_fails_stays_quiet() {
        let session = session::Session {
            volume: 50,
            repeat: queue::Repeat::Off,
            order: vec![1, 2, 3],
            unshuffled: None,
            current: Some(1),
            position_ms: 5_000,
        };

        // Absent (its drive is unplugged): nothing is loaded, and the
        // queue is kept for when it is back.
        let mut app = Model::default();
        app.restore(session.clone(), 1);
        let mut library: Vec<Track> = (1..=3).map(album_track).collect();
        library[1].present = false;
        let effects = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Loaded(library)),
        );
        assert!(commands(&effects).is_empty());
        assert_eq!(queued_ids(&app), [1, 2, 3]);
        assert_eq!(app.queue.current_index(), Some(1));

        // Unreadable: it fails to load, and nothing else starts.
        let mut app = Model::default();
        app.restore(session, 1);
        let library = (1..=3).map(album_track).collect();
        let effects = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Loaded(library)),
        );
        let Some(audio::Command::Cue { ticket, .. }) = commands(&effects).first().copied() else {
            panic!("the current track is cued");
        };
        let failed = audio::Event::Failed {
            ticket: *ticket,
            error: "unreadable".to_owned(),
        };
        let effects = update(&mut app, Message::Player(failed));
        assert_eq!(commands(&effects), [&audio::Command::Stop]);
        assert_eq!(app.playback.transport(), playback::Transport::Stopped);
    }

    /// What the player has, going by what it was told: the test's stand-in
    /// for the engine.
    #[derive(Default)]
    struct Engine {
        /// The ticket and file of what is loaded.
        loaded: Option<(u64, PathBuf)>,
        /// The ticket and file queued behind it.
        queued: Option<(u64, PathBuf)>,
        /// Files that cannot be opened.
        broken: Vec<PathBuf>,
        /// What it has to report of what it was told.
        reports: Vec<audio::Event>,
        /// The next change of the queued file comes too late: it has
        /// gone on to the one queued before.
        racing: bool,
    }

    impl Engine {
        fn take(&mut self, effects: &[Effect]) {
            for effect in effects {
                let Effect::Player(command) = effect else {
                    continue;
                };
                match command {
                    audio::Command::Load { ticket, path }
                    | audio::Command::Cue { ticket, path, .. } => {
                        self.queued = None;
                        if self.broken.contains(path) {
                            self.loaded = None;
                            self.reports.push(audio::Event::Failed {
                                ticket: *ticket,
                                error: "broken".to_owned(),
                            });
                        } else {
                            self.loaded = Some((*ticket, path.clone()));
                        }
                    }
                    audio::Command::Queue { ticket, path } => {
                        self.go_on_if_racing();
                        if self.loaded.is_some() {
                            self.queued = Some((*ticket, path.clone()));
                        }
                    }
                    audio::Command::Unqueue => {
                        self.go_on_if_racing();
                        self.queued = None;
                    }
                    audio::Command::Seek { ticket, .. } => {
                        if let Some(loaded) = &mut self.loaded {
                            loaded.0 = *ticket;
                        }
                    }
                    audio::Command::Stop => {
                        self.loaded = None;
                        self.queued = None;
                    }
                    audio::Command::Play | audio::Command::Pause | audio::Command::Volume(_) => {}
                }
            }
        }

        fn go_on_if_racing(&mut self) {
            if std::mem::take(&mut self.racing)
                && let Some((ticket, path)) = self.queued.take()
            {
                self.loaded = Some((ticket, path));
                self.reports.push(audio::Event::Started {
                    ticket,
                    duration: None,
                });
            }
        }

        /// The loaded track plays to its end.
        fn end_track(&mut self) {
            match (self.queued.take(), &self.loaded) {
                (Some((ticket, path)), _) => {
                    self.loaded = Some((ticket, path));
                    self.reports.push(audio::Event::Started {
                        ticket,
                        duration: None,
                    });
                }
                (None, Some((ticket, _))) => {
                    self.reports
                        .push(audio::Event::Finished { ticket: *ticket });
                }
                (None, None) => {}
            }
        }

        /// The loaded track breaks off (its output is gone).
        fn fail(&mut self) {
            if let Some((ticket, _)) = self.loaded.take() {
                self.queued = None;
                self.reports.push(audio::Event::Failed {
                    ticket,
                    error: "broke off".to_owned(),
                });
            }
        }
    }

    /// A random number below `below` (xorshift64).
    fn random(seed: &mut u64, below: usize) -> usize {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        (*seed % below as u64) as usize
    }

    /// The invariant that keeps playback gapless: whatever happens, the
    /// file queued in the player is the track the queue says comes next,
    /// or nothing when there is none.
    #[test]
    fn the_player_always_has_queued_what_the_queue_says_comes_next() {
        const TRACKS: i64 = 12;

        for run in 1..=40_u64 {
            let mut seed = run.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut app = Model {
                queue: queue::State::seeded(seed),
                ..Model::default()
            };
            let _ = app.set_tracks((1..=TRACKS).map(album_track).collect());
            let mut engine = Engine::default();
            let mut history = Vec::new();

            for step in 0..600 {
                let track = random(&mut seed, TRACKS as usize) as i64 + 1;
                let entry = (!app.queue.is_empty()).then(|| {
                    let entries = app.queue.entries();
                    entries[random(&mut seed, entries.len())].key
                });
                let message = match random(&mut seed, 24) {
                    0 => Message::TrackPressed(Listing::Library, track),
                    1 => Message::PlayNextPressed(track),
                    2 => Message::AddToQueuePressed(track),
                    3 => Message::QueueRemovePressed(entry.unwrap_or(0)),
                    4 => Message::QueueMovePressed {
                        key: entry.unwrap_or(0),
                        to: random(&mut seed, 14),
                    },
                    5 => Message::QueueEntryPressed(entry.unwrap_or(0)),
                    6 => Message::ShufflePressed,
                    7 => Message::RepeatPressed,
                    8 => Message::NextPressed,
                    9 => Message::PreviousPressed,
                    10 => Message::PlayPausePressed,
                    11 => Message::PositionChanged(random(&mut seed, 9_000) as u64),
                    12 => match random(&mut seed, 6) {
                        0 => Message::QueueClearPressed,
                        1 => Message::SearchChanged(
                            ["", "1", "track 00"][random(&mut seed, 3)].into(),
                        ),
                        2 => Message::SortChanged(SortColumn::Title),
                        _ => Message::VolumeChanged(random(&mut seed, 101) as u8),
                    },
                    // The library: a track goes, comes back, or is renamed.
                    13 | 14 => {
                        let mut changed = app.tracks[app.track_positions[&track]].clone();
                        if random(&mut seed, 3) == 0 {
                            changed.path = PathBuf::from(format!("/music/moved/{step}.flac"));
                        } else {
                            changed.present = !changed.present;
                        }
                        Message::LibraryEvent(LibraryEvent::Changed(vec![changed]))
                    }
                    // The player: a file breaks, the track breaks off, a
                    // change of the queued file comes too late.
                    15 => {
                        let path = app.tracks[app.track_positions[&track]].path.clone();
                        match engine.broken.iter().position(|broken| *broken == path) {
                            Some(at) => drop(engine.broken.remove(at)),
                            None => engine.broken.push(path),
                        }
                        continue;
                    }
                    // (A closing window asks nothing of the player: it only
                    // carries what the player reports into the model.)
                    16 => {
                        engine.fail();
                        Message::Closing
                    }
                    17 => {
                        engine.racing = true;
                        continue;
                    }
                    // Mostly, tracks play to their ends.
                    _ => {
                        engine.end_track();
                        Message::Closing
                    }
                };

                // The message, then whatever the player reports of it.
                history.push(format!("{message:?}"));
                let effects = update(&mut app, message);
                engine.take(&effects);
                while !engine.reports.is_empty() {
                    for event in std::mem::take(&mut engine.reports) {
                        history.push(format!("{event:?}"));
                        let effects = update(&mut app, Message::Player(event));
                        engine.take(&effects);
                    }
                }

                let stopped = app.playback.transport() == playback::Transport::Stopped;
                let upcoming = (!stopped)
                    .then(|| app.queue.upcoming(|id| app.can_play(id)))
                    .flatten()
                    .map(|index| {
                        let id = app.queue.entries()[index].track_id;
                        app.tracks[app.track_positions[&id]].path.clone()
                    });
                let told = || {
                    let from = history.len().saturating_sub(12);
                    format!(
                        "run {run}, step {step}, after:\n{}",
                        history[from..].join("\n")
                    )
                };
                assert_eq!(
                    engine.queued.as_ref().map(|(_, path)| path),
                    upcoming.as_ref(),
                    "{}",
                    told()
                );
                assert_eq!(engine.loaded.is_some(), !stopped, "{}", told());
                // What plays is the queue's current track, where it was
                // when it was loaded.
                if let Some((_, loaded)) = &engine.loaded {
                    let current = app.queue.current().expect("a loaded track is current");
                    let track = &app.tracks[app.track_positions[&current.track_id]];
                    let renamed = track.path.starts_with("/music/moved");
                    assert!(*loaded == track.path || renamed, "{}", told());
                }
            }
        }
    }

    #[test]
    fn the_settings_tab_chooses_shuffle_repeat_and_its_category() {
        let mut app = app_with_tracks(10);
        let _ = update(&mut app, Message::TrackPressed(Listing::Library, 1));
        let _ = update(&mut app, Message::ShuffleChosen(true));
        assert!(app.queue.shuffled());
        // Chosen again it stays on: a choice, not a toggle.
        let _ = update(&mut app, Message::ShuffleChosen(true));
        assert!(app.queue.shuffled());
        let _ = update(&mut app, Message::ShuffleChosen(false));
        assert!(!app.queue.shuffled());

        let _ = update(&mut app, Message::RepeatChosen(queue::Repeat::One));
        assert_eq!(app.queue.repeat(), queue::Repeat::One);

        assert_eq!(app.settings_category, settings_tab::Category::Library);
        let _ = update(
            &mut app,
            Message::SettingsCategoryPressed(settings_tab::Category::Playback),
        );
        assert_eq!(app.settings_category, settings_tab::Category::Playback);
    }

    #[test]
    fn choosing_a_theme_applies_it_once() {
        let mut app = Model::default();
        assert_eq!(app.theme, crate::theme::Id::default());
        let effects = update(&mut app, Message::ThemeChosen(crate::theme::Id::Dracula));
        assert!(effects.contains(&Effect::ApplyTheme(crate::theme::Id::Dracula)));
        assert_eq!(app.theme, crate::theme::Id::Dracula);
        let effects = update(&mut app, Message::ThemeChosen(crate::theme::Id::Dracula));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::ApplyTheme(_)))
        );
    }

    fn playlist(id: i64, name: &str, tracks: &[i64]) -> crate::library::Playlist {
        crate::library::Playlist {
            id,
            name: name.to_owned(),
            tracks: tracks.to_vec(),
        }
    }

    /// What `effects` ask of the library.
    fn library_commands(effects: &[Effect]) -> Vec<&LibraryCommand> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Library(command) => Some(command),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_prompt_makes_a_playlist_of_the_track_once_it_has_a_name() {
        let mut app = app_with_three_tracks();
        let _ = update(&mut app, Message::NewPlaylistPressed(vec![2]));
        let _ = update(&mut app, Message::PromptNameChanged("   ".into()));
        // A name of nothing but spaces makes nothing, and the prompt stays.
        assert!(library_commands(&update(&mut app, Message::PromptConfirmed)).is_empty());
        assert!(app.playlists.prompt.is_some());

        let _ = update(&mut app, Message::PromptNameChanged(" Road Trip ".into()));
        let effects = update(&mut app, Message::PromptConfirmed);
        assert_eq!(
            library_commands(&effects),
            [&LibraryCommand::CreatePlaylist {
                name: "Road Trip".into(),
                tracks: vec![2],
            }]
        );
        assert!(app.playlists.prompt.is_none());

        let _ = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Playlists(vec![playlist(
                7,
                "Road Trip",
                &[2],
            )])),
        );
        let _ = update(&mut app, Message::RenamePlaylistPressed(7));
        assert_eq!(
            app.playlists
                .prompt
                .as_ref()
                .map(|prompt| prompt.name.as_str()),
            Some("Road Trip")
        );
        let _ = update(&mut app, Message::PromptCancelled);
        assert!(app.playlists.prompt.is_none());
    }

    #[test]
    fn a_track_goes_into_a_playlist_and_out_of_the_one_shown() {
        let mut app = app_with_three_tracks();
        let effects = update(
            &mut app,
            Message::AddToPlaylistPressed {
                playlist: 7,
                track: 3,
            },
        );
        assert_eq!(
            library_commands(&effects),
            [&LibraryCommand::AddToPlaylist {
                playlist: 7,
                tracks: vec![3],
            }]
        );
        // A track the library does not know goes nowhere.
        let effects = update(
            &mut app,
            Message::AddToPlaylistPressed {
                playlist: 7,
                track: 99,
            },
        );
        assert!(library_commands(&effects).is_empty());

        let _ = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Playlists(vec![
                playlist(7, "A", &[3]),
                playlist(8, "B", &[1]),
            ])),
        );
        let _ = update(&mut app, Message::PlaylistChosen(8));
        let effects = update(&mut app, Message::RemoveFromPlaylistPressed(1));
        assert_eq!(
            library_commands(&effects),
            [&LibraryCommand::RemoveFromPlaylist {
                playlist: 8,
                track: 1,
            }]
        );
    }

    #[test]
    fn the_shown_playlist_lists_its_tracks_in_its_order_with_its_first_tracks_artwork() {
        let mut app = app_with_three_tracks();
        let effects = update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Playlists(vec![
                playlist(7, "A", &[3, 1, 99]),
                playlist(8, "B", &[]),
            ])),
        );
        let listed = |app: &Model| app.listed_tracks(Listing::Playlist);
        assert_eq!(listed(&app), [3, 1]);
        let request = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::LoadPlaylistArtwork(request) => Some(request.clone()),
                _ => None,
            })
            .expect("the first track's artwork is read");
        assert_eq!(request.path, track_path(3));
        let _ = update(
            &mut app,
            Message::PlaylistArtworkLoaded {
                request_id: request.request_id,
                result: Ok(Some("data:art".into())),
            },
        );
        assert_eq!(
            app.playlists.art().map(|art| art.to_string()),
            Some("data:art".to_owned())
        );

        let _ = update(&mut app, Message::SearchChanged("Track 001".into()));
        assert_eq!(listed(&app), [1]);
        let _ = update(&mut app, Message::SearchChanged(String::new()));

        // Pressed, its track plays with the playlist as the queue.
        let _ = update(&mut app, Message::TrackPressed(Listing::Playlist, 1));
        assert_eq!(queued_ids(&app), [3, 1]);
        assert_eq!(current_id(&app), Some(1));

        let effects = update(&mut app, Message::PlaylistChosen(8));
        assert!(effects.contains(&Effect::ScrollToTop(Listing::Playlist)));
        assert!(listed(&app).is_empty());
        assert!(app.playlists.art().is_none());
        let summary = app.playlist_summary().unwrap();
        assert_eq!((summary.tracks, summary.missing), (0, 0));
    }
}

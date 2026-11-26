//! The app's store: the [`Model`] in one signal, the only writer of it, and
//! the effects its updates ask for.

use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;
use tokio::sync::mpsc::unbounded_channel;

use super::{Effect, Listing, Message, Model, playback, playlists, session, update};
use crate::audio;
use crate::library::{self, Library};

#[derive(Clone, Copy)]
pub(crate) struct Store {
    model: Signal<Model>,
    /// Each listing's list, which an update may scroll.
    lists: [ElementRef; Listing::COUNT],
    player: Stored<audio::Player>,
    library: Stored<Library>,
    session: Stored<session::Writer>,
}

impl Store {
    /// Made once, by the root: restores the workspace and the session,
    /// and starts the player and the library.
    pub(crate) fn new() -> Self {
        let (events, mut reports) = unbounded_channel();
        let (library_events, mut library_reports) = unbounded_channel();
        let model = Model::restored();
        // The theme the last run was in, before anything is drawn.
        crate::theme::apply(model.theme);
        let player = audio::Player::spawn(move |event| events.send(event).is_ok());
        // The volume the last run was at, before anything sounds.
        player.send(audio::Command::Volume(model.playback.volume()));
        let store = Self {
            model: signal_with(model, SignalEquals::Never),
            lists: Listing::ALL.map(|_| element_ref()),
            player: stored(player),
            library: stored(Library::spawn(move |event| {
                library_events.send(event).is_ok()
            })),
            session: stored(session::Writer::spawn()),
        };
        spawn(async move {
            while let Some(event) = reports.recv().await {
                store.dispatch(Message::Player(event));
            }
        });
        spawn(async move {
            while let Some(event) = library_reports.recv().await {
                store.dispatch(Message::LibraryEvent(event));
            }
        });
        super::input::bind(move |message| store.dispatch(Message::Tabs(message)));
        store
    }

    /// The handle of `listing`'s list, which an update may scroll.
    pub(super) fn list(&self, listing: Listing) -> ElementRef {
        self.lists[listing.index()]
    }

    /// A derived value of the model; its readers run again only when it
    /// changes.
    pub(super) fn select<T: PartialEq + 'static>(
        &self,
        read: impl Fn(&Model) -> T + 'static,
    ) -> Memo<T> {
        let model = self.model;
        computed(move || model.with(&read))
    }

    /// The window is about to close: the session is saved before it does.
    pub(crate) fn closing(&self) {
        self.dispatch(Message::Closing);
    }

    pub(super) fn dispatch(&self, message: Message) {
        let mut effects = Vec::new();
        self.model.update(|model| effects = update(model, message));
        for effect in effects {
            match effect {
                Effect::LoadArtwork(playback::ArtworkRequest { request_id, path }) => {
                    self.load_artwork(path, move |result| Message::ArtworkLoaded {
                        request_id,
                        result,
                    });
                }
                Effect::LoadPlaylistArtwork(playlists::ArtworkRequest { request_id, path }) => {
                    self.load_artwork(path, move |result| Message::PlaylistArtworkLoaded {
                        request_id,
                        result,
                    });
                }
                Effect::ScrollToTop(listing) => {
                    self.list(listing).scroll_to_offset(px(0.0), false);
                }
                Effect::Player(command) => self.player.with(|player| player.send(command)),
                Effect::PickFolder => self.pick_folder(),
                Effect::Library(command) => self.library.with(|library| library.send(command)),
                Effect::SaveSession(session) => self.session.with(|writer| writer.save(session)),
                Effect::FlushSession(session) => {
                    self.session.with(|writer| writer.flush(session));
                }
                Effect::ApplyTheme(theme) => {
                    crate::theme::apply(theme);
                    crate::theme::save(theme);
                }
                Effect::SaveListenPercent(percent) => super::listening::save(percent),
            }
        }
    }

    /// Reads the artwork embedded in the file at `path` off the UI thread,
    /// and tells what came of it in the message `done` makes.
    fn load_artwork(
        &self,
        path: std::path::PathBuf,
        done: impl FnOnce(Result<Option<SharedString>, String>) -> Message + 'static,
    ) {
        let store = *self;
        spawn(async move {
            let result = blocking(move || {
                library::load_embedded_data_uri(&path)
                    .map(|uri| uri.map(SharedString::from))
                    .map_err(|error| error.to_string())
            })
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
            store.dispatch(done(result));
        });
    }

    /// Asks the user for folders to add to the library, in the system's
    /// dialog.
    fn pick_folder(&self) {
        let store = *self;
        let dialog = OpenDialog::folders()
            .multiple(true)
            .title("Add folder to library")
            .button_label("Add folder");
        spawn(async move {
            match pick_paths(dialog).await {
                Ok(picked) => {
                    for path in picked.into_iter().flatten() {
                        store.dispatch(Message::FolderPicked(path));
                    }
                }
                Err(error) => eprintln!("Folder dialog failed: {error}"),
            }
        });
    }
}

pub(super) fn store() -> Store {
    use_context::<Store>().expect("the root provides the store")
}

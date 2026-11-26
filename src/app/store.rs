//! The app's store: the [`Model`] in one signal, the only writer of it, and
//! the effects its updates ask for.

use std::sync::mpsc;

use nsh_core::prelude::*;

use super::{Effect, Message, Model, playback, update};
use crate::library::{self, LibraryEvent};

#[derive(Clone, Copy)]
pub(crate) struct Store {
    model: Signal<Model>,
    library_list: ElementRef,
}

impl Store {
    /// Made once, by the root: restores the workspace and starts indexing.
    pub(crate) fn new() -> Self {
        let store = Self {
            model: signal_with(Model::restored(), SignalEquals::Never),
            library_list: element_ref(),
        };
        store.index_library();
        super::input::bind(move |message| store.dispatch(Message::Tabs(message)));
        store
    }

    /// The handle of the library's list, which an update may scroll.
    pub(super) fn library_list(&self) -> ElementRef {
        self.library_list
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

    pub(super) fn dispatch(&self, message: Message) {
        let mut effect = Effect::None;
        self.model.update(|model| effect = update(model, message));
        match effect {
            Effect::None => {}
            Effect::LoadArtwork(request) => self.load_artwork(request),
            Effect::ScrollLibraryToTop => self.library_list.scroll_to_offset(px(0.0), false),
        }
    }

    fn load_artwork(&self, request: playback::ArtworkRequest) {
        let store = *self;
        let playback::ArtworkRequest { request_id, path } = request;
        spawn(async move {
            let result = blocking(move || {
                library::load_embedded_data_uri(&path)
                    .map(|uri| uri.map(SharedString::from))
                    .map_err(|error| error.to_string())
            })
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
            store.dispatch(Message::ArtworkLoaded { request_id, result });
        });
    }

    /// Indexes the library on a thread of its own and applies each event as
    /// it arrives.
    fn index_library(&self) {
        let store = *self;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || library::index(|event| sender.send(event).is_ok()));
        spawn(async move {
            let mut receiver = receiver;
            loop {
                let received = blocking(move || {
                    let event = receiver.recv();
                    (receiver, event)
                })
                .await;
                let Ok((back, Ok(event))) = received else {
                    break;
                };
                receiver = back;
                let done = matches!(event, LibraryEvent::Complete | LibraryEvent::Failed(_));
                store.dispatch(Message::LibraryEvent(event));
                if done {
                    break;
                }
            }
        });
    }
}

pub(super) fn store() -> Store {
    use_context::<Store>().expect("the root provides the store")
}

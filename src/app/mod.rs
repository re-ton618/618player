mod input;
mod playback;
mod tab_store;
mod tabs;
mod view;

use std::collections::HashMap;

use iced::widget::{image, scrollable};
use iced::{Element, Size, Task, Theme, window};

use crate::library::{self, LibraryEvent, SortColumn, SortDirection, Track};

pub struct App {
    tracks: Vec<Track>,
    track_positions: HashMap<i64, usize>,
    visible_tracks: Vec<usize>,
    search_query: String,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    scroll_offset: f32,
    library_viewport_height: f32,
    tabs: tabs::State,
    playback: playback::State,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            track_positions: HashMap::new(),
            visible_tracks: Vec::new(),
            search_query: String::new(),
            sort_column: SortColumn::Title,
            sort_direction: SortDirection::Ascending,
            scroll_offset: 0.0,
            library_viewport_height: view::INITIAL_LIBRARY_VIEWPORT_HEIGHT,
            tabs: tabs::State::default(),
            playback: playback::State::default(),
        }
    }
}

#[allow(private_interfaces)]
#[derive(Debug, Clone)]
pub enum Message {
    LibraryEvent(LibraryEvent),
    SearchChanged(String),
    SortChanged(SortColumn),
    TrackPressed(i64),
    ArtworkLoaded {
        request_id: u64,
        result: Result<Option<image::Handle>, String>,
    },
    ArtworkAllocated {
        request_id: u64,
        result: Result<image::Allocation, image::Error>,
    },
    LibraryScrolled(scrollable::Viewport),
    LibraryViewportResized(Size),
    WindowDragged,
    WindowResize(window::Direction),
    WindowMinimized,
    WindowMaximized,
    WindowClosed,
    PlaybackControlPressed,
    FileMenuPressed,
    VolumeChanged(u8),
    Tabs(tabs::Message),
}

pub fn new() -> (App, Task<Message>) {
    let mut app = App::default();
    match tab_store::load() {
        Ok(Some(tabs)) => app.tabs = tabs,
        Ok(None) => {}
        Err(error) => eprintln!("Workspace restore failed: {error}"),
    }
    (app, Task::run(library::events(), Message::LibraryEvent))
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::LibraryEvent(event) => match event {
            LibraryEvent::Cached(tracks) | LibraryEvent::Reconciled(tracks) => {
                if let Some(request) = app.set_tracks(tracks) {
                    return load_artwork(request);
                }
            }
            LibraryEvent::MetadataBatch(updates) => {
                let current_track_id = app.playback.current_track_id();
                let mut current_track_updated = false;

                for update in updates {
                    let update_id = update.id;
                    if let Some(&position) = app.track_positions.get(&update_id) {
                        app.tracks[position].apply(update);
                        current_track_updated |= Some(update_id) == current_track_id;
                    }
                }

                if current_track_updated && let Some(request) = app.reconcile_current_track() {
                    return load_artwork(request);
                }
            }
            LibraryEvent::Complete => {
                app.rebuild_projection();
            }
            LibraryEvent::Failed(error) => {
                eprintln!("Library indexing failed: {error}");
            }
        },
        Message::SearchChanged(query) => {
            app.search_query = query;
            app.rebuild_projection();
            app.scroll_offset = 0.0;
            return iced::widget::operation::snap_to(
                view::library_scroll_id(),
                iced::widget::operation::RelativeOffset::START,
            );
        }
        Message::SortChanged(column) => {
            if app.sort_column == column {
                app.sort_direction = app.sort_direction.toggled();
            } else {
                app.sort_column = column;
                app.sort_direction = SortDirection::Ascending;
            }
            app.rebuild_projection();
            app.scroll_offset = 0.0;
            return iced::widget::operation::snap_to(
                view::library_scroll_id(),
                iced::widget::operation::RelativeOffset::START,
            );
        }
        Message::TrackPressed(track_id) => {
            let Some(&position) = app.track_positions.get(&track_id) else {
                return Task::none();
            };
            if let Some(request) = app.playback.select(&app.tracks[position]) {
                return load_artwork(request);
            }
        }
        Message::ArtworkLoaded { request_id, result } => {
            return apply_artwork_effect(app.playback.loaded(request_id, result), request_id);
        }
        Message::ArtworkAllocated { request_id, result } => {
            return apply_artwork_effect(app.playback.allocated(request_id, result), request_id);
        }
        Message::LibraryScrolled(viewport) => {
            app.scroll_offset = viewport.absolute_offset().y;
            app.library_viewport_height = viewport.bounds().height;
        }
        Message::LibraryViewportResized(size) => app.library_viewport_height = size.height,
        Message::WindowDragged => {
            return window::oldest().then(|id| match id {
                Some(id) => window::drag(id),
                None => Task::none(),
            });
        }
        Message::WindowResize(direction) => {
            return window::oldest().then(move |id| match id {
                Some(id) => window::drag_resize(id, direction),
                None => Task::none(),
            });
        }
        Message::WindowMinimized => {
            return window::oldest().then(|id| match id {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            });
        }
        Message::WindowMaximized => {
            return window::oldest().then(|id| match id {
                Some(id) => window::toggle_maximize(id),
                None => Task::none(),
            });
        }
        Message::WindowClosed => {
            if let Err(error) = tab_store::save(&app.tabs) {
                eprintln!("Workspace save failed: {error}");
            }
            return window::oldest().then(|id| match id {
                Some(id) => window::close(id),
                None => Task::none(),
            });
        }
        Message::PlaybackControlPressed | Message::FileMenuPressed => {}
        Message::VolumeChanged(volume) => app.playback.set_volume(volume),
        Message::Tabs(message) => {
            let outcome = app.tabs.update(message);
            if outcome.persist
                && let Err(error) = tab_store::save(&app.tabs)
            {
                eprintln!("Workspace save failed: {error}");
            }
            if outcome.library_activated {
                return iced::widget::operation::scroll_to(
                    view::library_scroll_id(),
                    iced::widget::operation::AbsoluteOffset {
                        x: 0.0,
                        y: app.scroll_offset,
                    },
                );
            }
        }
    }

    Task::none()
}

fn load_artwork(request: playback::ArtworkRequest) -> Task<Message> {
    let playback::ArtworkRequest { request_id, path } = request;
    Task::perform(
        async move {
            library::load_embedded(&path)
                .map(|bytes| bytes.map(image::Handle::from_bytes))
                .map_err(|error| error.to_string())
        },
        move |result| Message::ArtworkLoaded { request_id, result },
    )
}

fn apply_artwork_effect(effect: playback::ArtworkEffect, request_id: u64) -> Task<Message> {
    match effect {
        playback::ArtworkEffect::None => Task::none(),
        playback::ArtworkEffect::Allocate(handle) => image::allocate(handle.clone())
            .map(move |result| Message::ArtworkAllocated { request_id, result }),
        playback::ArtworkEffect::Load { request, error } => {
            if let Some(error) = error {
                eprintln!("Album artwork failed: {error}");
            }
            load_artwork(request)
        }
        playback::ArtworkEffect::Missing { error } => {
            if let Some(error) = error {
                eprintln!("Album artwork failed: {error}");
            }
            Task::none()
        }
    }
}

impl App {
    fn set_tracks(&mut self, tracks: Vec<Track>) -> Option<playback::ArtworkRequest> {
        self.tracks = tracks;
        self.track_positions = self
            .tracks
            .iter()
            .enumerate()
            .map(|(position, track)| (track.id, position))
            .collect();
        self.rebuild_projection();
        self.reconcile_current_track()
    }

    fn reconcile_current_track(&mut self) -> Option<playback::ArtworkRequest> {
        let track_id = self.playback.current_track_id()?;
        let Some(&position) = self.track_positions.get(&track_id) else {
            self.playback.clear();
            return None;
        };

        self.playback.select(&self.tracks[position])
    }

    pub(crate) fn current_track(&self) -> Option<&Track> {
        self.playback
            .current_track_id()
            .and_then(|track_id| self.track_positions.get(&track_id))
            .map(|&position| &self.tracks[position])
    }

    fn rebuild_projection(&mut self) {
        self.visible_tracks = library::matching_track_indices(
            &self.tracks,
            &self.search_query,
            self.sort_column,
            self.sort_direction,
        );
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    view::view(app)
}

pub fn theme(_app: &App) -> Theme {
    crate::theme::active()
}

pub fn subscription(_app: &App) -> iced::Subscription<Message> {
    input::subscription()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::library::test_track;

    use super::*;

    #[test]
    fn unknown_track_press_preserves_current_track() {
        let track = test_track(1, PathBuf::from("/music/album/current.flac"), Some("album"));
        let mut app = App::default();
        assert!(app.set_tracks(vec![track]).is_none());
        drop(update(&mut app, Message::TrackPressed(1)));
        assert_eq!(app.current_track().map(|track| track.id), Some(1));

        drop(update(&mut app, Message::TrackPressed(999)));

        assert_eq!(app.current_track().map(|track| track.id), Some(1));
    }

    #[test]
    fn library_replacement_without_selected_id_clears_current_track() {
        let selected = test_track(
            1,
            PathBuf::from("/music/first/selected.flac"),
            Some("first"),
        );
        let replacement = test_track(
            2,
            PathBuf::from("/music/second/replacement.flac"),
            Some("second"),
        );
        let mut app = App::default();
        assert!(app.set_tracks(vec![selected]).is_none());
        drop(update(&mut app, Message::TrackPressed(1)));
        assert!(app.current_track().is_some());

        drop(update(
            &mut app,
            Message::LibraryEvent(LibraryEvent::Reconciled(vec![replacement])),
        ));

        assert!(app.current_track().is_none());
    }
}

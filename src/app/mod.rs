mod view;

use std::collections::HashMap;

use iced::widget::scrollable;
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
    library_height: f32,
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
            library_height: view::INITIAL_LIBRARY_HEIGHT,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    LibraryEvent(LibraryEvent),
    SearchChanged(String),
    SortChanged(SortColumn),
    TrackPressed(i64),
    Scrolled(scrollable::Viewport),
    Resized(Size),
    WindowDragged,
    WindowResize(window::Direction),
    WindowMinimized,
    WindowMaximized,
    WindowClosed,
    PlaybackControlPressed,
}

pub fn new() -> (App, Task<Message>) {
    (
        App::default(),
        Task::run(library::events(), Message::LibraryEvent),
    )
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::LibraryEvent(event) => match event {
            LibraryEvent::Cached(tracks) => {
                app.set_tracks(tracks);
            }
            LibraryEvent::Reconciled(tracks) => {
                app.set_tracks(tracks);
            }
            LibraryEvent::MetadataBatch(updates) => {
                for update in updates {
                    if let Some(&position) = app.track_positions.get(&update.id) {
                        app.tracks[position].apply(update);
                    }
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
        Message::TrackPressed(_track_id) => {}
        Message::Scrolled(viewport) => {
            app.scroll_offset = viewport.absolute_offset().y;
            app.library_height = viewport.bounds().height;
        }
        Message::Resized(size) => app.library_height = size.height,
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
            return window::oldest().then(|id| match id {
                Some(id) => window::close(id),
                None => Task::none(),
            });
        }
        Message::PlaybackControlPressed => {}
    }

    Task::none()
}

impl App {
    fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
        self.track_positions = self
            .tracks
            .iter()
            .enumerate()
            .map(|(position, track)| (track.id, position))
            .collect();
        self.rebuild_projection();
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

mod view;

use std::path::PathBuf;

use iced::widget::scrollable;
use iced::{Element, Size, Task, Theme, window};

use crate::library;

pub struct App {
    tracks: Vec<PathBuf>,
    visible_tracks: Vec<usize>,
    search_query: String,
    scroll_offset: f32,
    library_height: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            visible_tracks: Vec::new(),
            search_query: String::new(),
            scroll_offset: 0.0,
            library_height: view::INITIAL_LIBRARY_HEIGHT,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Scanned(Vec<PathBuf>),
    SearchChanged(String),
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
    (App::default(), scan_library())
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Scanned(tracks) => {
            app.tracks = tracks;
            app.visible_tracks = library::matching_track_indices(&app.tracks, &app.search_query);
            app.scroll_offset = 0.0;
        }
        Message::SearchChanged(query) => {
            app.search_query = query;
            app.visible_tracks = library::matching_track_indices(&app.tracks, &app.search_query);
            app.scroll_offset = 0.0;
        }
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

pub fn view(app: &App) -> Element<'_, Message> {
    view::view(app)
}

pub fn theme(_app: &App) -> Theme {
    crate::theme::active()
}

fn scan_library() -> Task<Message> {
    Task::perform(library::scan_music_directory(), Message::Scanned)
}

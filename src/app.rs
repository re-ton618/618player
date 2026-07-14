use std::ops::Range;
use std::path::PathBuf;

use iced::widget::{
    Column, button, column, container, row, rule, scrollable, sensor, space, text, text_input,
};
use iced::{Background, Border, Color, Element, Fill, Size, Task, Theme};

use crate::library;

const TOP_BAR_HEIGHT: f32 = 48.0;
const PLAYBACK_BAR_HEIGHT: f32 = 40.0;
const INITIAL_LIBRARY_HEIGHT: f32 = 640.0 - TOP_BAR_HEIGHT - PLAYBACK_BAR_HEIGHT - 2.0;
const ROW_HEIGHT: f32 = 32.0;
const OVERSCAN_ROWS: usize = 5;

pub struct App {
    tracks: Vec<PathBuf>,
    visible_tracks: Vec<usize>,
    search_query: String,
    scanning: bool,
    scroll_offset: f32,
    library_height: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            visible_tracks: Vec::new(),
            search_query: String::new(),
            scanning: false,
            scroll_offset: 0.0,
            library_height: INITIAL_LIBRARY_HEIGHT,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Scanned(Vec<PathBuf>),
    SearchChanged(String),
    ScanRequested,
    Scrolled(scrollable::Viewport),
    Resized(Size),
}

pub fn new() -> (App, Task<Message>) {
    let app = App {
        scanning: true,
        ..App::default()
    };

    (app, scan_library())
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Scanned(tracks) => {
            app.tracks = tracks;
            app.visible_tracks = filter_tracks(&app.tracks, &app.search_query);
            app.scanning = false;
            app.scroll_offset = 0.0;
        }
        Message::SearchChanged(query) => {
            app.search_query = query;
            app.visible_tracks = filter_tracks(&app.tracks, &app.search_query);
            app.scroll_offset = 0.0;
        }
        Message::ScanRequested if !app.scanning => {
            app.scanning = true;
            return scan_library();
        }
        Message::ScanRequested => {}
        Message::Scrolled(viewport) => {
            app.scroll_offset = viewport.absolute_offset().y;
            app.library_height = viewport.bounds().height;
        }
        Message::Resized(size) => app.library_height = size.height,
    }

    Task::none()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let track_count = app.visible_tracks.len();
    let visible = visible_range(track_count, app.scroll_offset, app.library_height);
    let mut rows = Column::new().width(Fill);

    if visible.start > 0 {
        rows = rows.push(space().height(visible.start as f32 * ROW_HEIGHT));
    }

    for &track_index in &app.visible_tracks[visible.clone()] {
        let path = &app.tracks[track_index];

        rows = rows
            .push(
                container(text(path.to_string_lossy()).size(15))
                    .width(Fill)
                    .height(ROW_HEIGHT - 1.0)
                    .padding([0, 16])
                    .center_y(Fill),
            )
            .push(rule::horizontal(1));
    }

    if visible.end < track_count {
        rows = rows.push(space().height((track_count - visible.end) as f32 * ROW_HEIGHT));
    }

    let library = scrollable(rows)
        .width(Fill)
        .height(Fill)
        .on_scroll(Message::Scrolled);

    let context = container(text("RUST MUSIC  /  LIBRARY").size(13))
        .width(180)
        .height(Fill)
        .padding([0, 16])
        .center_y(Fill);

    let search = text_input("Search library", &app.search_query)
        .on_input(Message::SearchChanged)
        .width(Fill)
        .size(14)
        .padding([8, 0])
        .style(search_style);

    let search_region = container(search)
        .width(Fill)
        .height(Fill)
        .padding([0, 12])
        .center_y(Fill);

    let count_label = if app.search_query.is_empty() {
        format!("{} TRACKS", app.tracks.len())
    } else {
        format!("{} / {}", app.visible_tracks.len(), app.tracks.len())
    };
    let track_count = container(text(count_label).size(12))
        .width(96)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill);

    let scan_label = if app.scanning { "SCANNING" } else { "RESCAN" };
    let mut scan = button(text(scan_label).size(12))
        .width(92)
        .height(Fill)
        .padding([0, 16])
        .style(button::text);

    if !app.scanning {
        scan = scan.on_press(Message::ScanRequested);
    }

    let top_bar = row![
        context,
        rule::vertical(1),
        search_region,
        rule::vertical(1),
        track_count,
        rule::vertical(1),
        scan,
    ]
    .width(Fill)
    .height(TOP_BAR_HEIGHT);

    let playback_bar = container(text("NOTHING PLAYING").size(13))
        .width(Fill)
        .height(PLAYBACK_BAR_HEIGHT)
        .padding([0, 16])
        .center_y(Fill);

    column![
        top_bar,
        rule::horizontal(1),
        sensor(library)
            .on_show(Message::Resized)
            .on_resize(Message::Resized),
        rule::horizontal(1),
        playback_bar
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

pub fn theme(_app: &App) -> Theme {
    Theme::Dark
}

fn scan_library() -> Task<Message> {
    Task::perform(library::scan_music_directory(), Message::Scanned)
}

fn search_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    style
}

fn filter_tracks(tracks: &[PathBuf], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();

    if query.is_empty() {
        return (0..tracks.len()).collect();
    }

    tracks
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            path.to_string_lossy()
                .to_lowercase()
                .contains(&query)
                .then_some(index)
        })
        .collect()
}

fn visible_range(track_count: usize, offset: f32, viewport_height: f32) -> Range<usize> {
    let first_visible = (offset / ROW_HEIGHT).floor() as usize;
    let start = first_visible.saturating_sub(OVERSCAN_ROWS).min(track_count);
    let visible_rows = (viewport_height / ROW_HEIGHT).ceil() as usize;
    let end = first_visible
        .saturating_add(visible_rows)
        .saturating_add(OVERSCAN_ROWS)
        .min(track_count);

    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_is_overscanned_and_bounded() {
        assert_eq!(visible_range(500, 320.0, 320.0), 5..25);
        assert_eq!(visible_range(8, 0.0, 640.0), 0..8);
        assert_eq!(visible_range(500, 15_900.0, 320.0), 491..500);
    }

    #[test]
    fn filtering_is_case_insensitive_and_preserves_library_order() {
        let tracks = [
            PathBuf::from("Artist/First.FLAC"),
            PathBuf::from("Other/second.mp3"),
            PathBuf::from("Artist/Third.wav"),
        ];

        assert_eq!(filter_tracks(&tracks, "artist"), vec![0, 2]);
        assert_eq!(filter_tracks(&tracks, "FLAC"), vec![0]);
        assert_eq!(filter_tracks(&tracks, "  "), vec![0, 1, 2]);
    }
}

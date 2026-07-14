use std::ops::Range;
use std::path::PathBuf;

use iced::widget::stack;
use iced::widget::{
    Column, button, column, container, mouse_area, row, rule, scrollable, sensor, space, text,
    text_input,
};
use iced::{
    Background, Border, Center, Color, Element, Fill, Font, Length, Shadow, Size, Task, Theme,
    Vector, font, theme, window,
};
use iced::{alignment, mouse};

use crate::library;

const TOP_BAR_HEIGHT: f32 = 56.0;
const PLAYBACK_BAR_HEIGHT: f32 = 56.0;
const DESKTOP_PADDING: f32 = 12.0;
const SECTION_GAP: f32 = 12.0;
const INITIAL_LIBRARY_HEIGHT: f32 =
    640.0 - TOP_BAR_HEIGHT - PLAYBACK_BAR_HEIGHT - DESKTOP_PADDING * 2.0 - SECTION_GAP * 2.0;
const ROW_HEIGHT: f32 = 32.0;
const OVERSCAN_ROWS: usize = 5;
const RESIZE_HANDLE_SIZE: f32 = 6.0;

const BACKGROUND: Color = Color::from_rgb8(13, 14, 16);
const SURFACE: Color = Color::from_rgb8(23, 25, 28);
const SURFACE_HOVERED: Color = Color::from_rgb8(34, 37, 41);
const DIVIDER: Color = Color::from_rgb8(52, 55, 61);
const TEXT: Color = Color::from_rgb8(239, 237, 231);
const MUTED: Color = Color::from_rgb8(143, 147, 154);
const ACCENT: Color = Color::from_rgb8(211, 235, 111);
const DANGER: Color = Color::from_rgb8(210, 70, 76);
const STRONG_FONT: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};
const ICON_FONT: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::MONOSPACE
};

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
            library_height: INITIAL_LIBRARY_HEIGHT,
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
            app.visible_tracks = filter_tracks(&app.tracks, &app.search_query);
            app.scroll_offset = 0.0;
        }
        Message::SearchChanged(query) => {
            app.search_query = query;
            app.visible_tracks = filter_tracks(&app.tracks, &app.search_query);
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
            .push(rule::horizontal(1).style(divider_style));
    }

    if visible.end < track_count {
        rows = rows.push(space().height((track_count - visible.end) as f32 * ROW_HEIGHT));
    }

    let library = scrollable(rows)
        .width(Fill)
        .height(Fill)
        .on_scroll(Message::Scrolled)
        .style(scrollable_style);

    let library_section = container(
        sensor(library)
            .on_show(Message::Resized)
            .on_resize(Message::Resized),
    )
    .width(Fill)
    .height(Fill)
    .style(section_style);

    let content = container(
        column![top_bar(app), library_section, playback_bar()]
            .spacing(SECTION_GAP)
            .width(Fill)
            .height(Fill),
    )
    .padding(DESKTOP_PADDING)
    .width(Fill)
    .height(Fill)
    .style(root_style)
    .into();

    if cfg!(target_os = "windows") {
        resize_frame(content)
    } else {
        content
    }
}

fn resize_frame(content: Element<'_, Message>) -> Element<'_, Message> {
    use window::Direction;

    stack([
        content,
        resize_handle(Direction::North),
        resize_handle(Direction::South),
        resize_handle(Direction::East),
        resize_handle(Direction::West),
        resize_handle(Direction::NorthEast),
        resize_handle(Direction::NorthWest),
        resize_handle(Direction::SouthEast),
        resize_handle(Direction::SouthWest),
    ])
    .width(Fill)
    .height(Fill)
    .into()
}

fn resize_handle(direction: window::Direction) -> Element<'static, Message> {
    use alignment::{Horizontal, Vertical};
    use mouse::Interaction;
    use window::Direction;

    let (width, height, horizontal, vertical, interaction) = match direction {
        Direction::North => (
            Length::Fill,
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Center,
            Vertical::Top,
            Interaction::ResizingVertically,
        ),
        Direction::South => (
            Length::Fill,
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Center,
            Vertical::Bottom,
            Interaction::ResizingVertically,
        ),
        Direction::East => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fill,
            Horizontal::Right,
            Vertical::Center,
            Interaction::ResizingHorizontally,
        ),
        Direction::West => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fill,
            Horizontal::Left,
            Vertical::Center,
            Interaction::ResizingHorizontally,
        ),
        Direction::NorthEast => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Right,
            Vertical::Top,
            Interaction::ResizingDiagonallyUp,
        ),
        Direction::NorthWest => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Left,
            Vertical::Top,
            Interaction::ResizingDiagonallyDown,
        ),
        Direction::SouthEast => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Right,
            Vertical::Bottom,
            Interaction::ResizingDiagonallyDown,
        ),
        Direction::SouthWest => (
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Length::Fixed(RESIZE_HANDLE_SIZE),
            Horizontal::Left,
            Vertical::Bottom,
            Interaction::ResizingDiagonallyUp,
        ),
    };

    container(
        mouse_area(space().width(width).height(height))
            .on_press(Message::WindowResize(direction))
            .interaction(interaction),
    )
    .width(Fill)
    .height(Fill)
    .align_x(horizontal)
    .align_y(vertical)
    .into()
}

fn playback_bar() -> Element<'static, Message> {
    let controls = row![
        transport_button("|<", transport_button_style),
        rule::vertical(1).style(divider_style),
        transport_button(">", play_button_style),
        rule::vertical(1).style(divider_style),
        transport_button(">|", transport_button_style),
        rule::vertical(1).style(divider_style),
    ]
    .height(Fill);

    let left = row![controls, space().width(Fill)]
        .width(188)
        .height(Fill)
        .align_y(Center);

    let progress = container(
        row![
            text("0:00").size(10).font(STRONG_FONT),
            container(space())
                .width(Fill)
                .height(3)
                .style(progress_track_style),
            text("--:--").size(10).font(STRONG_FONT),
        ]
        .spacing(12)
        .align_y(Center),
    )
    .width(Fill)
    .height(Fill)
    .padding([0, 18])
    .center_y(Fill)
    .style(muted_text_style);

    let volume_track = row![
        container(space())
            .width(46)
            .height(3)
            .style(progress_fill_style),
        container(space())
            .width(Fill)
            .height(3)
            .style(progress_track_style),
    ]
    .width(72)
    .align_y(Center);

    let volume = container(
        row![text("VOL").size(10).font(STRONG_FONT), volume_track]
            .spacing(12)
            .align_y(Center),
    )
    .width(188)
    .height(Fill)
    .padding([0, 16])
    .center_y(Fill)
    .align_x(iced::alignment::Horizontal::Right)
    .style(muted_text_style);

    container(
        row![
            left,
            rule::vertical(1).style(divider_style),
            progress,
            rule::vertical(1).style(divider_style),
            volume,
        ]
        .width(Fill)
        .height(Fill)
        .align_y(Center),
    )
    .width(Fill)
    .height(PLAYBACK_BAR_HEIGHT)
    .style(top_bar_style)
    .into()
}

pub fn theme(_app: &App) -> Theme {
    Theme::custom(
        "618",
        theme::Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT,
            success: Color::from_rgb8(116, 190, 141),
            warning: Color::from_rgb8(224, 174, 88),
            danger: DANGER,
        },
    )
}

fn top_bar(app: &App) -> Element<'_, Message> {
    let leading_space = space().width(Length::FillPortion(1)).height(Fill);

    let search = text_input("Search tracks, artists, albums", &app.search_query)
        .on_input(Message::SearchChanged)
        .width(Fill)
        .size(14)
        .padding([9, 0])
        .style(search_style);

    let search_region = container(search)
        .width(300)
        .height(Fill)
        .padding([0, 16])
        .center_y(Fill);

    let window_controls = row![
        window_button("-", Message::WindowMinimized, window_button_style),
        rule::vertical(1).style(divider_style),
        window_button("[]", Message::WindowMaximized, window_button_style),
        rule::vertical(1).style(divider_style),
        window_button("X", Message::WindowClosed, close_button_style),
    ]
    .height(Fill);

    let actions = row![space().width(Fill), window_controls,]
        .width(Length::FillPortion(1))
        .height(Fill)
        .align_y(Center);

    let bar = container(
        row![
            leading_space,
            rule::vertical(1).style(divider_style),
            search_region,
            rule::vertical(1).style(divider_style),
            actions,
        ]
        .width(Fill)
        .height(Fill)
        .align_y(Center),
    )
    .width(Fill)
    .height(TOP_BAR_HEIGHT)
    .style(top_bar_style);

    mouse_area(bar).on_press(Message::WindowDragged).into()
}

fn window_button<'a>(
    label: &'a str,
    message: Message,
    style: fn(&Theme, button::Status) -> button::Style,
) -> iced::widget::Button<'a, Message> {
    button(
        container(text(label).size(12).font(ICON_FONT))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
    )
    .width(35)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(message)
}

fn transport_button<'a>(
    label: &'a str,
    style: fn(&Theme, button::Status) -> button::Style,
) -> iced::widget::Button<'a, Message> {
    button(
        container(text(label).size(12).font(ICON_FONT))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
    )
    .width(44)
    .height(Fill)
    .padding(0)
    .style(style)
    .on_press(Message::PlaybackControlPressed)
}

fn scan_library() -> Task<Message> {
    Task::perform(library::scan_music_directory(), Message::Scanned)
}

fn root_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(BACKGROUND)
}

fn top_bar_style(_theme: &Theme) -> container::Style {
    container::Style::default()
        .background(SURFACE)
        .border(Border {
            color: DIVIDER,
            width: 1.0,
            radius: 0.0.into(),
        })
        .shadow(Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.35),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        })
}

fn section_style(_theme: &Theme) -> container::Style {
    container::Style::default()
        .background(SURFACE)
        .border(Border {
            color: DIVIDER,
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn progress_track_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(DIVIDER)
}

fn progress_fill_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(ACCENT)
}

fn scrollable_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container.border.radius = 0.0.into();
    style.vertical_rail.border.radius = 0.0.into();
    style.vertical_rail.scroller.border.radius = 0.0.into();
    style.horizontal_rail.border.radius = 0.0.into();
    style.horizontal_rail.scroller.border.radius = 0.0.into();
    style.auto_scroll.border.radius = 0.0.into();
    style
}

fn muted_text_style(_theme: &Theme) -> container::Style {
    container::Style::default().color(MUTED)
}

fn divider_style(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: DIVIDER,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

fn search_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.35, ..ACCENT },
    }
}

fn window_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: if hovered { TEXT } else { MUTED },
        ..button::Style::default()
    }
}

fn close_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(DANGER)),
        text_color: if hovered { Color::WHITE } else { MUTED },
        ..button::Style::default()
    }
}

fn transport_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: if hovered { TEXT } else { MUTED },
        ..button::Style::default()
    }
}

fn play_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: ACCENT,
        ..button::Style::default()
    }
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

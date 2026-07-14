use iced::widget::{container, mouse_area, space, stack};
use iced::{Element, Fill, Length, alignment, mouse, window};

use crate::app::Message;

const RESIZE_HANDLE_SIZE: f32 = 6.0;

pub(super) fn view(content: Element<'_, Message>) -> Element<'_, Message> {
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

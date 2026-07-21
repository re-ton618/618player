use iced::event::Status;
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::mouse;

use super::tabs::{self, Direction};

fn tab_message(event: &iced::Event, _status: Status) -> Option<tabs::Message> {
    match event {
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(tabs::Message::DragFinished)
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key: _,
            modifiers,
            repeat: false,
            ..
        }) if *key == Key::Named(Named::Tab) && *modifiers == Modifiers::CTRL => {
            Some(tabs::Message::Cycle(Direction::Forward))
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key: _,
            modifiers,
            repeat: false,
            ..
        }) if *key == Key::Named(Named::Tab)
            && *modifiers == (Modifiers::CTRL | Modifiers::SHIFT) =>
        {
            Some(tabs::Message::Cycle(Direction::Backward))
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat: false,
            ..
        }) if key.to_latin(*physical_key) == Some('w') && *modifiers == Modifiers::COMMAND => {
            Some(tabs::Message::CloseActive)
        }
        _ => None,
    }
}

pub(super) fn subscription() -> iced::Subscription<crate::app::Message> {
    iced::event::listen_with(|event, status, _window| {
        tab_message(&event, status).map(crate::app::Message::Tabs)
    })
}

#[cfg(test)]
mod tests {
    use iced::event::Status;
    use iced::keyboard::{self, Key, Location, Modifiers, key};
    use iced::mouse;

    use super::tab_message;
    use crate::app::tabs::{Direction, Message};

    fn key_event(
        key: Key,
        physical_key: key::Physical,
        modifiers: Modifiers,
        repeat: bool,
    ) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key,
            location: Location::Standard,
            modifiers,
            text: None,
            repeat,
        })
    }

    fn tab(modifiers: Modifiers, repeat: bool) -> iced::Event {
        key_event(
            Key::Named(key::Named::Tab),
            key::Physical::Code(key::Code::Tab),
            modifiers,
            repeat,
        )
    }

    fn close(modifiers: Modifiers, repeat: bool) -> iced::Event {
        key_event(
            Key::Character("w".into()),
            key::Physical::Code(key::Code::KeyW),
            modifiers,
            repeat,
        )
    }

    #[test]
    fn maps_both_cycle_directions_for_every_status() {
        for status in [Status::Captured, Status::Ignored] {
            assert!(matches!(
                tab_message(&tab(Modifiers::CTRL, false), status),
                Some(Message::Cycle(Direction::Forward))
            ));
            assert!(matches!(
                tab_message(&tab(Modifiers::CTRL | Modifiers::SHIFT, false), status),
                Some(Message::Cycle(Direction::Backward))
            ));
        }
    }

    #[test]
    fn maps_platform_close_and_left_release() {
        assert!(matches!(
            tab_message(&close(Modifiers::COMMAND, false), Status::Captured),
            Some(Message::CloseActive)
        ));
        let release = iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        for status in [Status::Captured, Status::Ignored] {
            assert!(matches!(
                tab_message(&release, status),
                Some(Message::DragFinished)
            ));
        }
    }

    #[test]
    fn rejects_repeats_extra_modifiers_and_unrelated_events() {
        for event in [
            tab(Modifiers::CTRL, true),
            tab(Modifiers::CTRL | Modifiers::ALT, false),
            close(Modifiers::COMMAND, true),
            close(Modifiers::COMMAND | Modifiers::SHIFT, false),
            key_event(
                Key::Character("q".into()),
                key::Physical::Code(key::Code::KeyQ),
                Modifiers::COMMAND,
                false,
            ),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ] {
            assert!(tab_message(&event, Status::Ignored).is_none());
        }
    }
}

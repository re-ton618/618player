use nsh_core::keymap::When;
use nsh_core::prelude::*;

use super::tabs::{Direction, Message};

/// The workspace's keyboard shortcuts (`mod` is Command on Apple platforms,
/// Control elsewhere).
const SHORTCUTS: [(&str, Message); 3] = [
    ("ctrl+tab", Message::Cycle(Direction::Forward)),
    ("ctrl+shift+tab", Message::Cycle(Direction::Backward)),
    ("mod+w", Message::CloseActive),
];

/// Binds the shortcuts for as long as the calling owner lives; they fire in
/// the search field too, and once per press (no auto-repeat).
pub(super) fn bind(dispatch: impl Fn(Message) + Copy + 'static) {
    for (keystroke, message) in SHORTCUTS {
        let options = KeyOptions {
            when: When::Always,
            ..KeyOptions::default()
        };
        on_key_with(keystroke, options, move |_| dispatch(message.clone()));
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::keymap::Keystroke;
    use nsh_core::platform::Os;

    use super::SHORTCUTS;

    #[test]
    fn shortcuts_parse_and_are_not_reserved_anywhere() {
        for (keystroke, _) in SHORTCUTS {
            for apple in [false, true] {
                let parsed = Keystroke::parse_with(keystroke, apple)
                    .unwrap_or_else(|error| panic!("{keystroke}: {error}"));
                for os in [Os::MacOs, Os::Windows, Os::Linux] {
                    assert!(parsed.reserved(os).is_none(), "{keystroke} on {os:?}");
                }
            }
        }
    }
}

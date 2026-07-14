pub(crate) fn window() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(960.0, 640.0),
        position: iced::window::Position::Centered,
        min_size: Some(iced::Size::new(840.0, 520.0)),
        decorations: false,
        ..iced::window::Settings::default()
    }
}

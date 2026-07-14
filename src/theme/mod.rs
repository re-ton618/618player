use iced::widget::{button, container, rule, scrollable, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, font, theme};

const BACKGROUND: Color = Color::from_rgb8(13, 14, 16);
const SURFACE: Color = Color::from_rgb8(23, 25, 28);
const SURFACE_HOVERED: Color = Color::from_rgb8(34, 37, 41);
const BORDER: Color = Color::from_rgb8(48, 51, 56);
const DIVIDER: Color = Color::from_rgb8(34, 37, 41);
const TEXT: Color = Color::from_rgb8(239, 237, 231);
const MUTED: Color = Color::from_rgb8(143, 147, 154);
const ACCENT: Color = Color::from_rgb8(211, 235, 111);
const DANGER: Color = Color::from_rgb8(210, 70, 76);

pub(crate) const STRONG_FONT: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};
pub(crate) const ICON_FONT: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::MONOSPACE
};

pub(crate) fn active() -> Theme {
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

pub(crate) fn root_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(BACKGROUND)
}

pub(crate) fn top_bar_style(_theme: &Theme) -> container::Style {
    container::Style::default()
        .background(SURFACE)
        .border(Border {
            color: BORDER,
            width: 1.0,
            radius: 0.0.into(),
        })
        .shadow(Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.35),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        })
}

pub(crate) fn section_style(_theme: &Theme) -> container::Style {
    container::Style::default()
        .background(SURFACE)
        .border(Border {
            color: BORDER,
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(crate) fn progress_track_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(DIVIDER)
}

pub(crate) fn progress_fill_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(ACCENT)
}

pub(crate) fn scrollable_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container.border.radius = 0.0.into();
    style.vertical_rail.border.radius = 0.0.into();
    style.vertical_rail.scroller.border.radius = 0.0.into();
    style.horizontal_rail.border.radius = 0.0.into();
    style.horizontal_rail.scroller.border.radius = 0.0.into();
    style.auto_scroll.border.radius = 0.0.into();
    style
}

pub(crate) fn muted_text_style(_theme: &Theme) -> container::Style {
    container::Style::default().color(MUTED)
}

pub(crate) fn divider_style(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: DIVIDER,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub(crate) fn search_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.35, ..ACCENT },
    }
}

pub(crate) fn header_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        text_color: if hovered { TEXT } else { MUTED },
        ..button::Style::default()
    }
}

pub(crate) fn track_row_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(Color::from_rgb8(29, 32, 36))),
        text_color: TEXT,
        ..button::Style::default()
    }
}

pub(crate) fn window_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: if hovered { TEXT } else { MUTED },
        ..button::Style::default()
    }
}

pub(crate) fn close_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(DANGER)),
        text_color: if hovered { Color::WHITE } else { MUTED },
        ..button::Style::default()
    }
}

pub(crate) fn transport_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: if hovered { TEXT } else { MUTED },
        ..button::Style::default()
    }
}

pub(crate) fn play_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

    button::Style {
        background: hovered.then_some(Background::Color(SURFACE_HOVERED)),
        text_color: ACCENT,
        ..button::Style::default()
    }
}

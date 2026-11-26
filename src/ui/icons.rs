//! The app's icons: line glyphs on a 24 × 24 grid, drawn in the text color.

use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Settings,
    Minimize,
    Maximize,
    Close,
    Add,
    Previous,
    Play,
    Pause,
    Next,
    Shuffle,
    Repeat,
    RepeatOne,
    SortAscending,
    SortDescending,
    Search,
}

impl Icon {
    fn paths(self) -> &'static str {
        match self {
            Self::Settings => {
                r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#
            }
            Self::Minimize => r#"<path d="M5 12h14"/>"#,
            Self::Maximize => r#"<rect x="5" y="5" width="14" height="14"/>"#,
            Self::Close => r#"<path d="M18 6 6 18M6 6l12 12"/>"#,
            Self::Add => r#"<path d="M12 5v14M5 12h14"/>"#,
            Self::Previous => r#"<path d="M19 20 9 12l10-8z"/><path d="M5 19V5"/>"#,
            Self::Play => r#"<path d="M6 3l14 9-14 9z"/>"#,
            Self::Pause => r#"<path d="M8 4v16M16 4v16"/>"#,
            Self::Next => r#"<path d="M5 4l10 8-10 8z"/><path d="M19 5v14"/>"#,
            Self::Shuffle => {
                r#"<path d="m18 14 4 4-4 4"/><path d="m18 2 4 4-4 4"/><path d="M2 18h2l3-2 6-8 3-2h6"/><path d="M2 6h2l3 2 1 1.3"/><path d="M22 18h-6l-3-2-1-1.3"/>"#
            }
            Self::Repeat => {
                r#"<path d="m17 2 4 4-4 4"/><path d="M3 11V6h18"/><path d="m7 22-4-4 4-4"/><path d="M21 13v5H3"/>"#
            }
            Self::RepeatOne => {
                r#"<path d="m17 2 4 4-4 4"/><path d="M3 11V6h18"/><path d="m7 22-4-4 4-4"/><path d="M21 13v5H3"/><path d="M11 10h1v4"/>"#
            }
            Self::SortAscending => r#"<path d="m18 15-6-6-6 6"/>"#,
            Self::SortDescending => r#"<path d="m6 9 6 6 6-6"/>"#,
            Self::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/>"#,
        }
    }

    fn markup(self) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">{}</svg>"#,
            self.paths()
        )
    }

    /// The glyph, `size` points square, in its text color.
    pub(crate) fn view(self, size: f32) -> Svg {
        svg(ImageSrc::svg_markup(&self.markup()))
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
    }
}

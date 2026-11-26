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
    Next,
    SortAscending,
    SortDescending,
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
            Self::Next => r#"<path d="M5 4l10 8-10 8z"/><path d="M19 5v14"/>"#,
            Self::SortAscending => r#"<path d="m18 15-6-6-6 6"/>"#,
            Self::SortDescending => r#"<path d="m6 9 6 6 6-6"/>"#,
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

//! The app's design tokens: `theme.json` (shadcn `cssVars`), drawn through
//! `nsh_base::theme()`, and the app's own tokens by name.

use nsh_base::theme;
use nsh_core::prelude::*;

/// The theme file, also named by `nsh.json`'s `"theme"`.
const THEME_JSON: &str = include_str!("../../theme.json");

pub(crate) const CHROME_BORDER_WIDTH: f32 = 1.0;

/// Draws the app with `theme.json`, however it was started (`cargo run`
/// reads no `nsh.json`).
pub(crate) fn install() {
    set_color_scheme(Appearance::Dark);
    match nsh_base::tokens_of(Some(THEME_JSON)) {
        Ok(tokens) => set_tokens(tokens),
        Err(error) => eprintln!("Theme failed: {error}"),
    }
}

/// Lines between rows and tabs: quieter than `border`.
pub(crate) fn divider() -> AdaptiveColor {
    let theme = theme();
    theme.color("divider").unwrap_or(theme.destructive)
}

/// A hovered library row.
pub(crate) fn row_hover() -> AdaptiveColor {
    let theme = theme();
    theme.color("row-hover").unwrap_or(theme.destructive)
}

/// Glyphs on a hovered close button.
pub(crate) fn destructive_foreground() -> AdaptiveColor {
    let theme = theme();
    theme
        .color("destructive-foreground")
        .unwrap_or(theme.foreground)
}

#[cfg(test)]
mod tests {
    use super::THEME_JSON;

    #[test]
    fn theme_file_parses_with_the_app_tokens() {
        let theme = nsh_base::Theme::from_json(THEME_JSON).expect("theme.json parses");
        for token in ["divider", "row-hover", "destructive-foreground"] {
            assert!(theme.color(token).is_some(), "{token}");
        }
    }
}

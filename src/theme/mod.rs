//! The app's design tokens: `theme.json` (shadcn `cssVars`), drawn through
//! `nsh_base::theme()`, and the app's own tokens by name.
//!
//! The 618 roles map onto them as: `background` BACKGROUND, `card`
//! SURFACE, `popover` PANEL, `border` BORDER, `foreground` TEXT,
//! `muted-foreground` TEXT_MUTED, `text-faint` TEXT_FAINT, `primary`
//! ACCENT, `destructive` DANGER, `success` and `warning`.

use nsh_base::theme;
use nsh_core::prelude::*;

/// The theme file, also named by `nsh.json`'s `"theme"`.
const THEME_JSON: &str = include_str!("../../theme.json");

/// Every line in the app is one point wide.
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

/// Placeholders and glyphs: quieter than `muted-foreground`.
pub(crate) fn text_faint() -> AdaptiveColor {
    let theme = theme();
    theme.color("text-faint").unwrap_or(theme.muted_foreground)
}

/// Needs a look, being waited for; the minimize button's hover.
pub(crate) fn warning() -> AdaptiveColor {
    let theme = theme();
    theme.color("warning").unwrap_or(theme.foreground)
}

/// Done, on, connected; the maximize button's hover.
pub(crate) fn success() -> AdaptiveColor {
    let theme = theme();
    theme.color("success").unwrap_or(theme.foreground)
}

#[cfg(test)]
mod tests {
    use super::THEME_JSON;

    #[test]
    fn theme_file_parses_with_the_app_tokens() {
        let theme = nsh_base::Theme::from_json(THEME_JSON).expect("theme.json parses");
        for token in ["text-faint", "warning", "success"] {
            assert!(theme.color(token).is_some(), "{token}");
        }
    }
}

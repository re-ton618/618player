//! The app's design tokens: one of the [`Id`] themes (shadcn `cssVars`),
//! drawn through `nsh_base::theme()`, and the app's own tokens by name.
//! Every color is read through `theme()`, so [`apply`] redraws the whole
//! app in another theme while it runs.
//!
//! The 618 roles map onto them as: `background` BACKGROUND, `card`
//! SURFACE, `popover` PANEL, `border` BORDER, `foreground` TEXT,
//! `muted-foreground` TEXT_MUTED, `text-faint` TEXT_FAINT, `primary`
//! ACCENT, `destructive` DANGER, `success` and `warning`.

use std::fs;
use std::path::{Path, PathBuf};

use nsh_base::theme;
use nsh_core::prelude::*;

type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const CHOICE_HEADER: &str = "618player-theme-v1";

/// Every line in the app is one point wide.
pub(crate) const CHROME_BORDER_WIDTH: f32 = 1.0;

/// Declares [`Id`] from one line per theme: its variant, its name, the
/// slug it is saved as, its file and whether it is light. The first is
/// the default.
macro_rules! themes {
    (
        $default:ident: $default_label:literal, $default_slug:literal, $default_file:literal, $default_light:literal;
        $( $variant:ident: $label:literal, $slug:literal, $file:literal, $light:literal; )*
    ) => {
        /// A theme the app can be drawn in: 618's own, and ports of
        /// popular editor themes.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub(crate) enum Id {
            #[default]
            $default,
            $( $variant, )*
        }

        impl Id {
            /// Every theme, in the order Settings lists them.
            pub(crate) const ALL: [Self; 1 + [$( $slug ),*].len()] =
                [Self::$default, $( Self::$variant, )*];

            pub(crate) const fn label(self) -> &'static str {
                match self {
                    Self::$default => $default_label,
                    $( Self::$variant => $label, )*
                }
            }

            pub(crate) const fn slug(self) -> &'static str {
                match self {
                    Self::$default => $default_slug,
                    $( Self::$variant => $slug, )*
                }
            }

            pub(crate) const fn is_light(self) -> bool {
                match self {
                    Self::$default => $default_light,
                    $( Self::$variant => $light, )*
                }
            }

            const fn json(self) -> &'static str {
                match self {
                    Self::$default => include_str!($default_file),
                    $( Self::$variant => include_str!($file), )*
                }
            }
        }
    };
}

// The default's file is also named by `nsh.json`'s `"theme"`. Dark ones
// first, then light.
themes! {
    Graphite: "618 Graphite", "graphite", "../../theme.json", false;
    OneDarkPro: "One Dark Pro", "one-dark-pro", "../../themes/one-dark-pro.json", false;
    Dracula: "Dracula", "dracula", "../../themes/dracula.json", false;
    Monokai: "Monokai", "monokai", "../../themes/monokai.json", false;
    Nord: "Nord", "nord", "../../themes/nord.json", false;
    GitHubDark: "GitHub Dark", "github-dark", "../../themes/github-dark.json", false;
    TokyoNight: "Tokyo Night", "tokyo-night", "../../themes/tokyo-night.json", false;
    CatppuccinMocha: "Catppuccin Mocha", "catppuccin-mocha", "../../themes/catppuccin-mocha.json", false;
    GruvboxDark: "Gruvbox Dark", "gruvbox-dark", "../../themes/gruvbox-dark.json", false;
    SolarizedDark: "Solarized Dark", "solarized-dark", "../../themes/solarized-dark.json", false;
    NightOwl: "Night Owl", "night-owl", "../../themes/night-owl.json", false;
    RosePine: "Ros\u{e9} Pine", "rose-pine", "../../themes/rose-pine.json", false;
    GitHubLight: "GitHub Light", "github-light", "../../themes/github-light.json", true;
    OneLight: "One Light", "one-light", "../../themes/one-light.json", true;
    SolarizedLight: "Solarized Light", "solarized-light", "../../themes/solarized-light.json", true;
    CatppuccinLatte: "Catppuccin Latte", "catppuccin-latte", "../../themes/catppuccin-latte.json", true;
}

impl Id {
    pub(crate) fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.slug() == slug)
    }

    /// The theme's colors, to preview it without drawing the app in it.
    pub(crate) fn preview(self) -> Option<nsh_base::Theme> {
        nsh_base::Theme::from_json(self.json()).ok()
    }
}

/// Draws the app in `id`, however it was started (`cargo run` reads no
/// `nsh.json`): what read `theme()` is drawn again.
pub(crate) fn apply(id: Id) {
    set_color_scheme(if id.is_light() {
        Appearance::Light
    } else {
        Appearance::Dark
    });
    match nsh_base::tokens_of(Some(id.json())) {
        Ok(tokens) => set_tokens(tokens),
        Err(error) => eprintln!("Theme {} failed: {error}", id.slug()),
    }
}

/// The theme the user chose last, or the default.
pub(crate) fn saved() -> Id {
    match load_from(&choice_path()) {
        Ok(choice) => choice.unwrap_or_default(),
        Err(error) => {
            eprintln!("Theme restore failed: {error}");
            Id::default()
        }
    }
}

/// Keeps `id` as the theme of the next launch.
pub(crate) fn save(id: Id) {
    if let Err(error) = save_to(&choice_path(), id) {
        eprintln!("Theme save failed: {error}");
    }
}

fn choice_path() -> PathBuf {
    crate::platform::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("618player/theme-v1.txt")
}

fn load_from(path: &Path) -> StoreResult<Option<Id>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut lines = contents.lines();
    if lines.next() != Some(CHOICE_HEADER) {
        return Err("unknown theme header".into());
    }
    let slug = lines
        .next()
        .and_then(|line| line.strip_prefix("theme="))
        .ok_or("missing theme line")?;
    Id::from_slug(slug)
        .map(Some)
        .ok_or_else(|| format!("unknown theme {slug:?}").into())
}

fn save_to(path: &Path, id: Id) -> StoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{CHOICE_HEADER}\ntheme={}\n", id.slug()))?;
    Ok(())
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
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn every_theme_parses_with_the_app_tokens() {
        for id in Id::ALL {
            let theme = nsh_base::Theme::from_json(id.json())
                .unwrap_or_else(|error| panic!("{} parses: {error}", id.slug()));
            for token in ["text-faint", "warning", "success"] {
                assert!(theme.color(token).is_some(), "{} {token}", id.slug());
            }
            assert!(id.preview().is_some());
        }
    }

    #[test]
    fn slugs_name_one_theme_each() {
        for id in Id::ALL {
            assert_eq!(Id::from_slug(id.slug()), Some(id));
        }
        assert_eq!(Id::from_slug("solarized"), None);
    }

    #[test]
    fn the_choice_round_trips_and_a_bad_one_is_refused() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/theme.txt");
        assert_eq!(load_from(&path).unwrap(), None);
        for id in Id::ALL {
            save_to(&path, id).unwrap();
            assert_eq!(load_from(&path).unwrap(), Some(id));
        }
        fs::write(&path, format!("{CHOICE_HEADER}\r\ntheme=nord\r\n")).unwrap();
        assert_eq!(load_from(&path).unwrap(), Some(Id::Nord));
        for bad in ["theme=nord\n", "618player-theme-v1\ntheme=paper\n", ""] {
            fs::write(&path, bad).unwrap();
            assert!(load_from(&path).is_err(), "accepted {bad:?}");
        }
    }
}

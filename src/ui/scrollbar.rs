//! The app's scroll bar on every scroll view and list, in place of the
//! platform's indicator: a flat block along the view's edge, the color of
//! muted text, that comes in while the view scrolls or the pointer is on it
//! and fades soon after it rests, so it never sits over what it scrolls.

use nsh_base::{set_scrollbar_theme, ScrollbarLook, ScrollbarMode, ScrollbarTheme};
use nsh_core::prelude::*;

/// How long the bar stays after the last scroll or hover, and how long it
/// takes to come in and to fade (ms).
const IDLE: f64 = 500.0;
const ENTER: f64 = 80.0;
const EXIT: f64 = 120.0;
/// The shortest a thumb gets.
const MIN_THUMB_LENGTH: f32 = 32.0;
/// The thumb, at rest and while the pointer is on it.
const THUMB_CLASS: &str =
    "bg-muted-foreground/35 group-hover:bg-muted-foreground/70 transition-colors";

/// Draws every scroll view's and list's bar as the app's: 3 points along
/// its right edge (vertical) or its bottom edge (sideways), over what it
/// scrolls, shown also while the pointer is on it. Once, before the views
/// are drawn.
pub(crate) fn use_app_scrollbars() {
    set_scrollbar_theme(Some(app_scrollbar()));
}

fn app_scrollbar() -> ScrollbarTheme {
    let look = |class: &'static str| {
        let mut look = ScrollbarLook::default();
        look.class = Some(SharedString::from(class));
        look.thumb_class = Some(SharedString::from(THUMB_CLASS));
        look
    };

    let mut theme = ScrollbarTheme::default();
    theme.mode = ScrollbarMode::Hover;
    theme.motion.idle = IDLE;
    theme.motion.enter = ENTER;
    theme.motion.exit = EXIT;
    theme.min_thumb_length = px(MIN_THUMB_LENGTH);
    theme.vertical = Some(look("absolute inset-y-0 right-0 w-[3px]"));
    theme.horizontal = Some(look("absolute inset-x-0 bottom-0 h-[3px]"));
    theme
}

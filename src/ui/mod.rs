//! The app's styled controls: every screen draws with these, never with
//! `nsh_base`'s unstyled parts directly.

mod icons;

use nsh_base::{ButtonElement, button, theme};
use nsh_core::prelude::*;

pub(crate) use icons::Icon;

use crate::theme as tokens;

/// The colors of a [`chrome_button`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Muted, a raised wash on hover.
    Quiet,
    /// The accent color, a raised wash on hover.
    Accent,
    /// Muted, the destructive color on hover.
    Danger,
}

/// The bars' button look: a wash on hover, dimmed while pressed, a ring
/// on keyboard focus. A macro, as views and components take state styles
/// through different traits.
macro_rules! chrome_states {
    ($element:expr, $tone:expr) => {{
        let tone: Tone = $tone;
        $element
            .group()
            .h_full()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .hover(move |s| match tone {
                Tone::Danger => s.bg(move || theme().destructive),
                Tone::Quiet | Tone::Accent => s.bg(move || theme().secondary),
            })
            .active(|s| s.opacity(0.85))
            .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
    }};
}

/// A square, borderless button of the top and playback bars and the tab
/// strip, showing `icon`.
pub(crate) fn chrome_button(
    icon: Icon,
    icon_size: f32,
    label: &'static str,
    tone: Tone,
    on_press: impl Fn() + 'static,
) -> ButtonElement {
    chrome_states!(button(), tone)
        .accessibility_label(label)
        .on_press(on_press)
        .child(glyph(icon, icon_size, tone))
}

/// A [`chrome_button`]'s look with no press of its own: the trigger of a
/// menu or a popover, or a window button the platform presses.
pub(crate) fn chrome_glyph(icon: Icon, icon_size: f32, label: &'static str, tone: Tone) -> View {
    chrome_states!(view(), tone)
        .role(AccessibilityRole::Button)
        .label(label)
        .child(glyph(icon, icon_size, tone))
}

/// A [`chrome_button`] with text instead of an icon.
pub(crate) fn chrome_text_button(
    label: &'static str,
    width: f32,
    on_press: impl Fn() + 'static,
) -> ButtonElement {
    chrome_states!(button(), Tone::Quiet)
        .w(px(width))
        .on_press(on_press)
        .child(
            caption(label, 12.0)
                .text_color(move || theme().muted_foreground)
                .group_hover(|s| s.text_color(move || theme().foreground)),
        )
}

fn glyph(icon: Icon, size: f32, tone: Tone) -> Svg {
    icon.view(size)
        .text_color(move || match tone {
            Tone::Accent => theme().primary,
            Tone::Quiet | Tone::Danger => theme().muted_foreground,
        })
        .group_hover(move |s| match tone {
            Tone::Accent => s,
            Tone::Quiet => s.text_color(move || theme().foreground),
            Tone::Danger => s.text_color(tokens::destructive_foreground),
        })
}

/// What a rule separates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    /// Sections and bars: the `border` color.
    Border,
    /// Rows and tabs: the quieter divider.
    Divider,
}

impl Rule {
    fn color(self) -> AdaptiveColor {
        match self {
            Self::Border => theme().border,
            Self::Divider => tokens::divider(),
        }
    }
}

/// A one-point vertical line, as tall as its row.
pub(crate) fn vertical_rule(rule: Rule) -> View {
    view()
        .w(px(1.0))
        .flex_shrink_0()
        .self_stretch()
        .bg(move || rule.color())
}

/// A one-point horizontal line, as wide as its column.
pub(crate) fn horizontal_rule(rule: Rule) -> View {
    view()
        .h(px(1.0))
        .flex_shrink_0()
        .self_stretch()
        .bg(move || rule.color())
}

/// A small, semibold caption (column headers, tab labels, times).
pub(crate) fn caption(value: impl IntoReactive<SharedString>, size: f32) -> Text {
    text(value)
        .text_size(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .number_of_lines(1)
}

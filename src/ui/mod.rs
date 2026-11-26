//! The app's styled controls: every screen draws with these, never with
//! `nsh_base`'s unstyled parts directly.
//!
//! The controls are 618 cells: borderless, as tall as their bar, filling
//! the whole cell on hover (`background`) and press (`border`). Lines are
//! the cells' own edges, drawn by whoever owns the seam.

mod icons;
mod menus;
mod rows;

use nsh_base::{theme, ButtonElement};
use nsh_core::prelude::*;

pub(crate) use icons::Icon;
pub(crate) use menus::{bar_menu, menu_row, subject_menu};
pub(crate) use rows::{button, choice, label_column_row, section_title, ROW_HEIGHT};

use crate::theme as tokens;

/// The text of a strip.
const STRIP_TEXT_SIZE: f32 = 11.0;
/// A strip's inset: 10 across, 5 down.
const STRIP_PADDING_X: f32 = 10.0;
const STRIP_PADDING_Y: f32 = 5.0;
/// A fitted label cell's inset in a bar.
const LABEL_CELL_PADDING: f32 = 12.0;
/// The edge that tells a meaningful strip from a plain one.
const STRIP_EDGE_WIDTH: f32 = 3.0;
/// The tint of a meaningful strip: 10% of its color.
const STRIP_TINT: f32 = 0.1;
/// A tooltip: at most 420 wide, inset 6 across and 4 down, 10pt.
const HINT_MAX_WIDTH: f32 = 420.0;
const HINT_PADDING_X: f32 = 6.0;
const HINT_PADDING_Y: f32 = 4.0;
const HINT_TEXT_SIZE: f32 = 10.0;

/// The colors of a [`chrome_button`]: muted at rest, then on hover…
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// …the text color.
    Quiet,
    /// The accent color at rest and on hover.
    Accent,
    /// …the destructive color (close, remove).
    Danger,
    /// …the warning color (the minimize button).
    Warning,
    /// …the success color (the maximize button).
    Success,
}

/// The bars' cell look: `background` on hover, `border` while pressed, a
/// ring on keyboard focus. A macro, as views and components take state
/// styles through different traits.
macro_rules! chrome_states {
    ($element:expr) => {{
        $element
            .group()
            .h_full()
            .flex_row()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .transition_colors()
            .hover(|s| s.bg(move || theme().background))
            .active(|s| s.bg(move || theme().border))
            .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
    }};
}

/// A square, borderless cell of the top and playback bars and the tab
/// strip, showing `icon`. The caller sets its width and its edge.
pub(crate) fn chrome_button(
    icon: Icon,
    icon_size: f32,
    label: &'static str,
    tone: Tone,
    on_press: impl Fn() + 'static,
) -> ButtonElement {
    chrome_states!(nsh_base::button())
        .accessibility_label(label)
        .on_press(on_press)
        .child(glyph(icon, icon_size, tone))
}

/// A [`chrome_button`]'s look with no press of its own: the trigger of a
/// menu or a popover, or a window button the platform presses.
pub(crate) fn chrome_glyph(icon: Icon, icon_size: f32, label: &'static str, tone: Tone) -> View {
    chrome_states!(view())
        .role(AccessibilityRole::Button)
        .label(label)
        .child(glyph(icon, icon_size, tone))
}

/// A fitted label cell of a bar: 12pt muted text, inset 12.
pub(crate) fn chrome_text_button(
    label: &'static str,
    on_press: impl Fn() + 'static,
) -> ButtonElement {
    chrome_states!(nsh_base::button())
        .px(px(LABEL_CELL_PADDING))
        .accessibility_label(label)
        .on_press(on_press)
        .child(cell_label(label))
}

/// A [`chrome_text_button`]'s look with no press of its own: the trigger
/// of a menu.
pub(crate) fn chrome_text_glyph(label: &'static str) -> View {
    chrome_states!(view())
        .px(px(LABEL_CELL_PADDING))
        .role(AccessibilityRole::Button)
        .label(label)
        .child(cell_label(label))
}

fn cell_label(label: &'static str) -> Text {
    text(label)
        .text_size(px(12.0))
        .number_of_lines(1)
        .text_color(move || theme().muted_foreground)
        .group_hover(|s| s.text_color(move || theme().foreground))
}

fn glyph(icon: Icon, size: f32, tone: Tone) -> Svg {
    icon.view(size)
        .text_color(move || match tone {
            Tone::Accent => theme().primary,
            Tone::Quiet | Tone::Danger | Tone::Warning | Tone::Success => theme().muted_foreground,
        })
        .group_hover(move |s| match tone {
            Tone::Accent => s,
            Tone::Quiet => s.text_color(move || theme().foreground),
            Tone::Danger => s.text_color(move || theme().destructive),
            Tone::Warning => s.text_color(tokens::warning),
            Tone::Success => s.text_color(tokens::success),
        })
}

/// A small, semibold caption (times, the volume).
pub(crate) fn caption(value: impl IntoReactive<SharedString>, size: f32) -> Text {
    text(value)
        .text_size(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .number_of_lines(1)
}

/// A plain message strip: a full-width block that says one thing, in
/// muted 11pt text. The caller gives it the stack's line.
pub(crate) fn strip(said: impl IntoReactive<SharedString>) -> View {
    view()
        .w_full()
        .flex_shrink_0()
        .px(px(STRIP_PADDING_X))
        .py(px(STRIP_PADDING_Y))
        .role(AccessibilityRole::Text)
        .child(
            text(said)
                .text_size(px(STRIP_TEXT_SIZE))
                .line_height(nsh_core::LineHeight::Multiple(1.5))
                .text_color(move || theme().muted_foreground),
        )
}

/// A strip for what needs a look or is being waited for: a 3pt
/// `warning` edge, a 10% tint of it, its text in it. The caller gives it
/// the stack's line.
pub(crate) fn warning_strip(said: impl IntoReactive<SharedString>) -> View {
    view()
        .w_full()
        .flex_shrink_0()
        .px(px(STRIP_PADDING_X))
        .py(px(STRIP_PADDING_Y))
        .border_l(px(STRIP_EDGE_WIDTH))
        .border_l_color(tokens::warning)
        .bg(move || tokens::warning().opacity(STRIP_TINT))
        .role(AccessibilityRole::Text)
        .child(
            text(said)
                .text_size(px(STRIP_TEXT_SIZE))
                .line_height(nsh_core::LineHeight::Multiple(1.5))
                .text_color(tokens::warning),
        )
}

/// A tooltip: what an element's `.tooltip(..)` shows, as the full text of
/// something cut short. A `popover` box with its line, 10pt text.
pub(crate) fn hint(said: impl IntoReactive<SharedString>) -> AnyElement {
    view()
        .max_w(px(HINT_MAX_WIDTH))
        .px(px(HINT_PADDING_X))
        .py(px(HINT_PADDING_Y))
        .bg(move || theme().popover)
        .border_1()
        .border_color(move || theme().border)
        .child(
            text(said)
                .text_size(px(HINT_TEXT_SIZE))
                .text_color(move || theme().popover_foreground),
        )
        .into_element()
}

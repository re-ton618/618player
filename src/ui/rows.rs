//! The rows of a form and the cells in them: a name in a fixed column,
//! then cells that fill the rest, each with its left edge.

use nsh_base::{theme, ButtonElement};
use nsh_core::prelude::*;

use crate::theme::CHROME_BORDER_WIDTH;

/// A form row, and a single-line field in one.
pub(crate) const ROW_HEIGHT: f32 = 36.0;
/// The name column of a [`label_column_row`].
const LABEL_COLUMN_WIDTH: f32 = 78.0;
/// A cell's inset, and its text.
const CELL_PADDING: f32 = 10.0;
const CELL_TEXT_SIZE: f32 = 11.0;
/// A section title: 28 and its bottom line, as column titles are.
const SECTION_TITLE_HEIGHT: f32 = 29.0;
const SECTION_TITLE_TEXT_SIZE: f32 = 10.0;
const SECTION_TITLE_LETTER_SPACING: f32 = 0.8;
/// The fill of the chosen cell of a [`choice`]: 10% of the accent.
const CHOSEN_TINT: f32 = 0.1;

/// A text cell of a row on `background`: as tall as its row, `card` on
/// hover, `border` while pressed. The caller sets its width and its edge.
macro_rules! cell_states {
    ($element:expr) => {{
        $element
            .group()
            .h_full()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .px(px(CELL_PADDING))
            .transition_colors()
            .hover(|s| s.bg(move || theme().card))
            .active(|s| s.bg(move || theme().border))
            .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
    }};
}

/// A flat, borderless text cell of a row, in the text color. (A quiet
/// and a destructive tone join it when a row first needs them.)
pub(crate) fn button(label: &'static str, on_press: impl Fn() + 'static) -> ButtonElement {
    cell_states!(nsh_base::button())
        .accessibility_label(label)
        .on_press(on_press)
        .child(
            text(label)
                .text_size(px(CELL_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || theme().foreground),
        )
}

/// One of a row of equal cells that choose one of two or three: the
/// chosen one in the accent on a tint of it, the rest muted. The caller
/// gives each its left edge.
pub(crate) fn choice(
    label: &'static str,
    chosen: impl Fn() -> bool + Copy + 'static,
    on_press: impl Fn() + 'static,
) -> ButtonElement {
    cell_states!(nsh_base::button())
        .flex_1()
        .min_w(px(0.0))
        .justify_center()
        .style_when(chosen, |s| {
            s.bg(move || theme().primary.opacity(CHOSEN_TINT))
        })
        .selected(chosen)
        .accessibility_label(label)
        .on_press(on_press)
        .child(
            text(label)
                .text_size(px(CELL_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || {
                    if chosen() {
                        theme().primary
                    } else {
                        theme().muted_foreground
                    }
                })
                .group_hover(move |s| {
                    s.text_color(move || {
                        if chosen() {
                            theme().primary
                        } else {
                            theme().foreground
                        }
                    })
                }),
        )
}

/// A form row, 36 and its bottom line: its name in a fixed column, muted,
/// then the cells the caller adds, each with its left edge, filling the
/// rest. Rows of a scrolling page each own the line under them.
pub(crate) fn label_column_row(name: &'static str) -> View {
    view()
        .w_full()
        .h(px(ROW_HEIGHT + CHROME_BORDER_WIDTH))
        .flex_shrink_0()
        .flex_row()
        .border_b_1()
        .border_color(move || theme().border)
        .child(
            view()
                .w(px(LABEL_COLUMN_WIDTH))
                .h_full()
                .flex_shrink_0()
                .px(px(CELL_PADDING))
                .justify_center()
                .child(
                    text(name)
                        .text_size(px(CELL_TEXT_SIZE))
                        .number_of_lines(1)
                        .text_color(move || theme().muted_foreground),
                ),
        )
}

/// The title of a section of a page: 28 and its bottom line on `card`, a
/// 10pt uppercase tag, then room for the section's actions, which the
/// caller adds as trailing cells with their left edges.
pub(crate) fn section_title(label: &'static str) -> View {
    view()
        .w_full()
        .h(px(SECTION_TITLE_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .bg(move || theme().card)
        .border_b_1()
        .border_color(move || theme().border)
        .role(AccessibilityRole::Header)
        .label(label)
        .child(
            view()
                .h_full()
                .flex_shrink_0()
                .px(px(CELL_PADDING))
                .justify_center()
                .child(
                    super::caption(label.to_uppercase(), SECTION_TITLE_TEXT_SIZE)
                        .letter_spacing(AbsoluteLength::from(px(SECTION_TITLE_LETTER_SPACING)))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(view().flex_1())
}

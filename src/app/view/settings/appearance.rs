//! Appearance: the theme the app is drawn in.

use nsh_base::{button, theme, ButtonElement};
use nsh_core::prelude::*;

use super::{CELL_PADDING, CHOSEN_TINT, TAG_LETTER_SPACING, TAG_TEXT_SIZE};
use crate::app::store::Store;
use crate::app::Message;
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{self, caption, section_title};

/// A theme line, a form row and its bottom line.
const THEME_ROW_HEIGHT: f32 = ui::ROW_HEIGHT + CHROME_BORDER_WIDTH;
/// A theme's preview: its background, surface, text and accent, each a
/// square with a line around it.
const SWATCH_SIZE: f32 = 14.0;
const SWATCH_GAP: f32 = 4.0;
const THEME_TEXT_SIZE: f32 = 11.0;

/// Appearance: every theme, each with a preview of its colors; the chosen
/// one in the accent on a tint of it. Pressing one redraws the app in it.
pub(super) fn page(store: Store) -> View {
    view().w_full().child(section_title("Theme")).children(
        tokens::Id::ALL
            .into_iter()
            .map(|choice| theme_row(store, choice)),
    )
}

/// A theme: its preview, its name, and whether it is light or dark.
fn theme_row(store: Store, choice: tokens::Id) -> ButtonElement {
    let chosen = store.select(move |model| model.theme == choice);
    let border = move || theme().border;
    let swatches = choice.preview().map_or_else(Vec::new, |preview| {
        vec![
            preview.background,
            preview.card,
            preview.foreground,
            preview.primary,
        ]
    });

    button()
        .group()
        .w_full()
        .h(px(THEME_ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .border_b_1()
        .border_color(border)
        .transition_colors()
        .style_when(chosen, |s| {
            s.bg(move || theme().primary.opacity(CHOSEN_TINT))
        })
        .hover(|s| s.bg(move || theme().card))
        .active(|s| s.bg(move || theme().border))
        .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
        .selected(chosen)
        .accessibility_label(choice.label())
        .on_press(move || store.dispatch(Message::ThemeChosen(choice)))
        .child(
            view()
                .h_full()
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .gap(px(SWATCH_GAP))
                .px(px(CELL_PADDING))
                .children(swatches.into_iter().map(move |swatch| {
                    view()
                        .w(px(SWATCH_SIZE))
                        .h(px(SWATCH_SIZE))
                        .flex_shrink_0()
                        .bg(move || swatch)
                        .border_1()
                        .border_color(border)
                })),
        )
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .px(px(CELL_PADDING))
                .justify_center()
                .border_l_1()
                .border_color(border)
                .child(
                    text(choice.label())
                        .text_size(px(THEME_TEXT_SIZE))
                        .number_of_lines(1)
                        .text_color(move || {
                            if chosen.get() {
                                theme().primary
                            } else {
                                theme().foreground
                            }
                        }),
                ),
        )
        .child(
            view()
                .h_full()
                .flex_shrink_0()
                .px(px(CELL_PADDING))
                .justify_center()
                .border_l_1()
                .border_color(border)
                .child(
                    caption(
                        if choice.is_light() { "LIGHT" } else { "DARK" },
                        TAG_TEXT_SIZE,
                    )
                    .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
                    .text_color(move || theme().muted_foreground),
                ),
        )
}

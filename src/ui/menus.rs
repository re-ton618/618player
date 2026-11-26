//! The app's menus: a `popover` panel with one line around it, of rows
//! that fill on hover as the bars' cells do.

use nsh_base::{
    ContextMenuElement, MenuElement, MenuItemElement, context_menu, menu, menu_item, theme,
};
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

/// A menu row, and the menu's inset above and below its rows.
const ROW_HEIGHT: f32 = 30.0;
const ROW_PADDING: f32 = 10.0;
const ROW_TEXT_SIZE: f32 = 11.0;
const MENU_PADDING: f32 = 4.0;
const MENU_WIDTH: f32 = 180.0;
/// Clears the line under a bar's trigger.
const MENU_OFFSET: f32 = 1.0;

/// A row of a menu, which closes it and runs `on_select`.
pub(crate) fn menu_row(label: &'static str, on_select: impl Fn() + 'static) -> MenuItemElement {
    menu_item()
        .on_select(on_select)
        .h(px(ROW_HEIGHT))
        .flex_row()
        .items_center()
        .px(px(ROW_PADDING))
        .hover(|s| s.bg(move || theme().card))
        .focus_visible(|s| s.bg(move || theme().card))
        .active(|s| s.bg(move || theme().border))
        .child(
            text(label)
                .text_size(px(ROW_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || theme().popover_foreground),
        )
}

/// A menu hanging from `trigger`, a cell of a bar; the caller gives it its
/// rows ([`menu_row`]).
pub(crate) fn bar_menu(trigger: impl IntoElement) -> MenuElement {
    menu()
        .offset(px(MENU_OFFSET))
        .popup_style(panel)
        .trigger(trigger)
}

/// A menu about `subject`, which the secondary button opens where the
/// pointer is; the caller gives it its rows ([`menu_row`]).
pub(crate) fn subject_menu(subject: impl IntoElement) -> ContextMenuElement {
    context_menu().popup_style(panel).trigger(subject)
}

fn panel(style: StyleRefinement) -> StyleRefinement {
    style
        .min_w(px(MENU_WIDTH))
        .py(px(MENU_PADDING))
        .bg(move || theme().popover)
        .border_1()
        .border_color(move || theme().border)
}

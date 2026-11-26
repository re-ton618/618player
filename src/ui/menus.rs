//! The app's menus: a `popover` panel with one line around it, of rows
//! that fill on hover as the bars' cells do.

use nsh_base::{
    context_menu, menu, menu_item, menu_separator, submenu, theme, ContextMenuElement, MenuElement,
    MenuItemElement, MenuSeparatorElement, SubmenuElement,
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
pub(crate) fn menu_row(
    label: impl Into<SharedString>,
    on_select: impl Fn() + 'static,
) -> MenuItemElement {
    let label: SharedString = label.into();
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

/// A row of a menu that opens the menu beside it, which the caller gives
/// its rows ([`menu_row`]); it ends with a chevron.
pub(crate) fn menu_submenu(label: &'static str) -> SubmenuElement {
    submenu().trigger(
        view()
            .h(px(ROW_HEIGHT))
            .flex_row()
            .items_center()
            .gap(px(ROW_PADDING))
            .px(px(ROW_PADDING))
            .hover(|s| s.bg(move || theme().card))
            .child(
                text(label)
                    .flex_1()
                    .text_size(px(ROW_TEXT_SIZE))
                    .number_of_lines(1)
                    .text_color(move || theme().popover_foreground),
            )
            .child(
                text("\u{203a}")
                    .text_size(px(ROW_TEXT_SIZE))
                    .text_color(move || theme().muted_foreground),
            ),
    )
}

/// The line between groups of a menu's rows.
pub(crate) fn menu_divider() -> MenuSeparatorElement {
    menu_separator()
        .h(px(1.0))
        .my(px(MENU_PADDING))
        .bg(move || theme().border)
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

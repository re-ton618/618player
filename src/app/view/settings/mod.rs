//! The Settings tab: the column of categories and the page of the chosen
//! one. Each category's page lives in its own module; a category that grows
//! subcategories becomes a folder (`library/mod.rs` and a file per one).

mod about;
mod appearance;
mod library;
mod playback;

use nsh_base::{button, theme, ButtonElement};
use nsh_core::prelude::*;

use crate::app::settings_tab::Category;
use crate::app::store::{store, Store};
use crate::app::Message;
use crate::theme::CHROME_BORDER_WIDTH;

/// The category list, its right edge included: as wide as a tab, so its
/// edge runs on from the first tab's.
const CATEGORY_COLUMN_WIDTH: f32 = 160.0;
/// A category, 36 and its bottom line; inset as a tab's title is.
const CATEGORY_ROW_HEIGHT: f32 = 36.0 + CHROME_BORDER_WIDTH;
const CATEGORY_PADDING: f32 = 12.0;
const CATEGORY_TEXT_SIZE: f32 = 11.0;

// Shared by the pages.
/// The fill of the chosen category or choice: 10% of the accent.
const CHOSEN_TINT: f32 = 0.1;
const CELL_PADDING: f32 = 10.0;
const TAG_TEXT_SIZE: f32 = 9.0;
const TAG_LETTER_SPACING: f32 = 0.8;

/// The Settings tab: the categories in a column on the left, and on the
/// right the chosen one's page, a column of sections that scrolls on its
/// own. Every block of the page owns the line under it.
pub(super) fn settings() -> impl IntoElement {
    let store = store();
    let category = store.select(|model| model.settings_category);

    let page = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .child(dynamic(move || {
            let sections = match category.get() {
                Category::Library => library::page(store),
                Category::Playback => playback::page(store),
                Category::Appearance => appearance::page(store),
                Category::About => about::page(store),
            };
            scroll()
                .flex_1()
                .min_h(px(0.0))
                .child(sections)
                .into_element()
        }));

    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(categories(store))
        .child(page)
}

/// The column of categories on `card`, with its right edge where it meets
/// the page; it scrolls when there are more than it can show.
fn categories(store: Store) -> View {
    view()
        .w(px(CATEGORY_COLUMN_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(move || theme().card)
        .border_r_1()
        .border_color(move || theme().border)
        .role(AccessibilityRole::Tablist)
        .label("Settings categories")
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view().w_full().children(
                    Category::ALL
                        .into_iter()
                        .map(|category| category_row(store, category)),
                ),
            ),
        )
}

/// A category: `background` on hover as the bars' cells are; chosen, the
/// accent on a tint of it.
fn category_row(store: Store, category: Category) -> ButtonElement {
    let chosen = store.select(move |model| model.settings_category == category);
    button()
        .group()
        .w_full()
        .h(px(CATEGORY_ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .px(px(CATEGORY_PADDING))
        .border_b_1()
        .border_color(move || theme().border)
        .transition_colors()
        .style_when(chosen, |s| {
            s.bg(move || theme().primary.opacity(CHOSEN_TINT))
        })
        .hover(|s| s.bg(move || theme().background))
        .active(|s| s.bg(move || theme().border))
        .focus_visible(|s| s.outline(px(1.0)).outline_color(move || theme().ring))
        .selected(chosen)
        .accessibility_label(category.label())
        .on_press(move || store.dispatch(Message::SettingsCategoryPressed(category)))
        .child(
            text(category.label())
                .text_size(px(CATEGORY_TEXT_SIZE))
                .number_of_lines(1)
                .text_color(move || {
                    if chosen.get() {
                        theme().primary
                    } else {
                        theme().muted_foreground
                    }
                })
                .group_hover(move |s| {
                    s.text_color(move || {
                        if chosen.get() {
                            theme().primary
                        } else {
                            theme().foreground
                        }
                    })
                }),
        )
}

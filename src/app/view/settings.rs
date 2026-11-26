use nsh_base::{button, theme, ButtonElement};
use nsh_core::prelude::*;

use crate::app::queue::Repeat;
use crate::app::settings_tab::Category;
use crate::app::store::{store, Store};
use crate::app::Message;
use crate::library::Folder;
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{
    self, caption, chrome_button, chrome_text_button, hint, label_column_row, section_title, Icon,
    Tone,
};

/// The category list, its right edge included: as wide as a tab, so its
/// edge runs on from the first tab's.
const CATEGORY_COLUMN_WIDTH: f32 = 160.0;
/// A category, 36 and its bottom line; inset as a tab's title is.
const CATEGORY_ROW_HEIGHT: f32 = 36.0 + CHROME_BORDER_WIDTH;
const CATEGORY_PADDING: f32 = 12.0;
const CATEGORY_TEXT_SIZE: f32 = 11.0;
/// The fill of the chosen category: 10% of the accent.
const CHOSEN_TINT: f32 = 0.1;
/// A folder line, 28 and its bottom line; its remove cell is square to it.
const FOLDER_ROW_HEIGHT: f32 = 28.0 + CHROME_BORDER_WIDTH;
const FOLDER_TEXT_SIZE: f32 = 11.0;
const CELL_PADDING: f32 = 10.0;
const TAG_TEXT_SIZE: f32 = 9.0;
const TAG_LETTER_SPACING: f32 = 0.8;
const REMOVE_ICON_SIZE: f32 = 10.0;
/// A − or + cell of the volume: square to its row.
const STEPPER_WIDTH: f32 = ui::ROW_HEIGHT + CHROME_BORDER_WIDTH;
/// How far a press of − or + moves the volume, in percent.
const VOLUME_STEP: u8 = 5;
const MAX_VOLUME: u8 = 100;
/// A theme line, a form row and its bottom line.
const THEME_ROW_HEIGHT: f32 = ui::ROW_HEIGHT + CHROME_BORDER_WIDTH;
/// A theme's preview: its background, surface, text and accent, each a
/// square with a line around it.
const SWATCH_SIZE: f32 = 14.0;
const SWATCH_GAP: f32 = 4.0;
const THEME_TEXT_SIZE: f32 = 11.0;

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
                Category::Library => library(store),
                Category::Playback => playback(store),
                Category::Appearance => appearance(store),
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

/// Library: the folders it is made of, each with the cell that takes it
/// out, under the title whose cell adds one; then what it holds.
fn library(store: Store) -> View {
    let border = move || theme().border;
    let folders = store.select(|model| model.folders.clone());
    let empty = store.select(|model| model.folders.is_empty());
    let summary = store.select(|model| {
        let catalog = &model.catalog;
        SharedString::from(format!(
            "{} tracks \u{b7} {} albums \u{b7} {} album artists",
            catalog.track_count(),
            catalog.albums().len(),
            catalog.artists().len()
        ))
    });

    view()
        .w_full()
        .child(
            section_title("Folders").child(
                chrome_text_button("Add folder", move || {
                    store.dispatch(Message::AddFolderPressed)
                })
                .border_l_1()
                .border_color(border),
            ),
        )
        .child(when(empty, move || {
            ui::strip("No folders in the library.")
                .border_b_1()
                .border_color(border)
        }))
        .child(each(
            folders,
            // A folder that is lost or back is drawn again.
            |folder: &Folder| (folder.id, folder.available),
            move |folder, _| folder_row(store, folder.get_untracked()),
        ))
        .child(ui::strip(summary).border_b_1().border_color(border))
}

/// A folder: its path, `NOT FOUND` while it is not there (an unplugged
/// drive), and the cell that takes it out of the library.
fn folder_row(store: Store, folder: Folder) -> View {
    let Folder {
        id,
        path,
        available,
    } = folder;
    let border = move || theme().border;
    let shown = path.to_string_lossy().into_owned();

    view()
        .w_full()
        .h(px(FOLDER_ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .border_b_1()
        .border_color(border)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .px(px(CELL_PADDING))
                .justify_center()
                .tooltip(hint(shown.clone()))
                .child(
                    text(shown)
                        .text_size(px(FOLDER_TEXT_SIZE))
                        .number_of_lines(1)
                        .text_color(move || {
                            if available {
                                theme().foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                ),
        )
        .when_some((!available).then_some(()), |row, ()| {
            row.child(
                view()
                    .h_full()
                    .flex_shrink_0()
                    .px(px(CELL_PADDING))
                    .justify_center()
                    .border_l_1()
                    .border_color(border)
                    .child(
                        caption("NOT FOUND", TAG_TEXT_SIZE)
                            .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
                            .text_color(tokens::warning),
                    ),
            )
        })
        .child(
            chrome_button(
                Icon::Close,
                REMOVE_ICON_SIZE,
                "Remove folder from library",
                Tone::Danger,
                move || store.dispatch(Message::RemoveFolderPressed(id)),
            )
            .w(px(FOLDER_ROW_HEIGHT))
            .border_l_1()
            .border_color(border),
        )
}

/// Playback: what the queue does (shuffle, repeat) and the volume.
fn playback(store: Store) -> View {
    let border = move || theme().border;
    let shuffled = store.select(|model| model.queue.shuffled());
    let repeat = store.select(|model| model.queue.repeat());
    let volume = store.select(|model| model.playback.volume());
    let shown_volume =
        store.select(|model| SharedString::from(format!("{}%", model.playback.volume())));

    let shuffle = move |label, on: bool| {
        ui::choice(
            label,
            move || shuffled.get() == on,
            move || store.dispatch(Message::ShuffleChosen(on)),
        )
        .border_l_1()
        .border_color(border)
    };
    let mode = move |label, mode: Repeat| {
        ui::choice(
            label,
            move || repeat.get() == mode,
            move || store.dispatch(Message::RepeatChosen(mode)),
        )
        .border_l_1()
        .border_color(border)
    };
    let step = move |label, change: fn(u8) -> u8| {
        ui::button(label, move || {
            store.dispatch(Message::VolumeChanged(change(volume.get_untracked())))
        })
        .w(px(STEPPER_WIDTH))
        .justify_center()
        .border_l_1()
        .border_color(border)
    };

    view()
        .w_full()
        .child(section_title("Queue"))
        .child(
            label_column_row("Shuffle")
                .child(shuffle("Off", false))
                .child(shuffle("On", true)),
        )
        .child(
            label_column_row("Repeat")
                .child(mode("Off", Repeat::Off))
                .child(mode("All", Repeat::All))
                .child(mode("One", Repeat::One)),
        )
        .child(section_title("Output"))
        .child(
            label_column_row("Volume")
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
                            text(shown_volume)
                                .text_size(px(FOLDER_TEXT_SIZE))
                                .tabular_nums(),
                        ),
                )
                .child(step("\u{2212}", |volume| {
                    volume.saturating_sub(VOLUME_STEP)
                }))
                .child(step("+", |volume| {
                    volume.saturating_add(VOLUME_STEP).min(MAX_VOLUME)
                })),
        )
}

/// Appearance: every theme, each with a preview of its colors; the chosen
/// one in the accent on a tint of it. Pressing one redraws the app in it.
fn appearance(store: Store) -> View {
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

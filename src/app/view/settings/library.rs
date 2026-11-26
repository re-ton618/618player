//! Library: the folders it is made of, and what it holds.

use nsh_base::theme;
use nsh_core::prelude::*;

use super::{CELL_PADDING, TAG_LETTER_SPACING, TAG_TEXT_SIZE};
use crate::app::store::Store;
use crate::app::Message;
use crate::library::Folder;
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{
    self, caption, chrome_button, chrome_text_button, hint, section_title, Icon, Tone,
};

/// A folder line, 28 and its bottom line; its remove cell is square to it.
const FOLDER_ROW_HEIGHT: f32 = 28.0 + CHROME_BORDER_WIDTH;
const FOLDER_TEXT_SIZE: f32 = 11.0;
const REMOVE_ICON_SIZE: f32 = 10.0;

/// Library: the folders it is made of, each with the cell that takes it
/// out, under the title whose cell adds one; then what it holds.
pub(super) fn page(store: Store) -> View {
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

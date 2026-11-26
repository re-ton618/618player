use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app::store::{store, Store};
use crate::app::Message;
use crate::library::Folder;
use crate::theme as tokens;
use crate::ui::{caption, chrome_button, chrome_text_button, Icon, Tone};

/// The section title: 28 and its bottom line, as the library's column
/// titles are.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
/// A folder row, its bottom line included.
const ROW_HEIGHT: f32 = 32.0;
const ROW_TEXT_SIZE: f32 = 12.0;
const ROW_GAP: f32 = 12.0;
const HORIZONTAL_PADDING: f32 = 10.0;
const REMOVE_ICON_SIZE: f32 = 10.0;
const SUMMARY_TEXT_SIZE: f32 = 11.0;
const SUMMARY_PADDING_Y: f32 = 5.0;

/// The Settings tab: the folders the library is made of, each with the
/// cell that removes it, under the cell that adds one; then what the
/// library holds.
pub(super) fn settings() -> impl IntoElement {
    let store = store();
    let folders = store.select(|model| model.folders.clone());
    let summary = store.select(|model| {
        let catalog = &model.catalog;
        SharedString::from(format!(
            "{} tracks \u{b7} {} albums \u{b7} {} album artists",
            catalog.track_count(),
            catalog.albums().len(),
            catalog.artists().len()
        ))
    });
    let border = move || theme().border;

    let header = view()
        .h(px(HEADER_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .pl(px(HORIZONTAL_PADDING))
        .bg(move || theme().card)
        .border_b_1()
        .border_color(border)
        .child(
            caption("LIBRARY FOLDERS", HEADER_TEXT_SIZE)
                .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
                .text_color(move || theme().muted_foreground),
        )
        .child(view().flex_1())
        .child(
            chrome_text_button("Add Folder", move || {
                store.dispatch(Message::AddFolderPressed)
            })
            .border_l_1()
            .border_color(border),
        );

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(header)
        .child(each(
            folders,
            // A folder that is lost or back is drawn again.
            |folder: &Folder| (folder.id, folder.available),
            move |folder, _| folder_row(store, folder.get_untracked()),
        ))
        .child(
            view()
                .w_full()
                .flex_shrink_0()
                .px(px(HORIZONTAL_PADDING))
                .py(px(SUMMARY_PADDING_Y))
                .child(
                    text(summary)
                        .text_size(px(SUMMARY_TEXT_SIZE))
                        .line_height(nsh_core::LineHeight::Multiple(1.5))
                        .text_color(move || theme().muted_foreground),
                ),
        )
}

/// A folder of the library: its path, a note when it is not there (an
/// unplugged drive), and the cell that takes it out of the library.
fn folder_row(store: Store, folder: Folder) -> impl IntoElement {
    let Folder {
        id,
        path,
        available,
    } = folder;
    view()
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP))
        .pl(px(HORIZONTAL_PADDING))
        .border_b_1()
        .border_color(move || theme().border)
        .child(
            view().flex_1().min_w(px(0.0)).child(
                text(path.to_string_lossy().into_owned())
                    .text_size(px(ROW_TEXT_SIZE))
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
        .child(when(
            move || !available,
            || {
                caption("NOT FOUND", HEADER_TEXT_SIZE)
                    .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
                    .text_color(tokens::warning)
            },
        ))
        .child(
            chrome_button(
                Icon::Close,
                REMOVE_ICON_SIZE,
                "Remove folder from library",
                Tone::Danger,
                move || store.dispatch(Message::RemoveFolderPressed(id)),
            )
            .w(px(ROW_HEIGHT))
            .border_l_1()
            .border_color(move || theme().border),
        )
}

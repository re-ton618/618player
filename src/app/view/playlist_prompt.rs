//! The prompt that names a playlist, a new one or one renamed: a panel
//! over the window, its field focused. Return confirms; Escape, a press
//! outside it and Cancel close it.

use nsh_base::{button, dialog, input, theme};
use nsh_core::prelude::*;

use crate::app::Message;
use crate::app::playlists::Prompt;
use crate::app::store::{Store, store};
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{caption, chrome_text_button};

const PANEL_WIDTH: f32 = 340.0;
/// The title, 28 and its bottom line, as section titles are.
const TITLE_HEIGHT: f32 = 28.0 + CHROME_BORDER_WIDTH;
const TITLE_TEXT_SIZE: f32 = 10.0;
const TITLE_LETTER_SPACING: f32 = 0.8;
/// The field's row, 36 and its bottom line; the buttons' row, 32.
const FIELD_HEIGHT: f32 = 36.0 + CHROME_BORDER_WIDTH;
const FIELD_PADDING: f32 = 10.0;
const FIELD_TEXT_SIZE: f32 = 12.0;
const BUTTONS_HEIGHT: f32 = 32.0;
const NAME_MAX_LENGTH: u32 = 120;
/// What the window shows through the backdrop.
const BACKDROP_OPACITY: f32 = 0.6;

/// The prompt, while the model has one.
pub(super) fn playlist_prompt() -> impl IntoElement {
    let store = store();
    let open = store.select(|model| model.playlists.prompt.is_some());
    when(open, move || prompt(store))
}

fn prompt(store: Store) -> impl IntoElement {
    let border = move || theme().border;
    // What it is for stays while it is open.
    let (title, confirm) = store
        .select(|model| {
            model
                .playlists
                .prompt
                .as_ref()
                .map(|prompt| (prompt.title(), prompt.confirm_label()))
        })
        .get_untracked()
        .unwrap_or(("", ""));
    let name = store.select(|model| {
        model
            .playlists
            .prompt
            .as_ref()
            .map(|prompt| prompt.name.clone())
            .unwrap_or_default()
    });
    let nameless = store.select(|model| {
        model
            .playlists
            .prompt
            .as_ref()
            .is_none_or(|prompt: &Prompt| prompt.name.trim().is_empty())
    });
    let cancel = move || store.dispatch(Message::PromptCancelled);
    let confirmed = move || store.dispatch(Message::PromptConfirmed);

    let field = input()
        .value(name)
        .on_change(move |text: &str| store.dispatch(Message::PromptNameChanged(text.to_owned())))
        .on_submit(move |_: &str| confirmed())
        .auto_focus(true)
        .select_text_on_focus(true)
        .max_length(NAME_MAX_LENGTH)
        .placeholder("Playlist name")
        .placeholder_color(|| color(tokens::text_faint()))
        .caret_color(|| color(theme().primary))
        .selection_color(|| color(theme().popover).blend(color(theme().primary.opacity(0.35))))
        .flex_1()
        .min_w(px(0.0))
        .text_size(px(FIELD_TEXT_SIZE))
        .text_color(move || theme().foreground);

    let panel = view()
        .w(px(PANEL_WIDTH))
        .bg(move || theme().popover)
        .border_1()
        .border_color(border)
        .child(
            view()
                .h(px(TITLE_HEIGHT))
                .flex_row()
                .items_center()
                .px(px(FIELD_PADDING))
                .bg(move || theme().card)
                .border_b_1()
                .border_color(border)
                .child(
                    caption(title.to_uppercase(), TITLE_TEXT_SIZE)
                        .letter_spacing(AbsoluteLength::from(px(TITLE_LETTER_SPACING)))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .h(px(FIELD_HEIGHT))
                .flex_row()
                .items_center()
                .px(px(FIELD_PADDING))
                .border_b_1()
                .border_color(border)
                .focus_within(|s| s.bg(move || theme().background))
                .child(field),
        )
        .child(
            view()
                .h(px(BUTTONS_HEIGHT))
                .flex_row()
                .child(view().flex_1())
                .child(
                    chrome_text_button("Cancel", cancel)
                        .border_l_1()
                        .border_color(border),
                )
                .child(
                    chrome_text_button(confirm, confirmed)
                        .disabled(nameless)
                        .border_l_1()
                        .border_color(border),
                ),
        );

    dialog()
        .animation(ModalAnimation::Fade)
        .label(title)
        .on_cancel(cancel)
        .child(
            view()
                .flex_1()
                .w_full()
                .h_full()
                .items_center()
                .justify_center()
                .child(
                    button()
                        .absolute()
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .bg(move || theme().background.opacity(BACKDROP_OPACITY))
                        .accessibility_label("Cancel")
                        .on_press(cancel),
                )
                .child(panel),
        )
}

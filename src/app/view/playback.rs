use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use super::volume_control;
use super::PLAYBACK_BAR_HEIGHT;
use crate::app::store::store;
use crate::app::Message;
use crate::theme::CHROME_BORDER_WIDTH;
use crate::ui::{caption, chrome_button, vertical_rule, Icon, Rule, Tone};

const SQUARE_SIZE: f32 = PLAYBACK_BAR_HEIGHT - 2.0 * CHROME_BORDER_WIDTH;
const TRANSPORT_BUTTON_WIDTH: f32 = SQUARE_SIZE;
const CONTROL_REGION_WIDTH: f32 = TRANSPORT_BUTTON_WIDTH * 3.0 + 3.0 + 53.0;
const SIDE_REGION_WIDTH: f32 = CONTROL_REGION_WIDTH + SQUARE_SIZE + 1.0;
const TRANSPORT_ICON_SIZE: f32 = 12.0;

/// The bar under the workspace: artwork and transport on the left, the
/// progress in the middle, the volume on the right.
pub(super) fn playback_bar() -> impl IntoElement {
    let store = store();
    let transport = move |icon, label, tone| {
        chrome_button(icon, TRANSPORT_ICON_SIZE, label, tone, move || {
            store.dispatch(Message::PlaybackControlPressed)
        })
        .w(px(TRANSPORT_BUTTON_WIDTH))
    };

    let left = view()
        .w(px(SIDE_REGION_WIDTH))
        .flex_shrink_0()
        .flex_row()
        .child(artwork_cell())
        .child(vertical_rule(Rule::Border))
        .child(transport(Icon::Previous, "Previous", Tone::Quiet))
        .child(vertical_rule(Rule::Border))
        .child(transport(Icon::Play, "Play", Tone::Accent))
        .child(vertical_rule(Rule::Border))
        .child(transport(Icon::Next, "Next", Tone::Quiet))
        .child(vertical_rule(Rule::Border));

    let muted = move || theme().muted_foreground;
    let progress = view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .px(px(18.0))
        .child(caption("0:00", 10.0).text_color(muted).tabular_nums())
        .child(view().flex_1().h(px(3.0)).bg(move || theme().muted))
        .child(caption("--:--", 10.0).text_color(muted).tabular_nums());

    let right = view()
        .w(px(SIDE_REGION_WIDTH))
        .flex_shrink_0()
        .flex_row()
        .justify_end()
        .child(vertical_rule(Rule::Border))
        .child(volume_control::volume_control(SQUARE_SIZE));

    view()
        .h(px(PLAYBACK_BAR_HEIGHT))
        .flex_shrink_0()
        .flex_row()
        .p(px(CHROME_BORDER_WIDTH))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || theme().border)
        .shadow_lg()
        .child(left)
        .child(vertical_rule(Rule::Border))
        .child(progress)
        .child(vertical_rule(Rule::Border))
        .child(right)
}

/// The current track's artwork, or an empty square.
fn artwork_cell() -> impl IntoElement {
    let store = store();
    let artwork = store.select(|model| {
        model
            .current_track()
            .and(model.playback.artwork_image())
            .cloned()
            .unwrap_or_default()
    });
    let shown = computed(move || artwork.with(|uri| !uri.is_empty()));
    view()
        .w(px(SQUARE_SIZE))
        .flex_shrink_0()
        .bg(move || theme().muted)
        .child(when(shown, move || {
            image(move || ImageSrc::from(artwork.get()))
                .w(px(SQUARE_SIZE))
                .h(px(SQUARE_SIZE))
                .object_fit(ResizeMode::Cover)
                .label("Album artwork")
        }))
}

use nsh_base::{SliderValue, popover, slider, theme};
use nsh_core::geometry::Axis;
use nsh_core::prelude::*;

use crate::app::Message;
use crate::app::store::store;
use crate::ui::caption;

const POPUP_HEIGHT: f32 = 120.0;
/// Clears the bar's line over the trigger.
const POPUP_OFFSET: f32 = 1.0;
const POPUP_PADDING: f32 = 12.0;
const SLIDER_WIDTH: f32 = 16.0;
const TAG_SIZE: f32 = 9.0;
const TAG_SPACING: f32 = 0.8;
const VALUE_SIZE: f32 = 12.0;

/// The volume, as a cell of the bar with its left edge, that opens a
/// vertical slider hanging above it.
pub(super) fn volume_control(cell_width: f32) -> impl IntoElement {
    let store = store();
    let volume = store.select(|model| model.playback.volume());

    let trigger = view()
        .group()
        .w(px(cell_width))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .border_l_1()
        .border_color(move || theme().border)
        .role(AccessibilityRole::Button)
        .label("Volume")
        .transition_colors()
        .hover(|s| s.bg(move || theme().background))
        .active(|s| s.bg(move || theme().border))
        .child(
            caption("VOL", TAG_SIZE)
                .letter_spacing(AbsoluteLength::from(px(TAG_SPACING)))
                .text_color(move || theme().muted_foreground)
                .group_hover(|s| s.text_color(move || theme().foreground)),
        )
        .child(
            caption(
                move || SharedString::from(volume.get().to_string()),
                VALUE_SIZE,
            )
            .font_weight(FontWeight::MEDIUM)
            .tabular_nums()
            .text_color(move || theme().muted_foreground)
            .group_hover(|s| s.text_color(move || theme().foreground)),
        );

    let volume_slider = slider()
        .axis(Axis::Vertical)
        .min(0.0)
        .max(100.0)
        .step(1.0)
        .value(move || Some(SliderValue::Single(f64::from(volume.get()))))
        .on_change(move |value: &SliderValue| {
            if let SliderValue::Single(value) = value {
                store.dispatch(Message::VolumeChanged(value.round().clamp(0.0, 100.0) as u8));
            }
        })
        .accessibility_label("Volume")
        .w(px(SLIDER_WIDTH))
        .flex_1()
        .track_style(|s| s.flex_1().w(px(SLIDER_WIDTH)).items_center())
        .indicator_style(|s| s.flex_1().w(px(3.0)).bg(move || theme().border))
        .range_style(|s| s.bg(move || theme().primary))
        .thumb_class("bg-primary hover:bg-foreground active:bg-foreground")
        .thumb_style(|s| s.w(px(SLIDER_WIDTH)).h(px(2.0)));

    popover()
        .placement(Placement::Top)
        .offset(px(POPUP_OFFSET))
        .trigger(trigger)
        .popup_style(move |s| {
            s.w(px(cell_width))
                .h(px(POPUP_HEIGHT))
                .py(px(POPUP_PADDING))
                .items_center()
                .bg(move || theme().popover)
                .border_1()
                .border_color(move || theme().border)
        })
        .child(volume_slider)
}

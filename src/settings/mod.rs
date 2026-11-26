use nsh_core::desktop::WindowAttributes;
#[cfg(not(target_os = "macos"))]
use nsh_core::desktop::WindowFrameStyle;
#[cfg(target_os = "macos")]
use nsh_core::desktop::{TitleBarOptions, TitleBarStyle};
use nsh_core::prelude::*;

pub const TITLE: &str = "618 Player";
pub const APP_ID: &str = "com.player618.app";
pub const DEFAULT_SIZE: (i32, i32) = (960, 640);
const MIN_SIZE: (f32, f32) = (840.0, 520.0);
/// The top bar's place in the window (`app::view`): the window's padding
/// from its top left corner, and its height.
#[cfg(target_os = "macos")]
const TOP_BAR_INSET: f32 = 8.0;
#[cfg(target_os = "macos")]
const TOP_BAR_HEIGHT: f32 = 40.0;
/// The cell at the top bar's left end that macOS's traffic lights sit
/// centered in, as wide as two of its cells.
#[cfg(target_os = "macos")]
pub(crate) const TRAFFIC_LIGHTS_CELL_WIDTH: f32 = 80.0;
/// AppKit's traffic lights: from the close button's left to the zoom
/// button's right, and a button's frame height.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_SPAN: f32 = 54.0;
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_HEIGHT: f32 = 16.0;

/// The main window. On Windows and Linux it is frameless, as the app draws
/// its own title bar, window buttons and resize edges. On macOS a borderless
/// window can be neither minimized nor put in full screen, so it keeps the
/// system frame with its title bar hidden: the app draws the bar around the
/// system's traffic lights, and the system resizes the window.
pub(crate) fn window_attributes() -> WindowAttributes {
    let mut attributes = WindowAttributes::default();
    #[cfg(target_os = "macos")]
    {
        let mut title_bar = TitleBarOptions::default();
        title_bar.style = TitleBarStyle::Hidden;
        // As tall as the top bar with the padding above and below it.
        title_bar.height = Some(px(2.0 * TOP_BAR_INSET + TOP_BAR_HEIGHT));
        title_bar.controls_offset = Some(point(
            px(TOP_BAR_INSET + (TRAFFIC_LIGHTS_CELL_WIDTH - TRAFFIC_LIGHTS_SPAN) / 2.0),
            px(TOP_BAR_INSET + (TOP_BAR_HEIGHT - TRAFFIC_LIGHT_HEIGHT) / 2.0),
        ));
        attributes.title_bar = Some(title_bar);
    }
    #[cfg(not(target_os = "macos"))]
    {
        attributes.frame = Some(WindowFrameStyle::None);
    }
    attributes.min_size = Some(Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))));
    attributes
}

pub(crate) fn default_size() -> Size<Points> {
    size(px(DEFAULT_SIZE.0 as f32), px(DEFAULT_SIZE.1 as f32))
}

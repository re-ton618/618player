use nsh_core::desktop::{WindowAttributes, WindowFrameStyle};
use nsh_core::prelude::*;

pub const TITLE: &str = "618 Player";
pub const APP_ID: &str = "com.player618.app";
pub const DEFAULT_SIZE: (i32, i32) = (960, 640);
const MIN_SIZE: (f32, f32) = (840.0, 520.0);

/// The main window: frameless, as the app draws its own title bar, window
/// buttons and resize edges.
pub(crate) fn window_attributes() -> WindowAttributes {
    let mut attributes = WindowAttributes::default();
    attributes.frame = Some(WindowFrameStyle::None);
    attributes.min_size = Some(Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))));
    attributes
}

pub(crate) fn default_size() -> Size<Points> {
    size(px(DEFAULT_SIZE.0 as f32), px(DEFAULT_SIZE.1 as f32))
}

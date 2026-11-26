//! 618 Player: `nsh.json`'s `"root"` names [`App`].

mod app;
mod audio;
mod library;
mod settings;
mod theme;
mod ui;

use nsh_core::desktop::WindowPosition;
use nsh_core::element::desktop::window as app_window;
use nsh_core::prelude::*;

pub use settings::{APP_ID, DEFAULT_SIZE, TITLE};

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("618player/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The app's root: the store, and the main window that shows it.
#[component]
pub fn App() -> impl IntoElement {
    theme::install();
    provide_context(app::Store::new());
    app_window()
        .id("main")
        .title(TITLE)
        .default_size(settings::default_size())
        .default_position(WindowPosition::Center)
        .attributes(settings::window_attributes())
        .content(app::root)
}

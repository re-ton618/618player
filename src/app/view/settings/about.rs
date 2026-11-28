//! About: the version that runs, and on the desktop the cell that asks for
//! a newer one (a phone's app is updated by its store).

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app::store::Store;
#[cfg(not(target_os = "android"))]
use crate::app::Message;
use crate::ui::{self, section_title};

/// About: the app's version under its title, and the cell that checks for
/// a newer one.
pub(super) fn page(store: Store) -> View {
    let border = move || theme().border;
    let title = section_title("Version");
    #[cfg(not(target_os = "android"))]
    let title = title.child(
        ui::chrome_text_button("Check for updates", move || {
            store.dispatch(Message::CheckForUpdatesPressed)
        })
        .border_l_1()
        .border_color(border),
    );
    #[cfg(target_os = "android")]
    let _ = store;

    view()
        .w_full()
        .child(title)
        .child(ui::strip(version()).border_b_1().border_color(border))
}

/// "618 Player 0.2.0 (build 3)": as `nsh package` built it, else this
/// crate's version (a development run).
fn version() -> SharedString {
    let info = nsh_core::element::app_info();
    let version = info
        .as_ref()
        .and_then(|info| info.version.clone())
        .unwrap_or_else(|| format!("{} (development)", env!("CARGO_PKG_VERSION")));
    let build = info
        .and_then(|info| info.build)
        .map(|build| format!(" (build {build})"))
        .unwrap_or_default();
    SharedString::from(format!("{} {version}{build}", crate::TITLE))
}

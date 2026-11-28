//! The app's own updates: nsh's updater asks the source in `nsh.json`'s
//! `"updater"` (the `latest.json` of the newest GitHub Release, signed with
//! the update key) for a newer version and shows nsh's prompt, which
//! downloads it, replaces the installed app and starts it again.
//!
//! Only a packaged app updates (`nsh package`, the installer's or the zip's):
//! `cargo run` and `nsh run` carry no update configuration, so nothing is
//! checked there.

use nsh_core::prelude::spawn;
use nsh_core::runtime::{UpdatePromptOptions, prompt_for_update};

/// Checks for a newer version. `quiet` is a check the user did not ask for
/// (at launch): it shows nothing when the app is up to date, the check
/// fails or the user skipped that version. Otherwise every outcome shows.
pub(crate) fn check(quiet: bool) {
    spawn(prompt_for_update(UpdatePromptOptions::default().quiet(quiet)));
}

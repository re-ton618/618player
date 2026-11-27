//! macOS, Windows and Linux: the user's own folders, music any program may
//! read, the system's media controls ([`media`]) and Discord's Rich
//! Presence ([`presence`]).

pub(crate) mod media;
pub(crate) mod presence;

use std::path::PathBuf;

use super::AudioAccess;

pub(crate) fn init() {}

/// Where the settings and the session are kept (`618player/` under it).
pub(crate) fn config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

/// Where the library's database is kept (`618player/` under it).
pub(crate) fn data_local_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

/// The user's Music folder, the library's first.
pub(crate) fn audio_dir() -> Option<PathBuf> {
    dirs::audio_dir()
}

/// Nothing to ask: the desktop lets the app read the user's files.
pub(crate) async fn request_audio_access() -> AudioAccess {
    AudioAccess::Held
}

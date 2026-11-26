//! What differs between the systems the app is built for: where it keeps
//! its files, where the music is, and what it must be allowed first. Chosen
//! when the app is built, one module a system; how the app is laid out is
//! [`crate::app::Layout`]'s, chosen when it starts.

#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod desktop;

#[cfg(target_os = "android")]
use android as system;
#[cfg(not(target_os = "android"))]
use desktop as system;

pub(crate) use system::{audio_dir, config_dir, data_local_dir, init, request_audio_access};

/// What came of asking for the device's music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "android"),
    allow(dead_code, reason = "the desktop never asks")
)]
pub(crate) enum AudioAccess {
    /// The app had it already: what was scanned is all there is.
    Held,
    /// The user allowed it just now: what was scanned without it is to be
    /// scanned again.
    Granted,
    /// The user said no.
    Refused,
    /// Nobody was asked (the app was not in front): ask again once it is.
    Unasked,
}

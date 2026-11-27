//! The system's media controls. On Windows they are its media transport
//! controls ([`smtc`]): the volume flyout's player, the lock screen, the
//! keyboard's media keys, and whatever else reads them (Discord Music
//! Presence among others). macOS and Linux have none yet.

#[cfg(target_os = "windows")]
mod smtc;

use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub(crate) use smtc::listen;

/// What the system shows as playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NowPlaying {
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    /// The file, whose embedded artwork the controls show.
    pub(crate) path: PathBuf,
    pub(crate) duration_ms: Option<u64>,
    /// Where it is now: the system moves on from here while it plays.
    pub(crate) position_ms: u64,
    pub(crate) playing: bool,
}

/// What the user asked of the system's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Remote {
    Play,
    Pause,
    /// Android's alone: Windows asks for play or pause, as playback is.
    #[allow(dead_code, reason = "Android's controls toggle")]
    Toggle,
    Next,
    Previous,
    /// To this many milliseconds.
    Seek(u64),
    /// The app went for good while it played on (Android's): a desktop
    /// app's playback goes with its window.
    #[allow(dead_code, reason = "Android's activity alone outlives its UI")]
    Stop,
}

/// No controls to hear on this system.
#[cfg(not(target_os = "windows"))]
pub(crate) fn listen(_listener: impl Fn(Remote) + Send + Sync + 'static) {}

/// Shows `now` as playing, or nothing: in the system's controls (Windows'
/// alone) and on Discord.
pub(crate) fn show(now: Option<&NowPlaying>) {
    #[cfg(target_os = "windows")]
    smtc::show(now);
    super::presence::show(now);
}

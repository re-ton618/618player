//! Android: the app's own directory, which only the engine knows, the
//! shared Music folder, the user's leave to read it, and the system's media
//! controls ([`media`]).

pub(crate) mod media;

use std::path::PathBuf;
use std::sync::OnceLock;

use super::AudioAccess;

/// The app's own directory (`Context.getFilesDir`), for the threads that
/// cannot ask the engine (the library's).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Called by the root on the engine thread, before anything is read.
pub(crate) fn init() {
    if let Some(directory) = nsh_core::element::data_dir() {
        let _ = DATA_DIR.set(directory);
    }
}

/// Where the settings and the session are kept (`618player/` under it).
pub(crate) fn config_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

/// Where the library's database is kept (`618player/` under it).
pub(crate) fn data_local_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

/// The shared Music folder, the library's first.
pub(crate) fn audio_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/storage/emulated/0/Music"))
}

/// Asks for the device's music: `READ_MEDIA_AUDIO` from Android 13,
/// `READ_EXTERNAL_STORAGE` before it (both declared in `nsh.json`).
/// Android shows the prompt only while the app is in front. Then, the
/// leave to show notifications (Android 13+), for the playback
/// notification ([`media`]).
pub(crate) async fn request_audio_access() -> AudioAccess {
    use nsh_android::permissions::{self, Permission};
    const WANTED: [&str; 2] = [
        "android.permission.READ_MEDIA_AUDIO",
        "android.permission.READ_EXTERNAL_STORAGE",
    ];
    let access = if permissions::states(&WANTED)
        .is_ok_and(|states| states.contains(&Permission::Granted))
    {
        AudioAccess::Held
    } else {
        match permissions::request(&WANTED).await {
            Ok(answers) if answers.contains(&Permission::Granted) => AudioAccess::Granted,
            Ok(_) => AudioAccess::Refused,
            Err(error) => {
                log::warn!("Music access request failed: {error}");
                return AudioAccess::Unasked;
            }
        }
    };
    request_notifications().await;
    access
}

/// Asks to show notifications, once: Android 13+ hides the playback
/// notification of an app without it on some systems (ColorOS), though
/// media notifications are meant to be exempt.
async fn request_notifications() {
    use nsh_android::permissions::{self, Permission};
    const WANTED: [&str; 1] = ["android.permission.POST_NOTIFICATIONS"];
    if permissions::states(&WANTED).is_ok_and(|states| states.first() != Some(&Permission::Default)) {
        // Granted, refused for good, or not a permission on this system.
        return;
    }
    if let Err(error) = permissions::request(&WANTED).await {
        log::warn!("Notification permission request failed: {error}");
    }
}

//! The system's media controls: the media session (lock screen, headset
//! buttons, the shade's player with its progress bar), its notification and
//! the foreground service that keeps playing in the background. They are
//! Java (`android/java/com/player618/media`, built into `android/media.jar`
//! by `android/build-media.cmd`); this is the bridge to them.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use nsh_android::jni::errors::Result;
use nsh_android::jni::objects::{GlobalRef, JClass, JObject, JValue};
use nsh_android::jni::sys::{jint, jlong, jobject};
use nsh_android::jni::{JNIEnv, NativeMethod};

/// `Playback`, as its class loader names it.
const CLASS: &str = "com.player618.media.Playback";
const UPDATE: &str = "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;\
                      Ljava/lang/String;Ljava/lang/String;JJZ)V";

/// What the system shows as playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NowPlaying {
    pub(crate) title: String,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    /// The file, whose embedded artwork the notification shows.
    pub(crate) path: PathBuf,
    pub(crate) duration_ms: Option<u64>,
    /// Where it is now: the system moves on from here while it plays.
    pub(crate) position_ms: u64,
    pub(crate) playing: bool,
}

/// What the user asked of the system's controls (`Playback`'s codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Remote {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    /// To this many milliseconds.
    Seek(u64),
    /// The app's activity went for good (swiped away from the recent apps):
    /// playback stops with it.
    Stop,
}

impl Remote {
    fn of(code: jint, value: jlong) -> Option<Self> {
        Some(match code {
            1 => Self::Play,
            2 => Self::Pause,
            3 => Self::Toggle,
            4 => Self::Next,
            5 => Self::Previous,
            6 => Self::Seek(u64::try_from(value).unwrap_or(0)),
            7 => Self::Stop,
            _ => return None,
        })
    }
}

type Listener = Box<dyn Fn(Remote) + Send + Sync>;

/// Who hears the controls: the running app's store. A new activity's app
/// takes it over from the last one's.
static LISTENER: Mutex<Option<Listener>> = Mutex::new(None);

/// Has `listener` hear what the user asks of the system's controls, on the
/// main thread.
pub(crate) fn listen(listener: impl Fn(Remote) + Send + Sync + 'static) {
    *LISTENER.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(listener));
}

/// Shows `now` as playing, or nothing.
pub(crate) fn show(now: Option<&NowPlaying>) {
    let result = nsh_android::with_env(|env| {
        let (class, context) = bound(env)?;
        let Some(now) = now else {
            return env
                .call_static_method(class, "clear", "(Landroid/content/Context;)V", &[
                    JValue::Object(&context),
                ])
                .map(drop);
        };
        let title = JObject::from(env.new_string(&now.title)?);
        let artist = text(env, now.artist.as_deref())?;
        let album = text(env, now.album.as_deref())?;
        let path = text(env, now.path.to_str())?;
        let millis = |ms: u64| i64::try_from(ms).unwrap_or(i64::MAX);
        env.call_static_method(class, "update", UPDATE, &[
            JValue::Object(&context),
            JValue::Object(&title),
            JValue::Object(&artist),
            JValue::Object(&album),
            JValue::Object(&path),
            JValue::Long(now.duration_ms.map_or(0, millis)),
            JValue::Long(millis(now.position_ms)),
            JValue::Bool(u8::from(now.playing)),
        ])
        .map(drop)
    });
    if let Err(error) = result {
        log::warn!("Media controls: {error}");
    }
}

/// `Playback`, its `command` bound to [`command`] the first time, and the
/// application's `Context`.
fn bound<'local>(env: &mut JNIEnv<'local>) -> Result<(&'static GlobalRef, JObject<'local>)> {
    static PLAYBACK: OnceLock<GlobalRef> = OnceLock::new();
    // SAFETY: nsh sets it before the app's code runs, to a global reference
    // that is never deleted (`nsh_android::context`).
    let context = unsafe {
        JObject::from_raw(ndk_context::android_context().context() as jobject)
    };
    if let Some(class) = PLAYBACK.get() {
        return Ok((class, context));
    }
    // The app's classes are its loader's: a thread Rust started finds only
    // the system's by name.
    let loader = env
        .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = JObject::from(env.new_string(CLASS)?);
    let class = JClass::from(
        env.call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?,
    );
    env.register_native_methods(&class, &[NativeMethod {
        name: "command".into(),
        sig: "(IJ)V".into(),
        fn_ptr: command as *mut c_void,
    }])?;
    let class = env.new_global_ref(class)?;
    Ok((PLAYBACK.get_or_init(|| class), context))
}

fn text<'local>(env: &mut JNIEnv<'local>, value: Option<&str>) -> Result<JObject<'local>> {
    match value {
        Some(value) => Ok(env.new_string(value)?.into()),
        None => Ok(JObject::null()),
    }
}

/// `Playback.command`: what the user asked, on the main thread.
extern "system" fn command(_: JNIEnv<'_>, _: JClass<'_>, code: jint, value: jlong) {
    let Some(remote) = Remote::of(code, value) else {
        return;
    };
    if let Some(listener) = LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        listener(remote);
    }
}

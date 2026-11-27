//! Windows' System Media Transport Controls. A desktop app gets them from a
//! `MediaPlayer` of its own whose commands are off, as it plays through
//! the engine and not the player; they name the app by the process's
//! AppUserModelID (`settings::APP_ID`, which nsh sets as the window opens).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use windows::core::{Result, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::{
    DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference,
};

use super::{NowPlaying, Remote};

/// A `TimeSpan`'s ticks in a millisecond.
const TICKS_PER_MS: u64 = 10_000;

type Listener = Box<dyn Fn(Remote) + Send + Sync>;

/// Who hears the controls: the running app's store.
static LISTENER: Mutex<Option<Listener>> = Mutex::new(None);

/// The controls, made the first time something plays; `Err` when they
/// could not be (an old or stripped Windows), so that it is tried once.
static CONTROLS: Mutex<Option<std::result::Result<Controls, ()>>> = Mutex::new(None);

/// Has `listener` hear what the user asks of the system's controls, on a
/// thread of the system's.
pub(crate) fn listen(listener: impl Fn(Remote) + Send + Sync + 'static) {
    *LISTENER.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(listener));
}

/// Shows `now` as playing, or nothing.
pub(crate) fn show(now: Option<&NowPlaying>) {
    let mut controls = CONTROLS.lock().unwrap_or_else(PoisonError::into_inner);
    if controls.is_none() {
        if now.is_none() {
            return;
        }
        *controls = Some(Controls::new().map_err(|error| {
            eprintln!("Media controls unavailable: {error}");
        }));
    }
    let Some(Ok(controls)) = controls.as_mut() else {
        return;
    };
    if let Err(error) = controls.show(now) {
        eprintln!("Media controls: {error}");
    }
}

/// What the controls show but where it is: told again only as it changes.
#[derive(PartialEq, Eq)]
struct Shown {
    title: String,
    artist: Option<String>,
    album: Option<String>,
}

struct Controls {
    /// Owns the controls; dropped, they go.
    _player: MediaPlayer,
    smtc: SystemMediaTransportControls,
    shown: Option<Shown>,
    /// The file whose artwork shows, and the artwork once it is read (none
    /// while it is, or when the file has none).
    art: Option<(PathBuf, Option<RandomAccessStreamReference>)>,
}

impl Controls {
    fn new() -> Result<Self> {
        let player = MediaPlayer::new()?;
        // Its own play, pause and next would drive a player nothing plays in.
        player.CommandManager()?.SetIsEnabled(false)?;
        let smtc = player.SystemMediaTransportControls()?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;
        smtc.SetIsStopEnabled(false)?;
        smtc.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(|_, args| {
            let remote = match args.ok()?.Button()? {
                SystemMediaTransportControlsButton::Play => Remote::Play,
                SystemMediaTransportControlsButton::Pause => Remote::Pause,
                SystemMediaTransportControlsButton::Next => Remote::Next,
                SystemMediaTransportControlsButton::Previous => Remote::Previous,
                _ => return Ok(()),
            };
            heard(remote);
            Ok(())
        }))?;
        smtc.PlaybackPositionChangeRequested(&TypedEventHandler::<
            SystemMediaTransportControls,
            PlaybackPositionChangeRequestedEventArgs,
        >::new(|_, args| {
            let ticks = args.ok()?.RequestedPlaybackPosition()?.Duration;
            heard(Remote::Seek(
                u64::try_from(ticks).unwrap_or(0) / TICKS_PER_MS,
            ));
            Ok(())
        }))?;
        Ok(Self {
            _player: player,
            smtc,
            shown: None,
            art: None,
        })
    }

    fn show(&mut self, now: Option<&NowPlaying>) -> Result<()> {
        let Some(now) = now else {
            self.shown = None;
            self.art = None;
            let updater = self.smtc.DisplayUpdater()?;
            updater.ClearAll()?;
            updater.Update()?;
            self.smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
            // Gone from the flyout, and from what reads it.
            return self.smtc.SetIsEnabled(false);
        };
        self.smtc.SetIsEnabled(true)?;
        let shown = Shown {
            title: now.title.clone(),
            artist: now.artist.clone(),
            album: now.album.clone(),
        };
        let new_file = self.art.as_ref().is_none_or(|(path, _)| *path != now.path);
        if new_file {
            self.art = Some((now.path.clone(), None));
            read_artwork(now.path.clone());
        }
        if new_file || self.shown.as_ref() != Some(&shown) {
            let updater = self.smtc.DisplayUpdater()?;
            updater.ClearAll()?;
            updater.SetType(MediaPlaybackType::Music)?;
            let music = updater.MusicProperties()?;
            let text = |value: Option<&str>| HSTRING::from(value.unwrap_or_default());
            music.SetTitle(&HSTRING::from(&shown.title))?;
            music.SetArtist(&text(shown.artist.as_deref()))?;
            music.SetAlbumTitle(&text(shown.album.as_deref()))?;
            if let Some((_, Some(artwork))) = &self.art {
                updater.SetThumbnail(artwork)?;
            }
            updater.Update()?;
            self.shown = Some(shown);
        }
        let span = |ms: u64| TimeSpan {
            Duration: i64::try_from(ms.saturating_mul(TICKS_PER_MS)).unwrap_or(i64::MAX),
        };
        let end = now.duration_ms.unwrap_or(0);
        let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
        timeline.SetStartTime(span(0))?;
        timeline.SetMinSeekTime(span(0))?;
        timeline.SetEndTime(span(end))?;
        timeline.SetMaxSeekTime(span(end))?;
        timeline.SetPosition(span(if end > 0 {
            now.position_ms.min(end)
        } else {
            now.position_ms
        }))?;
        self.smtc.UpdateTimelineProperties(&timeline)?;
        self.smtc.SetPlaybackStatus(if now.playing {
            MediaPlaybackStatus::Playing
        } else {
            MediaPlaybackStatus::Paused
        })
    }
}

/// What the user asked, to whoever listens.
fn heard(remote: Remote) {
    if let Some(listener) = LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        listener(remote);
    }
}

/// Reads the artwork embedded in `path` off the UI thread, and shows it if
/// the file still plays by then.
fn read_artwork(path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("media-artwork".to_owned())
        .spawn(move || {
            let artwork = match artwork(&path) {
                Ok(Some(artwork)) => artwork,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("Media controls artwork: {error}");
                    return;
                }
            };
            let mut controls = CONTROLS.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(Ok(controls)) = controls.as_mut() else {
                return;
            };
            let Some((shown, slot)) = controls.art.as_mut() else {
                return;
            };
            if *shown != path {
                return;
            }
            *slot = Some(artwork.clone());
            let shown = controls.smtc.DisplayUpdater().and_then(|updater| {
                updater.SetThumbnail(&artwork)?;
                updater.Update()
            });
            if let Err(error) = shown {
                eprintln!("Media controls artwork: {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("Media controls artwork thread failed to start: {error}");
    }
}

/// The embedded cover of `path` as a stream the controls read.
fn artwork(path: &Path) -> std::result::Result<Option<RandomAccessStreamReference>, String> {
    let Some(bytes) =
        crate::library::load_embedded_picture(path).map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let stream = (|| {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(&bytes)?;
        writer.StoreAsync()?.join()?;
        writer.FlushAsync()?.join()?;
        // The stream stays open after the writer goes.
        writer.DetachStream()?;
        stream.Seek(0)?;
        RandomAccessStreamReference::CreateFromStream(&stream)
    })();
    stream.map(Some).map_err(|error| error.to_string())
}

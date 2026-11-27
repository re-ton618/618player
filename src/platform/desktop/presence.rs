//! Discord's Rich Presence: what plays, on the user's Discord profile as
//! "Listening to" with its title, artist, progress and album cover. Discord
//! is spoken to over its local IPC (a named pipe on Windows, a Unix socket
//! elsewhere) on a thread of its own, so the app neither waits on Discord nor
//! needs it running; Discord takes the activity down as the app exits.
//!
//! Discord shows only artwork it can fetch, not a file's embedded one: the
//! cover is looked up once an album on Apple's iTunes Search (`curl`, which
//! Windows 10, macOS and Linux all have), and the app's icon shows without.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use super::media::NowPlaying;

/// The Discord application the activity is of: its name heads it
/// ("Listening to <name>") and its icon stands in for a missing cover. From
/// discord.com/developers/applications; empty, nothing is shown.
const CLIENT_ID: &str = "1575985731795427539";

/// Discord takes five activity updates in twenty seconds.
const MIN_GAP: Duration = Duration::from_secs(4);
/// How long to wait before looking for Discord again.
const RECONNECT: Duration = Duration::from_secs(15);
/// How long to wait after Discord turned the app away (a wrong client id).
const REFUSED: Duration = Duration::from_secs(600);
/// Two starts this close are the same: what playing on drifts by.
const SAME_START_MS: u64 = 2_000;
/// A text field's length in Discord, in UTF-16 units.
const MIN_FIELD: usize = 2;
const MAX_FIELD: usize = 128;
/// The biggest reply read from Discord.
const MAX_FRAME: usize = 64 * 1024;
/// The activity's kind ("Listening to") and what the member list names
/// (its `details`, the title).
const LISTENING: u8 = 2;
const SHOWS_DETAILS: u8 = 2;

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;

const HEADER: &str = "618player-discord-v1";

/// What Discord is to show: the track (its position left at zero), and
/// when it would have started had it played straight through (none while
/// paused, which shows no progress).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    now: NowPlaying,
    started_ms: Option<u64>,
}

struct Presence {
    /// The user's choice (Settings).
    shown: bool,
    /// What plays, to show once it is chosen again.
    last: Option<Wanted>,
    /// The thread that speaks to Discord, started as something first plays.
    worker: Option<Sender<Option<Wanted>>>,
}

static PRESENCE: Mutex<Option<Presence>> = Mutex::new(None);

/// Shows `now` as playing on Discord, or nothing.
pub(crate) fn show(now: Option<&NowPlaying>) {
    let wanted = now.map(|now| Wanted {
        started_ms: now
            .playing
            .then(|| unix_ms().saturating_sub(now.position_ms)),
        now: NowPlaying {
            position_ms: 0,
            ..now.clone()
        },
    });
    with(|presence| {
        presence.last.clone_from(&wanted);
        if presence.shown {
            presence.send(wanted);
        }
    });
}

/// Whether the user chose to show what plays on Discord (shown unless
/// they chose not to).
pub(crate) fn saved() -> bool {
    fs::read_to_string(choice_path())
        .ok()
        .and_then(|contents| parse(&contents))
        .unwrap_or(true)
}

/// Shows what plays on Discord or takes it down, and keeps the choice.
pub(crate) fn choose(shown: bool) {
    let path = choice_path();
    let saved = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&path, format!("{HEADER}\nshown={shown}\n")));
    if let Err(error) = saved {
        eprintln!("Discord presence choice save failed: {error}");
    }
    with(|presence| {
        if presence.shown != shown {
            presence.shown = shown;
            let wanted = if shown { presence.last.clone() } else { None };
            presence.send(wanted);
        }
    });
}

fn with<T>(f: impl FnOnce(&mut Presence) -> T) -> T {
    let mut presence = PRESENCE.lock().unwrap_or_else(PoisonError::into_inner);
    f(presence.get_or_insert_with(|| Presence {
        shown: saved(),
        last: None,
        worker: None,
    }))
}

impl Presence {
    fn send(&mut self, wanted: Option<Wanted>) {
        if CLIENT_ID.is_empty() {
            return;
        }
        if self.worker.is_none() {
            if wanted.is_none() {
                return;
            }
            let (orders, heard) = mpsc::channel();
            let spawned = thread::Builder::new()
                .name("discord-presence".into())
                .spawn(move || Worker::new().run(&heard));
            match spawned {
                Ok(_) => self.worker = Some(orders),
                Err(error) => {
                    eprintln!("Discord presence unavailable: {error}");
                    return;
                }
            }
        }
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.send(wanted).is_err())
        {
            self.worker = None;
        }
    }
}

fn choice_path() -> PathBuf {
    super::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("618player/discord-v1.txt")
}

fn parse(contents: &str) -> Option<bool> {
    let mut lines = contents.lines();
    if lines.next() != Some(HEADER) {
        return None;
    }
    lines.next()?.strip_prefix("shown=")?.parse().ok()
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Discord's end of the IPC.
trait Ipc: Read + Write + Send {}
impl<T: Read + Write + Send> Ipc for T {}

struct Worker {
    ipc: Option<Box<dyn Ipc>>,
    /// When Discord may be looked for again.
    retry_at: Instant,
    /// When Discord was last told something.
    sent_at: Option<Instant>,
    /// What Discord shows.
    shown: Option<Wanted>,
    /// An album's (or a lone track's) cover, or none found.
    covers: HashMap<String, Option<String>>,
    nonce: u64,
}

impl Worker {
    fn new() -> Self {
        Self {
            ipc: None,
            retry_at: Instant::now(),
            sent_at: None,
            shown: None,
            covers: HashMap::new(),
            nonce: 0,
        }
    }

    /// Tells Discord the latest it was asked, no faster than it takes it.
    fn run(mut self, orders: &Receiver<Option<Wanted>>) {
        let mut pending: Option<Option<Wanted>> = None;
        loop {
            let received = match pending {
                None => orders.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(_) => orders.recv_timeout(self.wait()),
            };
            match received {
                Ok(order) => pending = Some(order),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if self.wait().is_zero()
                && let Some(order) = &pending
                && self.apply(order)
            {
                pending = None;
            }
        }
    }

    fn wait(&self) -> Duration {
        let now = Instant::now();
        let mut ready = self.sent_at.map_or(now, |at| at + MIN_GAP);
        if self.ipc.is_none() {
            ready = ready.max(self.retry_at);
        }
        ready.saturating_duration_since(now)
    }

    /// Shows `order`; false when Discord could not be told.
    fn apply(&mut self, order: &Option<Wanted>) -> bool {
        if same(order, &self.shown) {
            return true;
        }
        if self.ipc.is_none() {
            if order.is_none() {
                self.shown = None;
                return true;
            }
            match connect() {
                Ok(ipc) => self.ipc = Some(ipc),
                Err(error) => {
                    let refused = error.kind() == io::ErrorKind::InvalidData;
                    if error.kind() != io::ErrorKind::NotFound {
                        eprintln!("Discord presence: {error}");
                    }
                    self.retry_at = Instant::now() + if refused { REFUSED } else { RECONNECT };
                    return false;
                }
            }
        }
        let activity = order.as_ref().map(|wanted| self.activity(wanted));
        if let Err(error) = self.set_activity(activity) {
            eprintln!("Discord presence: {error}");
            self.ipc = None;
            self.shown = None;
            self.retry_at = Instant::now() + RECONNECT;
            return false;
        }
        self.sent_at = Some(Instant::now());
        self.shown.clone_from(order);
        true
    }

    fn activity(&mut self, wanted: &Wanted) -> Value {
        let now = &wanted.now;
        let mut activity = json!({
            "type": LISTENING,
            "status_display_type": SHOWS_DETAILS,
            "details": field(&now.title),
        });
        if let Some(artist) = &now.artist {
            activity["state"] = field(artist).into();
        }
        if let (Some(start), Some(duration)) = (
            wanted.started_ms,
            now.duration_ms.filter(|&duration| duration > 0),
        ) {
            activity["timestamps"] = json!({ "start": start, "end": start + duration });
        }
        let mut assets = Map::new();
        if let Some(cover) = self.cover(now) {
            assets.insert("large_image".into(), cover.into());
        }
        if let Some(album) = &now.album {
            assets.insert("large_text".into(), field(album).into());
        }
        if !assets.is_empty() {
            activity["assets"] = Value::Object(assets);
        }
        activity
    }

    fn cover(&mut self, now: &NowPlaying) -> Option<String> {
        let (entity, name, wanted) = match &now.album {
            Some(album) => ("album", "collectionName", album),
            None => ("song", "trackName", &now.title),
        };
        let artist = now.artist.as_deref();
        let key = format!("{}\0{entity}\0{wanted}", artist.unwrap_or_default()).to_lowercase();
        self.covers
            .entry(key)
            .or_insert_with(|| look_up(artist, entity, name, wanted))
            .clone()
    }

    fn set_activity(&mut self, activity: Option<Value>) -> io::Result<()> {
        let ipc = self.ipc.as_deref_mut().ok_or(io::ErrorKind::NotConnected)?;
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let command = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": nonce,
        });
        write_frame(ipc, FRAME, &command)?;
        loop {
            match read_frame(ipc)? {
                (PING, body) => write_frame(ipc, PONG, &body)?,
                (CLOSE, body) => return Err(io::Error::other(format!("Discord closed: {body}"))),
                (FRAME, reply) if reply["nonce"] == nonce => {
                    if reply["evt"] == "ERROR" {
                        eprintln!("Discord presence refused: {}", reply["data"]["message"]);
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

/// Whether Discord, showing `a`, shows `b` too.
fn same(a: &Option<Wanted>, b: &Option<Wanted>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.now == b.now
                && match (a.started_ms, b.started_ms) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.abs_diff(b) <= SAME_START_MS,
                    _ => false,
                }
        }
        _ => false,
    }
}

/// A text as Discord takes it: 2 to 128 UTF-16 units.
fn field(value: &str) -> String {
    let value = value.trim();
    let mut field = String::new();
    if value.encode_utf16().count() <= MAX_FIELD {
        field.push_str(value);
    } else {
        let mut units = 0;
        for c in value.chars() {
            units += c.len_utf16();
            if units > MAX_FIELD - 1 {
                break;
            }
            field.push(c);
        }
        field.push('\u{2026}');
    }
    while field.encode_utf16().count() < MIN_FIELD {
        field.push('\u{200B}');
    }
    field
}

fn connect() -> io::Result<Box<dyn Ipc>> {
    let mut ipc = open()?;
    write_frame(
        ipc.as_mut(),
        HANDSHAKE,
        &json!({ "v": 1, "client_id": CLIENT_ID }),
    )?;
    match read_frame(ipc.as_mut())? {
        (FRAME, reply) if reply["evt"] == "READY" => Ok(ipc),
        (_, reply) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Discord refused the app: {reply}"),
        )),
    }
}

#[cfg(windows)]
fn open() -> io::Result<Box<dyn Ipc>> {
    let mut last = io::Error::from(io::ErrorKind::NotFound);
    for n in 0..10 {
        match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!(r"\\.\pipe\discord-ipc-{n}"))
        {
            Ok(pipe) => return Ok(Box::new(pipe)),
            Err(error) => last = error,
        }
    }
    Err(last)
}

#[cfg(unix)]
fn open() -> io::Result<Box<dyn Ipc>> {
    use std::os::unix::net::UnixStream;

    let roots = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain([PathBuf::from("/tmp")]);
    // Discord itself, then as Flatpak and Snap keep it, then Vesktop's.
    let homes = [
        "",
        "app/com.discordapp.Discord",
        "snap.discord",
        ".flatpak/dev.vencord.Vesktop/xdg-run",
    ];
    for root in roots {
        for home in homes {
            for n in 0..10 {
                if let Ok(socket) =
                    UnixStream::connect(root.join(home).join(format!("discord-ipc-{n}")))
                {
                    return Ok(Box::new(socket));
                }
            }
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

fn write_frame(ipc: &mut dyn Ipc, op: u32, body: &Value) -> io::Result<()> {
    let body = body.to_string();
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend(op.to_le_bytes());
    frame.extend(length.to_le_bytes());
    frame.extend(body.as_bytes());
    ipc.write_all(&frame)?;
    ipc.flush()
}

fn read_frame(ipc: &mut dyn Ipc) -> io::Result<(u32, Value)> {
    let mut header = [0; 8];
    ipc.read_exact(&mut header)?;
    let [a, b, c, d, e, f, g, h] = header;
    let op = u32::from_le_bytes([a, b, c, d]);
    let length = usize::try_from(u32::from_le_bytes([e, f, g, h])).map_err(io::Error::other)?;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Discord's reply is too long",
        ));
    }
    let mut body = vec![0; length];
    ipc.read_exact(&mut body)?;
    Ok((op, serde_json::from_slice(&body).map_err(io::Error::other)?))
}

/// The cover of `wanted` (an album, or a song) by `artist` on iTunes.
fn look_up(artist: Option<&str>, entity: &str, name: &str, wanted: &str) -> Option<String> {
    let term = artist.map_or_else(|| wanted.to_owned(), |artist| format!("{artist} {wanted}"));
    let mut curl = Command::new("curl");
    curl.args([
        "--silent",
        "--fail",
        "--location",
        "--max-time",
        "5",
        "--get",
    ])
    .arg("https://itunes.apple.com/search")
    .arg("--data-urlencode")
    .arg(format!("term={term}"))
    .args(["--data", "media=music", "--data", "limit=10", "--data"])
    .arg(format!("entity={entity}"))
    .stdin(Stdio::null())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// No console flashes up for it.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        curl.creation_flags(CREATE_NO_WINDOW);
    }
    let output = curl
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let found: Value = serde_json::from_slice(&output.stdout).ok()?;
    cover_in(&found, name, wanted, artist)
}

/// The first result named `wanted` by `artist`, its cover at 512 pixels.
fn cover_in(found: &Value, name: &str, wanted: &str, artist: Option<&str>) -> Option<String> {
    let wanted = folded(wanted);
    if wanted.is_empty() {
        return None;
    }
    let artist = artist.map(folded);
    let alike =
        |a: &str, b: &str| !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a));
    found["results"].as_array()?.iter().find(|result| {
        alike(&folded(result[name].as_str().unwrap_or_default()), &wanted)
            && artist.as_deref().is_none_or(|artist| {
                artist.is_empty()
                    || alike(
                        &folded(result["artistName"].as_str().unwrap_or_default()),
                        artist,
                    )
            })
    })?["artworkUrl100"]
        .as_str()
        .map(|url| url.replace("100x100bb", "512x512bb"))
}

/// Letters and digits alone, lowercase: names as both sides spell them.
fn folded(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn wanted(started_ms: Option<u64>) -> Option<Wanted> {
        Some(Wanted {
            now: NowPlaying {
                title: "Song".into(),
                artist: Some("Artist".into()),
                album: Some("Album".into()),
                path: PathBuf::from("song.flac"),
                duration_ms: Some(200_000),
                position_ms: 0,
                playing: started_ms.is_some(),
            },
            started_ms,
        })
    }

    #[test]
    fn fields_are_kept_to_discords_lengths() {
        assert_eq!(field("  Song "), "Song");
        assert_eq!(field("A").encode_utf16().count(), MIN_FIELD);
        let long = field(&"x".repeat(300));
        assert_eq!(long.encode_utf16().count(), MAX_FIELD);
        assert!(long.ends_with('\u{2026}'));
        let emoji = field(&"\u{1F3B5}".repeat(100));
        assert!(emoji.encode_utf16().count() <= MAX_FIELD);
    }

    #[test]
    fn playing_on_is_not_told_again_but_a_seek_or_a_pause_is() {
        assert!(same(&wanted(Some(10_000)), &wanted(Some(11_500))));
        assert!(!same(&wanted(Some(10_000)), &wanted(Some(40_000))));
        assert!(!same(&wanted(Some(10_000)), &wanted(None)));
        assert!(!same(&wanted(None), &None));
        assert!(same(&None, &None));
    }

    #[test]
    fn the_activity_shows_the_track_its_progress_and_its_album() {
        let mut worker = Worker::new();
        worker
            .covers
            .insert("artist\0album\0album".into(), Some("https://cover".into()));
        let activity = worker.activity(&wanted(Some(1_000)).unwrap());
        assert_eq!(activity["type"], 2);
        assert_eq!(activity["details"], "Song");
        assert_eq!(activity["state"], "Artist");
        assert_eq!(activity["timestamps"]["start"], 1_000);
        assert_eq!(activity["timestamps"]["end"], 201_000);
        assert_eq!(activity["assets"]["large_image"], "https://cover");
        assert_eq!(activity["assets"]["large_text"], "Album");
        let paused = worker.activity(&wanted(None).unwrap());
        assert!(paused.get("timestamps").is_none());
    }

    #[test]
    fn frames_are_an_op_a_length_and_json() {
        let mut ipc = Cursor::new(Vec::new());
        write_frame(&mut ipc, FRAME, &json!({ "evt": "READY" })).unwrap();
        assert_eq!(&ipc.get_ref()[..8], &[1, 0, 0, 0, 15, 0, 0, 0]);
        ipc.set_position(0);
        assert_eq!(
            read_frame(&mut ipc).unwrap(),
            (FRAME, json!({ "evt": "READY" }))
        );
    }

    #[test]
    fn the_cover_is_the_matching_albums_at_512_pixels() {
        let found = json!({ "results": [
            { "collectionName": "Other", "artistName": "Artist", "artworkUrl100": "https://a/100x100bb.jpg" },
            { "collectionName": "Album (Deluxe Edition)", "artistName": "Artist", "artworkUrl100": "https://b/100x100bb.jpg" },
        ]});
        assert_eq!(
            cover_in(
                &found,
                "collectionName",
                "Album",
                Some("Artist feat. Someone")
            )
            .as_deref(),
            Some("https://b/512x512bb.jpg")
        );
        assert_eq!(
            cover_in(&found, "collectionName", "Album", Some("Nobody")),
            None
        );
        assert_eq!(cover_in(&found, "collectionName", "!!", None), None);
    }

    #[test]
    fn the_choice_round_trips() {
        assert_eq!(parse(&format!("{HEADER}\nshown=false\n")), Some(false));
        assert_eq!(parse(&format!("{HEADER}\nshown=true\n")), Some(true));
        assert_eq!(parse("618player-discord-v9\nshown=false\n"), None);
    }
}

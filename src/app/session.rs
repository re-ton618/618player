//! What a run leaves for the next: the queue, where in it and in the
//! current track it was, shuffle, repeat and the volume. Kept beside the
//! workspace, as a few lines of text.
//!
//! Saving is off the UI thread ([`Writer`]): a queue of a whole library is
//! hundreds of kilobytes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use super::queue::Repeat;

type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HEADER: &str = "618player-session-v1";
/// How long quitting waits for the last save.
const FLUSH_PATIENCE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Session {
    pub(super) volume: u8,
    pub(super) repeat: Repeat,
    /// The queue's track ids, in play order.
    pub(super) order: Vec<i64>,
    /// While shuffled: the same tracks in the order they were queued in.
    pub(super) unshuffled: Option<Vec<i64>>,
    /// Where in `order` the current track is.
    pub(super) current: Option<usize>,
    /// Where in the current track playback was.
    pub(super) position_ms: u64,
}

pub(super) fn load() -> StoreResult<Option<Session>> {
    load_from(&session_path())
}

fn session_path() -> PathBuf {
    crate::platform::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("618player/session-v1.txt")
}

fn load_from(path: &Path) -> StoreResult<Option<Session>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    parse(&contents).map(Some)
}

fn parse(contents: &str) -> StoreResult<Session> {
    let mut lines = contents.lines();
    if lines.next() != Some(HEADER) {
        return Err("unknown session header".into());
    }
    let mut field = |name: &str| -> StoreResult<&str> {
        lines
            .next()
            .and_then(|line| line.strip_prefix(name))
            .and_then(|line| line.strip_prefix('='))
            .ok_or_else(|| format!("missing {name} line").into())
    };
    let ids = |list: &str| -> StoreResult<Vec<i64>> {
        list.split(',')
            .filter(|id| !id.is_empty())
            .map(|id| id.parse::<i64>().map_err(Into::into))
            .collect()
    };

    let volume = field("volume")?.parse::<u8>()?.min(100);
    let shuffle = match field("shuffle")? {
        "0" => false,
        "1" => true,
        _ => return Err("shuffle is neither 0 nor 1".into()),
    };
    let repeat = Repeat::from_slug(field("repeat")?).ok_or("unknown repeat mode")?;
    let current = match field("current")? {
        "" => None,
        index => Some(index.parse::<usize>()?),
    };
    let position_ms = field("position_ms")?.parse::<u64>()?;
    let order = ids(field("queue")?)?;
    let unshuffled = ids(field("unshuffled")?)?;
    if lines.next().is_some() {
        return Err("lines after the session's last".into());
    }

    Ok(Session {
        volume,
        repeat,
        order,
        unshuffled: shuffle.then_some(unshuffled),
        current,
        position_ms,
    })
}

fn format(session: &Session) -> String {
    let ids = |ids: &[i64]| ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    format!(
        "{HEADER}\nvolume={}\nshuffle={}\nrepeat={}\ncurrent={}\nposition_ms={}\nqueue={}\nunshuffled={}\n",
        session.volume,
        u8::from(session.unshuffled.is_some()),
        session.repeat.slug(),
        session
            .current
            .map_or_else(String::new, |index| index.to_string()),
        session.position_ms,
        ids(&session.order),
        ids(session.unshuffled.as_deref().unwrap_or_default()),
    )
}

/// Writes the whole file beside the old one, then puts it in its place: a
/// run that is killed mid-write leaves the session it found.
fn save_to(path: &Path, session: &Session) -> StoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let scratch = path.with_extension("tmp");
    fs::write(&scratch, format(session))?;
    fs::rename(&scratch, path)?;
    Ok(())
}

enum Job {
    Save(Session),
    /// As a save, and tells once it is on disk.
    Flush(Session, Sender<()>),
}

/// The handle of the thread that saves the session, which ends when this
/// is dropped.
pub(super) struct Writer {
    jobs: Sender<Job>,
}

impl Writer {
    pub(super) fn spawn() -> Self {
        Self::spawn_at(session_path())
    }

    fn spawn_at(path: PathBuf) -> Self {
        let (jobs, receiver) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("session".to_owned())
            .spawn(move || write(&path, &receiver))
        {
            eprintln!("Session thread failed to start: {error}");
        }
        Self { jobs }
    }

    /// Saves `session` soon. Of several asked for in a row only the last
    /// is written.
    pub(super) fn save(&self, session: Session) {
        let _ = self.jobs.send(Job::Save(session));
    }

    /// Saves `session` and waits until it is on disk: for when the app is
    /// about to go.
    pub(super) fn flush(&self, session: Session) {
        let (done, written) = mpsc::channel();
        if self.jobs.send(Job::Flush(session, done)).is_ok() {
            let _ = written.recv_timeout(FLUSH_PATIENCE);
        }
    }
}

fn write(path: &Path, jobs: &Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        // Whatever else was asked meanwhile: the last one is the session.
        let mut waiting = Vec::new();
        let mut latest = None;
        for job in std::iter::once(job).chain(jobs.try_iter()) {
            match job {
                Job::Save(session) => latest = Some(session),
                Job::Flush(session, done) => {
                    latest = Some(session);
                    waiting.push(done);
                }
            }
        }
        if let Some(session) = latest
            && let Err(error) = save_to(path, &session)
        {
            eprintln!("Session save failed: {error}");
        }
        for done in waiting {
            let _ = done.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn session() -> Session {
        Session {
            volume: 35,
            repeat: Repeat::All,
            order: vec![7, 3, 9, 3],
            unshuffled: Some(vec![3, 3, 7, 9]),
            current: Some(2),
            position_ms: 61_500,
        }
    }

    #[test]
    fn sessions_round_trip_shuffled_plain_and_empty() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/session.txt");
        assert!(load_from(&path).unwrap().is_none());

        for session in [
            session(),
            Session {
                unshuffled: None,
                repeat: Repeat::One,
                ..session()
            },
            Session {
                volume: 100,
                repeat: Repeat::Off,
                order: Vec::new(),
                unshuffled: None,
                current: None,
                position_ms: 0,
            },
        ] {
            save_to(&path, &session).unwrap();
            assert_eq!(load_from(&path).unwrap(), Some(session));
        }
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn a_session_is_seven_lines_of_text() {
        assert_eq!(
            format(&session()),
            "618player-session-v1\nvolume=35\nshuffle=1\nrepeat=all\ncurrent=2\n\
             position_ms=61500\nqueue=7,3,9,3\nunshuffled=3,3,7,9\n"
        );
        // Line endings an editor may have left are read all the same.
        let edited = format(&session()).replace('\n', "\r\n");
        assert_eq!(parse(&edited).unwrap(), session());
    }

    #[test]
    fn what_is_not_a_session_is_refused() {
        let good = format(&session());
        for bad in [
            good.replace("618player-session-v1", "618player-session-v9"),
            good.replace("volume=35", "volume=loud"),
            good.replace("shuffle=1", "shuffle=yes"),
            good.replace("repeat=all", "repeat=twice"),
            good.replace("current=2", "current=-1"),
            good.replace("queue=7,3,9,3", "queue=7,x"),
            good.replace("position_ms=61500\n", ""),
            format!("{good}extra\n"),
            String::new(),
        ] {
            assert!(parse(&bad).is_err(), "accepted {bad:?}");
        }
        // A volume beyond full is full.
        assert_eq!(
            parse(&good.replace("volume=35", "volume=200"))
                .unwrap()
                .volume,
            100
        );
    }

    #[test]
    fn the_writer_saves_off_thread_and_a_flush_waits_for_the_last() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("session.txt");
        let writer = Writer::spawn_at(path.clone());

        for position_ms in 0..200 {
            writer.save(Session {
                position_ms,
                ..session()
            });
        }
        let last = Session {
            position_ms: 999,
            ..session()
        };
        writer.flush(last.clone());

        assert_eq!(load_from(&path).unwrap(), Some(last));
    }
}

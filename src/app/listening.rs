//! How much of a track is to be heard before it counts as listened to:
//! the user's choice, kept beside the workspace as a line of text.

use std::fs;
use std::path::{Path, PathBuf};

use super::playback::DEFAULT_LISTEN_PERCENT;

type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HEADER: &str = "618player-listening-v1";

/// The share, in percent, the user chose last, or the default.
pub(super) fn saved() -> u8 {
    match load_from(&choice_path()) {
        Ok(percent) => percent.unwrap_or(DEFAULT_LISTEN_PERCENT),
        Err(error) => {
            eprintln!("Listening threshold restore failed: {error}");
            DEFAULT_LISTEN_PERCENT
        }
    }
}

/// Keeps `percent` for the next launch.
pub(super) fn save(percent: u8) {
    if let Err(error) = save_to(&choice_path(), percent) {
        eprintln!("Listening threshold save failed: {error}");
    }
}

fn choice_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("618player/listening-v1.txt")
}

fn load_from(path: &Path) -> StoreResult<Option<u8>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    parse(&contents).map(Some)
}

fn parse(contents: &str) -> StoreResult<u8> {
    let mut lines = contents.lines();
    if lines.next() != Some(HEADER) {
        return Err("unknown listening header".into());
    }
    let percent = lines
        .next()
        .and_then(|line| line.strip_prefix("percent="))
        .ok_or("missing percent line")?
        .parse::<u8>()?;
    Ok(percent.clamp(1, 100))
}

fn save_to(path: &Path, percent: u8) -> StoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{HEADER}\npercent={percent}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn the_threshold_round_trips_and_is_kept_to_a_percent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/listening.txt");
        assert_eq!(load_from(&path).unwrap(), None);

        for percent in [1, 10, 55, 100] {
            save_to(&path, percent).unwrap();
            assert_eq!(load_from(&path).unwrap(), Some(percent));
        }
        assert_eq!(parse(&format!("{HEADER}\npercent=0\n")).unwrap(), 1);
        assert_eq!(parse(&format!("{HEADER}\npercent=250\n")).unwrap(), 100);
        assert!(parse("618player-listening-v9\npercent=10\n").is_err());
        assert!(parse(&format!("{HEADER}\npercent=lots\n")).is_err());
    }
}

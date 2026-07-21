use std::fs;
use std::path::{Path, PathBuf};

use super::tabs::{Kind, State};

type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HEADER: &str = "618player-workspace-v1";

pub(super) fn load() -> StoreResult<Option<State>> {
    load_from(&workspace_path())
}

pub(super) fn save(state: &State) -> StoreResult<()> {
    save_to(&workspace_path(), state)
}

fn workspace_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("618player/workspace-v1.txt")
}

fn load_from(path: &Path) -> StoreResult<Option<State>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    parse(&contents).map(Some)
}

fn parse(contents: &str) -> StoreResult<State> {
    let logical = contents
        .strip_suffix("\r\n")
        .or_else(|| contents.strip_suffix('\n'))
        .unwrap_or(contents);
    let lines: Vec<_> = logical.split('\n').collect();
    if lines.len() != 3 {
        return Err("workspace must contain exactly three logical lines".into());
    }
    if lines[0] != HEADER {
        return Err("unknown workspace header".into());
    }
    let active = lines[1]
        .strip_prefix("active=")
        .ok_or("missing active line")?;
    let tabs = lines[2].strip_prefix("tabs=").ok_or("missing tabs line")?;

    let active = if active.is_empty() {
        None
    } else {
        Some(Kind::from_slug(active).ok_or("unknown active tab kind")?)
    };
    let order = if tabs.is_empty() {
        Vec::new()
    } else {
        tabs.split(',')
            .map(|slug| Kind::from_slug(slug).ok_or("unknown tab kind"))
            .collect::<Result<Vec<_>, _>>()?
    };

    State::from_parts(order, active).map_err(Into::into)
}

fn save_to(path: &Path, state: &State) -> StoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let active = state.active().map_or("", Kind::slug);
    let tabs = state
        .order()
        .iter()
        .map(|kind| kind.slug())
        .collect::<Vec<_>>()
        .join(",");
    fs::write(path, format!("{HEADER}\nactive={active}\ntabs={tabs}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{load_from, parse, save_to};
    use crate::app::tabs::{Kind, State};

    const L: Kind = Kind::Library;
    const N: Kind = Kind::NowPlaying;
    const S: Kind = Kind::SongInformation;

    fn assert_state(state: &State, order: &[Kind], active: Option<Kind>) {
        assert_eq!(state.order(), order);
        assert_eq!(state.active(), active);
    }

    #[test]
    fn populated_and_empty_states_round_trip() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/workspace.txt");
        let populated = State::from_parts(vec![S, L, N], Some(L)).unwrap();
        save_to(&path, &populated).unwrap();
        assert_state(&load_from(&path).unwrap().unwrap(), &[S, L, N], Some(L));

        let empty = State::from_parts(Vec::new(), None).unwrap();
        save_to(&path, &empty).unwrap();
        assert_state(&load_from(&path).unwrap().unwrap(), &[], None);
    }

    #[test]
    fn accepts_supported_terminal_endings() {
        for ending in ["", "\n", "\r\n"] {
            let text = format!("618player-workspace-v1\nactive=library\ntabs=library{ending}");
            assert_state(&parse(&text).unwrap(), &[L], Some(L));
        }
    }

    #[test]
    fn rejects_line_shape_and_header_errors() {
        for text in [
            "bad\nactive=library\ntabs=library",
            "618player-workspace-v1\nactive=library",
            "618player-workspace-v1\nactive=library\ntabs=library\nextra",
            "618player-workspace-v1\n\nactive=library\ntabs=library",
            "618player-workspace-v1\nactive=library\ntabs=library\n\n",
            "618player-workspace-v1\r\nactive=library\r\ntabs=library\r\n",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn rejects_duplicate_and_unknown_kinds() {
        for text in [
            "618player-workspace-v1\nactive=library\ntabs=library,library",
            "618player-workspace-v1\nactive=library\ntabs=library,queue",
            "618player-workspace-v1\nactive=queue\ntabs=library",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn rejects_active_list_mismatches() {
        for text in [
            "618player-workspace-v1\nactive=\ntabs=library",
            "618player-workspace-v1\nactive=library\ntabs=",
            "618player-workspace-v1\nactive=now-playing\ntabs=library",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn missing_file_returns_none() {
        let directory = tempdir().unwrap();
        assert!(
            load_from(&directory.path().join("missing"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reports_read_directory_and_write_errors() {
        let directory = tempdir().unwrap();
        assert!(load_from(directory.path()).is_err());

        let blocking_file = directory.path().join("parent");
        fs::write(&blocking_file, "not a directory").unwrap();
        assert!(save_to(&blocking_file.join("workspace"), &State::default()).is_err());
        assert!(save_to(directory.path(), &State::default()).is_err());
    }
}

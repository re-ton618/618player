use std::cmp::Ordering;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "flac", "m4a", "mp3", "ogg", "opus", "wav", "wma",
];

#[derive(Debug, Clone)]
pub(super) struct DiscoveredTrack {
    pub(super) path: PathBuf,
    pub(super) relative_path: PathBuf,
    pub(super) size: u64,
    pub(super) modified_ns: i64,
    pub(super) file_identity: Option<String>,
}

/// Every supported file under `directory`, which is the library folder
/// `root` or a folder inside it. What cannot be read is left out, and a
/// `directory` that is gone holds nothing.
pub(super) fn scan(root: &Path, directory: &Path) -> Vec<DiscoveredTrack> {
    let mut tracks = Vec::new();
    for result in WalkDir::new(directory).follow_links(false).min_depth(1) {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                // A folder that is gone is what the scan is here to find.
                if error.depth() > 0 {
                    eprintln!("Library scan skipped an entry: {error}");
                }
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_supported_audio(entry.path()) {
            continue;
        }
        let path = entry.into_path();
        let (Ok(relative_path), Ok(metadata)) = (path.strip_prefix(root), path.metadata()) else {
            continue;
        };
        let relative_path = relative_path.to_path_buf();
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| {
                duration.as_nanos().min(i64::MAX as u128) as i64
            });

        tracks.push(DiscoveredTrack {
            file_identity: file_identity(&path, &metadata),
            path,
            relative_path,
            size: metadata.len(),
            modified_ns,
        });
    }

    tracks.sort_by(|left, right| {
        compare_ascii_case_insensitive(&left.relative_path, &right.relative_path)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    tracks
}

/// What names the file itself rather than where it is, so that a renamed
/// or moved file is known again: the device and the inode.
#[cfg(unix)]
fn file_identity(_path: &Path, metadata: &Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;

    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}

/// What names the file itself rather than where it is, so that a renamed
/// or moved file is known again: the volume's serial number and the
/// file's index on it. NTFS keeps the index through renames and moves
/// within the volume and does not hand it to another file; FAT and exFAT
/// make one up from where the file's entry is, which a rename may change.
#[cfg(windows)]
fn file_identity(path: &Path, _metadata: &Metadata) -> Option<String> {
    let handle = winapi_util::Handle::from_path_any(path).ok()?;
    let information = winapi_util::file::information(&handle).ok()?;
    let index = information.file_index();
    (index != 0).then(|| format!("{:x}:{index:x}", information.volume_serial_number()))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_path: &Path, _metadata: &Metadata) -> Option<String> {
    None
}

pub(super) fn supports_embedded_metadata(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wma"))
}

pub(super) fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

/// `path` as the file system tells paths apart: Windows does not mind
/// their case, so there two spellings of one path give one key.
pub(super) fn path_key(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// Whether `path` is `directory` or somewhere under it.
pub(super) fn is_within(path: &Path, directory: &Path) -> bool {
    path_key(path).starts_with(path_key(directory))
}

fn compare_ascii_case_insensitive(left: &Path, right: &Path) -> Ordering {
    left.as_os_str()
        .to_string_lossy()
        .bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(
            right
                .as_os_str()
                .to_string_lossy()
                .bytes()
                .map(|byte| byte.to_ascii_lowercase()),
        )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn accepts_supported_extensions_case_insensitively() {
        for path in [
            "track.flac",
            "track.MP3",
            "track.Wav",
            "track.ogg",
            "track.OPUS",
            "track.m4a",
            "track.AAC",
            "track.wma",
        ] {
            assert!(is_supported_audio(Path::new(path)), "rejected {path}");
        }
    }

    #[test]
    fn wma_uses_fallback_metadata() {
        assert!(!supports_embedded_metadata(Path::new("track.wma")));
        assert!(supports_embedded_metadata(Path::new("track.flac")));
    }

    #[test]
    fn recursively_discovers_fingerprinted_supported_files() -> std::io::Result<()> {
        let root = tempdir()?;
        let nested = root.path().join("Artist").join("Album");
        fs::create_dir_all(&nested)?;
        fs::write(root.path().join("z-last.MP3"), [1, 2, 3])?;
        fs::write(nested.join("A-first.flac"), [1])?;
        fs::write(nested.join("cover.jpg"), [])?;

        let tracks = scan(root.path(), root.path());

        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks[0].relative_path,
            PathBuf::from("Artist").join("Album").join("A-first.flac")
        );
        assert_eq!(tracks[0].size, 1);
        assert!(tracks[0].path.is_absolute());
        #[cfg(any(unix, windows))]
        assert!(tracks[0].file_identity.is_some());
        assert_eq!(tracks[1].relative_path, PathBuf::from("z-last.MP3"));

        Ok(())
    }

    #[test]
    fn a_folder_inside_the_root_is_scanned_with_paths_from_the_root() -> std::io::Result<()> {
        let root = tempdir()?;
        let album = root.path().join("Artist").join("Album");
        fs::create_dir_all(&album)?;
        fs::write(album.join("1.flac"), [1])?;
        fs::write(root.path().join("elsewhere.flac"), [1])?;

        let tracks = scan(root.path(), &album);

        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].relative_path,
            PathBuf::from("Artist").join("Album").join("1.flac")
        );
        assert!(scan(root.path(), &root.path().join("gone")).is_empty());
        Ok(())
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_file_keeps_its_identity_when_renamed_and_moved() -> std::io::Result<()> {
        let root = tempdir()?;
        let other = root.path().join("other");
        fs::create_dir(&other)?;
        fs::write(root.path().join("old.flac"), [1, 2])?;
        fs::write(root.path().join("bystander.flac"), [1, 2])?;
        let before = scan(root.path(), root.path());
        let identity = |tracks: &[DiscoveredTrack], name: &str| {
            tracks
                .iter()
                .find(|track| track.path.file_name().is_some_and(|file| file == name))
                .and_then(|track| track.file_identity.clone())
                .unwrap_or_else(|| panic!("{name} has an identity"))
        };

        fs::rename(root.path().join("old.flac"), other.join("new.flac"))?;
        let after = scan(root.path(), root.path());

        assert_eq!(identity(&before, "old.flac"), identity(&after, "new.flac"));
        assert_ne!(
            identity(&after, "new.flac"),
            identity(&after, "bystander.flac")
        );
        Ok(())
    }

    #[test]
    fn paths_are_within_their_folders_whole_names_only() {
        assert!(is_within(Path::new("/music/a/1.flac"), Path::new("/music")));
        assert!(is_within(Path::new("/music"), Path::new("/music")));
        assert!(!is_within(Path::new("/music2/1.flac"), Path::new("/music")));
        assert_eq!(
            is_within(Path::new("/Music/1.flac"), Path::new("/music")),
            cfg!(windows)
        );
    }
}

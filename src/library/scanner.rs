use std::cmp::Ordering;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &["aac", "flac", "m4a", "mp3", "ogg", "opus", "wav", "wma"];

#[derive(Debug, Clone)]
pub(super) struct DiscoveredTrack {
    pub(super) path: PathBuf,
    pub(super) relative_path: PathBuf,
    pub(super) size: u64,
    pub(super) modified_ns: i64,
    pub(super) file_identity: Option<String>,
}

pub(super) fn scan_music_directory() -> Result<Vec<DiscoveredTrack>, String> {
    let root = dirs::audio_dir().ok_or_else(|| "Music directory is unavailable".to_owned())?;
    scan_directory(&root)
}

fn scan_directory(root: &Path) -> Result<Vec<DiscoveredTrack>, String> {
    if !root.is_dir() {
        return Err(format!(
            "Music directory does not exist: {}",
            root.display()
        ));
    }

    let mut tracks = Vec::new();
    for result in WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
    {
        let entry = result.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() || !is_supported_audio(entry.path()) {
            continue;
        }
        let path = entry.into_path();
        let relative_path = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| {
                duration.as_nanos().min(i64::MAX as u128) as i64
            });

        tracks.push(DiscoveredTrack {
            path,
            relative_path,
            size: metadata.len(),
            modified_ns,
            file_identity: file_identity(&metadata),
        });
    }

    tracks.sort_by(|left, right| {
        compare_ascii_case_insensitive(&left.relative_path, &right.relative_path)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(tracks)
}

#[cfg(unix)]
fn file_identity(metadata: &Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;

    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_identity(_metadata: &Metadata) -> Option<String> {
    None
}

pub(super) fn supports_embedded_metadata(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wma"))
}

fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
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

        let tracks = scan_directory(root.path()).map_err(std::io::Error::other)?;

        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks[0].relative_path,
            PathBuf::from("Artist").join("Album").join("A-first.flac")
        );
        assert_eq!(tracks[0].size, 1);
        assert!(tracks[0].path.is_absolute());
        #[cfg(unix)]
        assert!(tracks[0].file_identity.is_some());
        assert_eq!(tracks[1].relative_path, PathBuf::from("z-last.MP3"));

        Ok(())
    }
}

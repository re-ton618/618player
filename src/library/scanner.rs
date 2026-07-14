use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &["aac", "flac", "m4a", "mp3", "ogg", "opus", "wav", "wma"];

pub async fn scan_music_directory() -> Vec<PathBuf> {
    dirs::audio_dir().map_or_else(Vec::new, |root| scan_directory(&root))
}

fn scan_directory(root: &Path) -> Vec<PathBuf> {
    let mut tracks: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_supported_audio(entry.path()))
        .filter_map(|entry| entry.path().strip_prefix(root).ok().map(Path::to_path_buf))
        .collect();

    sort_paths(&mut tracks);
    tracks
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

fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|left, right| {
        compare_ascii_case_insensitive(left, right).then_with(|| left.cmp(right))
    });
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
    fn rejects_unsupported_or_missing_extensions() {
        for path in ["cover.jpg", "notes.txt", "track.mp3.bak", "no-extension"] {
            assert!(!is_supported_audio(Path::new(path)), "accepted {path}");
        }
    }

    #[test]
    fn sorting_is_case_insensitive_with_a_deterministic_tie_breaker() {
        let mut paths = vec![
            PathBuf::from("beta.mp3"),
            PathBuf::from("Alpha.mp3"),
            PathBuf::from("alpha.mp3"),
            PathBuf::from("gamma.mp3"),
        ];

        sort_paths(&mut paths);

        assert_eq!(
            paths,
            ["Alpha.mp3", "alpha.mp3", "beta.mp3", "gamma.mp3"].map(PathBuf::from)
        );
    }

    #[test]
    fn recursively_discovers_only_supported_files() -> std::io::Result<()> {
        let root = tempdir()?;
        let nested = root.path().join("Artist").join("Album");
        fs::create_dir_all(&nested)?;
        fs::write(root.path().join("z-last.MP3"), [])?;
        fs::write(nested.join("A-first.flac"), [])?;
        fs::write(nested.join("cover.jpg"), [])?;

        let tracks = scan_directory(root.path());

        assert_eq!(
            tracks,
            vec![
                PathBuf::from("Artist").join("Album").join("A-first.flac"),
                PathBuf::from("z-last.MP3"),
            ]
        );

        Ok(())
    }
}

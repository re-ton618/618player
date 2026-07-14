use std::path::PathBuf;

pub(crate) fn matching_track_indices(tracks: &[PathBuf], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();

    if query.is_empty() {
        return (0..tracks.len()).collect();
    }

    tracks
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            path.to_string_lossy()
                .to_lowercase()
                .contains(&query)
                .then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_is_case_insensitive_and_preserves_library_order() {
        let tracks = [
            PathBuf::from("Artist/First.FLAC"),
            PathBuf::from("Other/second.mp3"),
            PathBuf::from("Artist/Third.wav"),
        ];

        assert_eq!(matching_track_indices(&tracks, "artist"), vec![0, 2]);
        assert_eq!(matching_track_indices(&tracks, "FLAC"), vec![0]);
        assert_eq!(matching_track_indices(&tracks, "  "), vec![0, 1, 2]);
    }
}

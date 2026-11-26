use std::cmp::Ordering;

use super::catalog::compare_in_album;
use super::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortColumn {
    Title,
    Artist,
    Album,
    Year,
    Duration,
    /// When the file was last changed: when it came into the library, for
    /// as long as it was not written to since.
    Added,
    /// How many times the track was listened to.
    Plays,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Self::Ascending => ordering,
            Self::Descending => ordering.reverse(),
        }
    }
}

/// Where in `tracks` the present ones that match `query` are, sorted.
pub(crate) fn matching_track_indices(
    tracks: &[Track],
    query: &str,
    sort_column: SortColumn,
    sort_direction: SortDirection,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut indices: Vec<_> = tracks
        .iter()
        .enumerate()
        .filter_map(|(index, track)| matches(track, &query).then_some(index))
        .collect();

    indices.sort_unstable_by(|&left_index, &right_index| {
        let left = &tracks[left_index];
        let right = &tracks[right_index];
        compare_tracks(left, right, sort_column, sort_direction)
            .then_with(|| left.id.cmp(&right.id))
    });
    indices
}

/// Which of `positions` in `tracks` are of present tracks that match
/// `query`, in the order they are given (a playlist's).
pub(crate) fn matching_in_order(
    tracks: &[Track],
    positions: impl IntoIterator<Item = usize>,
    query: &str,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    positions
        .into_iter()
        .filter(|&position| matches(&tracks[position], &query))
        .collect()
}

/// The track is there and matches `query`, lowercased and trimmed.
fn matches(track: &Track, query: &str) -> bool {
    track.present && (query.is_empty() || track.search_key().contains(query))
}

fn compare_tracks(
    left: &Track,
    right: &Track,
    column: SortColumn,
    direction: SortDirection,
) -> Ordering {
    match column {
        SortColumn::Title => direction.apply(left.title_key().cmp(right.title_key())),
        SortColumn::Artist => compare_optional_text(
            left.artist.as_ref().map(|_| left.artist_key()),
            right.artist.as_ref().map(|_| right.artist_key()),
            direction,
        ),
        // Albums by title, and each album's tracks together in disc and
        // track order whichever way the albums run.
        SortColumn::Album => compare_optional_text(
            left.album.as_ref().map(|_| left.album_key()),
            right.album.as_ref().map(|_| right.album_key()),
            direction,
        )
        .then_with(|| direction.apply(left.album_identity().cmp(right.album_identity())))
        .then_with(|| compare_in_album(left, right)),
        SortColumn::Year => compare_optional(left.year, right.year, direction),
        SortColumn::Duration => compare_optional(left.duration_ms, right.duration_ms, direction),
        SortColumn::Added => direction.apply(left.modified_ns.cmp(&right.modified_ns)),
        // As often heard, by title.
        SortColumn::Plays => direction
            .apply(left.play_count.cmp(&right.play_count))
            .then_with(|| left.title_key().cmp(right.title_key())),
    }
}

fn compare_optional<T: Ord>(
    left: Option<T>,
    right: Option<T>,
    direction: SortDirection,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction.apply(left.cmp(&right)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_optional_text(
    left: Option<&str>,
    right: Option<&str>,
    direction: SortDirection,
) -> Ordering {
    compare_optional(left, right, direction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::track::tests::track_at;
    use crate::library::track::Metadata;

    fn track(id: i64, title: &str, artist: Option<&str>, year: Option<u32>) -> Track {
        track_at(
            id,
            &format!("{title}.mp3"),
            Metadata {
                title: title.into(),
                artist: artist.map(str::to_owned),
                year,
                ..Metadata::default()
            },
        )
    }

    fn album_track(id: i64, album: &str, artist: &str, disc: u32, number: u32) -> Track {
        track_at(
            id,
            &format!("{artist}/{album}/{disc}-{number}.flac"),
            Metadata {
                title: format!("Track {id}"),
                album: Some(album.into()),
                album_artist: Some(artist.into()),
                disc_number: Some(disc),
                track_number: Some(number),
                ..Metadata::default()
            },
        )
    }

    fn ids(tracks: &[Track], indices: Vec<usize>) -> Vec<i64> {
        indices.into_iter().map(|index| tracks[index].id).collect()
    }

    #[test]
    fn filtering_searches_metadata_case_insensitively() {
        let tracks = [
            track(1, "First", Some("Artist"), Some(2020)),
            track(2, "Second", Some("Other"), Some(2010)),
        ];

        assert_eq!(
            matching_track_indices(
                &tracks,
                "ARTIST",
                SortColumn::Title,
                SortDirection::Ascending
            ),
            vec![0]
        );
    }

    #[test]
    fn missing_values_sort_last_in_both_directions() {
        let tracks = [
            track(1, "Unknown", None, None),
            track(2, "New", Some("Beta"), Some(2020)),
            track(3, "Old", Some("Alpha"), Some(1990)),
        ];

        assert_eq!(
            matching_track_indices(&tracks, "", SortColumn::Year, SortDirection::Ascending),
            vec![2, 1, 0]
        );
        assert_eq!(
            matching_track_indices(&tracks, "", SortColumn::Year, SortDirection::Descending),
            vec![1, 2, 0]
        );
    }

    #[test]
    fn search_reaches_album_artist_genre_and_composer_and_skips_absent_tracks() {
        let tagged = |id: i64, title: &str, change: fn(&mut Metadata)| {
            let mut metadata = Metadata {
                title: title.into(),
                ..Metadata::default()
            };
            change(&mut metadata);
            track_at(id, &format!("{title}.mp3"), metadata)
        };
        let mut tracks = [
            tagged(1, "One", |tags| {
                tags.album_artist = Some("The Ensemble".into())
            }),
            tagged(2, "Two", |tags| tags.genre = Some("Baroque".into())),
            tagged(3, "Three", |tags| tags.composer = Some("Telemann".into())),
            tagged(4, "Four", |tags| tags.composer = Some("Telemann".into())),
        ];
        tracks[3].present = false;
        let found = |query: &str| {
            ids(
                &tracks,
                matching_track_indices(&tracks, query, SortColumn::Title, SortDirection::Ascending),
            )
        };

        assert_eq!(found("ensemble"), [1]);
        assert_eq!(found("BAROQUE"), [2]);
        assert_eq!(found("telemann"), [3]);
        assert_eq!(found(""), [1, 3, 2]);
    }

    #[test]
    fn sorting_by_album_keeps_each_album_together_in_disc_and_track_order() {
        let tracks = [
            album_track(1, "Hits", "Queen", 1, 2),
            album_track(2, "Hits", "ABBA", 1, 2),
            album_track(3, "Hits", "Queen", 1, 1),
            album_track(4, "Hits", "ABBA", 1, 1),
            album_track(5, "Box", "Band", 2, 1),
            album_track(6, "Box", "Band", 1, 10),
            album_track(7, "Box", "Band", 1, 9),
            track(8, "Loose", None, None),
        ];
        let sorted = |direction| {
            ids(
                &tracks,
                matching_track_indices(&tracks, "", SortColumn::Album, direction),
            )
        };

        // Two albums of one title do not mix; the albumless come last.
        assert_eq!(sorted(SortDirection::Ascending), [7, 6, 5, 4, 2, 3, 1, 8]);
        // Descending turns the albums around, not the tracks within one.
        assert_eq!(sorted(SortDirection::Descending), [3, 1, 4, 2, 7, 6, 5, 8]);
    }

    #[test]
    fn a_playlist_keeps_its_order_through_a_search_and_skips_absent_tracks() {
        let mut tracks = [
            track(1, "Alpha", Some("Band"), None),
            track(2, "Beta", Some("Other"), None),
            track(3, "Gamma", Some("Band"), None),
        ];
        tracks[1].present = false;
        assert_eq!(matching_in_order(&tracks, [2, 1, 0], ""), [2, 0]);
        assert_eq!(matching_in_order(&tracks, [2, 1, 0], " BAND "), [2, 0]);
        assert_eq!(matching_in_order(&tracks, [0, 2], "gamma"), [2]);
    }
}

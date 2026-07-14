use std::cmp::Ordering;

use super::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortColumn {
    Title,
    Artist,
    Album,
    Year,
    Duration,
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
        .filter_map(|(index, track)| {
            (query.is_empty() || track.search_key().contains(&query)).then_some(index)
        })
        .collect();

    indices.sort_unstable_by(|&left_index, &right_index| {
        let left = &tracks[left_index];
        let right = &tracks[right_index];
        compare_tracks(left, right, sort_column, sort_direction)
            .then_with(|| left.id.cmp(&right.id))
    });
    indices
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
        SortColumn::Album => compare_optional_text(
            left.album.as_ref().map(|_| left.album_key()),
            right.album.as_ref().map(|_| right.album_key()),
            direction,
        ),
        SortColumn::Year => compare_optional(left.year, right.year, direction),
        SortColumn::Duration => compare_optional(left.duration_ms, right.duration_ms, direction),
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
    use std::path::PathBuf;

    use super::*;
    use crate::library::track::MetadataState;

    fn track(id: i64, title: &str, artist: Option<&str>, year: Option<u32>) -> Track {
        Track::from_cache(
            id,
            PathBuf::from(format!("/music/{title}.mp3")),
            PathBuf::from(format!("{title}.mp3")),
            1,
            1,
            None,
            title.into(),
            artist.map(str::to_owned),
            None,
            year,
            None,
            None,
            None,
            MetadataState::Ready,
            None,
        )
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
}

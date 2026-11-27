//! The albums and album artists of the library: what its tracks, taken
//! together, are of. Derived from the tracks whenever they change, never
//! stored: a retagged file moves between albums by being read again.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::track::{Track, VARIOUS_ARTISTS};

/// An album: the present tracks that share an album identity
/// ([`Track::album_identity`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Album {
    pub(crate) title: String,
    /// Who it is filed under: its album artist, else the artist all its
    /// tracks agree on, else [`VARIOUS_ARTISTS`].
    pub(crate) artist: String,
    /// The year most of its tracks give.
    pub(crate) year: Option<u32>,
    /// Its tracks' ids, in disc and track order.
    pub(crate) tracks: Vec<i64>,
}

/// An album artist, and the albums filed under the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Artist {
    pub(crate) name: String,
    /// Where the artist's albums are in [`Catalog::albums`], oldest first.
    pub(crate) albums: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Catalog {
    albums: Vec<Album>,
    artists: Vec<Artist>,
    /// The present tracks, of an album or not.
    track_count: usize,
}

impl Catalog {
    /// The albums and album artists of the present tracks among `tracks`.
    pub(crate) fn of(tracks: &[Track]) -> Self {
        let mut groups: Vec<Vec<&Track>> = Vec::new();
        let mut group_of: HashMap<&str, usize> = HashMap::new();
        let mut track_count = 0;
        for track in tracks {
            if !track.present {
                continue;
            }
            track_count += 1;
            if track.album_identity().is_empty() {
                continue;
            }
            let group = *group_of.entry(track.album_identity()).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[group].push(track);
        }

        let mut albums: Vec<Album> = groups.into_iter().map(album).collect();
        albums.sort_by(|left, right| {
            compare_names(&left.artist, &right.artist)
                .then_with(|| left.year.cmp(&right.year))
                .then_with(|| compare_names(&left.title, &right.title))
                .then_with(|| left.tracks.cmp(&right.tracks))
        });

        // Sorted by artist already: an artist's albums are neighbors, and
        // in their order.
        let mut artists: Vec<Artist> = Vec::new();
        for (index, album) in albums.iter().enumerate() {
            match artists.last_mut() {
                Some(artist) if artist.name.to_lowercase() == album.artist.to_lowercase() => {
                    artist.albums.push(index);
                }
                _ => artists.push(Artist {
                    name: album.artist.clone(),
                    albums: vec![index],
                }),
            }
        }

        Self {
            albums,
            artists,
            track_count,
        }
    }

    /// How many tracks the library has whose files are there.
    pub(crate) fn track_count(&self) -> usize {
        self.track_count
    }

    /// Every album, by artist, then year, then title.
    pub(crate) fn albums(&self) -> &[Album] {
        &self.albums
    }

    /// Every album artist, by name.
    pub(crate) fn artists(&self) -> &[Artist] {
        &self.artists
    }
}

fn album(mut tracks: Vec<&Track>) -> Album {
    tracks.sort_by(|left, right| compare_in_album(left, right));
    let first = tracks[0];
    let artist = tracks
        .iter()
        .find_map(|track| track.album_artist.clone())
        .or_else(|| {
            let agreed = first.artist.as_ref()?;
            tracks
                .iter()
                .all(|track| track.artist_key() == first.artist_key())
                .then(|| agreed.clone())
        })
        .filter(|_| !first.album_identity().starts_with("compilation\n"))
        .unwrap_or_else(|| VARIOUS_ARTISTS.to_owned());

    Album {
        title: first.album.clone().unwrap_or_default(),
        artist,
        year: commonest(tracks.iter().filter_map(|track| track.year)),
        tracks: tracks.iter().map(|track| track.id).collect(),
    }
}

/// The order of two tracks of one album: by disc, a track without one
/// being of the first; then by number, the unnumbered last; then by file.
pub(super) fn compare_in_album(left: &Track, right: &Track) -> Ordering {
    let number = |track: &Track| track.track_number.unwrap_or(u32::MAX);
    left.disc_number
        .unwrap_or(1)
        .cmp(&right.disc_number.unwrap_or(1))
        .then_with(|| number(left).cmp(&number(right)))
        .then_with(|| left.relative_path.cmp(&right.relative_path))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_names(left: &str, right: &str) -> Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

/// The value most of `values` are; the least of those that tie.
fn commonest(values: impl Iterator<Item = u32>) -> Option<u32> {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::track::tests::track_at;
    use crate::library::track::Metadata;

    struct Tags<'a> {
        album: &'a str,
        artist: &'a str,
        album_artist: Option<&'a str>,
        disc: Option<u32>,
        number: Option<u32>,
        year: Option<u32>,
    }

    fn track(id: i64, relative: &str, tags: Tags<'_>) -> Track {
        track_at(
            id,
            relative,
            Metadata {
                title: format!("Track {id}"),
                artist: Some(tags.artist.into()),
                album: Some(tags.album.into()),
                album_artist: tags.album_artist.map(str::to_owned),
                disc_number: tags.disc,
                track_number: tags.number,
                year: tags.year,
                ..Metadata::default()
            },
        )
    }

    fn plain<'a>(album: &'a str, artist: &'a str, number: u32) -> Tags<'a> {
        Tags {
            album,
            artist,
            album_artist: None,
            disc: None,
            number: Some(number),
            year: None,
        }
    }

    fn titles(catalog: &Catalog) -> Vec<(&str, &str)> {
        catalog
            .albums()
            .iter()
            .map(|album| (album.artist.as_str(), album.title.as_str()))
            .collect()
    }

    #[test]
    fn an_albums_tracks_are_in_disc_and_track_order() {
        let of = |disc, number| Tags {
            album: "Set",
            artist: "Band",
            album_artist: Some("Band"),
            disc,
            number,
            year: Some(2001),
        };
        let tracks = [
            track(1, "Set/CD2/02.flac", of(Some(2), Some(2))),
            track(2, "Set/CD1/02.flac", of(Some(1), Some(2))),
            track(3, "Set/CD2/01.flac", of(Some(2), Some(1))),
            track(4, "Set/CD1/zz-hidden.flac", of(Some(1), None)),
            // Without a disc it is of the first.
            track(5, "Set/CD1/01.flac", of(None, Some(1))),
        ];

        let catalog = Catalog::of(&tracks);

        assert_eq!(catalog.albums().len(), 1);
        assert_eq!(catalog.albums()[0].tracks, [5, 2, 4, 3, 1]);
        assert_eq!(catalog.albums()[0].year, Some(2001));
    }

    #[test]
    fn a_compilation_is_one_album_whether_tagged_flagged_or_only_in_one_folder() {
        let flagged = |id, relative: &str, artist: &str, number| {
            track_at(
                id,
                relative,
                Metadata {
                    title: format!("Track {id}"),
                    artist: Some(artist.into()),
                    album: Some("Hits 1".into()),
                    track_number: Some(number),
                    compilation: true,
                    ..Metadata::default()
                },
            )
        };
        let tagged = |id, artist, number| {
            track(
                id,
                &format!("Tagged/{id}.flac"),
                Tags {
                    album_artist: Some("Various Artists"),
                    ..plain("Hits 2", artist, number)
                },
            )
        };
        let tracks = [
            flagged(1, "Flagged/a/1.flac", "One", 1),
            flagged(2, "Flagged/b/2.flac", "Two", 2),
            tagged(3, "One", 1),
            tagged(4, "Two", 2),
            track(5, "Untagged/1.flac", plain("Hits 3", "One", 1)),
            track(6, "Untagged/2.flac", plain("Hits 3", "Two", 2)),
        ];

        let catalog = Catalog::of(&tracks);

        assert_eq!(
            titles(&catalog),
            [
                (VARIOUS_ARTISTS, "Hits 1"),
                (VARIOUS_ARTISTS, "Hits 2"),
                (VARIOUS_ARTISTS, "Hits 3"),
            ]
        );
        assert_eq!(catalog.artists().len(), 1);
        assert_eq!(catalog.artists()[0].albums, [0, 1, 2]);
        assert_eq!(catalog.albums()[2].tracks, [5, 6]);
    }

    #[test]
    fn two_albums_that_share_a_title_are_two_albums() {
        let by = |id, folder: &str, artist| {
            track(
                id,
                &format!("{folder}/{id}.flac"),
                Tags {
                    album_artist: Some(artist),
                    ..plain("Greatest Hits", artist, 1)
                },
            )
        };
        let tracks = [
            by(1, "Queen", "Queen"),
            by(2, "ABBA", "ABBA"),
            by(3, "Queen", "Queen"),
            // Untagged, the folder tells them apart.
            track(4, "Live A/1.flac", plain("Live", "Solo", 1)),
            track(5, "Live B/1.flac", plain("Live", "Solo", 1)),
        ];

        let catalog = Catalog::of(&tracks);

        assert_eq!(
            titles(&catalog),
            [
                ("ABBA", "Greatest Hits"),
                ("Queen", "Greatest Hits"),
                ("Solo", "Live"),
                ("Solo", "Live"),
            ]
        );
        assert_eq!(catalog.albums()[1].tracks, [1, 3]);
    }

    #[test]
    fn artists_list_their_albums_oldest_first_and_absent_tracks_are_left_out() {
        let of = |id, album, year| {
            track(
                id,
                &format!("{album}/{id}.flac"),
                Tags {
                    album_artist: Some("Band"),
                    year: Some(year),
                    ..plain(album, "Band", 1)
                },
            )
        };
        let mut gone = of(4, "Lost", 1990);
        gone.present = false;
        let mut single = track_at(5, "single.flac", Metadata::default());
        single.artist = Some("Band".into());
        let tracks = [
            of(1, "Second", 2005),
            of(2, "First", 1999),
            of(3, "zeta", 1999),
            gone,
            single,
        ];

        let catalog = Catalog::of(&tracks);

        assert_eq!(catalog.artists().len(), 1);
        let band = &catalog.artists()[0];
        assert_eq!(band.name, "Band");
        let albums: Vec<&str> = band
            .albums
            .iter()
            .map(|&index| catalog.albums()[index].title.as_str())
            .collect();
        assert_eq!(albums, ["First", "zeta", "Second"]);
    }

    #[test]
    fn the_commonest_year_names_the_album() {
        assert_eq!(commonest([1999, 2010, 2010].into_iter()), Some(2010));
        assert_eq!(commonest([2010, 1999].into_iter()), Some(1999));
        assert_eq!(commonest(std::iter::empty()), None);
    }
}

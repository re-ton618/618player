//! The lists of tracks the workspace shows. Each is the present tracks,
//! searched, in an order of its own. A new one is a variant here, its
//! order in [`Model::sort_of`](super::Model), and its columns in the view.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Listing {
    /// The whole library, sorted as the user chose.
    Library,
    /// The last added first.
    RecentlyAdded,
    /// The tracks listened to, the most often first.
    MostPlayed,
    /// The playlist the Playlists tab shows, in its own order.
    Playlist,
}

impl Listing {
    pub(super) const ALL: [Self; 4] = [
        Self::Library,
        Self::RecentlyAdded,
        Self::MostPlayed,
        Self::Playlist,
    ];
    pub(super) const COUNT: usize = Self::ALL.len();

    /// Its place in [`Self::ALL`], and in what is kept per listing.
    pub(super) const fn index(self) -> usize {
        self as usize
    }

    /// Whether a press on a column title sorts it.
    pub(super) const fn sortable(self) -> bool {
        matches!(self, Self::Library)
    }
}

#[cfg(test)]
mod tests {
    use super::Listing;

    #[test]
    fn every_listing_is_in_all_at_its_index() {
        for (index, listing) in Listing::ALL.into_iter().enumerate() {
            assert_eq!(listing.index(), index);
        }
    }
}

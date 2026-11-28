//! The Settings tab's categories. Which one is shown belongs to the model,
//! so it stays chosen while other tabs are shown.

/// A category of settings: one entry of the list on the left of the tab,
/// and the page of sections it shows on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Category {
    /// The folders the library is made of, and what it holds.
    #[default]
    Library,
    /// What the queue does at its end, and the volume.
    Playback,
    /// The theme the app is drawn in.
    Appearance,
    /// The app's version, and its updates.
    About,
}

impl Category {
    /// Every category, in the order the list shows them.
    pub(super) const ALL: [Self; 4] = [Self::Library, Self::Playback, Self::Appearance, Self::About];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Playback => "Playback",
            Self::Appearance => "Appearance",
            Self::About => "About",
        }
    }
}

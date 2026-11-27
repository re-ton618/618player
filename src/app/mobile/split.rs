//! How the phone's two regions share the room between the bars: the tab
//! group over the Now Playing sheet.

use crate::app::tabs::Half;

/// A drag let go this near an end leaves the region there shut.
const SNAP: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Split {
    /// The tab group has the room; the sheet is its bar.
    Upper,
    /// The sheet has the room; the tab group is its header alone.
    Lower,
    /// Both show, as a drag left them: the upper takes this share of the
    /// room.
    Both(f32),
}

/// How tall each region is shut: the tab group its header, the sheet its
/// bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shut {
    pub(super) upper: f32,
    pub(super) lower: f32,
}

impl Split {
    /// The split once `half` was used: a region that was shut takes the
    /// room; regions that both show stay as they are.
    pub(super) fn revealing(self, half: Half) -> Self {
        match (self, half) {
            (Self::Lower, Half::Upper) => Self::Upper,
            (Self::Upper, Half::Lower) => Self::Lower,
            _ => self,
        }
    }

    /// The region that is shut.
    pub(super) fn collapsed(self) -> Option<Half> {
        match self {
            Self::Upper => Some(Half::Lower),
            Self::Lower => Some(Half::Upper),
            Self::Both(_) => None,
        }
    }

    /// How tall the upper region is where the two have `room` between
    /// them: neither is ever less than it is shut.
    pub(super) fn upper(self, room: f32, shut: Shut) -> f32 {
        let most = (room - shut.lower).max(shut.upper);
        match self {
            Self::Upper => most,
            Self::Lower => shut.upper,
            Self::Both(share) => (room * share).clamp(shut.upper, most),
        }
    }

    /// The split with the upper region dragged to `upper` tall.
    pub(super) fn dragged(upper: f32, room: f32, shut: Shut) -> Self {
        if room <= 0.0 {
            return Self::Upper;
        }
        Self::Both(Self::Both(upper / room).upper(room, shut) / room)
    }

    /// The split a drag leaves when it is let go: a region dragged almost
    /// shut is shut.
    pub(super) fn settled(self, room: f32, shut: Shut) -> Self {
        let upper = self.upper(room, shut);
        if upper <= shut.upper + SNAP {
            Self::Lower
        } else if upper >= room - shut.lower - SNAP {
            Self::Upper
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Shut, Split};
    use crate::app::tabs::Half;

    const ROOM: f32 = 600.0;
    const BAR: f32 = 46.0;
    const SHUT: Shut = Shut {
        upper: BAR,
        lower: BAR,
    };

    #[test]
    fn a_shut_region_takes_the_room_when_it_is_used() {
        assert_eq!(Split::Lower.revealing(Half::Upper), Split::Upper);
        assert_eq!(Split::Upper.revealing(Half::Lower), Split::Lower);
        for (split, half) in [
            (Split::Upper, Half::Upper),
            (Split::Lower, Half::Lower),
            (Split::Both(0.5), Half::Upper),
            (Split::Both(0.5), Half::Lower),
        ] {
            assert_eq!(split.revealing(half), split);
        }
        assert_eq!(Split::Upper.collapsed(), Some(Half::Lower));
        assert_eq!(Split::Lower.collapsed(), Some(Half::Upper));
        assert_eq!(Split::Both(0.5).collapsed(), None);
    }

    #[test]
    fn the_regions_fill_the_room_and_neither_is_less_than_shut() {
        assert_eq!(Split::Upper.upper(ROOM, SHUT), ROOM - BAR);
        assert_eq!(Split::Lower.upper(ROOM, SHUT), BAR);
        assert_eq!(Split::Both(0.5).upper(ROOM, SHUT), 300.0);
        assert_eq!(Split::Both(0.0).upper(ROOM, SHUT), BAR);
        assert_eq!(Split::Both(1.0).upper(ROOM, SHUT), ROOM - BAR);
        // The share holds when the room changes (the phone is turned).
        assert_eq!(Split::Both(0.5).upper(300.0, SHUT), 150.0);
        // No room yet, or too little for both shut.
        assert_eq!(Split::Upper.upper(0.0, SHUT), BAR);
        assert_eq!(Split::Both(0.5).upper(60.0, SHUT), BAR);
        // Each end keeps its own region's shut height.
        let sheet = Shut {
            upper: BAR,
            lower: 64.0,
        };
        assert_eq!(Split::Upper.upper(ROOM, sheet), ROOM - 64.0);
        assert_eq!(Split::Lower.upper(ROOM, sheet), BAR);
    }

    #[test]
    fn a_drag_follows_the_finger_and_shuts_a_region_let_go_near_an_end() {
        assert_eq!(Split::dragged(240.0, ROOM, SHUT), Split::Both(0.4));
        assert_eq!(Split::dragged(-50.0, ROOM, SHUT).upper(ROOM, SHUT), BAR);
        assert_eq!(
            Split::dragged(900.0, ROOM, SHUT).upper(ROOM, SHUT),
            ROOM - BAR
        );
        assert_eq!(Split::dragged(240.0, 0.0, SHUT), Split::Upper);

        assert_eq!(Split::Both(0.4).settled(ROOM, SHUT), Split::Both(0.4));
        assert_eq!(
            Split::dragged(80.0, ROOM, SHUT).settled(ROOM, SHUT),
            Split::Lower
        );
        assert_eq!(
            Split::dragged(520.0, ROOM, SHUT).settled(ROOM, SHUT),
            Split::Upper
        );
        assert_eq!(Split::Upper.settled(ROOM, SHUT), Split::Upper);
        assert_eq!(Split::Lower.settled(ROOM, SHUT), Split::Lower);
    }
}

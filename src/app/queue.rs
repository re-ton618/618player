//! The play queue: the tracks that play one after another, and which of
//! them is current. It is made from the library's list when a row is
//! pressed and is its own from then on: searching or sorting the library
//! afterwards does not change what comes next.
//!
//! It names tracks by id and never looks at one. Whether a track can be
//! played (its file is there) is the caller's to tell, as `playable`:
//! tracks that cannot are stepped over and kept, so that a drive that
//! comes back finds its tracks where they were.

/// What happens at the end of a track, and of the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Repeat {
    /// The queue plays to its end and stops.
    #[default]
    Off,
    /// After the last track the first plays on.
    All,
    /// The current track plays again, until the user moves on.
    One,
}

impl Repeat {
    /// The mode the repeat button goes to from this one.
    pub(super) const fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }

    pub(super) const fn slug(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::All => "all",
            Self::One => "one",
        }
    }

    pub(super) fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "off" => Some(Self::Off),
            "all" => Some(Self::All),
            "one" => Some(Self::One),
            _ => None,
        }
    }
}

/// A place in the queue. A track may be queued more than once, so the
/// place has a name of its own, `key`, which it keeps as it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) key: u64,
    pub(super) track_id: i64,
}

/// What removing an entry did to the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Removed {
    /// No entry has that key.
    Nothing,
    /// An entry that was not the current one.
    Other,
    /// The current entry. The one that followed it is current now; none
    /// when it was the last (the first again when all repeat).
    Current,
}

#[derive(Debug, Clone)]
pub(super) struct State {
    /// The entries, in the order they play.
    order: Vec<Entry>,
    /// While shuffled: the entries in the order they were queued in,
    /// which turning shuffle off returns to.
    unshuffled: Option<Vec<Entry>>,
    /// Where in `order` the current entry is.
    current: Option<usize>,
    repeat: Repeat,
    last_key: u64,
    /// The state of the shuffle's generator (xorshift64, never zero).
    seed: u64,
    /// Counts every change, so that who saves the queue can tell that
    /// there is something to save.
    revision: u64,
}

impl Default for State {
    fn default() -> Self {
        Self::seeded(0x9E37_79B9_7F4A_7C15)
    }
}

impl State {
    /// An empty queue whose shuffles follow from `seed`.
    pub(super) fn seeded(seed: u64) -> Self {
        Self {
            order: Vec::new(),
            unshuffled: None,
            current: None,
            repeat: Repeat::Off,
            last_key: 0,
            seed: seed.max(1),
            revision: 0,
        }
    }

    /// The queue a run left behind: the track ids in play order, those in
    /// the order they were queued in when it was shuffled, and where it
    /// was. What does not fit together (a hand-edited file) is dropped
    /// rather than guessed at: the place, the shuffle.
    pub(super) fn restored(
        order: Vec<i64>,
        unshuffled: Option<Vec<i64>>,
        current: Option<usize>,
        repeat: Repeat,
        seed: u64,
    ) -> Self {
        let mut queue = Self::seeded(seed);
        queue.repeat = repeat;
        queue.order = order.into_iter().map(|id| queue.entry(id)).collect();
        queue.current = current.filter(|&index| index < queue.order.len());
        // Each entry of the queued order is one of the play order: the
        // first of its track that no other took yet.
        queue.unshuffled = unshuffled.and_then(|ids| {
            let mut left: Vec<Option<Entry>> = queue.order.iter().copied().map(Some).collect();
            let entries: Vec<Entry> = ids
                .iter()
                .filter_map(|id| {
                    left.iter_mut()
                        .find(|entry| entry.is_some_and(|entry| entry.track_id == *id))
                        .and_then(Option::take)
                })
                .collect();
            (entries.len() == queue.order.len() && ids.len() == entries.len()).then_some(entries)
        });
        queue
    }

    /// The entries, in the order they play.
    pub(super) fn entries(&self) -> &[Entry] {
        &self.order
    }

    /// The entries in the order they were queued in, while shuffled.
    pub(super) fn unshuffled(&self) -> Option<&[Entry]> {
        self.unshuffled.as_deref()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Where in the play order the current entry is.
    pub(super) fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub(super) fn current(&self) -> Option<Entry> {
        self.current.map(|index| self.order[index])
    }

    pub(super) fn index_of(&self, key: u64) -> Option<usize> {
        self.order.iter().position(|entry| entry.key == key)
    }

    pub(super) fn shuffled(&self) -> bool {
        self.unshuffled.is_some()
    }

    pub(super) fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    /// Makes the queue `tracks`, with the one at `start` current. While
    /// shuffled that one plays first and the rest follow in a new shuffle.
    pub(super) fn replace(&mut self, tracks: impl IntoIterator<Item = i64>, start: usize) {
        let shuffled = self.shuffled();
        self.order = tracks.into_iter().map(|id| self.entry(id)).collect();
        self.current = (start < self.order.len()).then_some(start);
        self.unshuffled = None;
        if shuffled {
            self.unshuffled = Some(self.order.clone());
            if let Some(start) = self.current {
                let first = self.order.remove(start);
                self.order.insert(0, first);
                self.current = Some(0);
            }
            self.shuffle_from(self.current.map_or(0, |current| current + 1));
        }
        self.revision += 1;
    }

    /// Puts `track_id` right behind the current entry (first, without
    /// one), in the queued order as in the play order.
    pub(super) fn play_next(&mut self, track_id: i64) -> Entry {
        let entry = self.entry(track_id);
        let current = self.current();
        if let Some(unshuffled) = &mut self.unshuffled {
            let at = current
                .and_then(|current| unshuffled.iter().position(|entry| *entry == current))
                .map_or(0, |at| at + 1);
            unshuffled.insert(at, entry);
        }
        self.order
            .insert(self.current.map_or(0, |at| at + 1), entry);
        self.revision += 1;
        entry
    }

    /// Puts `track_id` at the end.
    pub(super) fn add(&mut self, track_id: i64) -> Entry {
        let entry = self.entry(track_id);
        if let Some(unshuffled) = &mut self.unshuffled {
            unshuffled.push(entry);
        }
        self.order.push(entry);
        self.revision += 1;
        entry
    }

    /// Takes the entry of `key` out.
    pub(super) fn remove(&mut self, key: u64) -> Removed {
        let Some(index) = self.index_of(key) else {
            return Removed::Nothing;
        };
        self.order.remove(index);
        if let Some(unshuffled) = &mut self.unshuffled {
            unshuffled.retain(|entry| entry.key != key);
        }
        self.revision += 1;
        match self.current {
            Some(current) if index < current => {
                self.current = Some(current - 1);
                Removed::Other
            }
            Some(current) if index == current => {
                self.current = if current < self.order.len() {
                    Some(current)
                } else if self.repeat == Repeat::All && !self.order.is_empty() {
                    Some(0)
                } else {
                    None
                };
                Removed::Current
            }
            _ => Removed::Other,
        }
    }

    /// Moves the entry of `key` to place `to` of the play order (its end,
    /// beyond that). The current entry stays the current one wherever it
    /// or the others go. The queued order under a shuffle is left as it
    /// is: the move is of what plays when.
    pub(super) fn move_to(&mut self, key: u64, to: usize) {
        let Some(from) = self.index_of(key) else {
            return;
        };
        let to = to.min(self.order.len() - 1);
        if from == to {
            return;
        }
        let current = self.current();
        let entry = self.order.remove(from);
        self.order.insert(to, entry);
        self.current = current.and_then(|current| self.index_of(current.key));
        self.revision += 1;
    }

    /// Empties the queue. With `keep_current` the current entry stays, as
    /// the whole of it: what is being heard is not taken away.
    pub(super) fn clear(&mut self, keep_current: bool) {
        let kept = self.current().filter(|_| keep_current);
        self.order = kept.into_iter().collect();
        self.unshuffled = self.unshuffled.as_ref().map(|_| self.order.clone());
        self.current = kept.map(|_| 0);
        self.revision += 1;
    }

    /// Turning shuffle on leaves the current entry and what played before
    /// it where they are and shuffles what is left. Turning it off
    /// returns to the order the entries were queued in, at the current
    /// entry.
    pub(super) fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffled() {
            return;
        }
        if on {
            self.unshuffled = Some(self.order.clone());
            self.shuffle_from(self.current.map_or(0, |current| current + 1));
        } else if let Some(unshuffled) = self.unshuffled.take() {
            let current = self.current();
            self.order = unshuffled;
            self.current = current.and_then(|current| self.index_of(current.key));
        }
        self.revision += 1;
    }

    pub(super) fn set_repeat(&mut self, repeat: Repeat) {
        if self.repeat != repeat {
            self.repeat = repeat;
            self.revision += 1;
        }
    }

    /// Makes the entry at `index` of the play order current.
    pub(super) fn go_to(&mut self, index: usize) {
        if index < self.order.len() && self.current != Some(index) {
            self.current = Some(index);
            self.revision += 1;
        }
    }

    /// The entry that plays when the current one ends by itself: the one
    /// the player is to have queued behind it. The current one again when
    /// it repeats; else the next that can be played, the first again
    /// after the last when all repeat. None at the end of the queue, and
    /// without a current entry.
    pub(super) fn upcoming(&self, playable: impl Fn(i64) -> bool) -> Option<usize> {
        let current = self.current?;
        if self.repeat == Repeat::One && playable(self.order[current].track_id) {
            return Some(current);
        }
        self.after(current, self.repeat == Repeat::All, &playable)
    }

    /// The entry the user moves on to with "next", and the one played
    /// after a track that failed: as [`Self::upcoming`], but never the
    /// current one for its repeating alone. Without a current entry, the
    /// first that can be played.
    pub(super) fn following(&self, playable: impl Fn(i64) -> bool) -> Option<usize> {
        match self.current {
            Some(current) => self.after(current, self.repeat == Repeat::All, &playable),
            None => (0..self.order.len()).find(|&index| playable(self.order[index].track_id)),
        }
    }

    /// The entry the user moves back to with "previous": the nearest
    /// before the current one that can be played, the last when all
    /// repeat and the current one is the first.
    pub(super) fn preceding(&self, playable: impl Fn(i64) -> bool) -> Option<usize> {
        let current = self.current?;
        let fits = |index: &usize| playable(self.order[*index].track_id);
        (0..current).rev().find(fits).or_else(|| {
            (self.repeat == Repeat::All)
                .then(|| (current + 1..self.order.len()).rev().find(fits))
                .flatten()
        })
    }

    /// The first playable entry after `index`; with `wrap`, on from the
    /// start up to and including `index` itself.
    fn after(&self, index: usize, wrap: bool, playable: &impl Fn(i64) -> bool) -> Option<usize> {
        let fits = |index: &usize| playable(self.order[*index].track_id);
        (index + 1..self.order.len())
            .find(fits)
            .or_else(|| wrap.then(|| (0..=index).find(fits)).flatten())
    }

    /// Shuffles the play order from place `from` on (Fisher-Yates).
    fn shuffle_from(&mut self, from: usize) {
        for index in (from + 1..self.order.len()).rev() {
            let other = from + (self.random() % (index - from + 1) as u64) as usize;
            self.order.swap(index, other);
        }
    }

    fn entry(&mut self, track_id: i64) -> Entry {
        self.last_key += 1;
        Entry {
            key: self.last_key,
            track_id,
        }
    }

    fn random(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANY: fn(i64) -> bool = |_| true;

    fn queue(tracks: &[i64], start: usize) -> State {
        let mut queue = State::seeded(7);
        queue.replace(tracks.iter().copied(), start);
        queue
    }

    fn ids(queue: &State) -> Vec<i64> {
        queue.entries().iter().map(|entry| entry.track_id).collect()
    }

    fn current_id(queue: &State) -> Option<i64> {
        queue.current().map(|entry| entry.track_id)
    }

    fn key_of(queue: &State, track_id: i64) -> u64 {
        queue
            .entries()
            .iter()
            .find(|entry| entry.track_id == track_id)
            .map(|entry| entry.key)
            .expect("the track is queued")
    }

    #[test]
    fn a_replaced_queue_starts_at_the_chosen_track() {
        let queue = queue(&[10, 20, 30], 1);

        assert_eq!(ids(&queue), [10, 20, 30]);
        assert_eq!(current_id(&queue), Some(20));
        assert_eq!(queue.upcoming(ANY), Some(2));
        assert_eq!(queue.preceding(ANY), Some(0));
    }

    #[test]
    fn play_next_goes_behind_the_current_track_and_add_to_the_end() {
        let mut queue = queue(&[10, 20, 30], 1);

        queue.play_next(40);
        queue.add(50);
        // The same track again is a place of its own.
        let again = queue.add(10);

        assert_eq!(ids(&queue), [10, 20, 40, 30, 50, 10]);
        assert_eq!(current_id(&queue), Some(20));
        assert_ne!(again.key, queue.entries()[0].key);

        // Into an empty queue either is the first, and nothing is current.
        let mut empty = State::default();
        empty.play_next(1);
        empty.play_next(2);
        assert_eq!(ids(&empty), [2, 1]);
        assert_eq!(empty.current(), None);
        assert_eq!(empty.following(ANY), Some(0));
        assert_eq!(empty.upcoming(ANY), None);
    }

    #[test]
    fn removing_keeps_the_current_track_or_moves_on_from_it() {
        let mut queue = queue(&[10, 20, 30, 40], 2);

        assert_eq!(queue.remove(key_of(&queue, 10)), Removed::Other);
        assert_eq!(current_id(&queue), Some(30));
        assert_eq!(queue.remove(key_of(&queue, 40)), Removed::Other);
        assert_eq!(current_id(&queue), Some(30));
        assert_eq!(queue.remove(999), Removed::Nothing);

        // The current one: what followed it is current...
        queue.add(50);
        assert_eq!(queue.remove(key_of(&queue, 30)), Removed::Current);
        assert_eq!(current_id(&queue), Some(50));
        // ...and after the last nothing is, or the first when all repeat.
        assert_eq!(queue.remove(key_of(&queue, 50)), Removed::Current);
        assert_eq!(queue.current(), None);
        assert_eq!(ids(&queue), [20]);

        let mut repeating = self::queue(&[10, 20], 1);
        repeating.set_repeat(Repeat::All);
        assert_eq!(repeating.remove(key_of(&repeating, 20)), Removed::Current);
        assert_eq!(current_id(&repeating), Some(10));
    }

    #[test]
    fn moving_an_entry_keeps_the_current_one_current() {
        let mut queue = queue(&[10, 20, 30, 40], 1);

        queue.move_to(key_of(&queue, 40), 0);
        assert_eq!(ids(&queue), [40, 10, 20, 30]);
        assert_eq!(current_id(&queue), Some(20));

        // The current one itself, and past the end.
        queue.move_to(key_of(&queue, 20), 99);
        assert_eq!(ids(&queue), [40, 10, 30, 20]);
        assert_eq!(current_id(&queue), Some(20));
        assert_eq!(queue.upcoming(ANY), None);

        let revision = queue.revision();
        queue.move_to(key_of(&queue, 20), 3);
        queue.move_to(999, 0);
        assert_eq!(queue.revision(), revision);
    }

    #[test]
    fn clearing_keeps_what_is_heard_when_asked_to() {
        let mut playing = queue(&[10, 20, 30], 1);
        playing.clear(true);
        assert_eq!(ids(&playing), [20]);
        assert_eq!(current_id(&playing), Some(20));
        assert_eq!(playing.upcoming(ANY), None);

        let mut stopped = queue(&[10, 20, 30], 1);
        stopped.clear(false);
        assert!(stopped.is_empty());
        assert_eq!(stopped.current(), None);
    }

    #[test]
    fn shuffle_keeps_the_current_track_and_shuffles_what_is_left() {
        let tracks: Vec<i64> = (1..=50).collect();
        let mut queue = queue(&tracks, 9);

        queue.set_shuffle(true);

        assert!(queue.shuffled());
        // The current track and what played before it stay...
        assert_eq!(ids(&queue)[..10], tracks[..10]);
        assert_eq!(queue.current_index(), Some(9));
        // ...and what is left is the same tracks in another order.
        let left = ids(&queue)[10..].to_vec();
        assert_ne!(left, tracks[10..]);
        let mut sorted = left.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, tracks[10..]);

        // On through the shuffle, then off: the queued order again, at
        // the track that is current by then.
        queue.go_to(11);
        let heard = current_id(&queue).unwrap();
        queue.set_shuffle(false);
        assert!(!queue.shuffled());
        assert_eq!(ids(&queue), tracks);
        assert_eq!(current_id(&queue), Some(heard));
        assert_eq!(queue.current_index(), Some(heard as usize - 1));
    }

    #[test]
    fn edits_under_a_shuffle_show_in_the_queued_order_too() {
        let tracks: Vec<i64> = (1..=8).collect();
        let mut queue = queue(&tracks, 2);
        queue.set_shuffle(true);

        queue.play_next(100);
        queue.add(200);
        queue.remove(key_of(&queue, 1));
        assert_eq!(ids(&queue)[2], 100);

        queue.set_shuffle(false);
        // Behind the current track, at the end, and gone.
        assert_eq!(ids(&queue), [2, 3, 100, 4, 5, 6, 7, 8, 200]);
        assert_eq!(current_id(&queue), Some(3));
    }

    #[test]
    fn a_row_pressed_under_a_shuffle_plays_first_and_the_rest_follow_shuffled() {
        let tracks: Vec<i64> = (1..=30).collect();
        let mut queue = State::seeded(3);
        queue.set_shuffle(true);

        queue.replace(tracks.iter().copied(), 12);

        assert!(queue.shuffled());
        assert_eq!(current_id(&queue), Some(13));
        assert_eq!(queue.current_index(), Some(0));
        let mut all = ids(&queue);
        assert_ne!(all, tracks);
        all.sort_unstable();
        assert_eq!(all, tracks);
        queue.set_shuffle(false);
        assert_eq!(ids(&queue), tracks);
        assert_eq!(queue.current_index(), Some(12));
    }

    #[test]
    fn repeat_decides_what_follows_the_last_track_and_the_current_one() {
        let mut queue = queue(&[10, 20, 30], 2);
        assert_eq!(queue.upcoming(ANY), None);
        assert_eq!(queue.following(ANY), None);

        queue.set_repeat(Repeat::All);
        assert_eq!(queue.upcoming(ANY), Some(0));
        assert_eq!(queue.following(ANY), Some(0));
        queue.go_to(0);
        assert_eq!(queue.preceding(ANY), Some(2));

        // One: the track again by itself, the next one when asked for.
        queue.set_repeat(Repeat::One);
        assert_eq!(queue.upcoming(ANY), Some(0));
        assert_eq!(queue.following(ANY), Some(1));
        assert_eq!(queue.preceding(ANY), None);
        queue.go_to(2);
        assert_eq!(queue.following(ANY), None);

        // A queue of one track repeats it either way.
        let mut single = self::queue(&[10], 0);
        single.set_repeat(Repeat::All);
        assert_eq!(single.upcoming(ANY), Some(0));
        assert_eq!(single.preceding(ANY), None);

        assert_eq!(Repeat::Off.next(), Repeat::All);
        assert_eq!(Repeat::All.next(), Repeat::One);
        assert_eq!(Repeat::One.next(), Repeat::Off);
    }

    #[test]
    fn tracks_that_cannot_be_played_are_stepped_over_and_kept() {
        let mut queue = queue(&[10, 20, 30, 40], 0);
        let present = |id: i64| id != 20 && id != 30;

        assert_eq!(queue.upcoming(present), Some(3));
        assert_eq!(queue.following(present), Some(3));
        queue.go_to(3);
        assert_eq!(queue.preceding(present), Some(0));
        assert_eq!(queue.upcoming(present), None);
        assert_eq!(ids(&queue), [10, 20, 30, 40]);

        queue.set_repeat(Repeat::All);
        assert_eq!(queue.upcoming(present), Some(0));
        // With nothing left to play nothing follows, however it repeats.
        assert_eq!(queue.upcoming(|_| false), None);
        queue.set_repeat(Repeat::One);
        assert_eq!(queue.upcoming(|_| false), None);
        // A current track that is gone does not repeat: the next plays.
        queue.go_to(1);
        assert_eq!(queue.upcoming(present), Some(3));
    }

    #[test]
    fn a_saved_queue_is_restored_with_its_shuffle_and_place() {
        let tracks: Vec<i64> = vec![1, 2, 3, 2, 5];
        let mut queue = queue(&tracks, 1);
        queue.set_shuffle(true);
        queue.set_repeat(Repeat::One);
        let order = ids(&queue);
        let unshuffled: Vec<i64> = queue
            .unshuffled()
            .unwrap()
            .iter()
            .map(|entry| entry.track_id)
            .collect();

        let mut restored = State::restored(
            order.clone(),
            Some(unshuffled),
            queue.current_index(),
            Repeat::One,
            7,
        );

        assert_eq!(ids(&restored), order);
        assert_eq!(restored.current_index(), Some(1));
        assert_eq!(restored.repeat(), Repeat::One);
        assert!(restored.shuffled());
        restored.set_shuffle(false);
        assert_eq!(ids(&restored), tracks);
        assert_eq!(current_id(&restored), Some(2));

        // What does not fit together is dropped, not guessed at.
        let broken = State::restored(vec![1, 2], Some(vec![1, 3]), Some(5), Repeat::Off, 7);
        assert_eq!(ids(&broken), [1, 2]);
        assert!(!broken.shuffled());
        assert_eq!(broken.current(), None);
    }

    #[test]
    fn every_change_and_nothing_else_moves_the_revision() {
        let mut queue = queue(&[10, 20, 30], 0);
        let mut last = queue.revision();
        let mut changed = |queue: &State| {
            let moved = queue.revision() != last;
            last = queue.revision();
            moved
        };

        queue.go_to(1);
        assert!(changed(&queue));
        queue.go_to(1);
        queue.go_to(99);
        queue.set_repeat(Repeat::Off);
        queue.set_shuffle(false);
        let _ = queue.upcoming(ANY);
        assert!(!changed(&queue));
        queue.set_repeat(Repeat::All);
        assert!(changed(&queue));
        queue.set_shuffle(true);
        assert!(changed(&queue));
        queue.add(40);
        assert!(changed(&queue));
        queue.remove(key_of(&queue, 40));
        assert!(changed(&queue));
        queue.clear(true);
        assert!(changed(&queue));
    }
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Library,
    NowPlaying,
    SongInformation,
}

impl Kind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::NowPlaying => "Now Playing",
            Self::SongInformation => "Song Information",
        }
    }

    pub(super) const fn slug(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::NowPlaying => "now-playing",
            Self::SongInformation => "song-information",
        }
    }

    pub(super) fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "library" => Some(Self::Library),
            "now-playing" => Some(Self::NowPlaying),
            "song-information" => Some(Self::SongInformation),
            _ => None,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub(super) enum Message {
    Open(Kind),
    Pressed(Kind),
    Close(Kind),
    DragOver { target: Kind, side: Side },
    DragLeftStrip,
    DragFinished,
    Cycle(Direction),
    CloseActive,
}

struct Drag {
    source: Kind,
    target: Option<(Kind, Side)>,
}

pub(super) struct State {
    order: Vec<Kind>,
    active: Option<Kind>,
    drag: Option<Drag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Outcome {
    pub(super) persist: bool,
    pub(super) library_activated: bool,
}

impl Default for State {
    fn default() -> Self {
        Self::from_parts(vec![Kind::Library], Some(Kind::Library)).unwrap()
    }
}

impl State {
    pub(super) fn from_parts(order: Vec<Kind>, active: Option<Kind>) -> Result<Self, &'static str> {
        if order
            .iter()
            .enumerate()
            .any(|(index, kind)| order[..index].contains(kind))
        {
            return Err("duplicate tab kind");
        }
        if order.is_empty() != active.is_none() {
            return Err("active tab and tab list must both be empty or non-empty");
        }
        if active.is_some_and(|kind| !order.contains(&kind)) {
            return Err("active tab is not open");
        }
        Ok(Self {
            order,
            active,
            drag: None,
        })
    }

    pub(super) fn order(&self) -> &[Kind] {
        &self.order
    }

    pub(super) fn active(&self) -> Option<Kind> {
        self.active
    }

    pub(super) fn update(&mut self, message: Message) -> Outcome {
        let old_order = self.order.clone();
        let old_active = self.active;

        match message {
            Message::Open(kind) => {
                self.drag = None;
                if !self.order.contains(&kind) {
                    self.order.push(kind);
                }
                self.active = Some(kind);
            }
            Message::Pressed(kind) if self.order.contains(&kind) => {
                self.active = Some(kind);
                self.drag = Some(Drag {
                    source: kind,
                    target: None,
                });
            }
            Message::Pressed(_) => {}
            Message::Close(kind) if self.order.contains(&kind) => {
                self.drag = None;
                self.close(kind);
            }
            Message::Close(_) => {}
            Message::DragOver { target, side } if self.order.contains(&target) => {
                if let Some(drag) = &mut self.drag {
                    drag.target = Some((target, side));
                }
            }
            Message::DragOver { .. } => {}
            Message::DragLeftStrip => {
                if let Some(drag) = &mut self.drag {
                    drag.target = None;
                }
            }
            Message::DragFinished => self.finish_drag(),
            Message::Cycle(direction) => {
                self.drag = None;
                self.cycle(direction);
            }
            Message::CloseActive => {
                self.drag = None;
                if let Some(active) = self.active {
                    self.close(active);
                }
            }
        }

        Outcome {
            persist: self.order != old_order || self.active != old_active,
            library_activated: old_active != Some(Kind::Library)
                && self.active == Some(Kind::Library),
        }
    }

    fn close(&mut self, kind: Kind) {
        let index = self.order.iter().position(|&item| item == kind).unwrap();
        self.order.remove(index);
        if self.active == Some(kind) {
            let right = self.order.get(index).copied();
            self.active = right.or(self.order.last().copied());
        }
    }

    fn cycle(&mut self, direction: Direction) {
        let Some(active) = self.active else { return };
        let index = self.order.iter().position(|&item| item == active).unwrap();
        let next = match direction {
            Direction::Forward => (index + 1) % self.order.len(),
            Direction::Backward => (index + self.order.len() - 1) % self.order.len(),
        };
        self.active = Some(self.order[next]);
    }

    fn finish_drag(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some((target, side)) = drag.target else {
            return;
        };
        let source = drag.source;
        if source == target || !self.order.contains(&source) || !self.order.contains(&target) {
            return;
        }
        let source_index = self.order.iter().position(|kind| *kind == source).unwrap();
        self.order.remove(source_index);
        let target_index = self.order.iter().position(|kind| *kind == target).unwrap();
        let insertion = target_index + usize::from(side == Side::After);
        self.order.insert(insertion, source);
    }
}

#[cfg(test)]
mod tests {
    use super::{Direction, Drag, Kind, Message, Side, State};
    use Kind::{Library as L, NowPlaying as N, SongInformation as S};

    fn state(order: &[Kind], active: Option<Kind>) -> State {
        State::from_parts(order.to_vec(), active).unwrap()
    }

    fn apply(state: &mut State, message: Message) {
        let before = (state.order.clone(), state.active);
        let outcome = state.update(message);
        let changed = before != (state.order.clone(), state.active);
        assert_eq!(outcome.persist, changed);
        let library_activated = before.1 != Some(L) && state.active == Some(L);
        assert_eq!(outcome.library_activated, library_activated);
    }

    fn over(target: Kind, side: Side) -> Message {
        Message::DragOver { target, side }
    }

    macro_rules! apply {
        ($state:ident; $($message:expr),+ $(,)?) => {
            $(apply(&mut $state, $message);)+
        };
    }

    #[test]
    fn singleton_activation_close_and_cycle_contracts() {
        let default = State::default();
        assert_eq!((default.order(), default.active()), (&[L][..], Some(L)));

        let mut tabs = default;
        apply!(tabs; Message::Open(N), Message::Open(N), Message::Pressed(L), Message::Pressed(S));
        assert_eq!((tabs.order(), tabs.active()), (&[L, N][..], Some(L)));

        let mut tabs = state(&[L, N, S], Some(N));
        apply!(tabs; Message::Close(L));
        assert_eq!(tabs.active(), Some(N));
        apply!(tabs; Message::Close(N));
        assert_eq!(tabs.active(), Some(S));
        apply!(tabs; Message::Open(L), Message::CloseActive, Message::CloseActive);
        assert_eq!((tabs.order(), tabs.active()), (&[][..], None));
        apply!(tabs; Message::Open(L));
        assert_eq!((tabs.order(), tabs.active()), (&[L][..], Some(L)));

        let mut tabs = state(&[L, N, S], Some(L));
        apply!(tabs; Message::Cycle(Direction::Backward));
        assert_eq!(tabs.active(), Some(S));
        apply!(tabs; Message::Cycle(Direction::Forward));
        assert_eq!(tabs.active(), Some(L));
        let mut empty = state(&[], None);
        apply!(empty; Message::Cycle(Direction::Forward));
    }

    #[test]
    fn drops_cover_sides_directions_adjacency_and_active_identity() {
        for (source, target, side, expected) in [
            (L, S, Side::Before, vec![N, L, S]),
            (L, N, Side::After, vec![N, L, S]),
            (S, L, Side::After, vec![L, S, N]),
            (S, N, Side::Before, vec![L, S, N]),
        ] {
            let mut tabs = state(&[L, N, S], Some(source));
            apply!(tabs; Message::Pressed(source), over(target, side), Message::DragFinished);
            assert_eq!(
                (tabs.order(), tabs.active()),
                (expected.as_slice(), Some(source))
            );
        }
        let mut tabs = state(&[L, N, S], Some(L));
        apply!(tabs; Message::Pressed(L), over(N, Side::After), Message::DragLeftStrip, Message::DragFinished);
        apply!(tabs; Message::Pressed(L), over(L, Side::Before), Message::DragFinished);
        tabs.drag = Some(Drag {
            source: L,
            target: Some((S, Side::Before)),
        });
        tabs.order.pop();
        apply!(tabs; Message::DragFinished);
        assert_eq!(tabs.order(), &[L, N]);

        apply!(tabs; Message::Pressed(L), Message::Open(S), Message::DragFinished);
        apply!(tabs; Message::Close(S), Message::Close(S), over(S, Side::Before));
        assert_eq!(tabs.order(), &[L, N]);
    }
}

//! The workspace's tabs: tab groups side by side or stacked in splits, each
//! showing one of its tabs, as the dock area draws and rearranges them
//! (its layout algebra, `nsh_base::dock`, is plain data). A tab is open once
//! in the whole workspace.

use std::fmt;

use nsh_base::dock::{
    DockAreaLayout, DockNode, InsertTarget, NodeId, PaneRef, PaneTree, PanelId, RootKind,
};
use nsh_core::element::Placement;
use nsh_core::geometry::Axis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Kind {
    Library,
    RecentlyAdded,
    MostPlayed,
    NowPlaying,
    Queue,
    SongInformation,
    Playlists,
    Settings,
}

impl Kind {
    /// Every tab, in the order the + menu offers them.
    pub(super) const ALL: [Self; 8] = [
        Self::Library,
        Self::RecentlyAdded,
        Self::MostPlayed,
        Self::Playlists,
        Self::NowPlaying,
        Self::Queue,
        Self::SongInformation,
        Self::Settings,
    ];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::RecentlyAdded => "Recently Added",
            Self::MostPlayed => "Most Played",
            Self::NowPlaying => "Now Playing",
            Self::Queue => "Queue",
            Self::SongInformation => "Song Information",
            Self::Playlists => "Playlists",
            Self::Settings => "Settings",
        }
    }

    pub(super) const fn slug(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::RecentlyAdded => "recently-added",
            Self::MostPlayed => "most-played",
            Self::NowPlaying => "now-playing",
            Self::Queue => "queue",
            Self::SongInformation => "song-information",
            Self::Playlists => "playlists",
            Self::Settings => "settings",
        }
    }

    pub(super) fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.slug() == slug)
    }

    /// The dock area's panel of this tab: its slug is the panel's key.
    pub(super) fn panel(self) -> PanelId {
        PanelId::from(self.slug())
    }

    /// The tab of a panel of the dock area.
    pub(super) fn of(panel: &PanelId) -> Option<Self> {
        Self::from_slug(panel.as_str())
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub(super) enum Message {
    /// Shows the tab where it is open, or opens it in the focused group.
    Open(Kind),
    /// Opens `kind` in the group `beside` is in (that group's + menu),
    /// taking it out of another group it is open in.
    OpenBeside {
        kind: Kind,
        beside: Kind,
    },
    Close(Kind),
    /// The pointer was used on this tab or in its body: its group has the
    /// focus.
    Focus(Kind),
    /// The dock area rearranged the workspace (a tab dragged or pressed, a
    /// split resized): the whole layout after it.
    Arranged(DockNode),
    /// Shows the next tab of the focused group.
    Cycle(Direction),
    /// Closes the focused tab.
    CloseActive,
}

/// One of the two tab groups a phone stacks ([`State::stacked`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Half {
    Upper,
    Lower,
}

/// A tab group: its tabs in order, and which of them shows.
struct Group {
    node: NodeId,
    tabs: Vec<Kind>,
    shown: usize,
}

impl Group {
    fn shown(&self) -> Option<Kind> {
        self.tabs.get(self.shown).copied()
    }
}

#[derive(Clone)]
pub(super) struct State {
    /// The tab groups and the splits they sit in.
    tree: PaneTree,
    /// The tab the keyboard acts on and new tabs open beside: one that
    /// shows in its group, or none with no tab open.
    focus: Option<Kind>,
    /// The workspace is a phone's ([`Self::stacked`]).
    stacked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Outcome {
    pub(super) persist: bool,
}

impl Default for State {
    fn default() -> Self {
        Self::from_node(&DockNode::Tabs {
            id: None,
            panels: vec![Kind::Library.panel()],
            active_index: 0,
        })
    }
}

impl State {
    /// The workspace `node` describes, its first group focused.
    pub(super) fn from_node(node: &DockNode) -> Self {
        let mut state = Self {
            tree: PaneTree::from_node(node, RootKind::Split),
            focus: None,
            stacked: false,
        };
        state.settle_focus(None);
        state
    }

    /// The workspace as a phone keeps it: two groups, one over the other,
    /// the lower Now Playing alone (the phone's own sheet, not a tab) and
    /// the upper every other tab, its last one staying. A workspace of
    /// another shape is made into one: every tab but Now Playing goes up.
    pub(super) fn stacked(mut self) -> Self {
        self.stacked = true;
        self.stack();
        self
    }

    /// The tabs of a half of a stacked workspace, in their order.
    pub(super) fn half(&self, half: Half) -> Vec<Kind> {
        let mut groups = self.groups();
        let at = half as usize;
        if at < groups.len() {
            groups.swap_remove(at).tabs
        } else {
            Vec::new()
        }
    }

    /// The half of a stacked workspace `kind` is open in.
    pub(super) fn half_of(&self, kind: Kind) -> Option<Half> {
        let at = self
            .groups()
            .iter()
            .position(|group| group.tabs.contains(&kind))?;
        Some(if at == 0 { Half::Upper } else { Half::Lower })
    }

    /// Whether `kind` stays where it is in a stacked workspace: Now
    /// Playing, and the upper group's only tab.
    pub(super) fn fixed(&self, kind: Kind) -> bool {
        self.stacked
            && self
                .group_of(kind)
                .is_some_and(|group| kind == Kind::NowPlaying || group.tabs.len() == 1)
    }

    /// The layout the dock area draws.
    pub(super) fn layout(&self) -> DockAreaLayout {
        DockAreaLayout::new(self.tree.to_node())
    }

    pub(super) fn focus(&self) -> Option<Kind> {
        self.focus
    }

    /// Whether `kind` is the tab its group shows.
    pub(super) fn shows(&self, kind: Kind) -> bool {
        let mut shows = false;
        self.tree.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind() {
                shows |= panels
                    .get(active_ix)
                    .is_some_and(|panel| panel.as_str() == kind.slug());
            }
        });
        shows
    }

    pub(super) fn update(&mut self, message: Message) -> Outcome {
        let before = self.tree.clone();

        match message {
            Message::Open(kind) => match self.group_of(kind) {
                Some(group) => self.show(&group, kind),
                None => self.place(kind, self.focus),
            },
            Message::OpenBeside { kind, beside } => self.place(kind, Some(beside)),
            Message::Close(kind) => self.close(kind),
            Message::Focus(kind) => {
                if let Some(group) = self.group_of(kind) {
                    self.focus = group.shown();
                }
            }
            Message::Arranged(layout) => self.arrange(&layout),
            Message::Cycle(direction) => self.cycle(direction),
            Message::CloseActive => {
                if let Some(focus) = self.focus {
                    self.close(focus);
                }
            }
        }

        if self.stacked {
            self.stack();
        }
        Outcome {
            persist: self.tree != before,
        }
    }

    /// Whether the workspace is two groups, one over the other, Now
    /// Playing alone in the lower.
    fn is_stacked(&self) -> bool {
        let PaneRef::Split {
            axis: Axis::Vertical,
            children: [upper, lower],
            ..
        } = self.tree.root().kind()
        else {
            return false;
        };
        let holds_now_playing = |panels: &[PanelId]| matches!(panels, [panel] if panel.as_str() == Kind::NowPlaying.slug());
        matches!(upper.kind(), PaneRef::Tabs { .. })
            && matches!(lower.kind(), PaneRef::Tabs { panels, .. } if holds_now_playing(panels))
    }

    /// Makes the workspace a stacked one, unless it is.
    fn stack(&mut self) {
        if self.is_stacked() {
            return;
        }
        let groups = self.groups();
        let mut upper: Vec<Kind> = groups
            .iter()
            .flat_map(|group| group.tabs.iter().copied())
            .filter(|&kind| kind != Kind::NowPlaying)
            .collect();
        if upper.is_empty() {
            upper.extend(Kind::ALL.into_iter().find(|&kind| kind != Kind::NowPlaying));
        }
        let upper_shown = self
            .focus
            .into_iter()
            .chain(groups.iter().filter_map(Group::shown))
            .find_map(|kind| upper.iter().position(|&tab| tab == kind))
            .unwrap_or(0);

        let tabs = |tabs: Vec<Kind>, active_index| DockNode::Tabs {
            id: None,
            panels: tabs.into_iter().map(Kind::panel).collect(),
            active_index,
        };
        self.focus = upper.get(upper_shown).copied();
        let stack = DockNode::Split {
            id: None,
            axis: Axis::Vertical,
            children: vec![tabs(upper, upper_shown), tabs(vec![Kind::NowPlaying], 0)],
            sizes: vec![None, None],
        };
        self.tree = PaneTree::from_node(&stack, RootKind::Split);
        self.settle_focus(None);
    }

    /// Every tab group, in the order the workspace lays them out.
    fn groups(&self) -> Vec<Group> {
        let mut groups = Vec::new();
        self.tree.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind() {
                groups.push(Group {
                    node: node.id(),
                    tabs: panels.iter().filter_map(Kind::of).collect(),
                    shown: active_ix,
                });
            }
        });
        groups
    }

    fn group_of(&self, kind: Kind) -> Option<Group> {
        self.groups()
            .into_iter()
            .find(|group| group.tabs.contains(&kind))
    }

    fn show(&mut self, group: &Group, kind: Kind) {
        if let Some(index) = group.tabs.iter().position(|&tab| tab == kind) {
            self.tree.set_active(group.node, index);
            self.focus = Some(kind);
        }
    }

    /// Puts `kind` last in the group `beside` is in (the first group
    /// without one, a new group with none) and shows it there. A stacked
    /// workspace opens every tab in its upper group.
    fn place(&mut self, kind: Kind, beside: Option<Kind>) {
        if self.fixed(kind) {
            if let Some(group) = self.group_of(kind) {
                self.show(&group, kind);
            }
            return;
        }
        let group = beside
            .filter(|_| !self.stacked)
            .and_then(|beside| self.group_of(beside))
            .or_else(|| self.groups().into_iter().next());
        let target = match group {
            Some(group) if group.tabs.contains(&kind) => return self.show(&group, kind),
            Some(group) => InsertTarget::Tabs {
                node: group.node,
                ix: None,
                activate: true,
            },
            // No group to join: the tab makes one beside the empty root.
            None => InsertTarget::Split {
                node: self.tree.root().id(),
                placement: Placement::Right,
                size: None,
            },
        };
        self.tree.move_panel(kind.panel(), target);
        self.focus = Some(kind);
    }

    /// Closes `kind`; its group shows the tab to its right, or its last.
    /// A group that loses its last tab goes, and what was beside it takes
    /// its room.
    fn close(&mut self, kind: Kind) {
        if self.fixed(kind) {
            return;
        }
        let groups = self.groups();
        let Some(place) = groups.iter().position(|group| group.tabs.contains(&kind)) else {
            return;
        };
        self.tree.remove_panel(kind.slug());
        self.settle_focus(Some(place));
    }

    fn cycle(&mut self, direction: Direction) {
        let Some(group) = self.focus.and_then(|focus| self.group_of(focus)) else {
            return;
        };
        let count = group.tabs.len();
        let next = match direction {
            Direction::Forward => (group.shown + 1) % count,
            Direction::Backward => (group.shown + count - 1) % count,
        };
        self.show(&group, group.tabs[next]);
    }

    /// Takes the layout the dock area made. The focus goes with a tab that
    /// was dragged to another group, or to the tab a press brought up.
    fn arrange(&mut self, layout: &DockNode) {
        let before = self.groups();
        self.tree = PaneTree::from_node(layout, RootKind::Split);
        let after = self.groups();

        let home = |groups: &[Group], kind: Kind| {
            groups
                .iter()
                .find(|group| group.tabs.contains(&kind))
                .map(|group| group.node)
        };
        let moved = after
            .iter()
            .flat_map(|group| group.tabs.iter().copied())
            .find(|&kind| home(&before, kind) != home(&after, kind));
        let raised = after
            .iter()
            .filter_map(Group::shown)
            .find(|&kind| !before.iter().any(|group| group.shown() == Some(kind)));
        if let Some(kind) = moved.or(raised) {
            self.focus = Some(kind);
        }
        self.settle_focus(None);
    }

    /// Keeps the focus on a tab that shows: where it is while it does,
    /// else the tab its group shows, else that of the group now at place
    /// `near` among the groups (the last, past them), else the first
    /// group's.
    fn settle_focus(&mut self, near: Option<usize>) {
        let groups = self.groups();
        let focus = self.focus;
        if focus.is_some_and(|focus| groups.iter().any(|group| group.shown() == Some(focus))) {
            return;
        }
        let own = focus.and_then(|focus| groups.iter().find(|group| group.tabs.contains(&focus)));
        let near = near.and_then(|near| groups.get(near).or(groups.last()));
        self.focus = own.or(near).or(groups.first()).and_then(Group::shown);
    }
}

#[cfg(test)]
mod tests {
    use nsh_base::dock::InsertTarget;
    use nsh_core::element::Placement;
    use nsh_core::geometry::px;

    use super::{Direction, Half, Kind, Message, State};
    use crate::app::tab_store::{decode, encode};
    use Kind::{Library as L, NowPlaying as N, Queue as Q, Settings as G, SongInformation as S};

    /// The workspace of a layout as the workspace file writes it.
    fn state(layout: &str) -> State {
        State::from_node(&decode(layout).unwrap())
    }

    fn text(state: &State) -> String {
        encode(&state.layout().center)
    }

    /// Applies `message`, checking that it asks to be saved exactly when
    /// the layout changed.
    fn apply(state: &mut State, message: Message) {
        let before = text(state);
        let outcome = state.update(message);
        assert_eq!(outcome.persist, before != text(state));
    }

    macro_rules! apply {
        ($state:ident; $($message:expr),+ $(,)?) => {
            $(
                let message = $message;
                apply(&mut $state, message);
            )+
        };
    }

    /// What the dock area reports when `kind` is dragged onto the group
    /// of `onto`: into it, or to a new group on its `side`.
    fn dragged(state: &State, kind: Kind, onto: Kind, side: Option<Placement>) -> Message {
        let mut tree = state.tree.clone();
        let node = tree.find_panel_node(onto.slug()).unwrap();
        let target = match side {
            Some(placement) => InsertTarget::Split {
                node,
                placement,
                size: None,
            },
            None => InsertTarget::Tabs {
                node,
                ix: None,
                activate: true,
            },
        };
        tree.move_panel(kind.panel(), target);
        Message::Arranged(tree.to_node())
    }

    /// What the dock area reports when the tab of `kind` is pressed.
    fn pressed(state: &State, kind: Kind) -> Message {
        let mut tree = state.tree.clone();
        let group = state.group_of(kind).unwrap();
        let index = group.tabs.iter().position(|&tab| tab == kind).unwrap();
        tree.set_active(group.node, index);
        Message::Arranged(tree.to_node())
    }

    #[test]
    fn one_group_opens_shows_closes_and_cycles_its_tabs() {
        let mut tabs = State::default();
        assert_eq!(
            (text(&tabs).as_str(), tabs.focus()),
            ("h(t(library*))", Some(L))
        );

        apply!(tabs; Message::Open(N), Message::Open(N), Message::Open(L), Message::Close(S));
        assert_eq!(
            (text(&tabs).as_str(), tabs.focus()),
            ("h(t(library*,now-playing))", Some(L))
        );
        assert!(tabs.shows(L) && !tabs.shows(N) && !tabs.shows(S));

        // Closing the shown tab shows the one to its right, or the last.
        let mut tabs = state("h(t(library,now-playing*,song-information))");
        apply!(tabs; Message::Close(L));
        assert_eq!(tabs.focus(), Some(N));
        apply!(tabs; Message::Close(N));
        assert_eq!(tabs.focus(), Some(S));
        apply!(tabs; Message::Open(L), Message::CloseActive, Message::CloseActive);
        assert_eq!((text(&tabs).as_str(), tabs.focus()), ("h()", None));
        apply!(tabs; Message::CloseActive, Message::Cycle(Direction::Forward));
        apply!(tabs; Message::Open(G));
        assert_eq!(
            (text(&tabs).as_str(), tabs.focus()),
            ("h(t(settings*))", Some(G))
        );

        let mut tabs = state("h(t(library*,now-playing,song-information))");
        apply!(tabs; Message::Cycle(Direction::Backward));
        assert_eq!(tabs.focus(), Some(S));
        apply!(tabs; Message::Cycle(Direction::Forward));
        assert_eq!(tabs.focus(), Some(L));
    }

    #[test]
    fn every_kind_is_offered_once_and_its_slug_names_it() {
        for kind in Kind::ALL {
            // A kind left out of `ALL` stops this from compiling.
            match kind {
                L | Kind::RecentlyAdded | Kind::MostPlayed | Kind::Playlists | N | Q | S | G => {}
            }
            assert_eq!(Kind::from_slug(kind.slug()), Some(kind));
            assert_eq!(Kind::of(&kind.panel()), Some(kind));
        }
    }

    #[test]
    fn a_tab_dragged_to_a_side_splits_its_group_and_takes_the_focus() {
        let mut tabs = state("h(t(library*,now-playing,queue))");
        apply!(tabs; dragged(&tabs, N, L, Some(Placement::Right)));
        assert_eq!(text(&tabs), "h(t(library*,queue),t(now-playing*))");
        assert_eq!(tabs.focus(), Some(N));
        assert!(tabs.shows(L) && tabs.shows(N));

        // Stacked under the group it is dropped at the bottom of.
        apply!(tabs; dragged(&tabs, Q, N, Some(Placement::Bottom)));
        assert_eq!(text(&tabs), "h(t(library*),v(t(now-playing*),t(queue*)))");
        assert_eq!(tabs.focus(), Some(Q));

        // Dropped into a group, it leaves its own, which goes with its
        // last tab.
        apply!(tabs; dragged(&tabs, Q, L, None));
        assert_eq!(text(&tabs), "h(t(library,queue*),t(now-playing*))");
        assert_eq!(tabs.focus(), Some(Q));
    }

    #[test]
    fn the_keyboard_and_new_tabs_go_to_the_focused_group() {
        let mut tabs = state("h(t(library*,queue),t(now-playing*,song-information))");
        assert_eq!(tabs.focus(), Some(L));

        // A press in another group focuses the tab it shows, and saves
        // nothing.
        apply!(tabs; Message::Focus(S));
        assert_eq!(tabs.focus(), Some(N));
        apply!(tabs; Message::Cycle(Direction::Forward));
        assert_eq!(
            text(&tabs),
            "h(t(library*,queue),t(now-playing,song-information*))"
        );
        apply!(tabs; Message::Open(G));
        assert_eq!(
            text(&tabs),
            "h(t(library*,queue),t(now-playing,song-information,settings*))"
        );

        // A tab open elsewhere is shown where it is.
        apply!(tabs; Message::Open(Q));
        assert_eq!(
            text(&tabs),
            "h(t(library,queue*),t(now-playing,song-information,settings*))"
        );
        assert_eq!(tabs.focus(), Some(Q));

        // A pressed tab takes the focus to its group.
        apply!(tabs; pressed(&tabs, N));
        assert_eq!(tabs.focus(), Some(N));
        apply!(tabs; Message::CloseActive);
        assert_eq!(tabs.focus(), Some(S));
    }

    #[test]
    fn a_group_s_menu_opens_tabs_in_it_and_takes_them_from_other_groups() {
        let mut tabs = state("h(t(library*),t(now-playing*),t(queue*))");
        apply!(tabs; Message::OpenBeside { kind: G, beside: N });
        assert_eq!(
            text(&tabs),
            "h(t(library*),t(now-playing,settings*),t(queue*))"
        );
        assert_eq!(tabs.focus(), Some(G));

        // The only tab of another group: that group goes.
        apply!(tabs; Message::OpenBeside { kind: Q, beside: L });
        assert_eq!(text(&tabs), "h(t(library,queue*),t(now-playing,settings*))");
        assert_eq!(tabs.focus(), Some(Q));

        // Already in the group: shown there.
        apply!(tabs; Message::OpenBeside { kind: L, beside: Q });
        assert_eq!(text(&tabs), "h(t(library*,queue),t(now-playing,settings*))");
        assert_eq!(tabs.focus(), Some(L));
    }

    #[test]
    fn closing_a_group_s_last_tab_gives_its_room_and_the_focus_away() {
        let mut tabs = state("h(t(library*):300,v(t(now-playing*),t(queue*)))");
        apply!(tabs; Message::Focus(Q), Message::CloseActive);
        assert_eq!(text(&tabs), "h(t(library*):300,t(now-playing*))");
        assert_eq!(tabs.focus(), Some(N));

        // Closing a tab of a group without the focus leaves the focus. The
        // group left alone fills the region, whatever size it had.
        apply!(tabs; Message::Focus(L), Message::Close(N));
        assert_eq!(
            (text(&tabs).as_str(), tabs.focus()),
            ("h(t(library*):300)", Some(L))
        );
    }

    #[test]
    fn a_phone_stacks_any_workspace_with_now_playing_below() {
        for (layout, stacked) in [
            ("h(t(library*))", "v(t(library*),t(now-playing*))"),
            ("h()", "v(t(library*),t(now-playing*))"),
            ("h(t(now-playing*))", "v(t(library*),t(now-playing*))"),
            (
                "h(t(library,now-playing,queue*))",
                "v(t(library,queue*),t(now-playing*))",
            ),
            // Every tab but Now Playing goes up, in the groups' order.
            (
                "h(t(settings*,queue),v(t(now-playing,playlists*),t(recently-added*)))",
                "v(t(settings*,queue,playlists,recently-added),t(now-playing*))",
            ),
            (
                "h(t(now-playing*,queue),t(library*))",
                "v(t(queue,library*),t(now-playing*))",
            ),
            // A stack of the phone's old shape: the lower's tabs go up.
            (
                "v(t(queue*):300,t(settings,now-playing*))",
                "v(t(queue*,settings),t(now-playing*))",
            ),
            // A stacked one stays as it is, sizes and all.
            (
                "v(t(queue*):300,t(now-playing*))",
                "v(t(queue*):300,t(now-playing*))",
            ),
        ] {
            let tabs = state(layout).stacked();
            assert_eq!(text(&tabs), stacked, "{layout}");
            assert!(tabs.focus().is_some_and(|focus| tabs.shows(focus)));
            assert_eq!(tabs.half(Half::Lower), [N], "{layout}");
        }
    }

    #[test]
    fn a_phone_keeps_now_playing_alone_below_and_opens_the_rest_above() {
        let mut tabs = state("v(t(library*),t(now-playing*))").stacked();
        assert_eq!(tabs.half(Half::Upper), [L]);
        assert_eq!(tabs.half(Half::Lower), [N]);
        assert_eq!(
            (tabs.half_of(L), tabs.half_of(N), tabs.half_of(G)),
            (Some(Half::Upper), Some(Half::Lower), None)
        );
        assert!(tabs.fixed(L) && tabs.fixed(N) && !tabs.fixed(G));

        // The upper's last tab and Now Playing neither close nor move: they
        // are shown where they are.
        apply!(tabs; Message::Close(L), Message::Close(N));
        apply!(tabs; Message::OpenBeside { kind: N, beside: L });
        apply!(tabs; Message::OpenBeside { kind: L, beside: N });
        assert_eq!(text(&tabs), "v(t(library*),t(now-playing*))");
        apply!(tabs; Message::CloseActive, Message::Open(N), Message::CloseActive);
        assert_eq!(text(&tabs), "v(t(library*),t(now-playing*))");

        // The other tabs open above, whichever group asks, and close.
        apply!(tabs; Message::OpenBeside { kind: G, beside: L });
        apply!(tabs; Message::OpenBeside { kind: Q, beside: N });
        assert_eq!(text(&tabs), "v(t(library,settings,queue*),t(now-playing*))");
        apply!(tabs; Message::Focus(N), Message::Open(S), Message::Close(G));
        assert_eq!(
            text(&tabs),
            "v(t(library,queue,song-information*),t(now-playing*))"
        );
        assert!(!tabs.fixed(L));
        apply!(tabs; Message::Close(L), Message::Close(Q));
        assert_eq!(text(&tabs), "v(t(song-information*),t(now-playing*))");
        assert!(tabs.fixed(S));
    }

    #[test]
    fn a_resized_split_is_saved_and_keeps_the_focus() {
        let mut tabs = state("h(t(library*),t(now-playing*))");
        apply!(tabs; Message::Focus(N));
        let mut tree = tabs.tree.clone();
        let root = tree.root().id();
        tree.set_sizes(root, vec![Some(px(420.0)), Some(px(580.0))]);
        apply!(tabs; Message::Arranged(tree.to_node()));
        assert_eq!(text(&tabs), "h(t(library*):420,t(now-playing*):580)");
        assert_eq!(tabs.focus(), Some(N));
    }
}

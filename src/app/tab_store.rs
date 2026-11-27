//! The workspace file: the tab groups and the splits they sit in, as the
//! last run left them.

use std::fs;
use std::path::{Path, PathBuf};

use nsh_base::dock::DockNode;
use nsh_core::geometry::{Axis, Points, px};

use super::tabs::{Kind, State};

type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HEADER: &str = "618player-workspace-v2";
const FILE: &str = "618player/workspace-v2.txt";
/// One group of tabs, before the workspace split: read once, when no
/// newer file is there.
const LEGACY_HEADER: &str = "618player-workspace-v1";
const LEGACY_FILE: &str = "618player/workspace-v1.txt";
/// Splits inside splits a file may hold.
const MAX_DEPTH: usize = 16;

pub(super) fn load() -> StoreResult<Option<State>> {
    let directory = directory();
    match load_from(&directory.join(FILE))? {
        Some(state) => Ok(Some(state)),
        None => load_legacy_from(&directory.join(LEGACY_FILE)),
    }
}

pub(super) fn save(state: &State) -> StoreResult<()> {
    save_to(&directory().join(FILE), state)
}

fn directory() -> PathBuf {
    crate::platform::config_dir().unwrap_or_else(std::env::temp_dir)
}

fn read(path: &Path) -> StoreResult<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn load_from(path: &Path) -> StoreResult<Option<State>> {
    let Some(contents) = read(path)? else {
        return Ok(None);
    };
    parse(&contents).map(Some)
}

fn load_legacy_from(path: &Path) -> StoreResult<Option<State>> {
    let Some(contents) = read(path)? else {
        return Ok(None);
    };
    parse_legacy(&contents).map(Some)
}

fn parse(contents: &str) -> StoreResult<State> {
    let lines: Vec<_> = contents.lines().collect();
    let [header, layout] = lines[..] else {
        return Err("workspace must contain exactly two lines".into());
    };
    if header != HEADER {
        return Err("unknown workspace header".into());
    }
    let layout = layout
        .strip_prefix("layout=")
        .ok_or("missing layout line")?;
    Ok(State::from_node(&decode(layout)?))
}

/// The one group of a v1 file, every tab in it.
fn parse_legacy(contents: &str) -> StoreResult<State> {
    let lines: Vec<_> = contents.lines().collect();
    let [header, active, tabs] = lines[..] else {
        return Err("workspace must contain exactly three lines".into());
    };
    if header != LEGACY_HEADER {
        return Err("unknown workspace header".into());
    }
    let active = active
        .strip_prefix("active=")
        .ok_or("missing active line")?;
    let tabs = tabs.strip_prefix("tabs=").ok_or("missing tabs line")?;
    if tabs.is_empty() && active.is_empty() {
        return Ok(State::from_node(&DockNode::default()));
    }

    let mut order = Vec::new();
    for slug in tabs.split(',') {
        let kind = Kind::from_slug(slug).ok_or("unknown tab kind")?;
        if order.contains(&kind) {
            return Err("duplicate tab kind".into());
        }
        order.push(kind);
    }
    let active = Kind::from_slug(active).ok_or("unknown active tab kind")?;
    let active_index = order
        .iter()
        .position(|&kind| kind == active)
        .ok_or("active tab is not open")?;
    Ok(State::from_node(&DockNode::Tabs {
        id: None,
        panels: order.into_iter().map(Kind::panel).collect(),
        active_index,
    }))
}

fn save_to(path: &Path, state: &State) -> StoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let layout = encode(&state.layout().center);
    fs::write(path, format!("{HEADER}\nlayout={layout}\n"))?;
    Ok(())
}

/// A layout as one line: a split is `h(..)` (side by side) or `v(..)`
/// (stacked) of what it holds, each with its size in points after a colon
/// when it has one; a tab group is `t(..)` of its tabs' slugs, the one it
/// shows starred. `h(t(library*,queue):420,t(now-playing*))`.
pub(super) fn encode(node: &DockNode) -> String {
    match node {
        DockNode::Split {
            axis,
            children,
            sizes,
            ..
        } => {
            let children: Vec<String> = children
                .iter()
                .enumerate()
                .map(|(index, child)| match sizes.get(index).copied().flatten() {
                    Some(size) => format!("{}:{}", encode(child), size.0),
                    None => encode(child),
                })
                .collect();
            let axis = match axis {
                Axis::Horizontal => 'h',
                Axis::Vertical => 'v',
            };
            format!("{axis}({})", children.join(","))
        }
        DockNode::Tabs {
            panels,
            active_index,
            ..
        } => {
            let tabs: Vec<String> = panels
                .iter()
                .enumerate()
                .map(|(index, panel)| match index == *active_index {
                    true => format!("{panel}*"),
                    false => panel.to_string(),
                })
                .collect();
            format!("t({})", tabs.join(","))
        }
    }
}

/// The layout [`encode`] wrote. A tab may be open once, and every group
/// shows one of its tabs.
pub(super) fn decode(text: &str) -> StoreResult<DockNode> {
    let mut decoder = Decoder {
        rest: text,
        seen: Vec::new(),
    };
    let node = decoder.node(0)?;
    if !decoder.rest.is_empty() {
        return Err("text after the layout".into());
    }
    Ok(node)
}

struct Decoder<'a> {
    rest: &'a str,
    /// The tabs read so far.
    seen: Vec<Kind>,
}

impl Decoder<'_> {
    fn take(&mut self, token: &str) -> bool {
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn node(&mut self, depth: usize) -> StoreResult<DockNode> {
        if depth > MAX_DEPTH {
            return Err("splits nested too deep".into());
        }
        if self.take("t(") {
            return self.tabs();
        }
        let axis = if self.take("h(") {
            Axis::Horizontal
        } else if self.take("v(") {
            Axis::Vertical
        } else {
            return Err("expected a split or a tab group".into());
        };
        let mut children = Vec::new();
        let mut sizes = Vec::new();
        while !self.take(")") {
            if !children.is_empty() && !self.take(",") {
                return Err("expected , or ) in a split".into());
            }
            children.push(self.node(depth + 1)?);
            sizes.push(match self.take(":") {
                true => Some(self.size()?),
                false => None,
            });
        }
        Ok(DockNode::Split {
            id: None,
            axis,
            children,
            sizes,
        })
    }

    fn tabs(&mut self) -> StoreResult<DockNode> {
        let (tabs, rest) = self.rest.split_once(')').ok_or("unclosed tab group")?;
        self.rest = rest;
        let mut panels = Vec::new();
        let mut shown = None;
        for (index, slug) in tabs.split(',').enumerate() {
            let (slug, starred) = match slug.strip_suffix('*') {
                Some(slug) => (slug, true),
                None => (slug, false),
            };
            let kind = Kind::from_slug(slug).ok_or("unknown tab kind")?;
            if self.seen.contains(&kind) {
                return Err("duplicate tab kind".into());
            }
            self.seen.push(kind);
            if starred && shown.replace(index).is_some() {
                return Err("a tab group shows one tab".into());
            }
            panels.push(kind.panel());
        }
        Ok(DockNode::Tabs {
            id: None,
            panels,
            active_index: shown.ok_or("a tab group shows one tab")?,
        })
    }

    fn size(&mut self) -> StoreResult<Points> {
        let end = self.rest.find([',', ')']).ok_or("unclosed split")?;
        let (number, rest) = self.rest.split_at(end);
        self.rest = rest;
        let size: f32 = number.parse()?;
        if !size.is_finite() || size <= 0.0 {
            return Err("a size is a positive number".into());
        }
        Ok(px(size))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{decode, encode, load_from, load_legacy_from, parse, parse_legacy, save_to};
    use crate::app::tabs::{Kind, State};

    fn state(layout: &str) -> State {
        State::from_node(&decode(layout).unwrap())
    }

    fn text(state: &State) -> String {
        encode(&state.layout().center)
    }

    #[test]
    fn split_single_and_empty_workspaces_round_trip() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/workspace.txt");
        for layout in [
            "h(t(song-information,library*,now-playing))",
            "h(t(library*,queue):412.5,v(t(now-playing*),t(settings*,playlists):300))",
            "v(t(library*),t(queue*))",
            "h()",
        ] {
            save_to(&path, &state(layout)).unwrap();
            let loaded = load_from(&path).unwrap().unwrap();
            assert_eq!(text(&loaded), layout);
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "618player-workspace-v2\nlayout=h()\n"
        );
    }

    #[test]
    fn a_layout_is_tidied_as_it_is_read() {
        // A split of one, and a split in a split of its own direction.
        assert_eq!(text(&state("v(h(t(library*)))")), "v(t(library*))");
        assert_eq!(
            text(&state("h(t(library*),h(t(queue*),t(settings*)))")),
            "h(t(library*),t(queue*),t(settings*))"
        );
        // A lone group sits in the root split.
        assert_eq!(text(&state("t(library*,queue)")), "h(t(library*,queue))");
    }

    #[test]
    fn accepts_supported_terminal_endings() {
        for ending in ["", "\n", "\r\n"] {
            let text = format!("618player-workspace-v2\nlayout=h(t(library*)){ending}");
            assert_eq!(parse(&text).unwrap().focus(), Some(Kind::Library));
        }
    }

    #[test]
    fn rejects_line_shape_and_header_errors() {
        for text in [
            "bad\nlayout=h(t(library*))",
            "618player-workspace-v2",
            "618player-workspace-v2\nh(t(library*))",
            "618player-workspace-v2\nlayout=h(t(library*))\nextra",
            "618player-workspace-v2\n\nlayout=h(t(library*))",
            "618player-workspace-v1\nactive=library\ntabs=library",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn rejects_layouts_that_are_not_a_workspace() {
        for layout in [
            "",
            "x(t(library*))",
            "h(t(library*)",
            "h(t(library*)))",
            "h(t(library*) t(queue*))",
            "h(t(library*),)",
            "h(t())",
            "h(t(library))",
            "h(t(library*,queue*))",
            "h(t(library*,library))",
            "h(t(library*),t(library*))",
            "h(t(unknown*))",
            "h(t(library*):wide,t(queue*))",
            "h(t(library*):-4,t(queue*))",
            "h(t(library*):inf,t(queue*))",
            &format!("{}t(library*){}", "h(".repeat(40), ")".repeat(40)),
        ] {
            assert!(decode(layout).is_err(), "accepted {layout:?}");
        }
    }

    #[test]
    fn a_v1_file_is_read_as_one_group() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace-v1.txt");
        for (file, layout, focus) in [
            (
                "618player-workspace-v1\nactive=library\ntabs=song-information,library,now-playing\n",
                "h(t(song-information,library*,now-playing))",
                Some(Kind::Library),
            ),
            (
                "618player-workspace-v1\r\nactive=\r\ntabs=\r\n",
                "h()",
                None,
            ),
        ] {
            fs::write(&path, file).unwrap();
            let loaded = load_legacy_from(&path).unwrap().unwrap();
            assert_eq!((text(&loaded).as_str(), loaded.focus()), (layout, focus));
        }
        for text in [
            "618player-workspace-v2\nactive=library\ntabs=library",
            "618player-workspace-v1\nactive=library",
            "618player-workspace-v1\nactive=library\ntabs=library,library",
            "618player-workspace-v1\nactive=library\ntabs=library,unknown",
            "618player-workspace-v1\nactive=unknown\ntabs=library",
            "618player-workspace-v1\nactive=\ntabs=library",
            "618player-workspace-v1\nactive=library\ntabs=",
            "618player-workspace-v1\nactive=now-playing\ntabs=library",
        ] {
            assert!(parse_legacy(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn missing_file_returns_none() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing");
        assert!(load_from(&missing).unwrap().is_none());
        assert!(load_legacy_from(&missing).unwrap().is_none());
    }

    #[test]
    fn reports_read_directory_and_write_errors() {
        let directory = tempdir().unwrap();
        assert!(load_from(directory.path()).is_err());

        let blocking_file = directory.path().join("parent");
        fs::write(&blocking_file, "not a directory").unwrap();
        assert!(save_to(&blocking_file.join("workspace"), &State::default()).is_err());
        assert!(save_to(directory.path(), &State::default()).is_err());
    }
}

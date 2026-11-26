//! Which views the app is drawn with: the desktop's (a window, a mouse,
//! tabs) or a phone's (one screen, touch). Independent of the system the
//! app runs on ([`crate::platform`]): a desktop build shows the phone's
//! views with `PLAYER618_LAYOUT=mobile`, to try them at the desk
//! (`nsh run --window-size 412x915`, a Galaxy S Ultra's size in points).

/// The environment variable that picks the layout on the desktop.
const LAYOUT_VARIABLE: &str = "PLAYER618_LAYOUT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Desktop,
    Mobile,
}

impl Layout {
    /// A phone's always; on the desktop, [`LAYOUT_VARIABLE`]'s.
    pub(crate) fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            return Self::Mobile;
        }
        Self::from_setting(std::env::var(LAYOUT_VARIABLE).ok().as_deref())
    }

    fn from_setting(setting: Option<&str>) -> Self {
        match setting.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("mobile") => Self::Mobile,
            _ => Self::Desktop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Layout;

    #[test]
    fn only_mobile_picks_the_phone_layout() {
        assert_eq!(Layout::from_setting(None), Layout::Desktop);
        assert_eq!(Layout::from_setting(Some("")), Layout::Desktop);
        assert_eq!(Layout::from_setting(Some("tablet")), Layout::Desktop);
        assert_eq!(Layout::from_setting(Some("desktop")), Layout::Desktop);
        assert_eq!(Layout::from_setting(Some("mobile")), Layout::Mobile);
        assert_eq!(Layout::from_setting(Some(" Mobile ")), Layout::Mobile);
    }
}

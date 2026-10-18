//! The control center's icon set, embedded into the binary.
//!
//! The 1.5 UI handoff ships 60 SVGs (Lucide 1.8.0 plus DotIsle's own drawings). gpui renders an
//! SVG as an alpha mask tinted by the element's text colour, so one copy of each file serves both
//! appearances and every accent.
use std::borrow::Cow;

use gpui::{prelude::*, *};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        /// The bytes behind `icons/<name>.svg`, or `None` for an unknown name.
        pub fn bytes(name: &str) -> Option<&'static [u8]> {
            match name {
                $($name => Some(include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons!(
    "alarm-clock", "arrow-left", "arrow-right", "arrow-up-right", "bell-ring", "bell",
    "calendar-days", "check", "chevron-down", "chevron-right", "chevron-up", "circle-alert",
    "circle-check", "circle-help", "clock-3", "database-backup", "dotisle-countdown",
    "dotisle-cover-palette", "dotisle-fluid-island", "dotisle-notification-recall",
    "dotisle-phone-connected", "dotisle-phone-disconnected", "dotisle-pomodoro",
    "dotisle-stopwatch", "dotisle-task-focus", "dotisle-usb-drive", "download", "eye-off",
    "hard-drive", "headphones", "heart", "keyboard", "layout-dashboard", "maximize-2",
    "message-circle", "minimize-2", "minus", "music-2", "pause", "pill", "play", "plug",
    "plus", "refresh-cw", "repeat", "rotate-ccw", "settings-2", "shuffle", "skip-back",
    "skip-forward", "smartphone", "square", "timer", "trash-2", "unplug", "upload", "usb",
    "volume-2", "volume-x", "x",
);

/// Serves the embedded icons to gpui's `svg()` element.
pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let name = path.strip_prefix("icons/").and_then(|rest| rest.strip_suffix(".svg")).unwrap_or(path);
        Ok(bytes(name).map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// A 16px icon in the primary text colour.
///
/// gpui does not inherit the text colour into an `Svg` element, and an `Svg` without a colour is
/// not painted at all, so every icon carries its tint explicitly.
pub fn icon(name: &str) -> Svg {
    tinted(name, crate::theme::text())
}

/// A 16px icon in `color`.
pub fn tinted(name: &str, color: impl Into<Hsla>) -> Svg {
    svg().path(SharedString::from(format!("icons/{name}.svg"))).w(px(16.)).h(px(16.)).text_color(color)
}

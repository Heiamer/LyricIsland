//! Design tokens, in a light and a dark set.
//!
//! Following the bundled `apple-design` skill: depth comes from layered surfaces and hairlines
//! rather than heavy borders, the palette stays restrained around the product's mint accent, and
//! the type scale is small and deliberate. Both appearances are first-class — light is not a
//! washed-out copy of dark but its own set of surfaces.
//!
//! The active palette is held in a thread-local because the UI is built by free functions that
//! would otherwise each need a palette parameter threaded through every call site. It is set once
//! per frame from `Render` and read only while building that frame.
use std::cell::Cell;

use gpui::Rgba;

const fn c(r: u32, g: u32, b: u32, a: f32) -> Rgba {
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

/// Every colour the settings window draws with.
#[derive(Clone, Copy)]
pub struct Palette {
    /// Window base, one step darker (or lighter) than any content surface.
    pub base: Rgba,
    /// Navigation rail.
    pub sidebar: Rgba,
    /// A card resting on the base.
    pub surface: Rgba,
    /// A card under the pointer, or a selected navigation row.
    pub surface_hi: Rgba,
    /// A row under the pointer that is not otherwise highlighted.
    pub hover: Rgba,
    /// Separators: a wash, never a line of solid colour.
    pub hairline: Rgba,
    pub hairline_bright: Rgba,
    /// An inset panel, such as the colour picker.
    pub field: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    pub text_faint: Rgba,
    /// The product accent, shared with the capsule's lyric fill.
    pub accent: Rgba,
    pub accent_dim: Rgba,
    /// Whatever reads clearly on top of `accent`.
    pub on_accent: Rgba,
    pub track: Rgba,
    pub knob: Rgba,
    pub danger: Rgba,
}

pub const DARK: Palette = Palette {
    base: c(0x0E, 0x13, 0x16, 1.0),
    sidebar: c(0x12, 0x1A, 0x1E, 1.0),
    surface: c(0x18, 0x21, 0x26, 1.0),
    surface_hi: c(0x21, 0x2D, 0x33, 1.0),
    hover: c(0x1B, 0x25, 0x2B, 1.0),
    hairline: c(0xFF, 0xFF, 0xFF, 0.06),
    hairline_bright: c(0xFF, 0xFF, 0xFF, 0.14),
    field: c(0x0B, 0x10, 0x12, 1.0),
    text: c(0xF2, 0xF5, 0xF6, 1.0),
    text_dim: c(0x9B, 0xA8, 0xAE, 1.0),
    text_faint: c(0x66, 0x74, 0x7B, 1.0),
    accent: c(0xA9, 0xE6, 0xCD, 1.0),
    accent_dim: c(0xA9, 0xE6, 0xCD, 0.18),
    on_accent: c(0x0E, 0x13, 0x16, 1.0),
    track: c(0x2A, 0x36, 0x3C, 1.0),
    knob: c(0xE8, 0xEE, 0xF0, 1.0),
    danger: c(0xFF, 0xB4, 0xA9, 1.0),
};

pub const LIGHT: Palette = Palette {
    base: c(0xF4, 0xF5, 0xF7, 1.0),
    sidebar: c(0xEA, 0xEC, 0xEF, 1.0),
    surface: c(0xFF, 0xFF, 0xFF, 1.0),
    surface_hi: c(0xE1, 0xE5, 0xE9, 1.0),
    hover: c(0xFA, 0xFB, 0xFC, 1.0),
    hairline: c(0x00, 0x00, 0x00, 0.09),
    hairline_bright: c(0x00, 0x00, 0x00, 0.18),
    field: c(0xEC, 0xEE, 0xF1, 1.0),
    text: c(0x1B, 0x1F, 0x23, 1.0),
    text_dim: c(0x5B, 0x66, 0x6E, 1.0),
    text_faint: c(0x8A, 0x94, 0x9C, 1.0),
    // The capsule's mint is too pale on white, so light mode uses a deeper tint of the same hue.
    accent: c(0x18, 0x8F, 0x6B, 1.0),
    accent_dim: c(0x18, 0x8F, 0x6B, 0.14),
    on_accent: c(0xFF, 0xFF, 0xFF, 1.0),
    track: c(0xD2, 0xD7, 0xDC, 1.0),
    knob: c(0xFF, 0xFF, 0xFF, 1.0),
    danger: c(0xB4, 0x36, 0x2C, 1.0),
};

/// The full Noto Sans SC face shipped with the application.
pub const FONT: &str = "Noto Sans SC";

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(DARK) };
}

/// Pick the palette for this frame.
pub fn select(theme: &str, system_is_dark: bool) -> Palette {
    match theme {
        "Light" => LIGHT,
        "Dark" => DARK,
        // "System" and anything unrecognised follow Windows.
        _ => if system_is_dark { DARK } else { LIGHT },
    }
}

/// Make `palette` current for the frame being built.
pub fn set_current(palette: Palette) {
    CURRENT.with(|current| current.set(palette));
}

fn current() -> Palette {
    CURRENT.with(Cell::get)
}

macro_rules! token {
    ($($name:ident),* $(,)?) => {
        $(pub fn $name() -> Rgba { current().$name })*
    };
}

token!(
    base, sidebar, surface, surface_hi, hover, hairline, hairline_bright, field, text, text_dim, text_faint, accent,
    accent_dim, on_accent, track, knob, danger,
);

/// Parse `#RRGGBB`; anything else falls back to a neutral so a bad setting never panics a frame.
pub fn parse(hex: &str) -> Rgba {
    if hex.len() == 7 && hex.starts_with('#') {
        if let Ok(value) = u32::from_str_radix(&hex[1..], 16) {
            return c((value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF, 1.0);
        }
    }
    current().text
}

/// Render back to the `#RRGGBB` the native validator accepts.
pub fn to_hex(color: Rgba) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("#{:02X}{:02X}{:02X}", byte(color.r), byte(color.g), byte(color.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_the_color_type() {
        for hex in ["#000000", "#FFFFFF", "#A9E6CD", "#B1E8D1", "#F4F6F8"] {
            assert_eq!(to_hex(parse(hex)), hex);
        }
    }

    #[test]
    fn invalid_input_falls_back_instead_of_panicking() {
        set_current(DARK);
        assert_eq!(to_hex(parse("nonsense")), to_hex(DARK.text));
        assert_eq!(to_hex(parse("#12345")), to_hex(DARK.text));
    }

    #[test]
    fn light_and_dark_are_distinct_and_system_follows_windows() {
        assert!(LIGHT.base.r > DARK.base.r, "light base must be lighter than dark");
        assert!(LIGHT.text.r < DARK.text.r, "light text must be darker than dark text");
        assert_eq!(select("Light", true).base.r, LIGHT.base.r);
        assert_eq!(select("Dark", false).base.r, DARK.base.r);
        assert_eq!(select("System", true).base.r, DARK.base.r);
        assert_eq!(select("System", false).base.r, LIGHT.base.r);
        // An unknown value must not leave the window unpainted.
        assert_eq!(select("nonsense", false).base.r, LIGHT.base.r);
    }

    #[test]
    fn selecting_a_palette_changes_what_the_frame_draws() {
        set_current(LIGHT);
        assert_eq!(text().r, LIGHT.text.r);
        set_current(DARK);
        assert_eq!(text().r, DARK.text.r);
    }
}

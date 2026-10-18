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
    /// A row under the pointer *inside* a panel.
    ///
    /// The two appearances need different directions here, not the same value. On white, a hover
    /// steps back — a light grey fill reads as "this row is live". The same relationship inverts in
    /// the dark set, where the base is far *darker* than the panel: painting a row with it turns the
    /// card into a hole with a shadow inside it. Dark therefore lifts one step above `surface`.
    pub row_hover: Rgba,
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
    /// Wash behind green-tinted symbols and toasts.
    pub green_soft: Rgba,
    /// Warm highlight surface, worn by the next-reminder card.
    pub yellow: Rgba,
    /// Stripe colour on a yellow todo card.
    pub yellow_line: Rgba,
    /// Quiet green todo surface.
    pub mint: Rgba,
    /// Quiet violet todo surface.
    pub lilac: Rgba,
    /// Stripe colour on a lilac todo card.
    pub lilac_bar: Rgba,
    /// Drop shadow under raised surfaces.
    pub shadow: Rgba,
    /// Wash over the page while a drawer is open.
    pub scrim: Rgba,
    pub track: Rgba,
    pub knob: Rgba,
    pub danger: Rgba,
}

pub const DARK: Palette = Palette {
    base: c(0x19, 0x1B, 0x1F, 1.0),
    sidebar: c(0x22, 0x25, 0x2B, 1.0),
    surface: c(0x29, 0x2D, 0x33, 1.0),
    surface_hi: c(0x35, 0x3C, 0x44, 1.0),
    hover: c(0x19, 0x1B, 0x1F, 1.0),
    row_hover: c(0x30, 0x36, 0x3E, 1.0),
    hairline: c(0x39, 0x3F, 0x47, 1.0),
    hairline_bright: c(0xFF, 0xFF, 0xFF, 0.14),
    field: c(0x19, 0x1B, 0x1F, 1.0),
    text: c(0xED, 0xF0, 0xF2, 1.0),
    text_dim: c(0xAE, 0xB7, 0xC1, 1.0),
    text_faint: c(0x7C, 0x86, 0x91, 1.0),
    accent: c(0x73, 0xD4, 0xB7, 1.0),
    accent_dim: c(0x73, 0xD4, 0xB7, 0.18),
    on_accent: c(0x09, 0x2B, 0x23, 1.0),
    green_soft: c(0x24, 0x3E, 0x36, 1.0),
    yellow: c(0x3F, 0x38, 0x29, 1.0),
    yellow_line: c(0xE8, 0xC5, 0x6B, 1.0),
    mint: c(0x29, 0x3D, 0x34, 1.0),
    lilac: c(0x34, 0x30, 0x44, 1.0),
    lilac_bar: c(0xB8, 0xA6, 0xD8, 1.0),
    shadow: c(0x00, 0x00, 0x00, 0.25),
    scrim: c(0x00, 0x00, 0x00, 0.42),
    track: c(0x35, 0x3C, 0x44, 1.0),
    knob: c(0xE8, 0xEE, 0xF0, 1.0),
    danger: c(0xFF, 0xB4, 0xA9, 1.0),
};

pub const LIGHT: Palette = Palette {
    base: c(0xF4, 0xF5, 0xF7, 1.0),
    sidebar: c(0xEA, 0xED, 0xF0, 1.0),
    surface: c(0xFF, 0xFF, 0xFF, 1.0),
    surface_hi: c(0xE2, 0xE7, 0xEB, 1.0),
    hover: c(0xF4, 0xF5, 0xF7, 1.0),
    row_hover: c(0xF3, 0xF5, 0xF7, 1.0),
    hairline: c(0xE0, 0xE4, 0xE8, 1.0),
    hairline_bright: c(0x00, 0x00, 0x00, 0.18),
    field: c(0xF4, 0xF5, 0xF7, 1.0),
    text: c(0x23, 0x28, 0x2D, 1.0),
    text_dim: c(0x64, 0x6F, 0x79, 1.0),
    text_faint: c(0x8A, 0x94, 0x9C, 1.0),
    accent: c(0x11, 0x86, 0x68, 1.0),
    accent_dim: c(0x11, 0x86, 0x68, 0.14),
    on_accent: c(0xFF, 0xFF, 0xFF, 1.0),
    green_soft: c(0xE8, 0xF5, 0xEF, 1.0),
    yellow: c(0xFB, 0xF4, 0xE3, 1.0),
    yellow_line: c(0xD5, 0xAA, 0x45, 1.0),
    mint: c(0xE9, 0xF5, 0xEE, 1.0),
    lilac: c(0xF0, 0xED, 0xF8, 1.0),
    lilac_bar: c(0xA9, 0x9A, 0xCB, 1.0),
    shadow: c(0x19, 0x27, 0x33, 0.10),
    scrim: c(0x1D, 0x27, 0x36, 0.17),
    track: c(0xE2, 0xE7, 0xEB, 1.0),
    knob: c(0xFF, 0xFF, 0xFF, 1.0),
    danger: c(0xB4, 0x36, 0x2C, 1.0),
};

/// The full Noto Sans SC face shipped with the application.
pub const FONT: &str = "Noto Sans SC";
/// The faces the reference design asks for: Windows' own UI font first, its Chinese companion
/// second, the shipped face as the last resort.
pub const FONT_UI: &str = "Segoe UI";
pub const FONT_ZH: &str = "Microsoft YaHei UI";

/// The window-wide font stack.
pub fn ui_font() -> gpui::Font {
    gpui::Font {
        family: FONT_UI.into(),
        features: gpui::FontFeatures::default(),
        weight: gpui::FontWeight::default(),
        style: gpui::FontStyle::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![FONT_ZH.to_owned(), FONT.to_owned()])),
    }
}

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

/// Mix two palettes, field by field.
///
/// Switching appearance is a large change in brightness, and snapping between the two sets of
/// surfaces reads as a flash. Easing the palette itself — rather than fading a screenshot of the old
/// one — keeps every hairline and shadow coherent while the window travels between the two.
pub fn blend(from: Palette, to: Palette, progress: f32) -> Palette {
    // The ends are returned outright: `a + (b - a) * 1.0` lands a rounding step away from `b`, and a
    // finished theme change should draw the target palette exactly, not almost.
    if !(progress > 0.0) {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    let mix = |a: Rgba, b: Rgba| Rgba {
        r: a.r + (b.r - a.r) * progress,
        g: a.g + (b.g - a.g) * progress,
        b: a.b + (b.b - a.b) * progress,
        a: a.a + (b.a - a.a) * progress,
    };
    macro_rules! fields {
        ($($name:ident),* $(,)?) => {
            Palette { $($name: mix(from.$name, to.$name)),* }
        };
    }
    fields!(
        base, sidebar, surface, surface_hi, hover, row_hover, hairline, hairline_bright, field, text,
        text_dim, text_faint, accent, accent_dim, on_accent, green_soft, yellow, yellow_line, mint,
        lilac, lilac_bar, shadow, scrim, track, knob, danger,
    )
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
    base, sidebar, surface, surface_hi, hover, row_hover, hairline, hairline_bright, field, text, text_dim, text_faint, accent,
    accent_dim, on_accent, green_soft, yellow, yellow_line, mint, lilac, lilac_bar, shadow, scrim, track, knob, danger,
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

    #[test]
    fn blending_between_appearances_lands_exactly_on_both_ends() {
        // The ends must be the palettes themselves, or a finished theme change would leave the
        // window a hair off the colour it claims to be drawing.
        let start = blend(LIGHT, DARK, 0.0);
        assert_eq!(start.base.r, LIGHT.base.r);
        assert_eq!(start.text.r, LIGHT.text.r);
        let end = blend(LIGHT, DARK, 1.0);
        assert_eq!(end.base.r, DARK.base.r);
        assert_eq!(end.sidebar.g, DARK.sidebar.g);
        // Half way is neither end: the cross-fade has to actually travel.
        let middle = blend(LIGHT, DARK, 0.5);
        assert!(middle.base.r < LIGHT.base.r && middle.base.r > DARK.base.r);
        // An out-of-range progress must not paint a colour that exists nowhere.
        assert_eq!(blend(LIGHT, DARK, 2.0).base.r, DARK.base.r);
    }
}

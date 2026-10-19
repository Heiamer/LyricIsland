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
    /// Caption bar. One step off `base`, so the chrome reads as its own material rather than as a
    /// strip of the page that happens to sit above it.
    pub chrome: Rgba,
    /// Navigation rail.
    pub sidebar: Rgba,
    /// A card resting on the base.
    pub surface: Rgba,
    /// The two ends of the vertical wash inside a raised panel. Tinting the top a hair lighter and
    /// the bottom a hair deeper gives a flat fill a faint sense of thickness without a second card.
    pub panel_top: Rgba,
    pub panel_bottom: Rgba,
    /// Inset highlight along a panel's top edge: the one-pixel "lit edge" that makes a flat card
    /// read as a raised sheet of glass.
    pub sheen: Rgba,
    /// A card under the pointer, or a selected navigation row.
    pub surface_hi: Rgba,
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
    /// The deeper stop of an accent gradient: brand mark, primary buttons, the active tab.
    pub accent_deep: Rgba,
    /// A wide, very quiet accent wash. The page header paints one behind its title.
    pub glow: Rgba,
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
    base: c(0x0A, 0x10, 0x0E, 0.88),
    chrome: c(0x0D, 0x14, 0x12, 0.88),
    sidebar: c(0x0E, 0x15, 0x13, 0.88),
    surface: c(0x13, 0x1D, 0x19, 1.0),
    panel_top: c(0x17, 0x23, 0x1E, 1.0),
    panel_bottom: c(0x12, 0x1C, 0x18, 1.0),
    sheen: c(0xFF, 0xFF, 0xFF, 0.10),
    surface_hi: c(0x1D, 0x2A, 0x25, 1.0),
    row_hover: c(0x19, 0x24, 0x1F, 1.0),
    hairline: c(0x22, 0x30, 0x29, 1.0),
    hairline_bright: c(0xFF, 0xFF, 0xFF, 0.14),
    field: c(0x0D, 0x14, 0x12, 1.0),
    text: c(0xE6, 0xEF, 0xEA, 1.0),
    text_dim: c(0xA3, 0xB2, 0xAB, 1.0),
    text_faint: c(0x6E, 0x7F, 0x77, 1.0),
    accent: c(0x67, 0xDD, 0xBA, 1.0),
    accent_deep: c(0x43, 0xC6, 0xA2, 1.0),
    glow: c(0x67, 0xDD, 0xBA, 0.10),
    accent_dim: c(0x67, 0xDD, 0xBA, 0.15),
    on_accent: c(0x04, 0x25, 0x1C, 1.0),
    green_soft: c(0x16, 0x33, 0x2A, 1.0),
    yellow: c(0x33, 0x2F, 0x24, 1.0),
    yellow_line: c(0xE5, 0xC0, 0x69, 1.0),
    mint: c(0x18, 0x30, 0x26, 1.0),
    lilac: c(0x28, 0x25, 0x38, 1.0),
    lilac_bar: c(0xB4, 0xA5, 0xD6, 1.0),
    shadow: c(0x00, 0x00, 0x00, 0.34),
    scrim: c(0x00, 0x00, 0x00, 0.50),
    track: c(0x26, 0x34, 0x30, 1.0),
    knob: c(0xEA, 0xF2, 0xED, 1.0),
    danger: c(0xFF, 0xB3, 0xA7, 1.0),
};

pub const LIGHT: Palette = Palette {
    base: c(0xF1, 0xF4, 0xF2, 0.88),
    chrome: c(0xF6, 0xF8, 0xF7, 0.88),
    sidebar: c(0xE7, 0xEC, 0xE9, 0.88),
    surface: c(0xFB, 0xFC, 0xFB, 1.0),
    panel_top: c(0xFD, 0xFE, 0xFD, 1.0),
    panel_bottom: c(0xF7, 0xFA, 0xF8, 1.0),
    sheen: c(0xFF, 0xFF, 0xFF, 0.85),
    surface_hi: c(0xE3, 0xEA, 0xE6, 1.0),
    row_hover: c(0xEF, 0xF3, 0xF0, 1.0),
    hairline: c(0xDD, 0xE5, 0xE0, 1.0),
    hairline_bright: c(0x00, 0x00, 0x00, 0.16),
    field: c(0xF2, 0xF5, 0xF3, 1.0),
    text: c(0x14, 0x20, 0x1B, 1.0),
    text_dim: c(0x5B, 0x68, 0x61, 1.0),
    text_faint: c(0x8C, 0x97, 0x90, 1.0),
    accent: c(0x0D, 0x7A, 0x5E, 1.0),
    accent_deep: c(0x0A, 0x68, 0x51, 1.0),
    glow: c(0x0D, 0x7A, 0x5E, 0.10),
    accent_dim: c(0x0D, 0x7A, 0x5E, 0.12),
    on_accent: c(0xFF, 0xFF, 0xFF, 1.0),
    green_soft: c(0xE3, 0xF2, 0xEA, 1.0),
    yellow: c(0xF9, 0xF2, 0xDF, 1.0),
    yellow_line: c(0xD2, 0xA3, 0x3E, 1.0),
    mint: c(0xE6, 0xF2, 0xEA, 1.0),
    lilac: c(0xEF, 0xEC, 0xF7, 1.0),
    lilac_bar: c(0xA7, 0x9A, 0xC9, 1.0),
    shadow: c(0x10, 0x23, 0x1B, 0.10),
    scrim: c(0x0D, 0x1A, 0x15, 0.24),
    track: c(0xE1, 0xE8, 0xE4, 1.0),
    knob: c(0xFF, 0xFF, 0xFF, 1.0),
    danger: c(0xB2, 0x3A, 0x30, 1.0),
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

/// Display face. The bundled Noto Sans SC carries both Chinese and Latin, and its slightly larger
/// x-height lets the page titles hold their weight without shouting.
pub fn display_font() -> gpui::Font {
    gpui::Font {
        family: FONT.into(),
        features: gpui::FontFeatures::default(),
        weight: gpui::FontWeight::default(),
        style: gpui::FontStyle::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![FONT_UI.to_owned(), FONT_ZH.to_owned()])),
    }
}

/// Small caps-ish meta face for eyebrows and numbers. Cascadia ships with Windows 11; Consolas is
/// the fallback on older installs, so a missing face never turns a label into tofu.
pub fn mono_font() -> gpui::Font {
    gpui::Font {
        family: "Cascadia Mono".into(),
        features: gpui::FontFeatures::default(),
        weight: gpui::FontWeight::default(),
        style: gpui::FontStyle::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec!["Consolas".to_owned(), FONT_UI.to_owned(), FONT_ZH.to_owned()])),
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
        base, chrome, sidebar, surface, panel_top, panel_bottom, sheen, surface_hi, row_hover,
        hairline, hairline_bright, field, text, text_dim, text_faint, accent, accent_deep, glow,
        accent_dim, on_accent, green_soft, yellow, yellow_line, mint, lilac, lilac_bar, shadow,
        scrim, track, knob, danger,
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
    base, chrome, sidebar, surface, panel_top, panel_bottom, sheen, surface_hi, row_hover,
    hairline, hairline_bright, field, text, text_dim, text_faint, accent, accent_deep, glow,
    accent_dim, on_accent, green_soft, yellow, yellow_line, mint, lilac, lilac_bar, shadow,
    scrim, track, knob, danger,
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

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
    base: c(0x0A, 0x0D, 0x14, 0.80),
    chrome: c(0x1E, 0x24, 0x30, 0.56),
    sidebar: c(0x14, 0x1B, 0x27, 0.44),
    surface: c(0x20, 0x27, 0x33, 0.94),
    panel_top: c(0x35, 0x3E, 0x4F, 0.68),
    panel_bottom: c(0x1C, 0x24, 0x32, 0.76),
    sheen: c(0xFF, 0xFF, 0xFF, 0.20),
    surface_hi: c(0x1E, 0x25, 0x33, 1.0),
    row_hover: c(0x19, 0x20, 0x2C, 1.0),
    hairline: c(0xBD, 0xD0, 0xEF, 0.13),
    hairline_bright: c(0xFF, 0xFF, 0xFF, 0.14),
    field: c(0x16, 0x1D, 0x29, 1.0),
    text: c(0xE8, 0xED, 0xF5, 1.0),
    text_dim: c(0xA4, 0xAF, 0xBF, 1.0),
    text_faint: c(0x6F, 0x7B, 0x8D, 1.0),
    accent: c(0x6E, 0xA8, 0xFF, 1.0),
    accent_deep: c(0x3B, 0x82, 0xF6, 1.0),
    glow: c(0x6E, 0xA8, 0xFF, 0.10),
    accent_dim: c(0x6E, 0xA8, 0xFF, 0.15),
    on_accent: c(0x06, 0x12, 0x25, 1.0),
    green_soft: c(0x16, 0x22, 0x38, 1.0),
    yellow: c(0x33, 0x2C, 0x22, 1.0),
    yellow_line: c(0xE5, 0xC0, 0x69, 1.0),
    mint: c(0x17, 0x24, 0x3A, 1.0),
    lilac: c(0x25, 0x25, 0x38, 1.0),
    lilac_bar: c(0xB4, 0xA5, 0xD6, 1.0),
    shadow: c(0x00, 0x00, 0x00, 0.34),
    scrim: c(0x00, 0x00, 0x00, 0.50),
    track: c(0x27, 0x31, 0x42, 1.0),
    knob: c(0xEC, 0xF1, 0xF9, 1.0),
    danger: c(0xFF, 0xB3, 0xA7, 1.0),
};

pub const LIGHT: Palette = Palette {
    base: c(0xEC, 0xF1, 0xF9, 0.80),
    chrome: c(0xFF, 0xFF, 0xFF, 0.60),
    sidebar: c(0xFF, 0xFF, 0xFF, 0.34),
    surface: c(0xF9, 0xFB, 0xFF, 0.94),
    panel_top: c(0xFF, 0xFF, 0xFF, 0.86),
    panel_bottom: c(0xF6, 0xF9, 0xFF, 0.58),
    sheen: c(0xFF, 0xFF, 0xFF, 0.94),
    surface_hi: c(0xEF, 0xF4, 0xFE, 1.0),
    row_hover: c(0xF4, 0xF7, 0xFD, 1.0),
    hairline: c(0x8E, 0xA1, 0xBD, 0.24),
    hairline_bright: c(0x0F, 0x17, 0x2A, 0.12),
    field: c(0xF1, 0xF5, 0xFC, 1.0),
    text: c(0x0F, 0x17, 0x2A, 1.0),
    text_dim: c(0x5A, 0x65, 0x75, 1.0),
    text_faint: c(0x8B, 0x95, 0xA5, 1.0),
    accent: c(0x25, 0x63, 0xEB, 1.0),
    accent_deep: c(0x1D, 0x4E, 0xD8, 1.0),
    glow: c(0x25, 0x63, 0xEB, 0.08),
    accent_dim: c(0x25, 0x63, 0xEB, 0.12),
    on_accent: c(0xFF, 0xFF, 0xFF, 1.0),
    green_soft: c(0xEE, 0xF4, 0xFF, 1.0),
    yellow: c(0xFF, 0xF8, 0xE8, 1.0),
    yellow_line: c(0xD2, 0xA3, 0x3E, 1.0),
    mint: c(0xEE, 0xF4, 0xFF, 1.0),
    lilac: c(0xF3, 0xEF, 0xFB, 1.0),
    lilac_bar: c(0xA7, 0x9A, 0xC9, 1.0),
    shadow: c(0x0F, 0x17, 0x2A, 0.10),
    scrim: c(0x0F, 0x17, 0x2A, 0.24),
    track: c(0xE6, 0xEC, 0xF7, 1.0),
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

/// Preserve the chosen hue while deriving readable foreground and control colours.
pub fn customize(mut p:Palette,light:&str,dark:&str,is_dark:bool)->Palette{
    let hex=if is_dark{dark}else{light};
    let value=if hex.len()==7&&hex.starts_with('#'){u32::from_str_radix(&hex[1..],16).ok()}else{None};
    let Some(v)=value else{return p};
    if v==if is_dark{0x6EA8FF}else{0x2563EB}{return p;}
    let raw=c((v>>16)&255,(v>>8)&255,v&255,1.0);
    let mix=|a:Rgba,b:Rgba,t:f32|Rgba{r:a.r+(b.r-a.r)*t,g:a.g+(b.g-a.g)*t,b:a.b+(b.b-a.b)*t,a:1.0};
    let lum=|v:Rgba|{let ch=|x:f32|if x<=0.04045{x/12.92}else{((x+0.055)/1.055).powf(2.4)};0.2126*ch(v.r)+0.7152*ch(v.g)+0.0722*ch(v.b)};
    let ratio=|a:Rgba,b:Rgba|{let x=lum(a);let y=lum(b);(x.max(y)+0.05)/(x.min(y)+0.05)};
    let white=c(255,255,255,1.0);let black=c(0,0,0,1.0);
    let mut accent=raw;
    let safe=if is_dark{white}else{black};
    for _ in 0..32{if ratio(accent,p.base)>=4.5{break}accent=mix(accent,safe,0.08);}
    p.accent=accent;
    p.accent_deep=mix(accent,black,0.16);
    p.on_accent=if ratio(white,accent)>=ratio(black,accent){white}else{black};
    if ratio(p.on_accent,p.accent_deep)<4.5{p.accent_deep=accent;}
    p.glow=Rgba{a:0.14,..raw};p.accent_dim=Rgba{a:0.14,..accent};
    p.green_soft=Rgba{a:0.12,..accent};p.mint=Rgba{a:0.10,..accent};
    p
}

/// The smoke harness covers adversarial picker colours in both appearances.
pub fn accent_regression()->bool{
    let lum=|v:Rgba|{let ch=|x:f32|if x<=0.04045{x/12.92}else{((x+0.055)/1.055).powf(2.4)};0.2126*ch(v.r)+0.7152*ch(v.g)+0.0722*ch(v.b)};
    let ratio=|a:Rgba,b:Rgba|{let x=lum(a);let y=lum(b);(x.max(y)+0.05)/(x.min(y)+0.05)};
    for dark in [false,true]{for hex in ["#000000","#FFFFFF","#FFFF00","#FF0000","#00FF00","#0000FF","#777777","#FF00FF"]{
        let p=customize(if dark{DARK}else{LIGHT},hex,hex,dark);
        if ratio(p.accent,p.base)<4.5||ratio(p.on_accent,p.accent)<4.5||ratio(p.on_accent,p.accent_deep)<4.5{return false}
    }}
    to_hex(customize(LIGHT,"#2563EB","#6EA8FF",false).accent_deep)==to_hex(LIGHT.accent_deep)
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

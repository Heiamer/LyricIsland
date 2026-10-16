//! Colour maths for the picker, kept free of any UI type so it can be tested directly.
//!
//! The picker works in HSV because that is what a hue strip plus a saturation/value pad map onto
//! one-to-one. gpui paints in RGB, so conversion happens at the edge.
use gpui::Rgba;

/// Hue, saturation and value, each in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub const fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }

    pub fn from_rgb(color: Rgba) -> Self {
        let (r, g, b) = (color.r, color.g, color.b);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let h = if delta <= f32::EPSILON {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            (((b - r) / delta) + 2.0) / 6.0
        } else {
            (((r - g) / delta) + 4.0) / 6.0
        };
        let s = if max <= f32::EPSILON { 0.0 } else { delta / max };
        Self { h: h.rem_euclid(1.0), s, v: max }
    }

    pub fn to_rgb(self) -> Rgba {
        let h = self.h.rem_euclid(1.0) * 6.0;
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);
        let sector = h.floor();
        let f = h - sector;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match sector as i32 % 6 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Rgba { r, g, b, a: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(value: u32) -> Rgba {
        Rgba {
            r: ((value >> 16) & 0xFF) as f32 / 255.0,
            g: ((value >> 8) & 0xFF) as f32 / 255.0,
            b: (value & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.005
    }

    #[test]
    fn primaries_land_on_the_expected_hue() {
        assert!(close(Hsv::from_rgb(rgb(0xFF0000)).h, 0.0));
        assert!(close(Hsv::from_rgb(rgb(0x00FF00)).h, 1.0 / 3.0));
        assert!(close(Hsv::from_rgb(rgb(0x0000FF)).h, 2.0 / 3.0));
        assert!(close(Hsv::from_rgb(rgb(0x00FFFF)).h, 0.5));
    }

    #[test]
    fn greys_have_no_saturation_and_keep_their_value() {
        let white = Hsv::from_rgb(rgb(0xFFFFFF));
        assert!(close(white.s, 0.0) && close(white.v, 1.0));
        let mid = Hsv::from_rgb(rgb(0x808080));
        assert!(close(mid.s, 0.0) && close(mid.v, 0.502, ), "value was {}", mid.v);
    }

    #[test]
    fn every_colour_survives_a_round_trip_through_hsv() {
        for hex in [0x000000, 0xFFFFFF, 0xA9E6CD, 0xB1E8D1, 0xF4F6F8, 0xFF6699, 0x123456] {
            let original = rgb(hex);
            let back = Hsv::from_rgb(original).to_rgb();
            assert!(
                close(original.r, back.r) && close(original.g, back.g) && close(original.b, back.b),
                "{hex:06X} did not survive: {:?} -> {:?}",
                (original.r, original.g, original.b),
                (back.r, back.g, back.b)
            );
        }
    }

    #[test]
    fn hue_stays_inside_the_unit_interval() {
        // A hue of exactly 1.0 is red again, and must not produce a seventh sector.
        let red = Hsv::new(1.0, 1.0, 1.0).to_rgb();
        assert!(close(red.r, 1.0) && close(red.g, 0.0) && close(red.b, 0.0));
        assert_eq!(Hsv::new(-0.25, 1.0, 1.0), Hsv::new(-0.25, 1.0, 1.0));
        let wrapped = Hsv::new(-0.25, 1.0, 1.0).to_rgb();
        let expected = Hsv::new(0.75, 1.0, 1.0).to_rgb();
        assert!(close(wrapped.r, expected.r) && close(wrapped.g, expected.g) && close(wrapped.b, expected.b));
    }
}

/// Accept RGB hex with an optional hash; keep the persistence format canonical.
pub fn normalize_hex(value:&str)->Option<String>{let v=value.trim().strip_prefix('#').unwrap_or(value.trim());if v.len()==6&&v.bytes().all(|c|c.is_ascii_hexdigit()){Some(format!("#{}",v.to_ascii_uppercase()))}else{None}}
#[cfg(test)]mod hex_tests{use super::*;#[test]fn validates_hex(){assert_eq!(normalize_hex(" a9e6cd ").as_deref(),Some("#A9E6CD"));for v in ["#fff","#12345G","1234567",""]{assert!(normalize_hex(v).is_none());}}}

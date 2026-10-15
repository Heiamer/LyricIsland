//! Value maths shared by the interactive rows.
//!
//! Deliberately free of any UI type: keeping the mapping out of a module that imports gpui's element
//! macros is what makes it testable (their expansion is deep enough to hit the crate's recursion
//! limit inside a test harness).

/// Map a pointer position onto a slider's range.
///
/// `origin_x`/`width` describe the painted track, `pointer_x` is a window coordinate. The result is
/// clamped to the range, so dragging past either end pins to the limit instead of running away.
pub fn position_to_value(min: f64, max: f64, origin_x: f32, width: f32, pointer_x: f32) -> f64 {
    if width <= 0.0 {
        return min;
    }
    let fraction = ((pointer_x - origin_x) / width).clamp(0.0, 1.0) as f64;
    min + fraction * (max - min)
}

/// Compact read-out: whole numbers for wide ranges, one decimal for the spring constants.
pub fn format_value(value: f64, min: f64, max: f64, unit: &str) -> String {
    let text = if max - min > 100.0 { format!("{value:.0}") } else { format!("{value:.1}") };
    format!("{text}{unit}")
}

/// Step one character to the left, staying on a UTF-8 boundary.
pub fn prev_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    if index == 0 {
        return 0;
    }
    index -= 1;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Step one character to the right, staying on a UTF-8 boundary.
pub fn next_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut index = index + 1;
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_maps_the_track_onto_the_range() {
        // A track from x=100 to x=300 (width 200) spanning 0..=100.
        assert_eq!(position_to_value(0.0, 100.0, 100.0, 200.0, 100.0), 0.0);
        assert_eq!(position_to_value(0.0, 100.0, 100.0, 200.0, 200.0), 50.0);
        assert_eq!(position_to_value(0.0, 100.0, 100.0, 200.0, 300.0), 100.0);
    }

    #[test]
    fn slider_clamps_outside_the_track_and_survives_a_collapsed_one() {
        assert_eq!(position_to_value(0.0, 100.0, 100.0, 200.0, 0.0), 0.0);
        assert_eq!(position_to_value(0.0, 100.0, 100.0, 200.0, 9999.0), 100.0);
        // A zero-width track must not divide by zero.
        assert_eq!(position_to_value(5.0, 100.0, 0.0, 0.0, 50.0), 5.0);
    }

    #[test]
    fn slider_handles_a_negative_range() {
        // Lyric offset runs from -5000 ms to 5000 ms with zero in the middle.
        assert_eq!(position_to_value(-5000.0, 5000.0, 0.0, 100.0, 50.0), 0.0);
        assert_eq!(position_to_value(-5000.0, 5000.0, 0.0, 100.0, 25.0), -2500.0);
    }

    #[test]
    fn readouts_use_whole_numbers_for_wide_ranges() {
        assert_eq!(format_value(260.0, 140.0, 480.0, "px"), "260px");
        assert_eq!(format_value(420.0, 100.0, 1000.0, ""), "420");
        assert_eq!(format_value(0.8, 0.3, 2.0, ""), "0.8");
    }

    #[test]
    fn caret_steps_over_whole_characters_only() {
        // "a中b": a is 1 byte, 中 is 3 bytes, b is 1 byte.
        let text = "a中b";
        assert_eq!(prev_boundary(text, 0), 0);
        assert_eq!(next_boundary(text, 0), 1);
        // Stepping onto the middle of a multi-byte character must land on its edge instead.
        assert!(text.is_char_boundary(next_boundary(text, 1)));
        assert_eq!(next_boundary(text, 1), 4);
        assert_eq!(next_boundary(text, 4), 5);
        assert_eq!(next_boundary(text, 5), 5);
        assert_eq!(prev_boundary(text, 5), 4);
        assert_eq!(prev_boundary(text, 4), 1);
        assert_eq!(prev_boundary(text, 1), 0);
        // Out-of-range indices are clamped rather than panicking.
        assert_eq!(prev_boundary(text, 99), 4);
        assert_eq!(next_boundary(text, 99), 5);
    }
}

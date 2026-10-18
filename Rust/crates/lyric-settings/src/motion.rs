//! Motion tokens, following the bundled `apple-design` skill.
//!
//! Three ideas run through everything here:
//!
//! * **Springs, not durations.** Anything that can be interrupted is driven by `with_spring`, which
//!   integrates from the value already on screen and carries velocity across a re-target. A
//!   pre-scripted curve cannot be grabbed and reversed; a spring can.
//! * **Damping ratio + response.** Apple replaced mass/stiffness/damping with two parameters a
//!   designer can reason about. [`spring`] converts them back, so the call sites read like the
//!   table in the skill (drawer: `0.82 / 0.36`, default: `1.0 / 0.34`).
//! * **The user's switch decides.** The 界面动画 preference — not the Windows animation setting —
//!   governs motion: `Render` folds it into `reduce_motion`, which every animation element already
//!   respects, and into [`enabled`] for the few places that schedule their own frames.
//!
//! Nothing here holds per-element state; gpui keys that off the `ElementId` passed to
//! `with_spring` / `with_animation`.
use std::cell::Cell;
use std::time::Duration;

use gpui::*;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(true) };
}

/// Set once per frame from the 界面动画 setting, before anything is built.
pub fn set_enabled(on: bool) {
    ENABLED.with(|enabled| enabled.set(on));
}

/// Whether this frame is allowed to move.
pub fn enabled() -> bool {
    ENABLED.with(Cell::get)
}

// ---------------------------------------------------------------------------------------------
// Springs
// ---------------------------------------------------------------------------------------------

/// Damping ratio + response (seconds) → the physical triple gpui integrates.
///
/// `response` is not a duration: it is roughly how long the value takes to reach the target, and
/// the settle time emerges from the two parameters.
pub fn spring(damping_ratio: f32, response: f32) -> SpringConfig {
    let omega = std::f32::consts::TAU / response.max(0.05);
    SpringConfig::new(omega * omega, 2.0 * damping_ratio * omega, 1.0)
}

/// Critically damped, no overshoot — the default for anything that merely appears or moves.
pub fn standard() -> SpringConfig {
    spring(1.0, 0.34)
}

/// The same spring with a shorter response, for small controls: switches, pills, knobs.
pub fn snappy() -> SpringConfig {
    spring(1.0, 0.24)
}

/// A drawer or sheet: a touch of bounce, because it arrives from the edge rather than fading in.
pub fn sheet() -> SpringConfig {
    spring(0.82, 0.36)
}

/// The dimming layer behind a drawer.
///
/// A scrim is not an object arriving — it is the room getting darker. Bounce on it would be
/// meaningless (the alpha is clamped at 1 anyway), and it has to be down *before* the panel
/// finishes travelling, so the content behind is already dimmed when the panel lands. So:
/// critically damped, and quicker than [`sheet`].
pub fn scrim() -> SpringConfig {
    spring(1.0, 0.22)
}

/// A spring that drives a 0..1 phase from `on`, preserving whatever the element is already doing.
pub fn phase(config: SpringConfig, on: bool) -> SpringAnimation<bool> {
    SpringAnimation::new(config).to(on).with_epsilon(0.0015)
}

/// A spring phase that starts at `from` on a fresh mount — the enter half of a transition.
pub fn phase_from(config: SpringConfig, on: bool, from: bool) -> SpringAnimation<bool> {
    SpringAnimation::new(config).to(on).from(from).with_epsilon(0.0015)
}

/// A spring that drives a bare number, for the states that are levels rather than switches.
pub fn level(config: SpringConfig, target: f32) -> SpringAnimation<f32> {
    SpringAnimation::new(config).to(target).with_epsilon(0.004)
}

// ---------------------------------------------------------------------------------------------
// Pointer liveness
// ---------------------------------------------------------------------------------------------
//
// gpui's `.hover()` swaps a style the instant the pointer arrives, and that is the one motion this
// window cannot settle for: an instantaneous fill reads as a flicker. So the view tracks the pointer
// itself — reported through `on_hover`, whose transitions gpui also fires when layout moves under a
// stationary mouse — and every live control springs toward it.
//
// One spring carries all three states, because two springs on one element fight over its frames.
// The target is a *level*: rest, hover, or pressed. Reading it back out gives the two halves.

/// Where the pointer is on a live control: 0 at rest, [`HOVER`] under it, 1 while it is pressed.
pub const HOVER: f32 = 0.55;

pub fn liveness(hovered: bool, pressed: bool) -> f32 {
    if pressed {
        1.0
    } else if hovered {
        HOVER
    } else {
        0.0
    }
}

/// The hover half of a liveness value: 0..1, already saturated by the time a press arrives.
pub fn wash(live: f32) -> f32 {
    (live / HOVER).clamp(0.0, 1.0)
}

/// The press half: zero until the button is actually down, so hovering never sinks a control.
pub fn sink(live: f32) -> f32 {
    ((live - HOVER) / (1.0 - HOVER)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------------------------
// Easing
// ---------------------------------------------------------------------------------------------

/// Evaluate a cubic bézier with the CSS control-point convention.
///
/// x is solved for with a few Newton steps — the same thing a browser does — so the curve matches
/// the one a designer would write in CSS.
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let curve = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    let slope = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    };
    let mut t = x;
    for _ in 0..8 {
        let error = curve(t, x1, x2) - x;
        if error.abs() < 1.0e-4 {
            break;
        }
        let derivative = slope(t, x1, x2);
        if derivative.abs() < 1.0e-4 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    curve(t, y1, y2)
}

/// Decelerate into place: the curve an entering surface uses. Reads as "arriving", not "stopping".
///
/// `cubic-bezier(0.23, 1, 0.32, 1)` — the strong ease-out the skill lists, not the browser's
/// own `ease-out`, which is too weak to read as intentional.
pub fn ease_out(progress: f32) -> f32 {
    bezier(0.23, 1.0, 0.32, 1.0, progress)
}

/// Slow at both ends — for a value that changes in place, such as a colour wash.
pub fn ease_in_out(progress: f32) -> f32 {
    bezier(0.4, 0.0, 0.2, 1.0, progress)
}

/// A one-shot entrance.
///
/// `delay_ms` is baked into the curve rather than scheduled with a timer: the animation simply
/// holds at 0 until its turn, so a page can cascade without owning any per-row bookkeeping.
pub fn enter(duration_ms: u64, delay_ms: u64) -> Animation {
    let total = duration_ms.saturating_add(delay_ms).max(1);
    let start = delay_ms as f32 / total as f32;
    Animation::new(Duration::from_millis(total)).with_easing(move |progress| {
        if progress <= start {
            0.0
        } else {
            ease_out((progress - start) / (1.0 - start).max(0.001))
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Composition helpers
// ---------------------------------------------------------------------------------------------

/// Slide up + fade, the entrance every surface in this window uses.
///
/// The offset is painted (`relative().top()`), never laid out: a cascading list must not reflow the
/// scroll container on every frame. The offset keeps a spring's overshoot — that last millimetre of
/// travel is what makes a landing feel physical — while the fade stays inside 0..1.
pub fn rise<E: Styled>(element: E, phase: f32, distance: f32) -> E {
    element.opacity(phase.clamp(0.0, 1.0)).relative().top(px(distance * (1.0 - phase)))
}

/// Slide in from the right + fade, for a panel that owns the right edge.
pub fn slide_in<E: Styled>(element: E, phase: f32, distance: f32) -> E {
    element.opacity(phase.clamp(0.0, 1.0)).relative().left(px(distance * (1.0 - phase)))
}

/// Interpolate two colours by a plain 0..1 amount.
///
/// The spring-phase version above is for `AnimationPhase` values; pointer liveness is already a
/// number, and clamping here keeps a spring's overshoot from painting a colour that exists nowhere.
pub fn mix(amount: f32, from: gpui::Rgba, to: gpui::Rgba) -> gpui::Rgba {
    AnimationPhase(alpha(AnimationPhase(amount))).interpolate_clamped(from, to)
}

/// A whole page of rows, entering one after another.
///
/// The first row is the page's own furniture — a tab bar or a back link — and keys off
/// `route_epoch` alone, so switching a filter never makes the tab bar blink. Everything below it
/// keys off `content_epoch`, which changes with the content it describes.
///
/// The whole cascade is budgeted, not just one row: a tab switch re-runs it, tabs are hit dozens
/// of times a session, and the skill's ceiling for a UI element is 300 ms. 200 ms per row with the
/// delay capped at four steps lands the last row at 288 ms — still a cascade, no longer a wait.
pub fn cascade(rows: Vec<AnyElement>, route_epoch: u32, content_epoch: u32, step_ms: u64) -> Vec<AnyElement> {
    let head = SharedString::from(format!("row-head-{route_epoch}"));
    let body = SharedString::from(format!("row-body-{content_epoch}"));
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let delay = (index as u64 * step_ms).min(step_ms * 4);
            let id = if index == 0 { (head.clone(), 0usize) } else { (body.clone(), index) };
            div()
                .w_full()
                .min_w_0()
                .with_animation(id, enter(200, delay), |element, progress| {
                    rise(element, progress, 8.0)
                })
                .child(row)
                .into_any_element()
        })
        .collect()
}

/// Clamp a spring's output into a usable alpha/phase range: springs overshoot by design.
pub fn alpha(phase: AnimationPhase) -> f32 {
    phase.0.clamp(0.0, 1.0)
}

/// Interpolate two colours along a spring phase, clamped so an overshoot never goes past the ends.
pub fn blend_color(phase: AnimationPhase, from: gpui::Rgba, to: gpui::Rgba) -> gpui::Rgba {
    AnimationPhase(alpha(phase)).interpolate_clamped(from, to)
}

#[cfg(test)]
mod tests {
    // Named imports on purpose: `use super::*` drags gpui's whole surface into the `#[test]`
    // expansion, which runs the compiler out of recursion budget before the test is even built.
    use super::{alpha, blend_color, ease_in_out, ease_out};
    use gpui::AnimationPhase;

    #[test]
    fn easing_is_monotonic_and_lands_exactly() {
        assert!(ease_out(0.0).abs() < 1e-4);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-3);
        assert!(ease_in_out(0.0).abs() < 1e-4);
        assert!((ease_in_out(1.0) - 1.0).abs() < 1e-3);
        let mut previous = -1.0;
        for step in 0..=20 {
            let value = ease_out(step as f32 / 20.0);
            assert!(value >= previous - 1e-4);
            previous = value;
        }
    }

    #[test]
    fn a_spring_overshoot_never_paints_past_the_two_ends() {
        assert_eq!(alpha(AnimationPhase(1.12)), 1.0);
        assert_eq!(alpha(AnimationPhase(-0.12)), 0.0);
        let from = gpui::rgba(0x000000ff);
        let to = gpui::rgba(0xffffffff);
        assert_eq!(blend_color(AnimationPhase(1.12), from, to), to);
        assert_eq!(blend_color(AnimationPhase(-0.12), from, to), from);
    }
}

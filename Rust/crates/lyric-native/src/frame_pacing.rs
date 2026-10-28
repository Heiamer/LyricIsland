//! Adaptive frame pacing for the island's animation loop.
//!
//! The compositor already synchronises `Present(1)` to the monitor. This module
//! adds the second half: follow the owning monitor refresh rate, and sleep only the remainder of that budget so a
//! 60 Hz panel is not delayed by a second wait.
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, HANDLE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, ENUM_CURRENT_SETTINGS,
    MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};

/// Reject impossible monitor values without limiting high-refresh displays.
pub const MAX_FPS: f64 = 1000.0;

/// Ceiling for redraws that exist only because playback is running.
///
/// The island used to rebuild its whole Direct2D scene once per panel refresh, so a
/// 144 Hz display paid for 144 full redraws a second to move a progress bar by a
/// pixel and three activity bars by a fraction. Nothing in that set changes fast
/// enough to tell 60 from 144. Transitions, fades, drags, banners and hovers still
/// run at the panel rate; only the steady-state playback loop is capped.
pub const CONTINUOUS_FPS: f64 = 60.0;

pub fn continuous_interval() -> Duration {
    Duration::from_secs_f64(1.0 / CONTINUOUS_FPS)
}

/// Read the current mode of the monitor that owns `hwnd`.
pub fn refresh_hz(hwnd: HWND) -> Option<f64> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
            return None;
        }
        let mut mode = DEVMODEW::default();
        mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if !EnumDisplaySettingsW(PCWSTR(info.szDevice.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
            return None;
        }
        let hz = f64::from(mode.dmDisplayFrequency);
        (hz >= 20.0 && hz <= 1000.0).then_some(hz)
    }
}

/// Cap the display rate at [`MAX_FPS`]. An unknown display falls back to 60 Hz,
/// the conservative value `DwmFlush` itself assumes.
pub fn target_hz(refresh: Option<f64>) -> f64 {
    refresh.filter(|hz| hz.is_finite() && *hz >= 20.0).map_or(60.0, |hz| hz.min(MAX_FPS))
}

pub fn target_interval(refresh: Option<f64>) -> Duration {
    Duration::from_secs_f64(1.0 / target_hz(refresh))
}

/// A pacing clock shared by every animated frame. `wait` is called before
/// `tick`; the render inside `tick` blocks in `Present(1)`, so the elapsed time
/// it reports already contains the compositor's wait.
pub struct FramePacer {
    interval: Duration,
    timer: Option<HANDLE>,
    last: Instant,
    hwnd:HWND,
    checked:Instant,
}

impl FramePacer {
    pub fn new(hwnd: HWND) -> Self {
        let interval = target_interval(refresh_hz(hwnd));
        let timer = unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
                .ok()
        };
        Self { interval, timer, last: Instant::now(),hwnd,checked:Instant::now() }
    }

    /// Sleep only what is left of this frame's budget. The first frame after
    /// an idle period is never delayed.
    pub fn wait(&mut self) {
        self.wait_interval(self.interval);
    }

    /// Same budget, but against a caller-chosen interval.
    ///
    /// Used for the steady playback loop, which does not need the panel rate; the
    /// monitor is still re-read once a second so plugging in a different display
    /// restores the full rate for transitions.
    pub fn wait_interval(&mut self, interval: Duration) {
        if self.checked.elapsed()>=Duration::from_secs(1){self.interval=target_interval(refresh_hz(self.hwnd));self.checked=Instant::now();}
        let remaining = interval.saturating_sub(self.last.elapsed());
        if !remaining.is_zero() {
            self.sleep(remaining);
        }
        self.last = Instant::now();
    }

    fn sleep(&self, duration: Duration) {
        let Some(timer) = self.timer else {
            std::thread::sleep(duration);
            return;
        };
        let due = -((duration.as_nanos() as i64 / 100).max(1));
        let success = unsafe {
            SetWaitableTimer(timer, &due, 0, None, None, false).is_ok()
                && WaitForSingleObject(timer, INFINITE).0 == 0
        };
        if !success {
            std::thread::sleep(duration);
        }
    }
}

impl Drop for FramePacer {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            unsafe { let _ = CloseHandle(timer); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slower_displays_keep_their_rate() {
        assert_eq!(target_hz(Some(60.0)), 60.0);
        assert_eq!(target_hz(Some(48.0)), 48.0);
    }

    #[test]
    fn high_refresh_displays_keep_their_rate() {
        assert_eq!(target_hz(Some(144.0)), 144.0);
        assert_eq!(target_hz(Some(240.0)), 240.0);
    }

    #[test]
    fn unknown_or_invalid_rates_fall_back_to_sixty() {
        assert_eq!(target_hz(None), 60.0);
        assert_eq!(target_hz(Some(0.0)), 60.0);
        assert_eq!(target_hz(Some(f64::NAN)), 60.0);
    }

    #[test]
    fn intervals_match_the_selected_rate() {
        assert!((target_interval(Some(60.0)).as_secs_f64() - 1.0 / 60.0).abs() < 1e-9);
        assert!((target_interval(Some(144.0)).as_secs_f64() - 1.0 / 144.0).abs() < 1e-9);
    }
}

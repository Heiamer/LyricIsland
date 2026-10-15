//! A display clock sampled on every rendered frame. Small source corrections
//! are blended while time keeps advancing, rather than stopping at each poll.
#[derive(Default)]
pub struct PlaybackProgress {
    anchor: f64,
    correction: f64,
    sampled_at: f64,
    duration: f64,
    running: bool,
    track: String,
}
impl PlaybackProgress {
    pub fn seconds(&self, now: f64) -> f64 {
        let age = (now - self.sampled_at).max(0.);
        let advance = if self.running { age.min(0.5) } else { 0. };
        let correction = self.correction * (1. - age / 0.15).clamp(0., 1.);
        let limit=if self.duration>0. {self.duration} else {86400.};
        (self.anchor + advance + correction).clamp(0., limit)
    }
    pub fn fraction(&self, now: f64) -> f64 {
        if self.duration > 0. { self.seconds(now) / self.duration } else { 0. }
    }
    pub fn update(&mut self, now: f64, track: &str, position: f64, duration: f64, playing: bool, animate: bool) {
        let duration = if duration.is_finite() { duration.max(0.) } else { 0. };
        let limit=if duration>0. {duration} else {86400.};
        let position = if position.is_finite() { position.clamp(0., limit) } else { 0. };
        let current = self.seconds(now);
        let delta = position - current;
        // Ordinary polling jitter stays continuous. Seeks, track changes and
        // pause confirmations take effect immediately; never animate backwards.
        let continuous = self.track == track && self.running && playing && animate
            && (self.duration - duration).abs() < 0.001 && (-0.12..=0.75).contains(&delta);
        self.correction = if continuous { current - position } else { 0. };
        self.anchor = position;
        self.sampled_at = now;
        self.duration = duration;
        self.running = playing && animate;
        self.track = track.into();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advances_on_every_frame_between_source_updates() {
        let mut p=PlaybackProgress::default();
        p.update(0., "song", 30., 180., true, true);
        for i in 1..14 { assert!(p.seconds(i as f64/165.) > p.seconds((i-1)as f64/165.)); }
        let before=p.seconds(0.09);
        p.update(0.09,"song",30.08,180.,true,true);
        assert!((p.seconds(0.09)-before).abs()<1e-10);
        assert!(p.seconds(0.096)>before);
    }
    #[test]
    fn pause_seek_and_track_change_land_on_confirmed_position() {
        let mut p=PlaybackProgress::default();
        p.update(0.,"a",30.,180.,true,true);
        p.update(0.1,"a",30.1,180.,false,true);
        assert_eq!(p.seconds(0.4),30.1);
        p.update(0.4,"a",10.,180.,true,true);
        assert_eq!(p.seconds(0.4),10.);
        p.update(0.5,"b",0.,240.,true,true);
        assert_eq!(p.fraction(0.5),0.);
    }
    #[test]
    fn does_not_run_indefinitely_when_source_stops_reporting() {
        let mut p=PlaybackProgress::default();
        p.update(0.,"a",179.9,180.,true,true);
        assert_eq!(p.fraction(10.),1.);
        p.update(0.,"a",10.,180.,true,false);
        assert_eq!(p.seconds(0.3),10.);
    }
    #[test]
    fn unknown_duration_keeps_elapsed_time_without_inventing_percentage() {
        let mut p=PlaybackProgress::default();
        p.update(0.,"a",30.,0.,true,true);
        assert_eq!(p.seconds(0.1),30.1);
        assert_eq!(p.fraction(0.1),0.);
    }
}

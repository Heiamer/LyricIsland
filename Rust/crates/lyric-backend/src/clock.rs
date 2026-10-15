//! Playback clock: monotonic interpolation, repeated SMTC timestamps, pause and calibration.
use lyric_core::{models::PlaybackSnapshot,sync::LyricSyncService};
#[derive(Default)]pub struct PlaybackClock{identity:String,anchor:f64,seconds:f64,playing:bool,last_raw:Option<f64>,last_update:Option<i64>,manual:bool,pub position:f64,pub local:bool,pub status:String}
impl PlaybackClock{
    fn estimate(&self,now:f64)->f64{self.seconds+if self.playing{(now-self.anchor).max(0.)}else{0.}}
    fn anchor(&mut self,seconds:f64,playing:bool,now:f64){self.seconds=if seconds.is_finite(){seconds.max(0.)}else{0.};self.anchor=now;self.playing=playing}
    pub fn observe(&mut self,p:&PlaybackSnapshot,updated:i64,unix:i64,now:f64,native:bool)->f64{
        let Some(t)=p.track.as_ref()else{*self=Self::default();self.status="等待播放器".into();return 0.};
        let identity=LyricSyncService::build_stable_track_identity(t);let mut raw=p.raw_position.unwrap_or_default().as_secs_f64();let duration=t.duration.as_secs_f64();
        if !raw.is_finite()||raw<0.||raw>=86400.||duration>0.&&raw>duration+5.{raw=0.}
        let timestamp=updated>0&&updated<=unix+5000;let selected=p.position.as_secs_f64();
        let valid=selected.is_finite()&&selected>=0.&&selected<86400.&&(duration<=0.||selected<=duration+5.);
        let system=timestamp&&valid&&(duration>0.||raw>0.);
        if identity!=self.identity{self.identity=identity;self.manual=false;self.last_raw=None;self.last_update=None;self.anchor(if system{selected}else{raw},p.is_playing,now)}
        if system{
            let repeated=self.last_update==Some(updated)&&self.last_raw.is_some_and(|r|(r-raw).abs()<0.05);
            if !repeated||self.manual{self.anchor(selected,p.is_playing,now)}else if self.playing!=p.is_playing{self.anchor(self.estimate(now),p.is_playing,now)}
            self.manual=false;self.position=self.estimate(now);self.local=false;self.status=if native{"网易云实时进度"}else{"系统时间轴"}.into();self.last_update=Some(updated);
        }else{
            let changed=raw>0.&&self.last_raw.is_none_or(|r|(r-raw).abs()>0.05);let estimate=if !self.manual&&changed{raw}else{self.estimate(now)};
            if self.playing!=p.is_playing||!self.manual&&changed{self.anchor(estimate,p.is_playing,now)}self.position=self.estimate(now);self.local=true;
            self.status=if self.manual{"本地计时 · 已校准"}else{"本地计时 · 播放器未提供有效进度"}.into();
        }
        self.last_raw=Some(raw);if duration>0.{self.position=self.position.min(duration)}self.position.max(0.)
    }
    pub fn synchronize(&mut self,seconds:f64,now:f64){if !self.identity.is_empty()&&seconds.is_finite()&&(0.0..=86400.0).contains(&seconds){self.manual=true;self.anchor(seconds,self.playing,now);self.position=seconds;self.local=true;self.status="本地计时 · 已校准".into()}}
    pub fn position(&self,now:f64)->f64{self.estimate(now).max(0.)}
}
#[cfg(test)]mod tests{use super::*;use lyric_core::models::TrackInfo;use std::time::Duration;#[test]fn repeated_paused_timestamp_does_not_count_pause_time(){let mut c=PlaybackClock::default();let mut p=PlaybackSnapshot::playing(TrackInfo::new("t","t","a","","Netease",Duration::from_secs(180)),Duration::from_secs(5));p.raw_position=Some(Duration::from_secs(5));assert_eq!(c.observe(&p,1000,1000,0.,false),5.);p.is_playing=false;assert_eq!(c.observe(&p,1000,2000,1.,false),6.);assert_eq!(c.observe(&p,1000,10000,9.,false),6.);p.is_playing=true;assert_eq!(c.observe(&p,1000,11000,10.,false),6.);assert_eq!(c.observe(&p,1000,12000,11.,false),7.);}}


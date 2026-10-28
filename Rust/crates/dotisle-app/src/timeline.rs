//! Bounded presentation of the real media position, with explicit seek acknowledgement.
use crate::model::Model;
#[derive(Default)]pub struct Timeline{pub position:f64,track:(String,String,String),sample:f64,age:f64,ready:bool,playing:bool,pending:Option<(f64,f64)>}
impl Timeline{
 pub fn seek(&mut self,position:f64){if position.is_finite(){self.position=position.max(0.);self.pending=Some((self.position,0.));self.age=0.;}}
 pub fn tick(&mut self,m:&Model,dt:f64)->bool{
  let before=self.position;let dt=if dt.is_finite(){dt.clamp(0.,0.1)}else{0.};
  if m.native_error.is_some(){self.age=0.;return false}
  let t=&m.snapshot["track"];let Some(sample)=m.snapshot["positionSeconds"].as_f64().filter(|v|v.is_finite()&&*v>=0.)else{self.age=0.;return false};
  let duration=t["durationSeconds"].as_f64().filter(|v|v.is_finite()&&*v>0.);
  let track=(t["id"].as_str().unwrap_or("").to_owned(),t["title"].as_str().unwrap_or("").to_owned(),t["artist"].as_str().unwrap_or("").to_owned());let playing=m.playing();
  if !self.ready||self.track!=track{self.position=sample;self.sample=sample;self.age=0.;self.track=track;self.ready=true;self.pending=None;}
  if let Some((position,age))=self.pending{let command=&m.snapshot["mediaCommand"];let matches=command["action"]=="seek"&&command["targetSeconds"].as_f64().is_some_and(|v|(v-position).abs()<0.1);let failed=matches&&command["state"]=="failed";let waiting=matches&&(command["state"]=="queued"||command["state"]=="pending");if failed||(sample-position).abs()<0.8||age>=if waiting{5.}else{1.2}{self.pending=None;self.sample=sample;self.age=0.;}else{self.pending=Some((position,age+dt));self.playing=playing;return (before-self.position).abs()>0.001}}
  if (sample-self.sample).abs()>0.0001||playing!=self.playing{self.sample=sample;self.age=0.;}else if playing{self.age=(self.age+dt).min(0.35);}else{self.age=0.;}
  let target=(sample+if playing{self.age}else{0.}).clamp(0.,duration.unwrap_or(86400.));let error=target-self.position;
  if m.prefs.reduced_motion||!playing||error.abs()>1.5{self.position=sample;}else{self.position+=error*(1.-(-dt*20.).exp());}
  self.playing=playing;
  playing&&!m.prefs.reduced_motion||(self.position-before).abs()>0.001
 }
}
#[cfg(test)]mod tests{use super::*;use serde_json::json;
 #[test]fn invalid_sample_and_pause_resume_do_not_jump(){let mut m=Model::new(std::path::PathBuf::new(),true);m.snapshot=json!({"track":{"title":"A","durationSeconds":100.},"positionSeconds":10.,"isPlaying":true});let mut t=Timeline::default();t.tick(&m,0.016);for _ in 0..60{t.tick(&m,0.016);}let before=t.position;m.snapshot.as_object_mut().unwrap().remove("positionSeconds");t.tick(&m,0.016);assert_eq!(t.position,before);m.snapshot["positionSeconds"]=json!(10.);m.snapshot["isPlaying"]=json!(false);t.tick(&m,0.016);m.snapshot["isPlaying"]=json!(true);t.tick(&m,0.016);assert!(t.position<10.02);t.tick(&m,f64::NAN);assert!(t.position.is_finite());}
 #[test]fn seek_waits_for_ack_and_paused_track_change_redraws(){let mut m=Model::new(std::path::PathBuf::new(),true);m.snapshot=json!({"track":{"title":"A","durationSeconds":100.},"positionSeconds":10.,"isPlaying":false});let mut t=Timeline::default();t.tick(&m,0.);t.seek(80.);for _ in 0..30{t.tick(&m,0.016);assert_eq!(t.position,80.);}m.snapshot["positionSeconds"]=json!(80.);t.tick(&m,0.016);assert!(t.pending.is_none());m.snapshot["track"]["title"]=json!("B");m.snapshot["positionSeconds"]=json!(2.);assert!(t.tick(&m,0.016));assert_eq!(t.position,2.);}
 #[test]fn presentation_is_bounded_and_seeks_and_track_changes_reset(){let mut m=Model::new(std::path::PathBuf::new(),true);m.snapshot=json!({"track":{"title":"A","artist":"B","durationSeconds":100.},"positionSeconds":10.,"isPlaying":true});let mut t=Timeline::default();t.tick(&m,0.016);for _ in 0..1000{t.tick(&m,0.016);}assert!(t.position<=10.35+0.001);m.snapshot["positionSeconds"]=json!(70.);t.tick(&m,0.016);assert_eq!(t.position,70.);m.snapshot["track"]["title"]=json!("C");m.snapshot["positionSeconds"]=json!(2.);t.tick(&m,0.016);assert!(t.position<2.02);m.prefs.reduced_motion=true;m.snapshot["positionSeconds"]=json!(3.);t.tick(&m,0.016);assert_eq!(t.position,3.);}
 #[test]fn failed_seek_reverts_preview_and_pending_seek_waits_for_real_position(){let mut m=Model::new(std::path::PathBuf::new(),true);m.snapshot=json!({"track":{"id":"a","title":"A","durationSeconds":100.},"positionSeconds":10.,"isPlaying":false});let mut t=Timeline::default();t.tick(&m,0.);t.seek(80.);m.snapshot["mediaCommand"]=json!({"action":"seek","targetSeconds":80.,"state":"pending"});for _ in 0..120{t.tick(&m,0.016);}assert_eq!(t.position,80.);m.snapshot["mediaCommand"]["state"]=json!("failed");t.tick(&m,0.016);assert_eq!(t.position,10.);assert!(t.pending.is_none());}

}

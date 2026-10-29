//! Compact lyric policy and an interruptible, critically damped resize spring.
use serde_json::Value;
/// Geometry follows settled content; text can continue to update on every snapshot.
/// Growth needs a short confirmation, while contraction must outlast a dropped frame,
/// late translation or a short-lived rewind at a lyric boundary.
#[derive(Default)]
pub struct Stability{candidate:Option<(String,f64,f64)>,since:f64}
impl Stability{
 pub fn cancel(&mut self){self.candidate=None;}
 #[cfg(test)]
 pub fn ready(&mut self,key:&str,size:(f64,f64),current:(f64,f64),now:f64,force:bool)->bool{
  self.ready_within(key,size,current,now,force,0.30,0.08)
 }
 /// shrink and grow are the confirmation windows.
 ///
 /// The caller stretches shrink while a translation is still on its way, because a
 /// width taken away now has to be given back the moment that translation lands —
 /// which is exactly the one-pump the capsule must never show. grow is short but
 /// non-zero: it filters a single bad snapshot (a seek rewind flashing a long line)
 /// without making the new line wait.
 pub fn ready_within(&mut self,key:&str,size:(f64,f64),current:(f64,f64),now:f64,force:bool,shrink:f64,grow:f64)->bool{
  if force||((size.0-current.0).abs()<0.05&&(size.1-current.1).abs()<0.05){self.candidate=None;return true}
  if !self.candidate.as_ref().is_some_and(|(seen,w,h)|seen==key&&(*w-size.0).abs()<0.05&&(*h-size.1).abs()<0.05){self.candidate=Some((key.into(),size.0,size.1));self.since=now;}
  let shrinking=size.0<current.0-0.05||size.1<current.1-0.05;
  now-self.since>=if shrinking{shrink}else{grow}
 }
}
pub fn mini_font_size(font:f64)->f64{(font*0.80).clamp(9.,11.)}

pub fn secondary<'a>(snapshot:&'a Value,translation:bool,chinese_next:bool)->&'a str{
 let f=&snapshot["frame"];let original=f["currentLine"].as_str().unwrap_or("");
 if translation{if let Some(t)=f["currentTranslation"].as_str().filter(|t|!t.trim().is_empty()&&t.trim()!=original.trim()){return t}}
 if chinese_next&&original.chars().any(|c|matches!(c as u32,0x3400..=0x9fff|0x20000..=0x2fa1f)){return f["nextLine"].as_str().unwrap_or("")} ""
}
/// One interruptible spring per axis. Width alone was not enough: when the second
/// line arrived the capsule snapped taller in one frame while the width glided,
/// so the lyric jumped under a surface that had not finished opening. Both axes
/// now start from the live value and keep their velocity, exactly like the island
/// morph, and the vertical channel runs slightly ahead so there is room for the
/// new line before it becomes readable.
#[derive(Clone,Copy,Debug)]
pub struct Resize{scheme:lyric_core::shared_motion::Scheme,pub width:f64,pub height:f64,pub width_target:f64,pub height_target:f64,width_velocity:f64,height_velocity:f64}
impl Resize{
 pub fn new(width:f64,height:f64)->Self{Self{scheme:lyric_core::shared_motion::Scheme::Expressive,width,height,width_target:width,height_target:height,width_velocity:0.,height_velocity:0.}}
 /// Set both targets. With animation off they land immediately, which is what
 /// 「界面动画」关闭 promises; otherwise the next tick starts from the live frame.
 pub fn target(&mut self,width:f64,height:f64,animate:bool){self.width_target=width.clamp(140.,480.);self.height_target=height.clamp(32.,240.);if !animate{self.width=self.width_target;self.height=self.height_target;self.width_velocity=0.;self.height_velocity=0.;}}
 /// Whether either axis still has somewhere to go. At rest both values are
 /// snapped exactly onto their targets, so the redraw loop can stop asking for
 /// the motion settings on every idle frame.
 pub fn target_mini(&mut self,width:f64,height:f64,animate:bool){self.target(width,32.,animate);self.height_target=height.clamp(14.,24.);if !animate{self.height=self.height_target;}}
 pub fn moving(&self)->bool{self.width!=self.width_target||self.height!=self.height_target}
 /// Advance both axes and report whether anything visible changed.
 pub fn configure(&mut self,scheme:lyric_core::shared_motion::Scheme){self.scheme=scheme;}
 pub fn tick(&mut self,dt:f64,pace:f64)->bool{
  use lyric_core::shared_motion::{Spec,Scheme,Kind,Speed};let pace=if pace>10.{1.}else{pace};let spec=Spec::token(self.scheme,Kind::Spatial,Speed::Default).paced(pace);
  let width=advance(&mut self.width,&mut self.width_velocity,self.width_target,spec,dt);let height=advance(&mut self.height,&mut self.height_velocity,self.height_target,spec.paced(1.15),dt);width||height
 }
}
fn advance(value:&mut f64,velocity:&mut f64,target:f64,spec:lyric_core::shared_motion::Spec,dt:f64)->bool{
 if (*value-target).abs()<0.05&&velocity.abs()<0.5{let changed=*value!=target;*value=target;*velocity=0.;return changed}
 let previous=*value;let(next,v)=spec.sample(previous,target,*velocity,dt);*value=next.clamp(previous.min(target),previous.max(target));*velocity=if *value!=next{0.}else{v};true
}
/// Collapsed-capsule line metrics, rounded to the physical pixel grid.
///
/// The shell sizes the capsule with this and the renderer places the two lines
/// with it, so a capsule can never be sized differently from the way it is drawn.
pub fn group_metrics(font_size:f64,line_spacing:f64,scale:f64,double:bool)->(f64,f64,f64){
 let round=|v:f64|(v*scale).round_ties_even()/scale;
 let line_h=round(font_size*line_spacing);let secondary_size=(font_size*0.78).clamp(9.,11.5);let secondary_h=round(secondary_size*line_spacing);
 let group_h=if double{line_h+secondary_h+1.}else{line_h};(group_h,line_h,secondary_h)
}
/// Capsule height: the group plus symmetric padding, never below the 1.2 height.
pub fn capsule_height(font_size:f64,line_spacing:f64,scale:f64,double:bool)->f64{(group_metrics(font_size,line_spacing,scale,double).0+6.).max(40.)}
#[cfg(test)]mod tests{use super::*;use serde_json::json;
 #[test]fn translation_wins_and_empty_translation_can_fall_back_to_next(){let s=json!({"frame":{"currentLine":"You","currentTranslation":"你","nextLine":"Next"}});assert_eq!(secondary(&s,true,true),"你");assert_eq!(secondary(&s,false,true),"");let s=json!({"frame":{"currentLine":"晚风轻轻","currentTranslation":"","nextLine":"音乐刚好"}});assert_eq!(secondary(&s,true,false),"");assert_eq!(secondary(&s,true,true),"音乐刚好");}
 #[test]fn resize_animates_both_axes_and_reverses_without_a_jump(){let mut r=Resize::new(260.,40.);r.target(480.,51.,true);for _ in 0..8{r.tick(1./60.,360.);}let(w,h)=(r.width,r.height);assert!(w>260.&&w<480.);assert!(h>40.&&h<51.);r.target(140.,40.,true);assert_eq!((r.width,r.height),(w,h));for _ in 0..240{r.tick(1./60.,360.);}assert!((r.width-140.).abs()<0.05);assert!((r.height-40.).abs()<0.05);}
 #[test]fn resize_targets_are_clamped_and_animation_off_snaps(){let mut r=Resize::new(260.,40.);r.target(1000.,300.,false);assert_eq!((r.width,r.height),(480.,240.));r.target(10.,10.,false);assert_eq!((r.width,r.height),(140.,32.));r.target(300.,44.,true);assert_eq!((r.width,r.height),(140.,32.));assert!(r.tick(1./60.,360.));}
 #[test]fn resize_height_leads_the_width(){let mut r=Resize::new(260.,40.);r.target(480.,60.,true);for _ in 0..12{r.tick(1./60.,360.);}let width=(r.width-260.)/220.;let height=(r.height-40.)/20.;assert!(height>width,"height {height} should lead width {width}");}
 #[test]fn resize_settles_inside_the_longest_lyric_pause(){let mut r=Resize::new(140.,40.);r.target(460.,51.,true);let mut elapsed=0.;while elapsed<1.2&&((r.width-r.width_target).abs()>=0.05||(r.height-r.height_target).abs()>=0.05){r.tick(1./120.,1200.);elapsed+=1./120.;}assert!(elapsed<0.95,"resize still moving after {elapsed}s");}
 #[test]fn capsule_grows_only_when_two_lines_need_the_room(){assert_eq!(capsule_height(12.5,1.448,1.,false),40.);assert_eq!(capsule_height(12.5,1.448,1.,true),40.);assert_eq!(capsule_height(17.,1.448,1.,false),40.);assert!(capsule_height(17.,1.448,1.,true)>=45.);assert!((capsule_height(17.,1.448,2.,true)-capsule_height(17.,1.448,1.,true)).abs()<=1.);}
}

#[cfg(test)]mod mini_resize_tests{use super::*;#[test]fn mini_resize_can_settle_at_half_height_without_changing_classic_minimum(){let mut r=Resize::new(260.,40.);r.target_mini(260.,20.,true);for _ in 0..240{r.tick(1./120.,480.);}assert_eq!(r.height,20.);r.target(260.,10.,false);assert_eq!(r.height,32.);r.target_mini(260.,20.,false);assert_eq!(r.height,20.);}}

#[cfg(test)]mod stability_tests{
 use super::*;
 #[test]fn alternating_short_and_long_snapshots_never_repeatedly_contract(){
  let mut gate=Stability::default();let current=(420.,40.);let mut commits=0;
  for i in 0..600{let now=i as f64/120.;let short=(now/0.25)as usize%2==0;let size=if short{(160.,40.)}else{current};
   if gate.ready(if short{"short"}else{"long"},size,current,now,false)&&size!=current{commits+=1;}
  }assert_eq!(commits,0);
  assert!(!gate.ready("real-short",(160.,40.),current,6.,false));
  assert!(gate.ready("real-short",(160.,40.),current,6.61,false));
 }
 #[test]fn growth_is_prompt_and_contraction_waits_for_a_settled_line(){
  let mut gate=Stability::default();let current=(200.,40.);
  assert!(!gate.ready("long",(420.,40.),current,0.,false));assert!(gate.ready("long",(420.,40.),current,0.09,false));
  assert!(!gate.ready("translation-missing",(240.,40.),(420.,40.),0.10,false));
  assert!(!gate.ready("translation-missing",(240.,40.),(420.,40.),0.35,false));
  assert!(gate.ready("translation-missing",(240.,40.),(420.,40.),0.41,false));
  assert!(gate.ready("long",(420.,40.),(420.,40.),0.45,false));
 }
 #[test]fn manual_changes_and_initial_layout_do_not_wait_for_lyrics(){
  let mut gate=Stability::default();assert!(gate.ready("manual",(260.,14.),(420.,40.),0.,true));
 }
 #[test]fn reversing_a_resize_never_moves_away_from_the_new_target(){
  let mut r=Resize::new(140.,40.);r.target(460.,40.,true);for _ in 0..8{r.tick(1./120.,480.);}
  r.target(140.,40.,true);let mut last=r.width;for _ in 0..240{r.tick(1./120.,480.);assert!(r.width<=last&&r.width>=140.);last=r.width;}assert_eq!(r.width,140.);
  r.target(460.,40.,true);for _ in 0..240{let before=r.width;r.tick(1./120.,480.);assert!(r.width>=before&&r.width<=460.);}assert_eq!(r.width,460.);
 }
}

#[cfg(test)]mod responsive_tests{use super::*;
 #[test]fn contraction_is_smooth_monotonic_and_nearly_settled_in_300ms(){let mut r=Resize::new(460.,40.);r.target(180.,40.,true);let mut previous=r.width;for _ in 0..36{r.tick(1./120.,480.);assert!(r.width<=previous&&r.width>=180.);previous=r.width;}assert!((r.width-180.).abs()<4.,"{}",r.width);}
 #[test]fn stable_short_lyrics_confirm_promptly_but_single_bad_samples_do_not(){let mut s=Stability::default();assert!(!s.ready_within("short",(180.,40.),(460.,40.),0.,false,0.10,0.04));assert!(!s.ready_within("short",(180.,40.),(460.,40.),0.05,false,0.10,0.04));assert!(s.ready_within("short",(180.,40.),(460.,40.),0.11,false,0.10,0.04));}
}

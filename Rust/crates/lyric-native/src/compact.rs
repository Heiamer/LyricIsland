//! Compact lyric policy and an interruptible, critically damped resize spring.
use serde_json::Value;
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
pub struct Resize{pub width:f64,pub height:f64,pub width_target:f64,pub height_target:f64,width_velocity:f64,height_velocity:f64}
impl Resize{
 pub fn new(width:f64,height:f64)->Self{Self{width,height,width_target:width,height_target:height,width_velocity:0.,height_velocity:0.}}
 /// Set both targets. With animation off they land immediately, which is what
 /// 「界面动画」关闭 promises; otherwise the next tick starts from the live frame.
 pub fn target(&mut self,width:f64,height:f64,animate:bool){self.width_target=width.clamp(140.,480.);self.height_target=height.clamp(32.,240.);if !animate{self.width=self.width_target;self.height=self.height_target;self.width_velocity=0.;self.height_velocity=0.;}}
 /// Whether either axis still has somewhere to go. At rest both values are
 /// snapped exactly onto their targets, so the redraw loop can stop asking for
 /// the motion settings on every idle frame.
 pub fn moving(&self)->bool{self.width!=self.width_target||self.height!=self.height_target}
 /// Advance both axes and report whether anything visible changed.
 pub fn tick(&mut self,dt:f64,expand_ms:f64)->bool{let dt=dt.clamp(0.,0.1);let omega=resize_omega(expand_ms);
  // Growth keeps up with the line that just became readable; contraction can
  // take the long way round because the old line is already fading out.
  let grow=if self.width_target>self.width{1.25}else{1.};
  let width=advance(&mut self.width,&mut self.width_velocity,self.width_target,omega*grow,dt);
  let height=advance(&mut self.height,&mut self.height_velocity,self.height_target,omega*1.4,dt);
  width||height}
}
fn advance(value:&mut f64,velocity:&mut f64,target:f64,omega:f64,dt:f64)->bool{
 if (*value-target).abs()<0.05&&velocity.abs()<0.5{let changed=*value!=target;*value=target;*velocity=0.;return changed}
 let d=*value-target;let b=*velocity+omega*d;let e=(-omega*dt).exp();*value=target+(d+b*dt)*e;*velocity=(*velocity-omega*b*dt)*e;true
}
/// Natural frequency for the capsule resize.
///
/// It follows 「展开响应」so the capsule and the island speak the same pace, but a
/// lyric line only lives for a few seconds: past ~420 ms the capsule would still
/// be catching up when the next line lands. The cap keeps the resize inside a
/// lyric-length budget while the 0.9 factor makes it read as breathing rather than
/// twitching, without losing the tail that made the old fixed 22 rad/s feel abrupt.
pub fn resize_omega(expand_ms:f64)->f64{let settle=(expand_ms.clamp(250.,1200.)*0.9).clamp(240.,420.)/1000.;5.83/settle}
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
 #[test]fn resize_height_leads_the_width(){assert!(resize_omega(250.)>resize_omega(1200.));let mut r=Resize::new(260.,40.);r.target(480.,60.,true);for _ in 0..12{r.tick(1./60.,360.);}let width=(r.width-260.)/220.;let height=(r.height-40.)/20.;assert!(height>width,"height {height} should lead width {width}");}
 #[test]fn resize_settles_inside_the_longest_lyric_pause(){let mut r=Resize::new(140.,40.);r.target(460.,51.,true);let mut elapsed=0.;while elapsed<1.2&&((r.width-r.width_target).abs()>=0.05||(r.height-r.height_target).abs()>=0.05){r.tick(1./120.,1200.);elapsed+=1./120.;}assert!(elapsed<0.95,"resize still moving after {elapsed}s");}
 #[test]fn capsule_grows_only_when_two_lines_need_the_room(){assert_eq!(capsule_height(12.5,1.448,1.,false),40.);assert_eq!(capsule_height(12.5,1.448,1.,true),40.);assert_eq!(capsule_height(17.,1.448,1.,false),40.);assert!(capsule_height(17.,1.448,1.,true)>=45.);assert!((capsule_height(17.,1.448,2.,true)-capsule_height(17.,1.448,1.,true)).abs()<=1.);}
}

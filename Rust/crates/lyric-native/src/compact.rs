//! Compact lyric policy and an interruptible, critically damped width spring.
use serde_json::Value;
pub fn secondary<'a>(snapshot:&'a Value,translation:bool,chinese_next:bool)->&'a str{
 let f=&snapshot["frame"];let original=f["currentLine"].as_str().unwrap_or("");
 if translation{if let Some(t)=f["currentTranslation"].as_str().filter(|t|!t.trim().is_empty()&&t.trim()!=original.trim()){return t}}
 if chinese_next&&original.chars().any(|c|matches!(c as u32,0x3400..=0x9fff|0x20000..=0x2fa1f)){return f["nextLine"].as_str().unwrap_or("")} ""
}
#[derive(Clone,Copy,Debug)]pub struct Width{pub value:f64,pub velocity:f64,pub target:f64}
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
impl Width{
 pub fn new(value:f64)->Self{Self{value,velocity:0.,target:value}}
 pub fn target(&mut self,value:f64,animate:bool){self.target=value.clamp(140.,480.);if !animate{self.value=self.target;self.velocity=0.;}}
 pub fn tick(&mut self,dt:f64)->bool{if (self.value-self.target).abs()<0.02&&self.velocity.abs()<0.1{let changed=self.value!=self.target;self.value=self.target;self.velocity=0.;return changed}let dt=dt.clamp(0.,0.1);let omega=22.;let d=self.value-self.target;let b=self.velocity+omega*d;let e=(-omega*dt).exp();self.value=self.target+(d+b*dt)*e;self.velocity=(self.velocity-omega*b*dt)*e;true}
}
#[cfg(test)]mod tests{use super::*;use serde_json::json;
 #[test]fn translation_wins_and_empty_translation_can_fall_back_to_next(){let s=json!({"frame":{"currentLine":"You","currentTranslation":"你","nextLine":"Next"}});assert_eq!(secondary(&s,true,true),"你");assert_eq!(secondary(&s,false,true),"");let s=json!({"frame":{"currentLine":"晚风轻轻","currentTranslation":"","nextLine":"音乐刚好"}});assert_eq!(secondary(&s,true,false),"");assert_eq!(secondary(&s,true,true),"音乐刚好");}
 #[test]fn width_can_reverse_without_a_jump_and_settles(){let mut w=Width::new(260.);w.target(480.,true);for _ in 0..8{w.tick(1./60.);}let before=w.value;w.target(140.,true);assert_eq!(w.value,before);for _ in 0..180{w.tick(1./60.);}assert!((w.value-140.).abs()<0.03);w.target(400.,false);assert_eq!(w.value,400.);}
 #[test]fn target_is_clamped_and_animation_off_snaps(){let mut w=Width::new(260.);w.target(1000.,false);assert_eq!(w.value,480.);w.target(10.,false);assert_eq!(w.value,140.);w.target(300.,true);assert_eq!(w.value,140.);assert!(w.tick(1./60.));}
 #[test]fn capsule_grows_only_when_two_lines_need_the_room(){assert_eq!(capsule_height(12.5,1.448,1.,false),40.);assert_eq!(capsule_height(12.5,1.448,1.,true),40.);assert_eq!(capsule_height(17.,1.448,1.,false),40.);assert!(capsule_height(17.,1.448,1.,true)>=45.);assert!((capsule_height(17.,1.448,2.,true)-capsule_height(17.,1.448,1.,true)).abs()<=1.);}
}

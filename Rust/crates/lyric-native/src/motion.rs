//! Shared Material motion. Every retarget carries live position and velocity.
use serde::{Deserialize,Serialize};
use lyric_core::shared_motion::{Scheme,Kind,Speed,Spec};
#[derive(Clone,Copy,Debug,Deserialize,Serialize)]#[serde(default,rename_all="PascalCase")]
pub struct Spring{pub stiffness:f64,pub damping:f64,pub mass:f64}
impl Default for Spring{fn default()->Self{Self{stiffness:380.,damping:0.8,mass:1.}}}
#[derive(Clone,Copy,Debug,Deserialize,Serialize)]#[serde(default,rename_all="PascalCase")]
pub struct Options{pub gap:f64,pub duration:f64,pub collapse_duration:f64,pub radius:f64,pub spring:Spring,pub curve:Option<[f64;4]>,pub scheme:Scheme,pub speed:f64}
impl Default for Options{fn default()->Self{Self{gap:8.,duration:480.,collapse_duration:440.,radius:30.,spring:Spring::default(),curve:None,scheme:Scheme::Expressive,speed:1.}}}
impl Options{pub fn spec(self,kind:Kind,speed:Speed)->Spec{Spec::token(self.scheme,kind,speed).paced(self.speed)}}
#[derive(Clone, Copy, Default, Debug, Deserialize, Serialize)]
pub struct Frame {
    pub capsule_scale_y:f64, pub card_reveal:f64, pub card_offset_y:f64,
    pub card_opacity:f64, pub card_scale:f64, pub card_blur:f64,
    pub content_opacity:f64, pub content_offset_y:f64,
}
impl Frame {
    pub fn hidden(o:Options)->Self { Self { capsule_scale_y:1., card_reveal:0.,card_offset_y:-o.gap-10.,card_opacity:0.,card_scale:0.98,card_blur:6.,content_opacity:0.,content_offset_y:6. } }
    pub fn expanded()->Self { Self {capsule_scale_y:1.,card_reveal:1.,card_offset_y:0.,card_opacity:1.,card_scale:1.,card_blur:0.,content_opacity:1.,content_offset_y:0.} }
    pub fn values(self)->[f64;8] { [self.capsule_scale_y,self.card_reveal,self.card_offset_y,self.card_opacity,self.card_scale,self.card_blur,self.content_opacity,self.content_offset_y] }
}


#[derive(Clone,Copy)]pub struct Transition{from:Frame,velocity:Frame,options:Options,reduced:bool,pub end:Frame,pub duration_ms:f64}
impl Transition{
 pub fn new(expanded:bool,from:Frame,options:Options,velocity:Frame,reduced:bool)->Self{
  let mut t=Self{from,velocity,options,reduced,end:if expanded{Frame::expanded()}else{Frame::hidden(options)},duration_ms:10000.};
  for ms in (8..=4000).step_by(8){let f=t.evaluate(ms as f64,false);let v=t.evaluate(ms as f64,true);if f.values().into_iter().zip(t.end.values()).enumerate().all(|(i,(a,b))|(a-b).abs()<if [2,5,7].contains(&i){0.03}else{0.001})&&v.values().into_iter().all(|v|v.abs()<0.015){t.duration_ms=ms as f64;break}}
  t
 }
 pub fn sample(&self,ms:f64)->Frame{self.evaluate(ms,false)}pub fn velocity(&self,ms:f64)->Frame{self.evaluate(ms,true)}
 fn evaluate(&self,ms:f64,velocity:bool)->Frame{
  if ms>=self.duration_ms{return if velocity{Frame::default()}else{self.end}}
  if ms<=0.{return if velocity{self.velocity}else{self.from}}
  let from=self.from.values();let incoming=self.velocity.values();let end=self.end.values();let mut values=[0.;8];
  for i in 0..8{let effects=[3,5,6].contains(&i);let spec=self.options.spec(if effects{Kind::Effects}else{Kind::Spatial},if i==5{Speed::Slow}else{Speed::Default});
   if self.reduced&&!effects{values[i]=if velocity{0.}else{end[i]};continue}
   // Text waits until there is room; an interrupted, visible channel never gets delayed.
   let delay=if i==6&&end[i]>from[i]&&from[i]<0.001&&incoming[i].abs()<0.001&&self.from.card_reveal<0.3{0.055}else{0.};
   let(x,v)=spec.sample(from[i],end[i],incoming[i],ms/1000.-delay);let bounds=match i{3|6=>Some((0.,1.)),5=>Some((0.,6.)),_=>None};
   values[i]=if velocity{if bounds.is_some_and(|(lo,hi)|x<lo||x>hi){0.}else{v}}else if let Some((lo,hi))=bounds{x.clamp(lo,hi)}else{x};
  }
  Frame{capsule_scale_y:values[0],card_reveal:values[1],card_offset_y:values[2],card_opacity:values[3],card_scale:values[4],card_blur:values[5],content_opacity:values[6],content_offset_y:values[7]}
 }
}
/// Geometry mapping, not a time curve; the caller supplies a sampled physical value.
pub fn blend(p:f64,start:f64,end:f64)->f64{let t=((p-start)/(end-start)).clamp(0.,1.);t*t*(3.-2.*t)}
pub fn sine(t:f64)->f64{(1.-(std::f64::consts::PI*t.clamp(0.,1.)).cos())/2.}
pub fn rubber(dx:f64,width:f64)->f64{let limit=width.abs().max(1.);let over=dx.abs()-limit;if over<=0.{return dx.clamp(-limit,limit)}let constant=0.55;dx.signum()*(limit+(over*limit*constant)/(limit+constant*over))}
#[derive(Clone,Copy)]pub struct Activity{pub playing:bool,started:f64,from:[f64;3],opacity_from:f64}
impl Activity{
 pub fn new()->Self{Self{playing:false,started:0.,from:[0.35;3],opacity_from:1.}}
 pub fn sample(&self,now:f64,animate:bool)->([f64;3],f64){let elapsed=(now-self.started).max(0.);let mut bars=[0.35;3];for i in 0..3{bars[i]=if self.playing&&animate{let period=320.+i as f64*95.;let phase=elapsed%(period*2.)/period;0.3+0.7*sine(if phase<=1.{phase}else{2.-phase})}else if self.playing{1.}else if animate{Spec::token(Scheme::Expressive,Kind::Spatial,Speed::Fast).sample(self.from[i],0.35,0.,elapsed/1000.).0}else{0.35};}let target=if self.playing{0.9}else{0.45};let opacity=if animate{Spec::token(Scheme::Expressive,Kind::Effects,Speed::Fast).sample(self.opacity_from,target,0.,elapsed/1000.).0}else{target};(bars,opacity)}
 pub fn retarget(&mut self,playing:bool,now:f64,animate:bool){let(from,opacity)=self.sample(now,animate);self.from=from;self.opacity_from=opacity;self.started=now;self.playing=playing;}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn reversal_preserves_each_position_and_velocity(){let o=Options::default();let a=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);for at in [10.,40.,90.,140.,230.]{let f=a.sample(at);let v=a.velocity(at);let b=Transition::new(false,f,o,v,false);assert_eq!(b.sample(0.).values(),f.values());assert_eq!(b.velocity(0.).values(),v.values());let dt=0.0001;for((start,next),speed)in f.values().into_iter().zip(b.sample(dt).values()).zip(v.values()){assert!((start-next).abs()<0.0001);assert!(((next-start)/(dt/1000.)-speed).abs()<0.05);}}}
 #[test]fn expressive_geometry_bounces_while_effects_remain_bounded(){let o=Options::default();let t=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);assert!((0..700).any(|i|t.sample(i as f64).card_reveal>1.));for i in 0..1000{let f=t.sample(i as f64);assert!(f.values().iter().all(|v|v.is_finite()));assert!((0. ..=1.).contains(&f.card_opacity));assert!((0. ..=1.).contains(&f.content_opacity));if f.content_opacity>0.05{assert!(f.card_reveal>0.3);}}assert!(t.duration_ms<900.);assert_eq!(t.sample(t.duration_ms).values(),t.end.values());}
 #[test]fn legacy_bezier_cannot_override_physics(){let o=Options::default();let mut legacy=o;legacy.curve=Some([0.,0.,1.,1.]);let a=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);let b=Transition::new(true,Frame::hidden(o),legacy,Frame::default(),false);for at in [0.,80.,180.,350.,800.]{assert_eq!(a.sample(at).values(),b.sample(at).values());}}
 #[test]fn rubber_band_resists_beyond_width(){assert_eq!(rubber(10.,100.),10.);assert!(rubber(200.,100.)>100.&&rubber(200.,100.)<200.);assert_eq!(rubber(-200.,100.),-rubber(200.,100.));}
}

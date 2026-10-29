//! Material 3 motion physics, sampled analytically and shared by every application surface.
//! Token values: AndroidX ExpressiveMotionTokens / StandardMotionTokens v0_14_0 (Apache-2.0).
//! https://m3.material.io/styles/motion — no frame-sized integration or easing approximation.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,serde::Deserialize,serde::Serialize)]pub enum Scheme{#[default]Expressive,Standard}
impl Scheme{pub fn parse(s:&str)->Self{if s=="Standard"{Self::Standard}else{Self::Expressive}}pub fn key(self)->&'static str{if self==Self::Standard{"Standard"}else{"Expressive"}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Speed{Fast,Default,Slow}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Kind{Spatial,Effects}
#[derive(Clone,Copy,Debug,PartialEq)]pub struct Spec{pub stiffness:f64,pub damping:f64}
impl Spec{pub const fn token(scheme:Scheme,kind:Kind,speed:Speed)->Self{
 let(stiffness,damping)=match(kind,scheme,speed){
  (Kind::Effects,_,Speed::Fast)=>(3800.,1.),(Kind::Effects,_,Speed::Default)=>(1600.,1.),(Kind::Effects,_,Speed::Slow)=>(800.,1.),
  (Kind::Spatial,Scheme::Expressive,Speed::Fast)=>(800.,0.6),(Kind::Spatial,Scheme::Expressive,Speed::Default)=>(380.,0.8),(Kind::Spatial,Scheme::Expressive,Speed::Slow)=>(200.,0.8),
  (Kind::Spatial,Scheme::Standard,Speed::Fast)=>(1400.,0.9),(Kind::Spatial,Scheme::Standard,Speed::Default)=>(700.,0.9),(Kind::Spatial,Scheme::Standard,Speed::Slow)=>(300.,0.9),
 };Self{stiffness,damping}}
 pub fn paced(self,speed:f64)->Self{let speed=if speed.is_finite(){speed.clamp(0.5,2.)}else{1.};Self{stiffness:self.stiffness*speed*speed,..self}}
 /// Exact solution of x'' + 2*zeta*omega*x' + omega^2*(x-target) = 0.
 pub fn sample(self,from:f64,target:f64,velocity:f64,time:f64)->(f64,f64){
  if !from.is_finite()||!target.is_finite()||!velocity.is_finite(){return(if target.is_finite(){target}else{0.},0.)}
  let t=if time.is_finite(){time.max(0.)}else{0.};if t==0.{return(from,velocity)}
  let w=self.stiffness.clamp(1.,100000.).sqrt();let z=self.damping.clamp(0.05,4.);let d=from-target;
  let(x,v)=if z<0.999999{let wd=w*(1.-z*z).sqrt();let b=(velocity+z*w*d)/wd;let e=(-z*w*t).exp();let c=(wd*t).cos();let s=(wd*t).sin();(e*(d*c+b*s),e*((-z*w)*(d*c+b*s)+wd*(-d*s+b*c)))}else if z<=1.000001{let b=velocity+w*d;let e=(-w*t).exp();((d+b*t)*e,(velocity-w*b*t)*e)}else{let q=(z*z-1.).sqrt();let r1=-w/(z+q);let r2=-w*(z+q);let a=(velocity-r2*d)/(r1-r2);let b=d-a;(a*(r1*t).exp()+b*(r2*t).exp(),a*r1*(r1*t).exp()+b*r2*(r2*t).exp())};(target+x,v)
 }
}
#[derive(Clone,Copy,Debug)]pub struct Spring{pub value:f64,pub target:f64,pub velocity:f64,pub stiffness:f64,pub damping:f64}
impl Spring{
 pub const fn new(value:f64)->Self{Self::spatial(value,Speed::Default)}
 pub const fn with_spec(value:f64,spec:Spec)->Self{Self{value,target:value,velocity:0.,stiffness:spec.stiffness,damping:spec.damping}}
 pub const fn spatial(value:f64,speed:Speed)->Self{Self::with_spec(value,Spec::token(Scheme::Expressive,Kind::Spatial,speed))}
 pub const fn effects(value:f64,speed:Speed)->Self{Self::with_spec(value,Spec::token(Scheme::Expressive,Kind::Effects,speed))}
 pub fn configure(&mut self,scheme:Scheme,kind:Kind,speed:Speed,pace:f64){let spec=Spec::token(scheme,kind,speed).paced(pace);self.stiffness=spec.stiffness;self.damping=spec.damping;}
 pub fn spec(&self)->Spec{Spec{stiffness:self.stiffness,damping:self.damping}}
 pub fn snap(&mut self,value:f64){self.value=value;self.target=value;self.velocity=0.;}
 pub fn settled(&self)->bool{self.settled_with(0.002,0.02)}
 pub fn settled_with(&self,distance:f64,velocity:f64)->bool{(self.value-self.target).abs()<distance&&self.velocity.abs()<velocity}
 pub fn tick(&mut self,dt:f64,reduced:bool)->bool{self.tick_with(dt,reduced,0.002,0.02)}
 pub fn tick_with(&mut self,dt:f64,reduced:bool,epsilon:f64,velocity_epsilon:f64)->bool{
  if !self.target.is_finite(){self.snap(0.);return true}if reduced||!self.value.is_finite()||!self.velocity.is_finite(){let changed=!self.settled_with(epsilon,velocity_epsilon);self.snap(self.target);return changed}if self.settled_with(epsilon,velocity_epsilon){self.snap(self.target);return false}if !dt.is_finite()||dt<=0.{return false}
  (self.value,self.velocity)=self.spec().sample(self.value,self.target,self.velocity,dt);if self.settled_with(epsilon,velocity_epsilon){self.snap(self.target);}true
 }
}
/// An absolute-time spring for renderers that can be sampled more than once per frame.
#[derive(Clone,Copy,Debug)]pub struct Track{pub from:f64,pub target:f64,pub velocity:f64,pub at:f64,pub spec:Spec}
impl Default for Track{fn default()->Self{Self::new(0.,Kind::Spatial,Speed::Default)}}
impl Track{
 pub const fn new(value:f64,kind:Kind,speed:Speed)->Self{Self{from:value,target:value,velocity:0.,at:0.,spec:Spec::token(Scheme::Expressive,kind,speed)}}
 pub fn sample(self,now:f64)->(f64,f64){if now<=self.at{return(self.from,self.velocity)}let(value,velocity)=self.spec.sample(self.from,self.target,self.velocity,now-self.at);if(value-self.target).abs()<0.00001&&velocity.abs()<0.0001{(self.target,0.)}else{(value,velocity)}}
 pub fn retarget(&mut self,target:f64,now:f64){let(from,velocity)=self.sample(now);self.from=from;self.velocity=velocity;self.target=target;self.at=now;}
 pub fn configure(&mut self,scheme:Scheme,kind:Kind,speed:Speed,pace:f64,now:f64){let spec=Spec::token(scheme,kind,speed).paced(pace);if self.spec!=spec{let(from,velocity)=self.sample(now);self.from=from;self.velocity=velocity;self.at=now;self.spec=spec;}}
 pub fn snap(&mut self,target:f64,now:f64){self.from=target;self.target=target;self.velocity=0.;self.at=now;}
 pub fn moving(self,now:f64)->bool{let(v,d)=self.sample(now);(v-self.target).abs()>=0.002||d.abs()>=0.02}
}
pub fn entrance(time:f64,kind:Kind,speed:Speed)->f64{Spec::token(Scheme::Expressive,kind,speed).sample(0.,1.,0.,time).0}
#[derive(Clone,Copy,Debug)]pub struct ColorTrack{channels:[Track;3]}
impl ColorTrack{
 pub fn new(rgb:u32)->Self{Self{channels:[16,8,0].map(|shift|Track::new(((rgb>>shift)&255)as f64,Kind::Effects,Speed::Default))}}
 pub fn retarget(&mut self,rgb:u32,now:f64,animate:bool){for(c,shift)in self.channels.iter_mut().zip([16,8,0]){let target=((rgb>>shift)&255)as f64;if animate{c.retarget(target,now)}else{c.snap(target,now)}}}
 pub fn sample(&self,now:f64)->u32{self.channels.iter().zip([16,8,0]).map(|(c,shift)|(c.sample(now).0.round().clamp(0.,255.)as u32)<<shift).fold(0,|a,b|a|b)}
 pub fn moving(&self,now:f64)->bool{self.channels.iter().any(|c|c.moving(now))}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn spatial_tokens_bounce_and_effects_are_monotonic_and_bounded(){for scheme in [Scheme::Expressive,Scheme::Standard]{for speed in [Speed::Fast,Speed::Default,Speed::Slow]{let p=Spec::token(scheme,Kind::Spatial,speed);assert!((0..1000).any(|i|p.sample(0.,1.,0.,i as f64/1000.).0>1.));let e=Spec::token(scheme,Kind::Effects,speed);let mut previous=0.;for i in 0..1000{let(v,_)=e.sample(0.,1.,0.,i as f64/1000.);assert!(v>=previous&&v<=1.);previous=v;}}}}
 #[test]fn retarget_preserves_position_and_velocity_and_reduced_motion_snaps(){let mut s=Spring::new(0.);s.target=100.;for _ in 0..6{s.tick(1./60.,false);}let before=(s.value,s.velocity);s.target=20.;assert_eq!((s.value,s.velocity),before);for _ in 0..180{s.tick(1./60.,false);}assert!(s.settled());s.target=80.;s.tick(0.01,true);assert_eq!(s.value,80.);}
 #[test]fn analytical_sampling_is_frame_rate_independent(){for kind in [Kind::Spatial,Kind::Effects]{let mut a=Spring::with_spec(0.,Spec::token(Scheme::Expressive,kind,Speed::Default));let mut b=a;a.target=1.;b.target=1.;for _ in 0..9{a.tick(1./30.,false);}for _ in 0..72{b.tick(1./240.,false);}assert!((a.value-b.value).abs()<1e-10);assert!((a.velocity-b.velocity).abs()<1e-9);}}
 #[test]fn tracks_retarget_in_flight_without_a_velocity_reset(){let mut t=Track::default();t.retarget(1.,0.);let mid=t.sample(0.08);t.retarget(-1.,0.08);assert_eq!(t.sample(0.08),mid);assert!(!t.moving(2.));}
 #[test]fn critical_and_overdamped_solutions_remain_finite(){for damping in [0.6,0.8,0.9,1.,1.8]{let spec=Spec{stiffness:380.,damping};for t in [0.,0.01,0.1,1.,10.]{let(x,v)=spec.sample(30.,0.,200.,t);assert!(x.is_finite()&&v.is_finite());}}}
}

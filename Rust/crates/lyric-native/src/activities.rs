//! Tool panel and a redirectable spring used when selecting island activities.
#[derive(Clone,Debug,Default,PartialEq)]pub struct Task{pub id:String,pub title:String,pub detail:String,pub color:u32}
#[derive(Clone,Debug,Default,PartialEq)]pub struct TimerPresentation{pub label:String,pub elapsed:String,pub footnote:String,pub primary:String,pub expanded_value:String,pub indefinite:bool}
#[derive(Clone,Debug,Default,PartialEq)]pub struct Panel{pub kind:u8,pub title:String,pub value:String,pub subtitle:String,pub progress:f64,pub running:bool,pub accent:u32,pub tasks:Vec<Task>,pub timer:Option<TimerPresentation>}
#[derive(Clone,Copy,Debug,Default)]pub struct Spring{pub value:f64,velocity:f64}
impl Spring{pub fn set(&mut self,v:f64){self.value=v;self.velocity=0.;}pub fn release(&mut self,value:f64,velocity:f64){self.value=value;self.velocity=velocity.clamp(-2400.,2400.);}pub fn tick(&mut self,dt:f64)->bool{if self.value.abs()<0.03&&self.velocity.abs()<0.1{let changed=self.value!=0.;self.value=0.;self.velocity=0.;return changed}let dt=dt.clamp(0.,0.1);let w=24.;let b=self.velocity+w*self.value;let e=(-w*dt).exp();self.value=(self.value+b*dt)*e;self.velocity=(self.velocity-w*b*dt)*e;true}}
#[derive(Clone,Copy)]pub struct Swipe{pub start:(f64,f64),pub dx:f64,pub active:bool,pub velocity:f64,last:std::time::Instant}
impl Swipe{pub fn new(x:f64,y:f64)->Self{Self{start:(x,y),dx:0.,active:false,velocity:0.,last:std::time::Instant::now()}}pub fn update(&mut self,x:f64,y:f64){let dx=x-self.start.0;let dy=y-self.start.1;if dx.abs()>8.&&dx.abs()>dy.abs()*1.25{self.active=true;}if self.active{let dt=self.last.elapsed().as_secs_f64();if dt>0.001{self.velocity=self.velocity*0.45+(dx-self.dx)/dt*0.55;}self.dx=dx;self.last=std::time::Instant::now();}}pub fn direction(&self)->i32{if self.active&&self.dx.abs()>=42.{if self.dx<0.{1}else{-1}}else{0}}
/// A quick flick counts even when it never travelled 42 px.
///
/// Requiring distance alone means a short, fast throw does nothing, which is the one
/// gesture people try first. Velocity is what they meant; distance is only the fallback
/// for a slow, deliberate drag. Thresholds are px/s: a real flick lands in the
/// 800–2000 range, a considered drag stays under ~300.
pub fn flick(&self)->i32{if !self.active{return 0}if self.velocity.abs()>=340.{if self.velocity<0.{1}else{-1}}else{0}}}
#[cfg(test)]mod tests{use super::*;#[test]fn vertical_motion_and_short_drags_do_not_switch(){let mut s=Swipe::new(0.,0.);s.update(9.,80.);assert_eq!(s.direction(),0);s.update(30.,0.);assert_eq!(s.direction(),0);s.update(-60.,0.);assert_eq!(s.direction(),1);s.update(60.,0.);assert_eq!(s.direction(),-1);}#[test]fn spring_retargets_from_its_current_position(){let mut s=Spring::default();s.set(300.);for _ in 0..6{s.tick(1./60.);}assert!(s.value>0.&&s.value<300.);let current=s.value;s.set(current);assert_eq!(s.value,current);for _ in 0..120{s.tick(1./60.);}assert!(s.value.abs()<0.1);}}

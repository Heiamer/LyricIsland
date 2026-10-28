//! Closed-form damped springs: retarget without losing velocity; sleep when settled.
#[derive(Clone,Copy,Debug)]pub struct Spring{pub value:f64,pub target:f64,velocity:f64,pub stiffness:f64,pub damping:f64}
impl Spring{
    pub const fn new(value:f64)->Self{Self{value,target:value,velocity:0.,stiffness:380.,damping:0.82}}
    pub fn snap(&mut self,value:f64){self.value=value;self.target=value;self.velocity=0.;}
    pub fn settled(&self)->bool{(self.value-self.target).abs()<0.002&&self.velocity.abs()<0.02}
    pub fn tick(&mut self,dt:f64,reduced:bool)->bool{if !self.target.is_finite(){self.snap(0.);return true}if reduced||!self.value.is_finite()||!self.velocity.is_finite(){let changed=!self.settled();self.snap(self.target);return changed}if self.settled(){self.snap(self.target);return false}if !dt.is_finite()||dt<=0.{return false}let dt=dt.min(10.);let omega=if self.stiffness.is_finite(){self.stiffness.clamp(1.,10000.).sqrt()}else{380f64.sqrt()};let z=if self.damping.is_finite(){self.damping.clamp(0.05,0.999)}else{0.82};let wd=omega*(1.-z*z).sqrt();let x=self.value-self.target;let a=x;let b=(self.velocity+z*omega*x)/wd;let e=(-z*omega*dt).exp();let c=(wd*dt).cos();let s=(wd*dt).sin();self.value=self.target+e*(a*c+b*s);self.velocity=e*((-z*omega)*(a*c+b*s)+wd*(-a*s+b*c));true}
}
#[cfg(test)]mod tests{use super::*;#[test]fn retargets_with_velocity_and_settles(){let mut s=Spring::new(0.);s.target=100.;for _ in 0..6{s.tick(1./60.,false);}let before=s.value;let v=s.velocity;s.target=20.;assert_eq!(s.value,before);assert_eq!(s.velocity,v);for _ in 0..180{s.tick(1./60.,false);}assert!(s.settled());}#[test]fn reduced_motion_is_immediate(){let mut s=Spring::new(0.);s.target=1.;s.tick(0.01,true);assert_eq!(s.value,1.);assert!(s.settled());}}

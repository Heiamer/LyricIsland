//! Port of IslandTransition.cs. All calculations remain f64, including velocity
//! on retarget. Frame time is elapsed time, never an accumulated fixed timestep.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Spring { pub stiffness: f64, pub damping: f64, pub mass: f64 }
impl Default for Spring { fn default() -> Self { Self { stiffness:420., damping:30., mass:0.8 } } }
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Options { pub gap:f64, pub duration:f64, pub collapse_duration:f64, pub radius:f64, pub spring:Spring }
impl Default for Options { fn default()->Self { Self { gap:8.,duration:360.,collapse_duration:300.,radius:30.,spring:Spring::default() } } }
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
#[derive(Clone, Copy)]
pub struct Transition { from:Frame, velocity:Frame, options:Options, reduced:bool, rate:f64, pub end:Frame, pub duration_ms:f64 }
impl Transition {
    pub fn new(expanded:bool,from:Frame,options:Options,velocity:Frame,reduced:bool)->Self {
        let rate=if expanded {options.duration} else {options.collapse_duration}/360.;
        let s=options.spring;let omega=(s.stiffness/s.mass).sqrt();let zeta=s.damping/(2.*(s.stiffness*s.mass).sqrt());
        let decay=if zeta>1. {omega/(zeta+(zeta*zeta-1.).sqrt())}else{zeta*omega};
        Self{from,velocity,options,reduced,rate,end:if expanded{Frame::expanded()}else{Frame::hidden(options)},duration_ms:if reduced{120.}else{(12./decay*1000.*rate).max(1.)}}
    }
    pub fn sample(&self,ms:f64)->Frame { self.evaluate(ms,false) }
    pub fn velocity(&self,ms:f64)->Frame {self.evaluate(ms,true)}
    fn evaluate(&self,ms:f64,velocity:bool)->Frame {
        if ms>=self.duration_ms {return if velocity{Frame::default()}else{self.end}}
        if !self.reduced && ms<=0. {return if velocity{self.velocity}else{self.from}}
        if self.reduced {
            if velocity{return Frame::default()}
            let t=(ms/self.duration_ms).clamp(0.,1.);let mut g=if self.end.card_reveal>0.{self.end}else{Frame::expanded()};
            g.card_opacity=self.from.card_opacity+(self.end.card_opacity-self.from.card_opacity)*t;
            g.content_opacity=self.from.content_opacity+(self.end.content_opacity-self.from.content_opacity)*t;return g;
        }
        let a=self.from.values();let b=self.end.values();let v=self.velocity.values();let mut r=[0.;8];
        let speeds=[1.,1.,1.,1.5,1.,1.2,1.3,1.];
        for i in 0..8 {
            let (p,v)=self.spring(a[i],b[i],v[i],ms.max(0.)/1000.,speeds[i]/self.rate);
            let (min,max)=match i{1=>(0.,1.035),3|6=>(0.,1.),5=>(0.,6.),_=>(f64::NEG_INFINITY,f64::INFINITY)};
            r[i]=if velocity{if p<min||p>max{0.}else{v}}else{p.clamp(min,max)};
        }
        Frame{capsule_scale_y:r[0],card_reveal:r[1],card_offset_y:r[2],card_opacity:r[3],card_scale:r[4],card_blur:r[5],content_opacity:r[6],content_offset_y:r[7]}
    }
    fn spring(&self,from:f64,target:f64,velocity:f64,time:f64,speed:f64)->(f64,f64) {
        let s=self.options.spring;let omega=(s.stiffness/s.mass).sqrt()*speed;let zeta=s.damping/(2.*(s.stiffness*s.mass).sqrt());let d=from-target;
        let (x,v)=if zeta<0.999999 {
            let decay=zeta*omega;let wd=omega*(1.-zeta*zeta).sqrt();let b=(velocity+decay*d)/wd;
            let cos=(wd*time).cos();let sin=(wd*time).sin();let e=(-decay*time).exp();
            (e*(d*cos+b*sin),e*((b*wd-decay*d)*cos-(d*wd+decay*b)*sin))
        }else if zeta<=1.000001 {
            let b=velocity+omega*d;let e=(-omega*time).exp();((d+b*time)*e,(velocity-omega*b*time)*e)
        }else {
            let root=(zeta*zeta-1.).sqrt();let r1=-omega/(zeta+root);let r2=-omega*(zeta+root);
            let a=(velocity-r2*d)/(r1-r2);let b=d-a;
            (a*(r1*time).exp()+b*(r2*time).exp(),a*r1*(r1*time).exp()+b*r2*(r2*time).exp())
        };(target+x,v)
    }
}
pub fn blend(p:f64,start:f64,end:f64)->f64 {let t=((p-start)/(end-start)).clamp(0.,1.);t*t*(3.-2.*t)}
pub fn sine(t:f64)->f64{(1.-(std::f64::consts::PI*t.clamp(0.,1.)).cos())/2.}
pub fn cubic_out(t:f64)->f64{1.-(1.-t.clamp(0.,1.)).powi(3)}
#[derive(Clone,Copy)]
pub struct Activity {pub playing:bool,started:f64,from:[f64;3],opacity_from:f64}
impl Activity {
    pub fn new()->Self{Self{playing:false,started:0.,from:[0.35;3],opacity_from:1.}}
    pub fn sample(&self,now:f64,animate:bool)->([f64;3],f64){
        let elapsed=(now-self.started).max(0.);let mut bars=[0.35;3];
        for i in 0..3{bars[i]=if self.playing&&animate{let period=320.+i as f64*95.;let phase=elapsed%(period*2.)/period;0.3+0.7*sine(if phase<=1.{phase}else{2.-phase})}else if self.playing{1.}else if animate{self.from[i]+(0.35-self.from[i])*cubic_out(elapsed/160.)}else{0.35};}
        let target=if self.playing{0.9}else{0.45};let opacity=if animate{self.opacity_from+(target-self.opacity_from)*sine(elapsed/180.)}else{target};(bars,opacity)
    }
    pub fn retarget(&mut self,playing:bool,now:f64,animate:bool){let (from,opacity)=self.sample(now,animate);self.from=from;self.opacity_from=opacity;self.started=now;self.playing=playing;}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn reversal_preserves_sampled_position_and_velocity() {
        let o=Options::default();let a=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        for t in [20.,90.,180.,320.] { let p=a.sample(t);let v=a.velocity(t);let b=Transition::new(false,p,o,v,false);
            assert_eq!(b.sample(0.).values(),p.values());assert_eq!(b.velocity(0.).values(),v.values());
            for (x,y) in b.sample(0.00001).values().iter().zip(p.values()) {assert!((x-y).abs()<0.0001);}
        }
    }
    #[test] fn damping_branches_settle_without_nan() {
        for damping in [10.,2.*(420_f64*0.8).sqrt(),80.] {let mut o=Options::default();o.spring.damping=damping;
            let a=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
            for i in 0..1000{assert!(a.sample(i as f64*2.).values().iter().all(|x|x.is_finite()));}
            assert_eq!(a.sample(a.duration_ms).values(),Frame::expanded().values());
        }
    }
}

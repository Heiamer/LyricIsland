//! Port of IslandTransition.cs. All calculations remain f64, including velocity
//! on retarget. Frame time is elapsed time, never an accumulated fixed timestep.
//!
//! One fluid spring drives the shape and its derived content channels.
//! Retargeting preserves every live position and velocity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Spring { pub stiffness: f64, pub damping: f64, pub mass: f64 }
impl Default for Spring { fn default() -> Self { Self { stiffness:420., damping:30., mass:0.8 } } }
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Options { pub gap:f64, pub duration:f64, pub collapse_duration:f64, pub radius:f64, pub spring:Spring,pub curve:Option<[f64;4]> }
impl Default for Options { fn default()->Self { Self { gap:8.,duration:480.,collapse_duration:440.,radius:30.,spring:Spring::default(),curve:None } } }
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

/// How fast the fluid master spring runs relative to the configured one.
///
/// At 1.5× the morph read as a snap: a critically damped spring front-loads its
/// travel, so most of the shape change landed inside the first ~130 ms and the rest
/// of the configured length was sub-pixel crawl. Running the master at the configured
/// pace — 1.0× — spreads the same travel over the length the 「展开响应」 slider names
/// (a 360 ms setting settles near 520 ms) while keeping the properties that make the
/// curve safe: no overshoot, and a reversal that carries every channel's velocity.
const FLUID_SPEED:f64=1.0;
/// Where the expanded card starts on its way in. Never 0: nothing in the real world
/// appears from nothing, so a barely-there shape reads as an arrival rather than a pop.
const FLUID_SCALE_FLOOR:f64=0.98;
#[derive(Clone, Copy)]
pub struct Transition { from:Frame, velocity:Frame, options:Options, reduced:bool, rate:f64, pub end:Frame, pub duration_ms:f64 }
impl Transition {
    pub fn new(expanded:bool,from:Frame,options:Options,velocity:Frame,reduced:bool)->Self {
        let rate=if expanded {options.duration} else {options.collapse_duration}/360.;
        let s=options.spring;let omega=(s.stiffness/s.mass).sqrt();let zeta=(s.damping/(2.*(s.stiffness*s.mass).sqrt())).max(1.);
        // The settle time has to follow the speed actually used to sample, or the
        // transition lingers after its last visible frame and keeps the redraw loop awake.
        let decay=(if zeta>1. {omega/(zeta+(zeta*zeta-1.).sqrt())}else{zeta*omega})*FLUID_SPEED;
        Self{from,velocity,options,reduced,rate,end:if expanded{Frame::expanded()}else{Frame::hidden(options)},duration_ms:if reduced{120.}else if options.curve.is_some(){if expanded{options.duration}else{options.collapse_duration}}else{(12./decay*1000.*rate).max(1.)}}
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
        self.fluid(ms,velocity)
    }
    /// One spring on the shape; every other channel is a pure function of it.
    ///
    /// Reading order is what makes a morph feel intentional: the outline opens first
    /// (0 → 0.74), the contents fade in once there is room for them (0.44 → 0.97), and
    /// the blur that masks the two-layer crossfade is gone by the time the text is
    /// readable. Because they all come from the same `p`, an interruption mid-flight
    /// carries every channel through the reversal together.
    fn fluid(&self,ms:f64,velocity:bool)->Frame {
        let time=ms.max(0.)/1000.;let speed=FLUID_SPEED/self.rate;
        let (p,dp)=if let Some(curve)=self.options.curve{let u=(ms/self.duration_ms).clamp(0.,1.);let(ease,slope)=lyric_core::motion_curve::sample(curve,u);let duration=self.duration_ms/1000.;let delta=self.end.card_reveal-self.from.card_reveal;let incoming=self.velocity.card_reveal;let start_slope=lyric_core::motion_curve::sample(curve,0.).1;let interrupted=self.from.card_reveal>0.0001&&self.from.card_reveal<0.9999||incoming.abs()>0.001;let residual=if interrupted{incoming-delta*start_slope/duration}else{0.};let correction=residual*duration*u*(1.-u).powi(2);(self.from.card_reveal+delta*ease+correction,delta*slope/duration+residual*(1.-u)*(1.-3.*u))}else{self.integrate(self.from.card_reveal,self.end.card_reveal,self.velocity.card_reveal,time,speed,1.0)};
        let channels=|value:f64,rate:f64|->([f64;8],[f64;8]){
            let q=value.clamp(0.,1.);let (fade,dfade)=step(q,0.16,0.72);
            let (content,dcontent)=step(q,0.58,0.985);let (scale,dscale)=step(q,0.,0.76);let (blur,dblur)=step(q,0.10,0.82);let lift=self.options.gap+10.;
            ([1.,value.clamp(0.,1.035),-(1.-q)*lift,fade,FLUID_SCALE_FLOOR+(1.-FLUID_SCALE_FLOOR)*scale,6.*(1.-blur),content,6.*(1.-content)],
             [0.,rate,lift*rate,dfade*rate,(1.-FLUID_SCALE_FLOOR)*dscale*rate,-6.*dblur*rate,dcontent*rate,-6.*dcontent*rate])
        };
        let (mut values,mut velocities)=channels(p,dp);let (initial,initial_velocity)=channels(self.from.card_reveal,self.velocity.card_reveal);
        // Preserve every presentation channel and its velocity on reversal.
        let omega=(self.options.spring.stiffness/self.options.spring.mass).sqrt()*speed;
        let decay=(-omega*time).exp();let from=self.from.values();let incoming=self.velocity.values();
        for i in 0..8{if i==1{continue}let offset=from[i]-initial[i];let tangent=incoming[i]-initial_velocity[i]+omega*offset;
            values[i]+=(offset+tangent*time)*decay;velocities[i]+=(tangent-omega*(offset+tangent*time))*decay;
            let bounds=match i{3|6=>Some((0.,1.)),5=>Some((0.,6.)),_=>None};
            if let Some((lo,hi))=bounds{if values[i]<lo||values[i]>hi{velocities[i]=0.;}values[i]=values[i].clamp(lo,hi);}
        }
        let r=if velocity{velocities}else{values};
        Frame{capsule_scale_y:r[0],card_reveal:r[1],card_offset_y:r[2],card_opacity:r[3],card_scale:r[4],card_blur:r[5],content_opacity:r[6],content_offset_y:r[7]}
    }
    /// The damping floor keeps the master spring from overshooting its measured card.
    fn integrate(&self,from:f64,target:f64,velocity:f64,time:f64,speed:f64,zeta_floor:f64)->(f64,f64) {
        let s=self.options.spring;let omega=(s.stiffness/s.mass).sqrt()*speed;let zeta=(s.damping/(2.*(s.stiffness*s.mass).sqrt())).max(zeta_floor);let d=from-target;
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
/// A smoothstep remap of `p` onto 0..1 across `[a,b]`, with its derivative in `p`.
///
/// The derivative is what lets one channel's velocity become the whole frame's velocity:
/// on a re-target every derived channel contributes `d/dp × dp/dt`, so nothing jumps.
fn step(p:f64,a:f64,b:f64)->(f64,f64){
    let span=(b-a).abs().max(1e-6);let t=((p-a)/span).clamp(0.,1.);
    (t*t*(3.-2.*t),6.*t*(1.-t)/span)
}
pub fn blend(p:f64,start:f64,end:f64)->f64 {let t=((p-start)/(end-start)).clamp(0.,1.);t*t*(3.-2.*t)}
pub fn sine(t:f64)->f64{(1.-(std::f64::consts::PI*t.clamp(0.,1.)).cos())/2.}
pub fn cubic_out(t:f64)->f64{1.-(1.-t.clamp(0.,1.)).powi(3)}
/// Rubber-banding for a drag taken past its own width.
///
/// A hard clamp reads as frozen — the surface stops dead under a finger that is still
/// moving. Real things slow down before they stop, so the excess is damped instead of
/// discarded: the further past the edge, the less of it you get.
pub fn rubber(dx:f64,width:f64)->f64{
    let limit=width.abs().max(1.);let over=dx.abs()-limit;
    if over<=0.{return dx.clamp(-limit,limit)}
    let constant=0.55;let resisted=(over*limit*constant)/(limit+constant*over);
    dx.signum()*(limit+resisted)
}
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
    /// Endpoints must match the geometry used for drawing and pointer hit testing.
    #[test] fn fluid_lands_on_the_same_endpoints() {
        let o=Options::default();
        let open=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        assert_eq!(open.sample(0.).values(),Frame::hidden(o).values());
        assert_eq!(open.sample(open.duration_ms).values(),Frame::expanded().values());
        let close=Transition::new(false,Frame::expanded(),o,Frame::default(),false);
        assert_eq!(close.sample(close.duration_ms).values(),Frame::hidden(o).values());
    }
    #[test] fn fluid_is_monotonic_and_never_opens_from_zero() {
        let o=Options::default();let t=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        let mut previous=-1.;
        for i in 0..=400{let f=t.sample(i as f64);assert!(f.card_reveal>=previous-1e-9);previous=f.card_reveal;
            assert!(f.card_scale>=FLUID_SCALE_FLOOR-1e-9,"scale dipped to {}",f.card_scale);
            assert!((0.0..=1.0).contains(&f.card_opacity));}
    }
    /// Contents must not be readable before the container has room for them.
    #[test] fn fluid_contents_trail_the_outline() {
        let o=Options::default();let t=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        for i in 0..200{let f=t.sample(i as f64*3.);if f.content_opacity>0.05{assert!(f.card_reveal>0.3,"contents visible at reveal {}",f.card_reveal);}}
    }
    #[test] fn fluid_reverses_without_a_jump() {
        let o=Options::default();let a=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        for t in [30.,120.,260.] {let p=a.sample(t);let v=a.velocity(t);let b=Transition::new(false,p,o,v,false);
            for (x,y) in b.sample(0.).values().iter().zip(p.values()){assert!((x-y).abs()<1e-6,"jumped at {t}ms");}}
    }
    /// Reversal preserves every channel even with the longer throw timing.
    #[test]fn fluid_reversal_preserves_the_first_real_frame_and_all_channel_velocities(){
        let o=Options::default();let opening=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        for at in [10.,40.,90.,140.,230.]{let from=opening.sample(at);let speed=opening.velocity(at);let closing=Transition::new(false,from,o,speed,false);
            let dt=0.0001;let frame=closing.sample(dt);for ((start,next),expected) in from.values().into_iter().zip(frame.values()).zip(speed.values()){
                assert!((start-next).abs()<0.0001,"position discontinuity at {at}ms");
                assert!(((next-start)/(dt/1000.)-expected).abs()<0.04,"velocity discontinuity at {at}ms");
            }
        }
    }
    #[test] fn fluid_settles_within_the_drawer_budget() {
        let o=Options::default();
        let fluid=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        assert!(fluid.duration_ms<760.,"drawer budget exceeded: {}",fluid.duration_ms);
    }
    #[test] fn rubber_band_resists_instead_of_stopping() {
        assert_eq!(rubber(10.,100.),10.);
        assert_eq!(rubber(100.,100.),100.);
        let past=rubber(200.,100.);
        assert!(past>100.&&past<200.);
        assert!(rubber(400.,100.)>past);
        assert_eq!(rubber(-200.,100.),-past);
    }
}

#[cfg(test)]mod curve_tests{use super::*;
 #[test]fn custom_curve_drives_the_real_master_channel_and_preserves_reversal(){let mut o=Options::default();o.curve=Some([0.,0.,1.,1.]);let t=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);assert!((t.sample(o.duration/2.).card_reveal-0.5).abs()<1e-6);let f=t.sample(180.);let v=t.velocity(180.);let r=Transition::new(false,f,o,v,false);assert_eq!(r.sample(0.).values(),f.values());assert_eq!(r.velocity(0.).values(),v.values());let near=(r.sample(0.001).card_reveal-f.card_reveal)/0.000001;assert!((near-v.card_reveal).abs()<0.01);assert_eq!(r.sample(r.duration_ms).card_reveal,0.);}
}

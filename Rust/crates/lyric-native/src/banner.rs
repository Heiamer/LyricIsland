//! A critically damped, interruptible notification surface. Typography is never
//! scaled with the shell. Position and velocity survive retargets.
use crate::{layout::Rect,notifications::Notice};
#[derive(Clone,Copy,Default)]struct Spring{from:f64,to:f64,velocity:f64,time:f64}
impl Spring {
    fn sample(self,now:f64)->(f64,f64){let t=(now-self.time).max(0.);let omega=22.;let d=self.from-self.to;let b=self.velocity+omega*d;let e=(-omega*t).exp();(self.to+(d+b*t)*e,(self.velocity-omega*b*t)*e)}
    fn retarget(&mut self,to:f64,now:f64){let (from,velocity)=self.sample(now);*self=Self{from,to,velocity,time:now};}
    fn snap(&mut self,to:f64,now:f64){*self=Self{from:to,to,time:now,velocity:0.};}
}
pub struct Banner {pub notice:Option<Notice>,motion:[Spring;6],pub open:bool,deadline:f64,changed:f64}
impl Default for Banner {fn default()->Self{Self{notice:None,motion:[Spring::default();6],open:false,deadline:0.,changed:0.}}}
impl Banner {
    pub fn visible(&self)->bool{self.notice.is_some()}
    pub fn presentation(&self,now:f64)->(Rect,f64,f64){let a=self.motion.map(|s|s.sample(now).0);(Rect::new(a[0],a[1],a[2],a[3]),a[4],a[5].clamp(0.,1.))}
    pub fn show(&mut self,notice:Notice,base:Rect,radius:f64,host_width:f64,height:f64,now:f64,animate:bool){
        if self.notice.is_none(){for (s,v) in self.motion.iter_mut().zip([base.x,base.y,base.w,base.h,radius,0.]){s.snap(v,now);}}
        self.notice=Some(notice);self.open=true;self.changed=now;self.deadline=now+7.;
        let width=440_f64.min(host_width-48.);let target=[(host_width-width)/2.,base.y,width,height.clamp(64.,96.),24.,1.];
        for (s,v) in self.motion.iter_mut().zip(target){if animate{s.retarget(v,now)}else{s.snap(v,now)}}
    }
    pub fn dismiss(&mut self,base:Rect,radius:f64,now:f64,animate:bool){
        if !self.visible()||!self.open{return}self.open=false;self.changed=now;
        for (s,v) in self.motion.iter_mut().zip([base.x,base.y,base.w,base.h,radius,0.]){if animate{s.retarget(v,now)}else{s.snap(v,now)}}
    }
    pub fn tick(&mut self,base:Rect,radius:f64,now:f64,hover:bool,animate:bool){
        if self.open {if hover{self.deadline=now+1.;}if now>=self.deadline{self.dismiss(base,radius,now,animate);}}
        if !self.open&&self.visible(){
            let target=[base.x,base.y,base.w,base.h,radius,0.];
            for (s,v) in self.motion.iter_mut().zip(target){if (s.to-v).abs()>0.001{if animate{s.retarget(v,now)}else{s.snap(v,now)}}}
            let settled=self.motion.iter().zip(target).all(|(s,v)|(s.sample(now).0-v).abs()<0.1);
            if !animate||(now-self.changed>0.65&&settled){self.notice=None;}
        }
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn retarget_preserves_position_and_velocity(){let mut s=Spring::default();s.snap(260.,0.);s.retarget(440.,0.);let before=s.sample(0.08);s.retarget(260.,0.08);assert_eq!(s.sample(0.08),before);assert!(s.sample(0.09).0>before.0);}
    #[test]fn close_returns_to_original_surface(){let base=Rect::new(114.,24.,260.,40.);let mut b=Banner::default();b.show(Notice::default(),base,20.,488.,80.,0.,true);b.dismiss(base,20.,0.12,true);b.tick(base,20.,1.,false,true);assert!(!b.visible());assert!((b.presentation(2.).0.w-260.).abs()<0.001);}
}

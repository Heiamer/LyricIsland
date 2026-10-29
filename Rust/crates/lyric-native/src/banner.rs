//! A critically damped, interruptible notification surface. Typography is never
//! scaled with the shell. Position and velocity survive retargets.
use crate::{layout::Rect,notifications::Notice};
use lyric_core::shared_motion::{Track,Scheme,Kind,Speed};
#[derive(Clone,Copy,Debug)]struct Spring{track:Track,to:f64}
impl Default for Spring{fn default()->Self{Self{track:Track::default(),to:0.}}}
impl Spring{
 fn sample(self,now:f64)->(f64,f64){self.track.sample(now)}
 fn retarget(&mut self,to:f64,now:f64){self.track.retarget(to,now);self.to=to;}
 fn snap(&mut self,to:f64,now:f64){self.track.snap(to,now);self.to=to;}
}
pub struct Banner {pub notice:Option<Notice>,motion:[Spring;6],pub open:bool,deadline:f64,changed:f64,compact:bool}
impl Default for Banner {fn default()->Self{Self{notice:None,motion:std::array::from_fn(|i|Spring{track:Track::new(0.,if i==5{Kind::Effects}else{Kind::Spatial},Speed::Default),to:0.}),open:false,deadline:0.,changed:0.,compact:false}}}
impl Banner {
    pub fn configure(&mut self,scheme:Scheme,pace:f64,now:f64){for(i,s)in self.motion.iter_mut().enumerate(){s.track.configure(scheme,if i==5{Kind::Effects}else{Kind::Spatial},if self.compact{Speed::Fast}else{Speed::Default},pace,now);}}
    pub fn visible(&self)->bool{self.notice.is_some()}
    /// Whether the hold has run out. The shell asks this so a queued message can take
    /// over the open surface instead of closing it and opening it again.
    pub fn due(&self,now:f64)->bool{self.open&&now>=self.deadline-0.08}
    pub fn presentation(&self,now:f64)->(Rect,f64,f64){let a=self.motion.map(|s|s.sample(now).0);(Rect::new(a[0],a[1],a[2],a[3]),a[4],a[5].clamp(0.,1.))}
    pub fn show(&mut self,notice:Notice,base:Rect,radius:f64,host_width:f64,height:f64,now:f64,animate:bool){
        if self.notice.is_none(){for (s,v) in self.motion.iter_mut().zip([base.x,base.y,base.w,base.h,radius,0.]){s.snap(v,now);}}
        self.notice=Some(notice);self.open=true;self.changed=now;self.deadline=now+7.;self.compact=false;
        let width=440_f64.min(host_width-48.);let target=[(host_width-width)/2.,base.y,width,height.clamp(64.,96.),24.,1.];
        for (s,v) in self.motion.iter_mut().zip(target){if animate{s.retarget(v,now)}else{s.snap(v,now)}}
    }
    /// A compact, non-interactive hint: same spring and interruption rules, shorter hold.
    pub fn show_compact(&mut self,notice:Notice,base:Rect,radius:f64,host_width:f64,now:f64,animate:bool){
        if self.notice.is_none(){for (s,v) in self.motion.iter_mut().zip([base.x,base.y,base.w,base.h,radius,0.]){s.snap(v,now);}}
        self.notice=Some(notice);self.open=true;self.changed=now;self.deadline=now+1.6;self.compact=true;
        let width=220_f64.min(host_width-48.);let target=[(host_width-width)/2.,base.y,width,44.,22.,1.];
        // Reduced motion still gets a short opacity fade; the shell never scales.
        for (i,(s,v)) in self.motion.iter_mut().zip(target).enumerate(){if animate||i==5{s.retarget(v,now)}else{s.snap(v,now)}}
    }
    pub fn dismiss(&mut self,base:Rect,radius:f64,now:f64,animate:bool){
        if !self.visible()||!self.open{return}self.open=false;self.changed=now;
        for (i,(s,v)) in self.motion.iter_mut().zip([base.x,base.y,base.w,base.h,radius,0.]).enumerate(){if animate||(self.compact&&i==5){s.retarget(v,now)}else{s.snap(v,now)}}
    }
    pub fn tick(&mut self,base:Rect,radius:f64,now:f64,hover:bool,animate:bool){
        if self.open {if hover{self.deadline=now+1.;}if now>=self.deadline{self.dismiss(base,radius,now,animate);}}
        if !self.open&&self.visible(){
            let target=[base.x,base.y,base.w,base.h,radius,0.];
            for (i,(s,v)) in self.motion.iter_mut().zip(target).enumerate(){if (s.to-v).abs()>0.001{if animate||(self.compact&&i==5){s.retarget(v,now)}else{s.snap(v,now)}}}
            let settled=self.motion.iter().zip(target).all(|(s,v)|(s.sample(now).0-v).abs()<0.1);
            let reduced=if self.compact{0.16}else{0.};
            if if animate{now-self.changed>if self.compact{0.45}else{0.65}&&settled}else{now-self.changed>reduced&&settled}{self.notice=None;}
        }
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn retarget_preserves_position_and_velocity(){let mut s=Spring::default();s.snap(260.,0.);s.retarget(440.,0.);let before=s.sample(0.08);s.retarget(260.,0.08);assert_eq!(s.sample(0.08),before);assert!(s.sample(0.09).0>before.0);}
    #[test]fn close_returns_to_original_surface(){let base=Rect::new(114.,24.,260.,40.);let mut b=Banner::default();b.show(Notice::default(),base,20.,488.,80.,0.,true);b.dismiss(base,20.,0.12,true);b.tick(base,20.,1.,false,true);assert!(!b.visible());assert!((b.presentation(2.).0.w-260.).abs()<0.001);}
}

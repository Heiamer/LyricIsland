//! Original island geometry shared with the visual motion preview.
use crate::music_layout::Rect;
fn blend(p:f64,a:f64,b:f64)->f64{let t=((p-a)/(b-a)).clamp(0.,1.);t*t*(3.-2.*t)}
#[derive(Clone,Copy)] pub struct Layout{pub capsule:Rect,pub card:Rect,pub host:Rect}
impl Layout {
    /// The host reserves the taller A layout so switching never resizes the window.
    pub fn new(width:f64)->Self{let cw=(width+112.).max(400.);let host=cw.max(440.);Self{capsule:Rect::new(24.+(host-width)/2.,24.,width,40.),card:Rect::new(24.+(host-cw)/2.,24.,cw,264.),host:Rect::new(0.,0.,host+48.,332.)}}
    pub fn reserve_compact(&mut self,max:f64){let width=(max+48.).max(self.host.w);self.host.w=width;self.capsule.x=(width-self.capsule.w)/2.;self.card.x=(width-self.card.w)/2.;}
    pub fn set_compact(&mut self,width:f64,height:f64){self.capsule.w=width;self.capsule.x=(self.host.w-width)/2.;self.capsule.h=height;}
    /// Horizontal expansion leads the vertical unfolding; both are derived from
    /// the same live spring, so reversing the gesture never swaps coordinate spaces.
    pub fn morph_shape(self,p:f64,r:f64)->(Rect,f64){self.morph_shape_with_radii(p,self.capsule.h/2.,r)}
    pub fn morph_shape_with_radii(self,p:f64,closed_radius:f64,r:f64)->(Rect,f64){
        let over=(p-p.clamp(0.,1.)).clamp(-0.04,0.04)*0.4;let p=p.clamp(0.,1.);let x=blend(p,0.10,0.88)+over;let y=blend(p,0.22,1.)+over;
        let a=self.capsule;let b=self.card;// A restrained parabolic toss: lift as a capsule, unfold, then settle.
        // One live progress parameter makes collapse retrace this exact path.
        let arc=(std::f64::consts::PI*p).sin();
        let rect=Rect::new(a.x+(b.x-a.x)*x+5.*arc,a.y+(b.y-a.y)*y-12.*arc,a.w+(b.w-a.w)*x,a.h+(b.h-a.h)*y);
        (rect,(closed_radius.clamp(0.,a.h/2.)+(r-closed_radius.clamp(0.,a.h/2.))*y).min(rect.w.min(rect.h)/2.).max(0.))
    }
    pub fn satellite(self)->Rect{Rect::new(self.capsule.x+self.capsule.w+8.,self.capsule.y,self.capsule.h,self.capsule.h)}
    pub fn compact_cover(self,scale:f64)->Rect{let round=|v:f64|(v*scale).round_ties_even()/scale;Rect::new(self.capsule.x+round(7.)+round(3.),self.capsule.y+round((self.capsule.h-24.)/2.),24.,24.)}
}

#[cfg(test)]mod independent_radius_tests{
 use super::*;
 #[test]fn radii_follow_the_same_geometry_and_remain_independent(){
  let l=Layout::new(260.);
  for closed in [0.,8.,20.,48.]{for opened in [8.,30.,48.]{
   let(a,ra)=l.morph_shape_with_radii(0.,closed,opened);let(b,rb)=l.morph_shape_with_radii(1.,closed,opened);
   assert_eq!(a,l.capsule);assert_eq!(b,l.card);assert_eq!(ra,closed.min(l.capsule.h/2.));assert_eq!(rb,opened);
   for i in 0..100{let p=i as f64/100.;let(shape,r)=l.morph_shape_with_radii(p,closed,opened);assert!(r>=0.&&r<=shape.w.min(shape.h)/2.);assert_eq!(shape,l.morph_shape(p,opened).0);}
  }}
 }
}

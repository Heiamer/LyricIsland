use serde::Serialize;
#[derive(Clone,Copy,Default,Debug,Serialize,PartialEq)]
pub struct Rect {pub x:f64,pub y:f64,pub w:f64,pub h:f64}
impl Rect {pub fn new(x:f64,y:f64,w:f64,h:f64)->Self{Self{x,y,w,h}} pub fn contains(self,x:f64,y:f64)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
#[derive(Clone,Copy)] pub struct Layout{pub capsule:Rect,pub card:Rect,pub host:Rect}
impl Layout {
    pub fn new(width:f64)->Self{let cw=(width+48.).max(340.);let host=cw.max(440.);Self{capsule:Rect::new(24.+(host-width)/2.,24.,width,40.),card:Rect::new(24.+(host-cw)/2.,24.,cw,200.),host:Rect::new(0.,0.,host+48.,248.)}}
    pub fn reserve_compact(&mut self,max:f64){let width=(max+48.).max(self.host.w);self.host.w=width;self.capsule.x=(width-self.capsule.w)/2.;self.card.x=(width-self.card.w)/2.;}
    pub fn set_compact(&mut self,width:f64,height:f64){self.capsule.w=width;self.capsule.x=(self.host.w-width)/2.;self.capsule.h=height;}
    pub fn shape(self,p:f64,r:f64)->(Rect,f64){let p=p.clamp(0.,1.035);let a=self.capsule;let b=self.card;let l=|x:f64,y:f64|x+(y-x)*p;let rect=Rect::new(l(a.x,b.x),l(a.y,b.y),l(a.w,b.w).max(1.),l(a.h,b.h).max(1.));(rect,l(a.h/2.,r).clamp(0.,rect.w.min(rect.h)/2.))}
    pub fn satellite(self)->Rect{Rect::new(self.capsule.x+self.capsule.w+8.,self.capsule.y,self.capsule.h,self.capsule.h)}
    pub fn compact_cover(self,scale:f64)->Rect{let round=|v:f64|(v*scale).round_ties_even()/scale;Rect::new(self.capsule.x+round(7.)+round(3.),self.capsule.y+round((self.capsule.h-24.)/2.),24.,24.)}
}
pub fn buttons(width:f64,scale:f64)->[(i32,Rect);8]{
    let round=|v:f64|(v*scale).round_ties_even()/scale;let center=round(width/2.);
    [(2,Rect::new(16.,155.,26.,26.)),(9,Rect::new(47.,155.,26.,26.)),(3,Rect::new(center-60.,153.,30.,30.)),(4,Rect::new(center-20.,148.,40.,40.)),(5,Rect::new(center+30.,153.,30.,30.)),(6,Rect::new(width-76.,155.,26.,26.)),(7,Rect::new(width-45.,155.,26.,26.)),(8,Rect::new(width-42.,16.,24.,24.))]
}
/// Shared by drawing and hit testing. Footer keeps a 12 px bottom safe area.
pub fn todo_rows(width:f64,count:usize)->impl Iterator<Item=Rect>{(0..count.min(3)).map(move|i|Rect::new(20.,52.+i as f64*36.,width-40.,32.))}
pub fn todo_footer(width:f64)->Rect{Rect::new(20.,164.,width-40.,24.)}
#[cfg(test)]mod tests{use super::*;#[test]fn todo_content_has_safe_insets_and_nonoverlapping_targets(){for width in [340.,420.,528.]{let rows=todo_rows(width,3).collect::<Vec<_>>();let footer=todo_footer(width);assert_eq!(rows[2].y+rows[2].h+8.,footer.y);assert_eq!(footer.y+footer.h,200.-12.);assert_eq!(rows[0].x,footer.x);for r in rows{assert!(!r.contains(footer.x+1.,footer.y+1.));}}}}

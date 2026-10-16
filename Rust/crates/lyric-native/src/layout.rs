use serde::Serialize;
#[derive(Clone,Copy,Default,Debug,Serialize,PartialEq)]
pub struct Rect {pub x:f64,pub y:f64,pub w:f64,pub h:f64}
impl Rect {pub fn new(x:f64,y:f64,w:f64,h:f64)->Self{Self{x,y,w,h}} pub fn contains(self,x:f64,y:f64)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
#[derive(Clone,Copy)] pub struct Layout{pub capsule:Rect,pub card:Rect,pub host:Rect}
impl Layout {
    pub fn new(width:f64)->Self{let cw=(width+48.).max(340.);let host=cw.max(440.);Self{capsule:Rect::new(24.+(host-width)/2.,24.,width,40.),card:Rect::new(24.+(host-cw)/2.,24.,cw,200.),host:Rect::new(0.,0.,host+48.,248.)}}
    pub fn shape(self,p:f64,r:f64)->(Rect,f64){let p=p.clamp(0.,1.035);let a=self.capsule;let b=self.card;let l=|x:f64,y:f64|x+(y-x)*p;let rect=Rect::new(l(a.x,b.x),l(a.y,b.y),l(a.w,b.w).max(1.),l(a.h,b.h).max(1.));(rect,l(a.h/2.,r).clamp(0.,rect.w.min(rect.h)/2.))}
    pub fn compact_cover(self,scale:f64)->Rect{let round=|v:f64|(v*scale).round_ties_even()/scale;Rect::new(self.capsule.x+round(7.)+round(3.),self.capsule.y+round(8.),24.,24.)}
}
pub fn buttons(width:f64,scale:f64)->[(i32,Rect);8]{
    let round=|v:f64|(v*scale).round_ties_even()/scale;let x=78.+round((width-278.)/2.);let y=144.+round(1.);
    [(2,Rect::new(18.,152.,24.,24.)),(9,Rect::new(48.,152.,24.,24.)),(3,Rect::new(x,y+4.,30.,30.)),(4,Rect::new(x+42.,y,38.,38.)),(5,Rect::new(x+92.,y+4.,30.,30.)),(6,Rect::new(width-72.,152.,24.,24.)),(7,Rect::new(width-42.,152.,24.,24.)),(8,Rect::new(width-42.,16.,24.,24.))]
}

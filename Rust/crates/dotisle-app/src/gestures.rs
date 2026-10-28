//! Gestures start only on the cloud summary; controls keep their normal click behavior.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Intent{Click,Left,Right,Expand,Collapse,Cancel}
#[derive(Clone,Copy)]pub struct Gesture{pub origin:(f64,f64),pub current:(f64,f64),pub cancelled:bool,travel:f64,axis:u8}
impl Gesture{
 pub fn new(p:(f64,f64))->Self{Self{origin:p,current:p,cancelled:false,travel:0.,axis:0}}
 pub fn cancel(&mut self){self.cancelled=true;self.current=self.origin;}
 pub fn update(&mut self,p:(f64,f64)){if self.cancelled{return}self.current=p;let dx=p.0-self.origin.0;let dy=p.1-self.origin.1;self.travel=self.travel.max(dx.abs().max(dy.abs()));if self.axis==0&&self.travel>=12.{if dx.abs()>dy.abs()*1.25{self.axis=1}else if dy.abs()>dx.abs()*1.25{self.axis=2}}}
 pub fn offset(&self)->f64{if self.cancelled||self.axis==2{return 0.}let dx=self.current.0-self.origin.0;dx/(1.+dx.abs()/100.)}
 pub fn finish(self)->Intent{if self.cancelled{return Intent::Cancel}let dx=self.current.0-self.origin.0;let dy=self.current.1-self.origin.1;if self.travel<8.{Intent::Click}else if self.axis==1&&dx.abs()>=40.{if dx<0.{Intent::Left}else{Intent::Right}}else if self.axis==2&&dy.abs()>=40.{if dy<0.{Intent::Collapse}else{Intent::Expand}}else{Intent::Cancel}}
}
#[cfg(test)]mod tests{use super::*;#[test]fn clicks_diagonals_and_swipes_are_distinct(){for(p,want)in [((3.,2.),Intent::Click),((20.,20.),Intent::Cancel),((-60.,4.),Intent::Left),((50.,2.),Intent::Right),((2.,-45.),Intent::Collapse),((1.,50.),Intent::Expand)]{let mut g=Gesture::new((0.,0.));g.update(p);assert_eq!(g.finish(),want);}}#[test]fn cancellation_and_returning_to_origin_never_click(){let mut g=Gesture::new((0.,0.));g.update((60.,0.));g.update((0.,0.));assert_eq!(g.finish(),Intent::Cancel);g.cancel();g.update((80.,0.));assert_eq!(g.offset(),0.);assert_eq!(g.finish(),Intent::Cancel);}}

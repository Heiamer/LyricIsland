//! Axis-locked upward dismissal, independent of activity count and horizontal switching.
use std::time::{Instant,Duration};
#[derive(Clone,Copy)]pub struct HideDrag{start:(f64,f64),pub dy:f64,pub active:bool,velocity:f64,last:Instant,pub at_top:bool}
impl HideDrag{
 pub fn new(x:f64,y:f64)->Self{Self{start:(x,y),dy:0.,active:false,velocity:0.,last:Instant::now(),at_top:false}}
 pub fn update(&mut self,x:f64,y:f64,at_top:bool){let dx=x-self.start.0;let dy=y-self.start.1;self.at_top=at_top;
  if !self.active&&dy< -8.&&-dy>dx.abs()*1.25{self.active=true}
  if self.active{let dt=self.last.elapsed().as_secs_f64();if dt>0.001{self.velocity=0.45*self.velocity+0.55*(dy-self.dy)/dt;}self.dy=dy.min(0.);self.last=Instant::now();}
 }
 pub fn dismiss(&self)->bool{self.active&&(self.dy<= -40.||self.at_top||(self.dy< -18.&&self.velocity< -360.&&self.last.elapsed()<Duration::from_millis(100)))}
 pub fn offset(&self)->f64{let d=(-self.dy).max(0.);-72.*d/(72.+d)}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn only_intentional_upward_drags_dismiss(){let mut d=HideDrag::new(80.,100.);d.update(82.,150.,false);assert!(!d.active);d.update(130.,90.,false);assert!(!d.active);d.update(82.,55.,false);assert!(d.dismiss());assert!(d.offset()<0.&&d.offset()>-45.);}
 #[test]fn hitting_the_screen_ceiling_dismisses_even_when_the_distance_is_short(){
  // The island parks at the top of the work area, so a real drag can only travel a dozen
  // DIP before the pointer stops at y=0. Reaching the ceiling is the intent, not the distance.
  let mut d=HideDrag::new(80.,100.);d.update(80.,88.,true);assert!(d.active);assert!(d.dismiss());
  let mut e=HideDrag::new(80.,100.);e.update(80.,88.,false);assert!(!e.dismiss());}
 #[test]fn short_drag_cancels_and_reversing_aborts(){let mut d=HideDrag::new(0.,100.);d.update(0.,90.,false);assert!(d.active);assert!(!d.dismiss());d.update(0.,30.,false);assert!(d.dismiss());d.update(0.,98.,false);assert!(!d.dismiss());}
}

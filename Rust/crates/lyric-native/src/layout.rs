use serde::Serialize;
#[derive(Clone,Copy,Default,Debug,Serialize,PartialEq)]
pub struct Rect {pub x:f64,pub y:f64,pub w:f64,pub h:f64}
impl Rect {pub fn new(x:f64,y:f64,w:f64,h:f64)->Self{Self{x,y,w,h}} pub fn contains(self,x:f64,y:f64)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
#[derive(Clone,Copy)] pub struct Layout{pub capsule:Rect,pub card:Rect,pub host:Rect}
impl Layout {
    /// The host reserves the taller A layout so switching never resizes the window.
    pub fn new(width:f64)->Self{let cw=(width+112.).max(400.);let host=cw.max(440.);Self{capsule:Rect::new(24.+(host-width)/2.,24.,width,40.),card:Rect::new(24.+(host-cw)/2.,24.,cw,264.),host:Rect::new(0.,0.,host+48.,332.)}}
    pub fn reserve_compact(&mut self,max:f64){let width=(max+48.).max(self.host.w);self.host.w=width;self.capsule.x=(width-self.capsule.w)/2.;self.card.x=(width-self.card.w)/2.;}
    pub fn set_compact(&mut self,width:f64,height:f64){self.capsule.w=width;self.capsule.x=(self.host.w-width)/2.;self.capsule.h=height;}
    pub fn shape(self,p:f64,r:f64)->(Rect,f64){let p=p.clamp(0.,1.035);let a=self.capsule;let b=self.card;let l=|x:f64,y:f64|x+(y-x)*p;let rect=Rect::new(l(a.x,b.x),l(a.y,b.y),l(a.w,b.w).max(1.),l(a.h,b.h).max(1.));(rect,l(a.h/2.,r).clamp(0.,rect.w.min(rect.h)/2.))}
    pub fn satellite(self)->Rect{Rect::new(self.capsule.x+self.capsule.w+8.,self.capsule.y,self.capsule.h,self.capsule.h)}
    pub fn compact_cover(self,scale:f64)->Rect{let round=|v:f64|(v*scale).round_ties_even()/scale;Rect::new(self.capsule.x+round(7.)+round(3.),self.capsule.y+round((self.capsule.h-24.)/2.),24.,24.)}
}
fn buttons_at(width:f64,scale:f64,offset:f64)->[(i32,Rect);7]{
    let round=|v:f64|(v*scale).round_ties_even()/scale;let center=round(width/2.);
    [(2,Rect::new(16.,215.+offset,26.,26.)),(9,Rect::new(47.,215.+offset,26.,26.)),(3,Rect::new(center-60.,213.+offset,30.,30.)),(4,Rect::new(center-20.,208.+offset,40.,40.)),(5,Rect::new(center+30.,213.+offset,30.,30.)),(6,Rect::new(width-76.,215.+offset,26.,26.)),(7,Rect::new(width-45.,215.+offset,26.,26.))]
}
/// The two expanded music layouts. This is a layout type, not a pair of switches.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum MusicLayout { Separated, SideBySide }
impl MusicLayout {
    pub fn from_setting(value:&str)->Self{if value=="Separated"{Self::Separated}else{Self::SideBySide}}
}
/// Everything drawing, hit testing and seek share for one expanded music card.
#[derive(Clone,Copy)]
pub struct MusicGeometry {
    pub cover:Rect,pub title:Rect,pub artist:Rect,pub lyric:Rect,pub secondary:Rect,
    pub status_dot:Rect,pub status_text:Rect,pub track:Rect,pub elapsed:Rect,pub total:Rect,
    pub buttons:[(i32,Rect);7],pub height:f64,pub progress_hit:Rect,pub text_hit:Rect,pub split:f64,
}
fn lerp_rect(a:Rect,b:Rect,t:f64)->Rect{Rect::new(a.x+(b.x-a.x)*t,a.y+(b.y-a.y)*t,a.w+(b.w-a.w)*t,a.h+(b.h-a.h)*t)}
impl MusicGeometry {
    /// Blend two layouts. Geometry is the only thing that interpolates, so drawing and
    /// hit testing stay in lockstep on every frame of the transition.
    fn between(a:&Self,b:&Self,t:f64)->Self{
        let mut buttons=a.buttons;
        for (slot,other) in buttons.iter_mut().zip(b.buttons){slot.1=lerp_rect(slot.1,other.1,t);}
        Self{cover:lerp_rect(a.cover,b.cover,t),title:lerp_rect(a.title,b.title,t),artist:lerp_rect(a.artist,b.artist,t),lyric:lerp_rect(a.lyric,b.lyric,t),secondary:lerp_rect(a.secondary,b.secondary,t),status_dot:lerp_rect(a.status_dot,b.status_dot,t),status_text:lerp_rect(a.status_text,b.status_text,t),track:lerp_rect(a.track,b.track,t),elapsed:lerp_rect(a.elapsed,b.elapsed,t),total:lerp_rect(a.total,b.total,t),buttons,height:a.height+(b.height-a.height)*t,progress_hit:lerp_rect(a.progress_hit,b.progress_hit,t),text_hit:lerp_rect(a.text_hit,b.text_hit,t),split:a.split+(b.split-a.split)*t}
    }
}
/// `mix` is 0 for B (side by side) and 1 for A (lyrics in their own section).
pub fn music_geometry(width:f64,layout:MusicLayout,mix:f64,scale:f64)->MusicGeometry{
    let side=MusicGeometry{
        cover:Rect::new(24.,24.,80.,80.),
        title:Rect::new(128.,24.,width-152.,28.),
        artist:Rect::new(128.,60.,width-152.,20.),
        lyric:Rect::new(128.,96.,width-152.,27.),
        secondary:Rect::new(128.,134.,width-152.,20.),
        status_dot:Rect::new(24.,125.,5.,5.),
        status_text:Rect::new(35.,120.,72.,15.),
        track:Rect::new(24.,170.,width-48.,3.),
        elapsed:Rect::new(24.,182.,70.,16.),
        total:Rect::new(width-94.,182.,70.,16.),
        buttons:buttons_at(width,scale,0.),height:264.,
        progress_hit:Rect::new(14.,161.,width-28.,20.),
        text_hit:Rect::new(128.,24.,width-152.,56.),split:0.,
    };
    let separated=MusicGeometry{
        cover:Rect::new(24.,24.,64.,64.),
        title:Rect::new(108.,24.,width-228.,28.),
        artist:Rect::new(108.,60.,width-132.,20.),
        lyric:Rect::new(24.,112.,width-48.,28.),
        secondary:Rect::new(24.,152.,width-48.,20.),
        status_dot:Rect::new(width-99.,35.,5.,5.),
        status_text:Rect::new(width-88.,30.,64.,15.),
        track:Rect::new(24.,190.,width-48.,3.),
        elapsed:Rect::new(24.,202.,70.,16.),
        total:Rect::new(width-94.,202.,70.,16.),
        buttons:buttons_at(width,scale,20.),height:284.,
        progress_hit:Rect::new(14.,181.,width-28.,20.),
        text_hit:Rect::new(108.,24.,width-228.,56.),split:1.,
    };
    let t=match layout{MusicLayout::Separated=>mix.clamp(0.,1.),MusicLayout::SideBySide=>0.};
    MusicGeometry::between(&side,&separated,t)
}
/// Shared by drawing and hit testing. Footer keeps a 12 px bottom safe area.
pub fn todo_rows(width:f64,count:usize)->impl Iterator<Item=Rect>{(0..count.min(3)).map(move|i|Rect::new(20.,52.+i as f64*36.,width-40.,32.))}
pub fn todo_footer(width:f64)->Rect{Rect::new(20.,164.,width-40.,24.)}
#[cfg(test)]mod tests{use super::*;#[test]fn todo_content_has_safe_insets_and_nonoverlapping_targets(){for width in [340.,420.,528.]{let rows=todo_rows(width,3).collect::<Vec<_>>();let footer=todo_footer(width);assert_eq!(rows[2].y+rows[2].h+8.,footer.y);assert_eq!(footer.y+footer.h,200.-12.);assert_eq!(rows[0].x,footer.x);for r in rows{assert!(!r.contains(footer.x+1.,footer.y+1.));}}}}

#[cfg(test)] mod music_spacing_tests {use super::*;
#[test]fn both_layouts_reserve_spacing_and_hits(){for width in [400.,440.,528.]{for kind in [MusicLayout::SideBySide,MusicLayout::Separated]{let g=music_geometry(width,kind,1.,1.5);assert!(g.secondary.y>=g.lyric.y+g.lyric.h+8.);assert!(g.track.y>=g.secondary.y+g.secondary.h+12.);assert!(g.buttons[3].1.y>=g.elapsed.y+g.elapsed.h+8.);assert!(g.buttons[3].1.y+40.<=g.height-12.);if kind==MusicLayout::Separated{assert!(g.lyric.y>=g.cover.y+g.cover.h+20.);}else{assert!(g.lyric.x>=g.cover.x+g.cover.w+24.);}}}}
}

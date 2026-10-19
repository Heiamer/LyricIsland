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
    /// Horizontal expansion leads the vertical unfolding; both are derived from
    /// the same live spring, so reversing the gesture never swaps coordinate spaces.
    pub fn morph_shape(self,p:f64,r:f64,fluid:bool)->(Rect,f64){
        if !fluid{return self.shape(p,r)}let p=p.clamp(0.,1.);let x=crate::motion::blend(p,0.10,0.88);let y=crate::motion::blend(p,0.22,1.);
        let a=self.capsule;let b=self.card;// A restrained parabolic toss: lift as a capsule, unfold, then settle.
        // One live progress parameter makes collapse retrace this exact path.
        let arc=(std::f64::consts::PI*p).sin();
        let rect=Rect::new(a.x+(b.x-a.x)*x+5.*arc,a.y+(b.y-a.y)*y-12.*arc,a.w+(b.w-a.w)*x,a.h+(b.h-a.h)*y);
        (rect,(a.h/2.+(r-a.h/2.)*y).min(rect.w.min(rect.h)/2.).max(0.))
    }
    pub fn satellite(self)->Rect{Rect::new(self.capsule.x+self.capsule.w+8.,self.capsule.y,self.capsule.h,self.capsule.h)}
    pub fn compact_cover(self,scale:f64)->Rect{let round=|v:f64|(v*scale).round_ties_even()/scale;Rect::new(self.capsule.x+round(7.)+round(3.),self.capsule.y+round((self.capsule.h-24.)/2.),24.,24.)}
}
fn buttons_at(width:f64,scale:f64,center_y:f64)->[(i32,Rect);7]{
    let snap=|v:f64|(v*scale).round_ties_even()/scale;let center=snap(width/2.);let y=snap(center_y);
    [(2,Rect::new(18.,y-14.,28.,28.)),(9,Rect::new(52.,y-14.,28.,28.)),(3,Rect::new(center-58.,y-16.,32.,32.)),(4,Rect::new(center-20.,y-20.,40.,40.)),(5,Rect::new(center+26.,y-16.,32.,32.)),(6,Rect::new(width-80.,y-14.,28.,28.)),(7,Rect::new(width-46.,y-14.,28.,28.))]
}
pub fn music_height(mix:f64,font:f64)->f64{220.+16.*mix.clamp(0.,1.)+(font+8.-24.).max(0.)}
/// The two expanded music layouts. This is a layout type, not a pair of switches.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum MusicLayout { Separated, SideBySide, LyricFocus }
impl MusicLayout {
    pub fn position(self)->f64{match self{Self::SideBySide=>0.,Self::Separated=>1.,Self::LyricFocus=>2.}}
    pub fn from_setting(value:&str)->Self{match value{"Separated"=>Self::Separated,"LyricFocus"=>Self::LyricFocus,_=>Self::SideBySide}}
}
/// Everything drawing, hit testing and seek share for one expanded music card.
#[derive(Clone,Copy)]
pub struct MusicGeometry {
    pub cover:Rect,pub title:Rect,pub artist:Rect,pub lyric:Rect,pub secondary:Rect,
    pub status_dot:Rect,pub status_text:Rect,pub track:Rect,pub elapsed:Rect,pub total:Rect,
    pub buttons:[(i32,Rect);7],pub height:f64,pub progress_hit:Rect,pub text_hit:Rect,pub split:f64,pub centered:f64,
}
fn lerp_rect(a:Rect,b:Rect,t:f64)->Rect{Rect::new(a.x+(b.x-a.x)*t,a.y+(b.y-a.y)*t,a.w+(b.w-a.w)*t,a.h+(b.h-a.h)*t)}
impl MusicGeometry {
    /// Blend two layouts. Geometry is the only thing that interpolates, so drawing and
    /// hit testing stay in lockstep on every frame of the transition.
    fn between(a:&Self,b:&Self,t:f64)->Self{
        let mut buttons=a.buttons;
        for (slot,other) in buttons.iter_mut().zip(b.buttons){slot.1=lerp_rect(slot.1,other.1,t);}
        Self{cover:lerp_rect(a.cover,b.cover,t),title:lerp_rect(a.title,b.title,t),artist:lerp_rect(a.artist,b.artist,t),lyric:lerp_rect(a.lyric,b.lyric,t),secondary:lerp_rect(a.secondary,b.secondary,t),status_dot:lerp_rect(a.status_dot,b.status_dot,t),status_text:lerp_rect(a.status_text,b.status_text,t),track:lerp_rect(a.track,b.track,t),elapsed:lerp_rect(a.elapsed,b.elapsed,t),total:lerp_rect(a.total,b.total,t),buttons,height:a.height+(b.height-a.height)*t,progress_hit:lerp_rect(a.progress_hit,b.progress_hit,t),text_hit:lerp_rect(a.text_hit,b.text_hit,t),split:a.split+(b.split-a.split)*t,centered:a.centered+(b.centered-a.centered)*t}
    }
}
/// `mix` is 0 for B (side by side) and 1 for A (lyrics in their own section).
pub fn music_geometry(width:f64,_layout:MusicLayout,mix:f64,scale:f64,font:f64)->MusicGeometry{
    let extra=(font+8.-24.).max(0.);let line=24.+extra;
    let side=MusicGeometry{
        cover:Rect::new(20.,20.,68.,68.),
        title:Rect::new(108.,18.,width-128.,24.),artist:Rect::new(108.,45.,width-128.,18.),
        lyric:Rect::new(108.,68.,width-128.,line),secondary:Rect::new(108.,94.+extra,width-128.,18.),
        status_dot:Rect::new(20.,101.+extra,4.,4.),status_text:Rect::new(30.,94.+extra,66.,16.),
        track:Rect::new(20.,134.+extra,width-40.,3.),elapsed:Rect::new(20.,144.+extra,70.,14.),total:Rect::new(width-90.,144.+extra,70.,14.),
        buttons:buttons_at(width,scale,185.+extra),height:music_height(0.,font),
        progress_hit:Rect::new(16.,128.+extra,width-32.,14.),text_hit:Rect::new(108.,18.,width-128.,46.),split:0.,centered:0.,
    };
    let separated=MusicGeometry{
        cover:Rect::new(20.,18.,56.,56.),
        title:Rect::new(96.,23.5,width-202.,24.),artist:Rect::new(96.,50.5,width-116.,18.),
        lyric:Rect::new(20.,94.,width-40.,line),secondary:Rect::new(20.,118.+extra,width-40.,18.),
        status_dot:Rect::new(width-94.,34.,4.,4.),status_text:Rect::new(width-84.,23.5,64.,24.),
        track:Rect::new(20.,150.+extra,width-40.,3.),elapsed:Rect::new(20.,160.+extra,70.,14.),total:Rect::new(width-90.,160.+extra,70.,14.),
        buttons:buttons_at(width,scale,201.+extra),height:music_height(1.,font),
        progress_hit:Rect::new(16.,144.+extra,width-32.,14.),text_hit:Rect::new(96.,23.5,width-192.,45.),split:1.,centered:0.,
    };
    // The live mix drives both directions; the destination enum must not snap a reverse morph.
    let mut focus=separated;
    focus.cover=Rect::new(20.,18.,48.,48.);focus.title=Rect::new(82.,19.,width-188.,24.);focus.artist=Rect::new(82.,46.,width-102.,18.);
    focus.status_text.y=19.;focus.status_dot.y=29.5;focus.centered=1.;focus.lyric.y=92.;focus.secondary.y=116.+extra;focus.text_hit=Rect::new(82.,19.,width-102.,45.);
    if mix<=1.{MusicGeometry::between(&side,&separated,mix.clamp(0.,1.))}else{MusicGeometry::between(&separated,&focus,(mix-1.).clamp(0.,1.))}
}
/// Digital timer controls share their painted bounds with pointer hit testing.
pub fn timer_buttons(width:f64)->[(i32,Rect);3]{[(31,Rect::new(24.,156.,66.,32.)),(30,Rect::new(width/2.-52.,156.,104.,32.)),(7,Rect::new(width-90.,156.,66.,32.))]}
/// Shared by drawing and hit testing. Footer keeps a 12 px bottom safe area.
pub fn todo_rows(width:f64,count:usize)->impl Iterator<Item=Rect>{(0..count.min(3)).map(move|i|Rect::new(20.,52.+i as f64*36.,width-40.,32.))}
pub fn todo_footer(width:f64)->Rect{Rect::new(20.,164.,width-40.,24.)}
#[cfg(test)]mod tests{use super::*;#[test]fn todo_content_has_safe_insets_and_nonoverlapping_targets(){for width in [340.,420.,528.]{let rows=todo_rows(width,3).collect::<Vec<_>>();let footer=todo_footer(width);assert_eq!(rows[2].y+rows[2].h+8.,footer.y);assert_eq!(footer.y+footer.h,200.-12.);assert_eq!(rows[0].x,footer.x);for r in rows{assert!(!r.contains(footer.x+1.,footer.y+1.));}}}}

#[cfg(test)] mod music_spacing_tests {use super::*;
#[test]fn compact_layouts_keep_text_controls_and_pointer_targets_disjoint(){
 for width in [400.,440.,592.]{for scale in [1.,1.25,1.5,2.]{for font in [11.,12.5,17.]{for mix in [0.,0.25,0.5,0.75,1.]{
  let g=music_geometry(width,MusicLayout::Separated,mix,scale,font);
  assert!(g.secondary.y>=g.lyric.y+g.lyric.h);
  assert!(g.track.y>=g.secondary.y+g.secondary.h+12.);
  assert!(g.buttons[3].1.y>=g.elapsed.y+g.elapsed.h+6.);
  let center=g.buttons[3].1.y+g.buttons[3].1.h/2.;
  for(_,r)in g.buttons{assert!((r.y+r.h/2.-center).abs()<0.01);assert!(r.y+r.h<=g.height-12.);assert!(!r.contains(g.track.x+1.,g.track.y));}
  assert_eq!(g.height,music_height(mix,font));
  if mix==0.{assert!(g.lyric.x>=g.cover.x+g.cover.w+20.);assert_eq!(g.title.x,g.lyric.x);}
  if mix==1.{assert!(g.lyric.y>=g.cover.y+g.cover.h+18.);assert_eq!((g.title.y+g.artist.y+g.artist.h)/2.,g.cover.y+g.cover.h/2.);assert_eq!(g.artist.y-g.title.y-g.title.h,3.);assert_eq!(g.status_text.y,g.title.y);assert!(g.title.x+g.title.w+12.<=g.status_dot.x);}
 }}}}
}
#[test]fn switching_layouts_uses_the_same_live_geometry_in_both_directions(){for mix in [0.,0.2,0.5,0.8,1.]{let a=music_geometry(400.,MusicLayout::Separated,mix,1.5,12.5);let b=music_geometry(400.,MusicLayout::SideBySide,mix,1.5,12.5);assert_eq!(a.lyric,b.lyric);assert_eq!(a.cover,b.cover);assert_eq!(a.buttons,b.buttons);}}
}

#[cfg(test)]mod focus_layout_tests{use super::*;
 #[test]fn all_three_layouts_keep_lyrics_and_controls_separate(){for width in [400.,440.,592.]{for mix in [0.,0.5,1.,1.5,2.]{let g=music_geometry(width,MusicLayout::LyricFocus,mix,1.5,17.);assert!(g.lyric.y>=g.cover.y+g.cover.h||g.lyric.x>=g.cover.x+g.cover.w+20.);assert!(g.track.y>=g.secondary.y+g.secondary.h+12.);assert!(g.buttons[3].1.y>=g.elapsed.y+g.elapsed.h+6.);if mix>=1.{assert!(g.title.x+g.title.w+12.<=g.status_dot.x);}}}}
}

#[cfg(test)] mod throw_geometry_tests {
    use super::*;
    #[test] fn toss_stays_in_host_and_retraces_the_same_path() {
        for width in [140.,260.,480.] {let l=Layout::new(width);
            assert_eq!(l.morph_shape(0.,30.,true),l.shape(0.,30.));
            let (end,r)=l.morph_shape(1.,30.,true);assert!((end.x-l.card.x).abs()<1e-8);assert!((end.y-l.card.y).abs()<1e-8);assert_eq!(r,30.);
            for i in 0..=1000 {let p=i as f64/1000.;let(a,_)=l.morph_shape(p,30.,true);let(b,_)=l.morph_shape(1.-(1.-p),30.,true);
                assert!(a.x>=0.&&a.y>=0.&&a.x+a.w<=l.host.w&&a.y+a.h<=l.host.h);
                assert!((a.y-b.y).abs()<1e-8);assert!((a.w-b.w).abs()<1e-8);
            }
        }
    }
}

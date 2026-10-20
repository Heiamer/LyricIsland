//! Shared live geometry for the native island, settings preview and layout thumbnails.
use serde::Serialize;
#[derive(Clone,Copy,Default,Debug,Serialize,PartialEq)]
pub struct Rect {pub x:f64,pub y:f64,pub w:f64,pub h:f64}
impl Rect {pub fn new(x:f64,y:f64,w:f64,h:f64)->Self{Self{x,y,w,h}} pub fn contains(self,x:f64,y:f64)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
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

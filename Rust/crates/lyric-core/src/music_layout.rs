//! Shared live geometry for the native island, settings preview and layout thumbnails.
use serde::Serialize;
#[derive(Clone,Copy,Default,Debug,Serialize,PartialEq)]
pub struct Rect {pub x:f64,pub y:f64,pub w:f64,pub h:f64}
impl Rect {pub fn new(x:f64,y:f64,w:f64,h:f64)->Self{Self{x,y,w,h}} pub fn contains(self,x:f64,y:f64)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
/// Expanded artwork uses half the card's configured radius, bounded by its own short edge.
pub fn cover_corner_radius(card_radius:f64,width:f64,height:f64)->f64{
    let radius=if card_radius.is_finite(){card_radius.max(0.)}else{30.};
    (radius*0.5).min(width.max(0.).min(height.max(0.))/2.)
}
fn buttons_at(width:f64,scale:f64,center_y:f64)->[(i32,Rect);7]{
    let snap=|v:f64|(v*scale).round_ties_even()/scale;let center=snap(width/2.);let y=snap(center_y);
    [(2,Rect::new(18.,y-14.,28.,28.)),(9,Rect::new(52.,y-14.,28.,28.)),(3,Rect::new(center-58.,y-16.,32.,32.)),(4,Rect::new(center-20.,y-20.,40.,40.)),(5,Rect::new(center+26.,y-16.,32.,32.)),(6,Rect::new(width-80.,y-14.,28.,28.)),(7,Rect::new(width-46.,y-14.,28.,28.))]
}
// Grow with the first stage that clears the artwork, before widening the lyric band.
pub fn music_height(mix:f64,font:f64)->f64{220.+16.*(mix*2.).clamp(0.,1.)+(font+8.-24.).max(0.)}
/// The expanded music layouts. This is a layout type, not a set of switches.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum MusicLayout { Separated, SideBySide, LyricFocus, LyricRight }
impl MusicLayout {
    pub fn position(self)->f64{match self{Self::SideBySide=>0.,Self::Separated=>1.,Self::LyricFocus=>2.,Self::LyricRight=>3.}}
    pub fn from_setting(value:&str)->Self{match value{"Separated"=>Self::Separated,"LyricFocus"=>Self::LyricFocus,"LyricRight"=>Self::LyricRight,_=>Self::SideBySide}}
}
/// Everything drawing, hit testing and seek share for one expanded music card.
///
/// `align` is the one horizontal anchor every consumer reads: 0 hugs the left edge,
/// 0.5 centres in the free space, 1 hugs the right edge. Drawing, the settings
/// preview and the clip rects all use this single number, so the preview can never
/// disagree with the island about where a line sits.
#[derive(Clone,Copy)]
pub struct MusicGeometry {
    pub cover:Rect,pub title:Rect,pub artist:Rect,pub lyric:Rect,pub secondary:Rect,
    pub status_dot:Rect,pub status_text:Rect,pub track:Rect,pub elapsed:Rect,pub total:Rect,
    pub buttons:[(i32,Rect);7],pub height:f64,pub progress_hit:Rect,pub text_hit:Rect,pub split:f64,pub align:f64,
}
fn lerp_rect(a:Rect,b:Rect,t:f64)->Rect{Rect::new(a.x+(b.x-a.x)*t,a.y+(b.y-a.y)*t,a.w+(b.w-a.w)*t,a.h+(b.h-a.h)*t)}
impl MusicGeometry {
    /// Blend two layouts. Geometry is the only thing that interpolates, so drawing and
    /// hit testing stay in lockstep on every frame of the transition.
    fn between(a:&Self,b:&Self,t:f64)->Self{
        let mut buttons=a.buttons;
        for (slot,other) in buttons.iter_mut().zip(b.buttons){slot.1=lerp_rect(slot.1,other.1,t);}
        Self{cover:lerp_rect(a.cover,b.cover,t),title:lerp_rect(a.title,b.title,t),artist:lerp_rect(a.artist,b.artist,t),lyric:lerp_rect(a.lyric,b.lyric,t),secondary:lerp_rect(a.secondary,b.secondary,t),status_dot:lerp_rect(a.status_dot,b.status_dot,t),status_text:lerp_rect(a.status_text,b.status_text,t),track:lerp_rect(a.track,b.track,t),elapsed:lerp_rect(a.elapsed,b.elapsed,t),total:lerp_rect(a.total,b.total,t),buttons,height:a.height+(b.height-a.height)*t,progress_hit:lerp_rect(a.progress_hit,b.progress_hit,t),text_hit:lerp_rect(a.text_hit,b.text_hit,t),split:a.split+(b.split-a.split)*t,align:a.align+(b.align-a.align)*t}
    }
}
/// `mix` is the live position: 0 side by side, 1 separated, 2 lyric focus, 3 lyric right.
pub fn music_geometry(width:f64,_layout:MusicLayout,mix:f64,scale:f64,font:f64)->MusicGeometry{
    let extra=(font+8.-24.).max(0.);let line=24.+extra;
    let side=MusicGeometry{
        cover:Rect::new(20.,20.,68.,68.),
        title:Rect::new(108.,18.,width-128.,24.),artist:Rect::new(108.,45.,width-128.,18.),
        lyric:Rect::new(108.,68.,width-128.,line),secondary:Rect::new(108.,94.+extra,width-128.,18.),
        status_dot:Rect::new(20.,101.+extra,4.,4.),status_text:Rect::new(30.,94.+extra,66.,16.),
        track:Rect::new(20.,134.+extra,width-40.,3.),elapsed:Rect::new(20.,144.+extra,70.,14.),total:Rect::new(width-90.,144.+extra,70.,14.),
        buttons:buttons_at(width,scale,185.+extra),height:music_height(0.,font),
        progress_hit:Rect::new(16.,128.+extra,width-32.,14.),text_hit:Rect::new(108.,18.,width-128.,46.),split:0.,align:0.,
    };
    let separated=MusicGeometry{
        cover:Rect::new(20.,18.,68.,68.),
        title:Rect::new(108.,29.5,width-214.,24.),artist:Rect::new(108.,56.5,width-128.,18.),
        lyric:Rect::new(20.,94.,width-40.,line),secondary:Rect::new(20.,118.+extra,width-40.,18.),
        status_dot:Rect::new(width-94.,40.,4.,4.),status_text:Rect::new(width-84.,29.5,64.,24.),
        track:Rect::new(20.,150.+extra,width-40.,3.),elapsed:Rect::new(20.,160.+extra,70.,14.),total:Rect::new(width-90.,160.+extra,70.,14.),
        buttons:buttons_at(width,scale,201.+extra),height:music_height(1.,font),
        progress_hit:Rect::new(16.,144.+extra,width-32.,14.),text_hit:Rect::new(108.,29.5,width-204.,45.),split:1.,align:0.,
    };
    // The live mix drives both directions; the destination enum must not snap a reverse morph.
    let mut focus=separated;
    focus.title=Rect::new(108.,19.,width-214.,24.);focus.artist=Rect::new(108.,46.,width-128.,18.);
    focus.status_text.y=19.;focus.status_dot.y=29.5;focus.align=0.5;focus.lyric.y=92.;focus.secondary.y=116.+extra;focus.text_hit=Rect::new(108.,19.,width-128.,45.);
    // 右韵 shares the focus stack and only moves the anchor: the same header, the same
    // two lines in the same band, with the words pulled to the right edge so the left
    // half of the card is left open.
    let mut right=focus;
    right.align=1.;
    if mix<=1.{
        // Clear the artwork vertically before widening the lyric band to the left.
        // A direct diagonal blend would draw lyrics through the full-size cover.
        let mut stacked=separated;
        stacked.lyric.x=side.lyric.x;stacked.lyric.w=side.lyric.w;
        stacked.secondary.x=side.secondary.x;stacked.secondary.w=side.secondary.w;
        let t=mix.clamp(0.,1.);
        if t<=0.5{MusicGeometry::between(&side,&stacked,t*2.)}
        else{MusicGeometry::between(&stacked,&separated,(t-0.5)*2.)}
    }
    else if mix<=2.{MusicGeometry::between(&separated,&focus,(mix-1.).clamp(0.,1.))}
    else{MusicGeometry::between(&focus,&right,(mix-2.).clamp(0.,1.))}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn every_layout_and_transition_keeps_uniform_artwork_and_text_clear(){
        let overlaps=|a:Rect,b:Rect|a.x<b.x+b.w&&b.x<a.x+a.w&&a.y<b.y+b.h&&b.y<a.y+a.h;
        for width in [400.,440.,592.]{for font in [11.,12.5,17.]{
            for step in 0..=24{
                let mix=step as f64/8.;
                let layout=if mix<=1.{MusicLayout::SideBySide}else if mix<=2.{MusicLayout::LyricFocus}else{MusicLayout::LyricRight};
                let g=music_geometry(width,layout,mix,1.5,font);
                assert_eq!((g.cover.w,g.cover.h),(68.,68.));
                for text in [g.title,g.artist,g.lyric,g.secondary]{assert!(!overlaps(g.cover,text));}
                assert!(g.title.y+g.title.h<=g.artist.y);
                assert!(g.artist.y+g.artist.h<=g.lyric.y);
                assert!(g.lyric.y+g.lyric.h<=g.secondary.y);
                assert!(g.track.y>=g.secondary.y+g.secondary.h+12.);
                assert!(g.lyric.x>=20.&&g.lyric.x+g.lyric.w<=width-20.);
                assert!(g.buttons[3].1.y+g.buttons[3].1.h<=g.height-8.);
            }
        }}
        for (layout,align) in [(MusicLayout::SideBySide,0.),(MusicLayout::Separated,0.),(MusicLayout::LyricFocus,0.5),(MusicLayout::LyricRight,1.)]{
            assert_eq!(music_geometry(400.,layout,layout.position(),1.5,12.5).align,align);
        }
    }
    #[test]fn the_right_layout_only_moves_the_anchor(){let a=music_geometry(400.,MusicLayout::LyricFocus,2.,1.5,12.5);let b=music_geometry(400.,MusicLayout::LyricRight,3.,1.5,12.5);
        assert_eq!(a.lyric,b.lyric);assert_eq!(a.cover,b.cover);assert_eq!(a.buttons,b.buttons);
        assert_eq!((a.align,b.align),(0.5,1.));
    }
}

#[cfg(test)]mod cover_corner_tests{use super::*;
 #[test]fn artwork_uses_half_card_radius_in_both_sizes_and_stays_within_its_bounds(){
  for edge in [40.,56.,68.]{for radius in [8.,16.,30.,48.]{assert_eq!(cover_corner_radius(radius,edge,edge),(radius*0.5).min(edge/2.));}}
  assert_eq!(cover_corner_radius(30.,68.,68.),15.);assert_eq!(cover_corner_radius(30.,40.,40.),15.);
 }
}

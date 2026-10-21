pub use lyric_core::music_layout::{Rect,MusicLayout,music_geometry,music_height,cover_corner_radius};
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
/// Digital timer controls share their painted bounds with pointer hit testing.
pub fn timer_buttons(width:f64)->[(i32,Rect);3]{[(31,Rect::new(24.,156.,66.,32.)),(30,Rect::new(width/2.-52.,156.,104.,32.)),(7,Rect::new(width-90.,156.,66.,32.))]}
/// Shared by drawing and hit testing. Footer keeps a 12 px bottom safe area.
pub fn todo_rows(width:f64,count:usize)->impl Iterator<Item=Rect>{(0..count.min(3)).map(move|i|Rect::new(20.,52.+i as f64*36.,width-40.,32.))}
pub fn todo_footer(width:f64)->Rect{Rect::new(20.,164.,width-40.,24.)}
/// Mini shares readable geometry with pointer hit testing, at half the classic area.
pub fn mini_track(w:f64,h:f64)->Rect{Rect::new(20.,h-57.,w-40.,3.)}
pub fn mini_music_buttons(w:f64,h:f64)->[(i32,Rect);5]{let y=h-43.;[(9,Rect::new(20.,y+4.,28.,28.)),(3,Rect::new(w/2.-58.,y+2.,32.,32.)),(4,Rect::new(w/2.-18.,y,36.,36.)),(5,Rect::new(w/2.+26.,y+2.,32.,32.)),(7,Rect::new(w-48.,y+4.,28.,28.))]}
pub fn mini_timer_buttons(w:f64,h:f64)->[(i32,Rect);3]{let y=h-32.;[(31,Rect::new(20.,y,64.,24.)),(30,Rect::new(w/2.-48.,y,96.,24.)),(7,Rect::new(w-84.,y,64.,24.))]}
pub fn mini_todos(w:f64)->[Rect;2]{[Rect::new(20.,31.,w-40.,20.),Rect::new(20.,54.,w-40.,20.)]}
pub fn mini_primary(w:f64,h:f64,_kind:u8)->Rect{Rect::new(20.,h-34.,w-40.,26.)}
pub fn mini_hit(w:f64,h:f64,panel:Option<&crate::activities::Panel>,x:f64,y:f64)->i32{
    match panel.map(|p|p.kind){
        None=>{let t=mini_track(w,h);if Rect::new(t.x,t.y-5.,t.w,13.).contains(x,y){return 10}for(id,r)in mini_music_buttons(w,h){if r.contains(x,y){return id}}},
        Some(2)=>{for(i,r)in mini_todos(w).iter().enumerate().take(panel.map(|p|p.tasks.len()).unwrap_or(0)){if r.contains(x,y){return 40+i as i32}}if Rect::new(20.,h-24.,w-40.,20.).contains(x,y){return 7}},
        Some(3)=>{if panel.is_some_and(|p|!p.tasks.is_empty())&&mini_primary(w,h,3).contains(x,y){return 50}},
        _=>{for(id,r)in mini_timer_buttons(w,h){if r.contains(x,y){return id}}}
    }0
}
#[cfg(test)]mod mini_tests{use super::*;
    #[test]fn mini_music_targets_stay_clear_of_timeline(){for w in [400.,480.,592.]{for h in [110.,118.,122.]{for(id,r)in mini_music_buttons(w,h){assert!(r.x>=20.&&r.x+r.w<=w-20.&&r.y+r.h<=h-7.);assert!(r.y>mini_track(w,h).y+mini_track(w,h).h+6.);assert_eq!(mini_hit(w,h,None,r.x+r.w/2.,r.y+r.h/2.),id);}}}}
    #[test]fn mini_timer_hits_match_painted_buttons(){let p=crate::activities::Panel{kind:1,..Default::default()};for w in [400.,592.]{for(id,r)in mini_timer_buttons(w,100.){assert!(r.y>=64.&&r.y+r.h<=92.);assert_eq!(mini_hit(w,100.,Some(&p),r.x+r.w/2.,r.y+r.h/2.),id);}}}
}
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
  if mix==1.{assert!(g.lyric.y>=g.cover.y+g.cover.h+8.);assert_eq!((g.title.y+g.artist.y+g.artist.h)/2.,g.cover.y+g.cover.h/2.);assert_eq!(g.artist.y-g.title.y-g.title.h,3.);assert_eq!(g.status_text.y,g.title.y);assert!(g.title.x+g.title.w+12.<=g.status_dot.x);}
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

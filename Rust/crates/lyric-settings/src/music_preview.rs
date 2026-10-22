//! The live preview and thumbnails share the island's geometry, rather than hand-drawn layouts.
use super::*;
use lyric_core::music_layout::{music_geometry,music_layout_mix,MusicGeometry,MusicLayout,Rect};

const LAYOUTS:[MusicLayout;4]=[MusicLayout::SideBySide,MusicLayout::Separated,MusicLayout::LyricFocus,MusicLayout::LyricRight];
fn layout_weights(layout:MusicLayout)->[f64;4]{let mut weights=[0.;4];weights[layout.position()as usize]=1.;weights}

/// Keep the last displayed position when a click interrupts the native transition.
/// A fresh preview opens at its selected layout; polling must not restart the clock.
#[derive(Default)]
pub struct LayoutMotion {
    shown: Option<[f64;4]>,
    from: [f64;4],
    target: Option<MusicLayout>,
    started: Option<Instant>,
}
impl LayoutMotion {
    pub fn value(&self)->[f64;4]{self.shown.unwrap_or([1.,0.,0.,0.])}
    pub fn update(&mut self,target:MusicLayout,animate:bool,now:Instant)->bool{
        let destination=layout_weights(target);
        if self.shown.is_none() || !animate {
            self.shown=Some(destination);self.target=Some(target);self.started=None;
            return false
        }
        if Some(target)!=self.target {
            self.from=self.value();self.target=Some(target);self.started=Some(now);
        }
        if let Some(started)=self.started {
            // Layout IDs are categories, not positions on a route through the other layouts.
            let phase=music_layout_mix(0.,1.,now.duration_since(started).as_secs_f64());
            self.shown=Some(std::array::from_fn(|i|self.from[i]+(destination[i]-self.from[i])*phase));
            if phase==1. {self.shown=Some(destination);self.started=None;}
        }
        self.started.is_some()
    }
}

struct PreviewGeometry {card:MusicGeometry,lyric_anchor:f64,secondary_anchor:f64}
fn preview_geometry(width:f64,weights:[f64;4],font:f64)->PreviewGeometry{
    let side=music_geometry(width,MusicLayout::SideBySide,0.,1.,font);
    let mut expanded=music_geometry(width,MusicLayout::Separated,1.,1.,font);
    let mut total=weights[1];
    for i in 2..4 {
        if weights[i]>0. {
            let next=music_geometry(width,LAYOUTS[i],i as f64,1.,font);
            expanded=MusicGeometry::between(&expanded,&next,weights[i]/(total+weights[i]));
            total+=weights[i];
        }
    }
    // Preserve v2.1.6's artwork clearance: lower the lyric band before widening it.
    // Each pair gets this full transition, regardless of how far apart its IDs are.
    let split=total.clamp(0.,1.);
    let mut stacked=expanded;
    stacked.lyric.x=side.lyric.x;stacked.lyric.w=side.lyric.w;
    stacked.secondary.x=side.secondary.x;stacked.secondary.w=side.secondary.w;
    stacked.align=0.;
    let band=((split-0.5)*2.).clamp(0.,1.);
    let card=if split<=0.5{MusicGeometry::between(&side,&stacked,split*2.)}
        else{MusicGeometry::between(&stacked,&expanded,band)};
    // Interpolate the text anchor itself. Multiplying changing width by changing
    // alignment bends the glyph path left and then back right, even on a smooth clock.
    let lyric_anchor=side.lyric.x+(expanded.lyric.x+expanded.lyric.w*expanded.align-side.lyric.x)*band;
    let secondary_anchor=side.secondary.x+(expanded.secondary.x+expanded.secondary.w*expanded.align-side.secondary.x)*band;
    PreviewGeometry{card,lyric_anchor,secondary_anchor}
}

fn positioned(r:Rect)->Div{div().absolute().left(px(r.x as f32)).top(px(r.y as f32)).w(px(r.w as f32)).h(px(r.h as f32))}
/// Keep the glyph anchor separate from the clipping band while it widens.
/// At rest this is the island's leading, centred or trailing alignment.
fn ink_label(r:Rect,text:String,size:f32,color:Rgba,align:f64,anchor:f64,highlight:Option<(Rgba,f64)>)->AnyElement{
    let line=canvas(|_,_,_|{},move |bounds,_,window,cx|{
        let mut font=t::display_font();if size==16.{font.weight=FontWeight::SEMIBOLD;}
        let run=TextRun{len:text.len(),font,color:color.into(),background_color:None,underline:None,strikethrough:None};
        let shaped=window.text_system().shape_line(text.clone().into(),px(size),&[run.clone()],None);
        let x=bounds.origin.x+px((anchor-r.x)as f32)-shaped.width.min(bounds.size.width)*align.clamp(0.,1.)as f32;
        let origin=point(x,bounds.origin.y);
        window.with_content_mask(Some(ContentMask{bounds}),|window|{
            let _=shaped.paint(origin,px(r.h as f32),TextAlign::Left,None,window,cx);
            if let Some((ink,progress))=highlight{
                let scan=window.text_system().shape_line(text.clone().into(),px(size),&[TextRun{color:ink.into(),..run}],None);
                let width=shaped.width.min(bounds.size.width)*progress.clamp(0.,1.)as f32;
                window.with_content_mask(Some(ContentMask{bounds:Bounds::new(origin,size_fn(width,bounds.size.height))}),|window|{let _=scan.paint(origin,px(r.h as f32),TextAlign::Left,None,window,cx);});
            }
        });
    }).w_full().h_full();
    positioned(r).child(line).into_any_element()
}
fn size_fn(w:Pixels,h:Pixels)->Size<Pixels>{size(w,h)}
fn label(r:Rect,text:String,size:f32,color:Rgba,align:f64)->AnyElement{ink_label(r,text,size,color,align,r.x+r.w*align,None)}
fn clock(s:f64)->String{let n=s.max(0.)as u64;format!("{}:{:02}",n/60,n%60)}

pub fn live(state:&Value,weights:[f64;4])->AnyElement{
    let data=state["musicPreview"].clone();
    let width=data["width"].as_f64().unwrap_or(400.).clamp(400.,592.);
    let title=text(state,"title","暂无播放").to_owned();let artist=text(state,"artist","打开播放器，开始一首音乐").to_owned();
    let playing=state["isPlaying"]==true;
    let font=setting(state,"FontSize").as_f64().unwrap_or(12.5);
    let text_color=t::parse(setting_str(state,"ExpandedTextColor","#F4F6F8"));
    let cover_enabled=flag(state,"CoverBackground",true);
    let accent=if cover_enabled{t::parse(data["accent"].as_str().unwrap_or("#B1E8D1"))}else{t::parse(setting_str(state,"ExpandedComponentColor","#B1E8D1"))};
    let lyric_color=if flag(state,"LyricColorFromCover",true){t::parse(data["accent"].as_str().unwrap_or("#A9E6CD"))}else{t::parse(setting_str(state,"LyricProgressColor","#A9E6CD"))};
    let background=if cover_enabled{t::parse(data["background"].as_str().unwrap_or("#1B1F25"))}else{rgb(0x1b1f25).into()};
    let material=setting_str(state,"IslandMaterial","Acrylic").to_owned();
    let radius=setting(state,"Motion")["Radius"].as_f64().unwrap_or(30.)as f32;
    div().w_full().flex().flex_col().items_center().gap_2().py_3()
        .child(div().w(px(width as f32)).child({
            let preview=preview_geometry(width,weights,font);
            let g=preview.card;
            let h=g.height;
            let mut card=div().relative().w(px(width as f32)).h(px(h as f32)).rounded(px(radius)).overflow_hidden().font(t::display_font())
                .bg(linear_gradient(145.,linear_color_stop(if material=="Glass"{background.opacity(0.88)}else{background},0.),linear_color_stop(if material=="Glass"{rgba(0x0c0f14e0)}else{rgba(0x0c0f14ff)},1.)))
                .border_1().border_color(if material=="Glass"{rgba(0xffffff66)}else{rgba(0xffffff28)})
                .shadow(vec![BoxShadow::new(px(0.),px(8.),rgba(0x00000026).into()).blur_radius(px(24.)).spread_radius(px(-6.))]);
            if material=="Glass"{
                card=card.shadow(vec![BoxShadow::new(px(0.),px(1.),rgba(0xffffff55).into()).inset(),BoxShadow::new(px(0.),px(8.),rgba(0x00000026).into()).blur_radius(px(24.))]);
            }
            let cover=data["coverPath"].as_str().filter(|s|!s.is_empty()).map(PathBuf::from).unwrap_or_else(||std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("Assets/LyricIsland-v2.png"))).unwrap_or_default());
            card=card.child(positioned(g.cover).rounded(px(lyric_core::music_layout::cover_corner_radius(radius as f64,g.cover.w,g.cover.h)as f32)).overflow_hidden().child(img(cover).w_full().h_full().object_fit(ObjectFit::Cover)))
                .child(label(g.title,title.clone(),16.,text_color,0.))
                .child(label(g.artist,artist.clone(),10.,text_color.opacity(0.72),0.))
                .child(ink_label(g.lyric,data["lyric"].as_str().unwrap_or("打开播放器，开始一首音乐").to_owned(),font as f32+2.,text_color,g.align,preview.lyric_anchor,Some((lyric_color,data["highlight"].as_f64().unwrap_or(0.)))))
                .child(ink_label(g.secondary,data["secondary"].as_str().unwrap_or("等待歌词").to_owned(),10.5,text_color.opacity(0.72),g.align,preview.secondary_anchor,None))
                .child(positioned(g.status_dot).rounded_full().bg(accent))
                .child(label(g.status_text,if title=="暂无播放"{"等待播放"}else if playing{"正在播放"}else{"已暂停"}.to_owned(),9.,text_color.opacity(0.72),0.))
                .child(positioned(g.track).rounded_full().bg(rgba(0xffffff20)))
                .child(positioned(Rect::new(g.track.x,g.track.y,g.track.w*data["progress"].as_f64().unwrap_or(0.).clamp(0.,1.),g.track.h)).rounded_full().bg(accent))
                .child(label(g.elapsed,clock(data["elapsed"].as_f64().unwrap_or(0.)),9.,text_color.opacity(0.72),0.))
                .child(positioned(g.total).text_right().child(label(Rect::new(0.,0.,g.total.w,g.total.h),clock(data["duration"].as_f64().unwrap_or(0.)),9.,text_color.opacity(0.72),1.)));
            for(id,r)in g.buttons{
                let icon=match id{2=>"maximize-2",9=>"heart",3=>"skip-back",4=>if playing{"pause"}else{"play"},5=>"skip-forward",6=>"music-2",_=>"settings-2"};
                let luminance=0.2126*accent.r+0.7152*accent.g+0.0722*accent.b;
                let ink=if id==4{if luminance>0.5{rgba(0x101820ff)}else{rgba(0xffffffff)}}else{accent};
                let mut button=positioned(r).rounded_full().flex().items_center().justify_center();
                if id==4{button=button.bg(accent);}
                card=card.child(button.child(if id==6{div().text_color(ink).text_size(px(13.)).child("译").into_any_element()}else{icons::tinted(icon,ink).into_any_element()}));
            }
            card
        }))
        .child(div().text_size(px(11.)).text_color(t::text_dim()).child("实时布局预览 · 跟随当前播放内容"))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{LayoutMotion,LAYOUTS,layout_weights,preview_geometry};
    use lyric_core::music_layout::{music_geometry,MusicLayout};
    use std::time::{Duration,Instant};

    #[test]
    fn every_layout_pair_follows_the_release_timing_even_when_polled() {
        // Samples of the v2.1.6 native 230 ms transition, including its slow start.
        let samples=[(23,0.028),(46,0.104),(115,0.5),(184,0.896),(230,1.)];
        for from in 0..4 {for target in 0..4 {
            if from==target {continue}
            let mut motion=LayoutMotion::default();let now=Instant::now();
            assert!(!motion.update(LAYOUTS[from],true,now));
            assert!(motion.update(LAYOUTS[target],true,now));
            assert_eq!(motion.value(),layout_weights(LAYOUTS[from]));
            for (ms,phase) in samples {
                let at=now+Duration::from_millis(ms);
                let moving=motion.update(LAYOUTS[target],true,at);
                for (i,weight) in motion.value().into_iter().enumerate() {
                    let expected=if i==from{1.-phase}else if i==target{phase}else{0.};
                    assert!((weight-expected).abs()<1e-12);
                }
                assert_eq!(moving,ms<230);
                let shown=motion.value();
                assert_eq!(motion.update(LAYOUTS[target],true,at),moving);
                assert_eq!(motion.value(),shown);
            }
            assert!(!motion.update(LAYOUTS[target],true,now+Duration::from_secs(1)));
            assert_eq!(motion.value(),layout_weights(LAYOUTS[target]));
        }}
    }

    #[test]
    fn interrupted_switches_start_at_the_displayed_position_and_disabled_motion_settles() {
        let mut motion=LayoutMotion::default();let now=Instant::now();
        motion.update(MusicLayout::SideBySide,true,now);motion.update(MusicLayout::LyricRight,true,now);
        let halfway=now+Duration::from_millis(115);
        motion.update(MusicLayout::LyricRight,true,halfway);
        assert_eq!(motion.value(),[0.5,0.,0.,0.5]);
        let before=preview_geometry(400.,motion.value(),12.5);
        motion.update(MusicLayout::SideBySide,true,halfway);
        let after=preview_geometry(400.,motion.value(),12.5);
        assert_eq!(before.card.lyric,after.card.lyric);
        assert_eq!(before.lyric_anchor,after.lyric_anchor);
        motion.update(MusicLayout::SideBySide,true,halfway+Duration::from_millis(115));
        assert_eq!(motion.value(),[0.75,0.,0.,0.25]);
        assert!(!motion.update(MusicLayout::LyricFocus,false,halfway+Duration::from_millis(116)));
        assert_eq!(motion.value(),[0.,0.,1.,0.]);
        assert!(!motion.update(MusicLayout::LyricFocus,true,halfway+Duration::from_millis(117)));
    }

    #[test]
    fn compact_focus_and_right_use_the_full_clock_without_visiting_other_layouts_or_turning_back() {
        for target in [MusicLayout::LyricFocus,MusicLayout::LyricRight] {
            for reverse in [false,true] {for width in [400.,592.] {for text_width in [40.0_f64,160.,220.,320.] {
                let (from,to)=if reverse{(target,MusicLayout::SideBySide)}else{(MusicLayout::SideBySide,target)};
                let start=preview_geometry(width,layout_weights(from),12.5);
                let end=preview_geometry(width,layout_weights(to),12.5);
                let origin=|g:&super::PreviewGeometry|g.lyric_anchor-text_width.min(g.card.lyric.w)*g.card.align;
                let (a,b)=(origin(&start),origin(&end));
                let direction=(b-a).signum();let mut previous=a;
                let mut motion=LayoutMotion::default();let now=Instant::now();
                motion.update(from,true,now);motion.update(to,true,now);
                for ms in 0..=230 {
                    motion.update(to,true,now+Duration::from_millis(ms));
                    let g=preview_geometry(width,motion.value(),12.5);
                    assert_eq!(g.card.cover.w,68.,"artwork must not shrink via Separated");
                    assert!(g.card.lyric.y>=68.&&g.card.lyric.y<=92.);
                    assert!(g.card.lyric.y>=g.card.cover.y+g.card.cover.h||g.card.lyric.x>=g.card.cover.x+g.card.cover.w+20.);
                    let x=origin(&g);
                    assert!(x>=a.min(b)-1e-9&&x<=a.max(b)+1e-9,"lyric detoured: {a} -> {x} -> {b}");
                    assert!((x-previous)*direction>=-1e-9,"lyric reversed direction");
                    previous=x;
                    if !reverse&&ms==46 {assert!((g.card.lyric.y-72.992).abs()<1e-9,"vertical travel was accelerated by the layout ID");}
                }
            }}}
        }
    }

    #[test]
    fn settled_layouts_and_the_existing_compact_separated_path_are_preserved() {
        for layout in LAYOUTS {
            let actual=preview_geometry(400.,layout_weights(layout),12.5);
            let expected=music_geometry(400.,layout,layout.position(),1.,12.5);
            assert_eq!(actual.card.lyric,expected.lyric);assert_eq!(actual.card.cover,expected.cover);
            assert_eq!(actual.card.title,expected.title);assert_eq!(actual.card.buttons,expected.buttons);
            assert_eq!(actual.card.align,expected.align);
            assert_eq!(actual.lyric_anchor,expected.lyric.x+expected.lyric.w*expected.align);
        }
        for step in 0..=100 {
            let p=step as f64/100.;
            let actual=preview_geometry(400.,[1.-p,p,0.,0.],12.5);
            let expected=music_geometry(400.,MusicLayout::Separated,p,1.,12.5);
            assert!((actual.card.lyric.x-expected.lyric.x).abs()<1e-9);
            assert!((actual.card.lyric.y-expected.lyric.y).abs()<1e-9);
            assert_eq!(actual.card.cover,expected.cover);
        }
    }
}

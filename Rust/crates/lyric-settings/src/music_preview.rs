//! The live preview and thumbnails share the island's geometry, rather than hand-drawn layouts.
use super::*;
use lyric_core::music_layout::{music_geometry,music_height,MusicLayout,Rect};

fn positioned(r:Rect)->Div{div().absolute().left(px(r.x as f32)).top(px(r.y as f32)).w(px(r.w as f32)).h(px(r.h as f32))}
fn ink_label(r:Rect,text:String,size:f32,color:Rgba,center:f64,highlight:Option<(Rgba,f64)>)->AnyElement{
    let line=canvas(|_,_,_|{},move |bounds,_,window,cx|{
        let mut font=t::display_font();if size==16.{font.weight=FontWeight::SEMIBOLD;}
        let run=TextRun{len:text.len(),font,color:color.into(),background_color:None,underline:None,strikethrough:None};
        let shaped=window.text_system().shape_line(text.clone().into(),px(size),&[run.clone()],None);
        let x=bounds.origin.x+(bounds.size.width-shaped.width).max(px(0.))*center as f32;
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
fn label(r:Rect,text:String,size:f32,color:Rgba,center:f64)->AnyElement{ink_label(r,text,size,color,center,None)}
fn clock(s:f64)->String{let n=s.max(0.)as u64;format!("{}:{:02}",n/60,n%60)}

pub fn live(state:&Value)->AnyElement{
    let data=state["musicPreview"].clone();
    let width=data["width"].as_f64().unwrap_or(400.).clamp(400.,592.);
    let target=MusicLayout::from_setting(setting_str(state,"MusicExpandedLayout","SideBySide")).position()as f32;
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
        .child(div().w(px(width as f32)).with_spring("live-music-layout",motion::level(motion::standard(),target),move |_,mix|{
            let g=music_geometry(width,MusicLayout::SideBySide,mix as f64,1.,font);
            let h=music_height(mix as f64,font);
            let mut card=div().relative().w(px(width as f32)).h(px(h as f32)).rounded(px(radius)).overflow_hidden().font(t::display_font())
                .bg(linear_gradient(145.,linear_color_stop(background,0.),linear_color_stop(rgb(0x0c0f14),1.)))
                .border_1().border_color(if material=="LiquidGlass"{rgba(0xffffff66)}else{rgba(0xffffff28)})
                .shadow(vec![BoxShadow::new(px(0.),px(8.),rgba(0x00000026).into()).blur_radius(px(24.)).spread_radius(px(-6.))]);
            if material=="LiquidGlass"{
                card=card.shadow(vec![BoxShadow::new(px(0.),px(1.),rgba(0xffffff55).into()).inset(),BoxShadow::new(px(0.),px(8.),rgba(0x00000026).into()).blur_radius(px(24.))]);
            }
            let cover=data["coverPath"].as_str().filter(|s|!s.is_empty()).map(PathBuf::from).unwrap_or_else(||std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("Assets/LyricIsland-v2.png"))).unwrap_or_default());
            card=card.child(positioned(g.cover).rounded(px(10.)).overflow_hidden().child(img(cover).w_full().h_full().object_fit(ObjectFit::Cover)))
                .child(label(g.title,title.clone(),16.,text_color,0.))
                .child(label(g.artist,artist.clone(),10.,text_color.opacity(0.72),0.))
                .child(ink_label(g.lyric,data["lyric"].as_str().unwrap_or("打开播放器，开始一首音乐").to_owned(),font as f32+2.,text_color,g.centered,Some((lyric_color,data["highlight"].as_f64().unwrap_or(0.)))))
                .child(label(g.secondary,data["secondary"].as_str().unwrap_or("等待歌词").to_owned(),10.5,text_color.opacity(0.72),g.centered))
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

pub fn thumbnail(bounds:Bounds<Pixels>,window:&mut Window,layout:u8){
    let mix=match layout{0=>1.,2=>2.,_=>0.};
    let g=music_geometry(400.,MusicLayout::SideBySide,mix,1.,12.5);
    let w=f32::from(bounds.size.width);let h=f32::from(bounds.size.height);let s=(w/400.).min(h/g.height as f32);
    let ox=bounds.origin.x+px((w-400.*s)/2.);let oy=bounds.origin.y+px((h-g.height as f32*s)/2.);
    let mut paint=|r:Rect,rad:f32,c:Rgba|window.paint_quad(quad(Bounds::new(point(ox+px(r.x as f32*s),oy+px(r.y as f32*s)),size(px(r.w as f32*s),px(r.h as f32*s))),px(rad*s),c,px(0.),transparent_black(),BorderStyle::Solid));
    paint(Rect::new(0.,0.,400.,g.height),30.,rgb(0x222829).into());
    paint(g.cover,10.,rgb(0x64747b).into());
    for(r,fraction,height,color)in[(g.title,0.73,10.,0xe0e5e8),(g.artist,0.42,6.,0x8f9ca0),(g.lyric,0.91,9.,0xd8eee7),(g.secondary,0.67,6.,0x8f9ca0)]{
        let width=r.w*fraction;paint(Rect::new(r.x+(r.w-width)*g.centered,r.y+(r.h-height)/2.,width,height),3.,rgb(color).into());
    }
    paint(g.status_dot,2.,rgb(0xa9e6cd).into());paint(Rect::new(g.status_text.x,g.status_text.y+6.,38.,5.),2.,rgb(0x8f9ca0).into());
    paint(g.track,1.5,rgb(0x586367).into());paint(Rect::new(g.track.x,g.track.y,g.track.w*0.35,g.track.h),1.5,rgb(0xb1e8d1).into());
    for r in [g.elapsed,g.total]{paint(Rect::new(r.x,r.y+5.,26.,4.),2.,rgb(0x8f9ca0).into());}
    for(id,r)in g.buttons{let d=if id==4{18.}else{9.};paint(Rect::new(r.x+(r.w-d)/2.,r.y+(r.h-d)/2.,d,d),if id==4{9.}else{2.},rgb(0xc9cdcb).into());}
}

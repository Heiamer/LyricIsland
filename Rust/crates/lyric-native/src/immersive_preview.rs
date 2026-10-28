//! Isolated production-renderer captures for the compact immersive music card.
use std::path::Path;
use serde_json::json;
use crate::{App,layout::{self,Rect,MusicLayout},motion::Frame,renderer::Renderer};

pub unsafe fn run(a:&mut App,out:&Path)->Result<(),Box<dyn std::error::Error>>{
 std::fs::create_dir_all(out)?;a.renderer=None;a.opacity=1.;a.expanded=true;a.frame=Frame::expanded();a.music_mix=4.;a.settings["MusicExpandedLayout"]=json!("Immersive");a.settings["Animation"]=json!(false);
 let font=a.base.join("Assets/Fonts/NotoSansSC.ttf");let cover=std::fs::read(a.base.join("Assets/DemoCover.png"))?;let mut captures=vec![];let mut cases=vec![];
 for width in [400.,592.]{for dpi in [96.,120.,144.,192.]{for size in [11.,14.,20.]{
  let scale=dpi/96.;a.layout=layout::Layout::new(260.);a.layout.host.w=width+48.;a.layout.card=Rect::new(24.,24.,width,layout::music_height(4.,size));a.settings["FontSize"]=json!(size);
  let mut r=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&font)?;r.set_cover(Some(&cover))?;
  for hovered in [false,true]{let mut s=a.scene();s.music_layout=MusicLayout::Immersive;s.music_mix=4.;s.layout_scale=scale;s.controls_opacity=if hovered{1.}else{0.};s.previous_context="晚风吹过了安静的街口";s.next_context="把未说完的故事留给海风";s.title="海边来信 · Letters from the sea";s.artist="点屿 · 音乐与歌词";s.lyric="风把远方的故事轻轻送来，落在此刻的旋律里";s.secondary="A quiet moment with your music";s.has_track=true;s.playing=true;s.play_shape=1.;s.lyric_age_ms=1000.;s.time_ms=0.;s.progress=0.38;s.elapsed=83.;s.duration=236.;s.hover=if hovered{4}else{-1};s.translation_tooltip=false;s.component_count=1;
   r.render(&s,false)?;let file=format!("immersive-{width:.0}-{dpi:.0}-{size:.0}-{}.png",if hovered{"hover"}else{"idle"});r.snapshot(&out.join(&file))?;let g=layout::music_geometry(width,MusicLayout::Immersive,4.,scale,size);let status=r.status_indicator("正在播放",g.status_dot,g.status_text);cases.push(json!({"statusDot":status,"statusText":g.status_text,"lyricWeight":500,"capture":file,"width":width,"dpi":dpi,"font":size,"hovered":hovered,"cardHeight":g.height,"cover":g.cover,"progress":g.progress_hit,"buttons":g.buttons}));captures.push(file);
  }
 }} }
 a.layout=layout::Layout::new(260.);a.settings["FontSize"]=json!(14.);a.layout.card.h=layout::music_height(4.,14.);let mut r=Renderer::new(a.visual_hwnd,a.layout.host.w as u32,a.layout.host.h as u32,96.,&font)?;r.set_cover(Some(&cover))?;
 for(label,title,lyric,translation,previous,next)in [
  ("pure","Sisyphean Hanami（徒见樱）","纯音乐，请欣赏","","",""),
  ("english","Red Moon","I watch the moon","我凝望皎月","Let it run my mood","Can't stop thinking of you"),
  ("empty","暂无播放","打开播放器，开始一首音乐","","",""),
  ("multilingual","君に届け · 🎵","ภาษาไทย · العربية · 한글 · 君に届け","多语言与 Emoji","前一句歌词","下一句歌词")]{
  for hovered in [false,true]{let mut s=a.scene();s.title=title;s.artist=if label=="pure"{"Jacken Chao"}else{"Chill5"};s.lyric=lyric;s.secondary=translation;s.previous_context=previous;s.next_context=next;s.has_track=label!="empty";s.playing=label!="empty";s.play_shape=if s.playing{1.}else{0.};s.controls_opacity=if hovered{1.}else{0.};s.lyric_age_ms=1000.;s.component_count=1;s.hover=-1;s.translation_tooltip=false;s.time_ms=0.;s.progress=0.4;s.elapsed=26.;s.duration=356.;r.render(&s,false)?;let file=format!("immersive-{label}-{}.png",if hovered{"hover"}else{"idle"});r.snapshot(&out.join(&file))?;captures.push(file);}
 }
 for (i,reveal)in [0.,0.25,0.5,0.75,1.,0.75,0.5,0.25,0.].into_iter().enumerate(){let mut s=a.scene();s.title="海边来信";s.artist="点屿";s.lyric="风把远方的故事轻轻送来";s.secondary="A quiet moment with your music";s.previous_context="晚风吹过了安静的街口";s.next_context="把故事留给海风";s.controls_opacity=reveal;s.lyric_age_ms=1000.;s.component_count=1;s.translation_tooltip=false;s.time_ms=0.;r.render(&s,false)?;let file=format!("immersive-reveal-{i:02}.png");r.snapshot(&out.join(&file))?;captures.push(file);}
 for(label,seed)in [("teal",0x1d2e33),("purple",0x7d2b85),("pink",0xf6a9fb)]{a.settings["IslandColorSource"]=json!("Custom");a.settings["IslandDynamicSeedColor"]=json!(format!("#{seed:06X}"));for state in 0..3{let mut s=a.scene();s.title="清晰的播放控制";s.artist="点屿";s.lyric="风把远方的故事轻轻送来";s.has_track=true;s.playing=true;s.play_shape=1.;s.controls_opacity=1.;s.component_count=1;s.hover=if state>0{3}else{-1};s.pressed=if state==2{3}else{-1};s.feedback[3]=match state{1=>0.08,2=>0.12,_=>0.};s.time_ms=0.;r.render(&s,false)?;let file=format!("transport-{label}-{state}.png");r.snapshot(&out.join(&file))?;captures.push(file);}}
 a.renderer=Some(r);std::fs::write(out.join("report.json"),serde_json::to_vec_pretty(&json!({"success":true,"captures":captures,"cases":cases,"scope":"Production GPU rendering: two widths, four DPI scales, three text sizes, idle/hover, multilingual/pure/empty lyrics and reversible reveal samples; no OS input is injected."}))?)?;Ok(())
}

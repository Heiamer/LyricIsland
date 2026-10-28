//! Production-scene matrix. All update values here are explicitly synthetic preview data.
use std::{path::Path,collections::HashMap};
use serde_json::json;
use crate::{App,renderer::Renderer,layout::{self,Rect},motion::Frame,activities::{Panel,Task}};
pub unsafe fn run(a:&mut App,dir:&Path)->Result<(),Box<dyn std::error::Error>>{
 std::fs::create_dir_all(dir)?;a.renderer=None;a.opacity=1.;a.frame=Frame::expanded();a.lyric="晚风轻轻，音乐刚好 · 君に届け".into();
 let font=a.base.join("Assets/Fonts/NotoSansSC.ttf");let mut captures=vec![];
 for dpi in if std::env::var_os("DOTISLE_MUSIC_ONLY").is_some(){vec![]}else{vec![96.,144.,192.]}{for dark in [false,true]{for mini in [false,true]{for w in [400.,592.]{
  a.settings["SettingsTheme"]=json!(if dark{"Dark"}else{"Light"});a.settings["IslandSizeMode"]=json!(if mini{"Mini"}else{"Classic"});a.settings["Animation"]=json!(false);
  a.layout=layout::Layout::new(260.);a.layout.host.w=w+48.;a.layout.card=Rect::new(24.,24.,w,220.);a.layout.set_compact(260.,if mini{14.}else{40.});
  let scale=dpi/96.;let mut r=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&font)?;
  for(name,p)in [("timer",Panel{kind:1,title:"专注计时".into(),value:"24:38".into(),subtitle:"专注 · 第 1 / 4 轮".into(),progress:0.43,running:true,..Default::default()}),("long-clock",Panel{kind:1,title:"长时间秒表".into(),value:"12:34:56.78".into(),subtitle:"正向计时 · 已暂停".into(),..Default::default()}),("todos",Panel{kind:2,title:"待办清单".into(),tasks:(0..3).map(|i|Task{title:format!("{} · 这是一条很长的中英文待办内容 Open source",i+1),detail:"今天 18:30 · 每天重复".into(),..Default::default()}).collect(),..Default::default()})]{
   let mut s=a.scene();s.panel=Some(&p);s.layout.card.h=220.;s.component_count=3;s.layout_scale=scale;
   r.render(&s,false)?;let name=format!("{name}-{}-{}-{w:.0}-{dpi:.0}.png",if dark{"dark"}else{"light"},if mini{"mini"}else{"classic"});r.snapshot(&dir.join(&name))?;captures.push(name);
  }
  for phase in ["checking","available","downloading","unknown","verifying","ready","applying","failed"]{
   a.updates.state=json!({"phase":if phase=="unknown"{"downloading"}else{phase},"version":"演示版本 3.2.0","percent":if phase=="unknown"{None}else{Some(43)},"receivedBytes":4300000,"totalBytes":if phase=="unknown"{None}else{Some(10000000)},"message":"预览状态 · 不会下载或安装"});let p=a.updates.panel();let mut s=a.scene();s.panel=Some(&p);s.layout.card.h=220.;s.component_count=1;s.layout_scale=scale;s.time_ms=800.;r.render(&s,false)?;
   let name=format!("update-{phase}-{}-{}-{w:.0}-{dpi:.0}.png",if dark{"dark"}else{"light"},if mini{"mini"}else{"classic"});r.snapshot(&dir.join(&name))?;captures.push(name);
  }
 }}}}
 // The takeover window reuses this exact material, so the export is captured here too.
 a.layout.host=Rect::new(0.,0.,468.,268.);a.layout.card=Rect::new(24.,24.,420.,220.);a.updates.state=json!({"phase":"applying","version":"3.2.0","message":"正在保存数据并安装"});
 if a.renderer.is_none(){a.renderer=Some(Renderer::new(a.visual_hwnd,468,268,96.,&font)?);}a.export_update_appearance_to(dir.join("appearance"));a.renderer=None;
 // The music card remains the same composition, with explicit font and translation states.
 let mut measured=HashMap::new();a.settings["IslandSizeMode"]=json!("Classic");a.settings["SettingsTheme"]=json!("Dark");a.layout=layout::Layout::new(260.);
 let mut r=Renderer::new(a.visual_hwnd,a.layout.host.w as u32,a.layout.host.h as u32,96.,&font)?;r.set_cover(Some(&std::fs::read(a.base.join("Assets/DemoCover.png"))?))?;
 for family in ["Noto Sans SC"]{a.settings["IslandFontFamily"]=json!(family);r.set_font_family(family);measured.insert(family,r.measure_line("晚风轻轻，音乐刚好 012345",17.)?);
  for(mix,label)in [(0.,"side"),(1.,"separated"),(2.,"focus"),(3.,"right"),(4.,"immersive")]{a.music_mix=mix;a.layout.card.h=layout::music_height(mix,17.);a.settings["FontSize"]=json!(17.);let mut s=a.scene();s.panel=None;s.lyric_age_ms=1000.;s.title="海边来信";s.artist="点屿 · 音乐与歌词";s.lyric="风把远方的故事轻轻送来，落在此刻的旋律里";s.secondary="A quiet moment with your music";s.play_shape=1.;s.playing=true;s.hover=if mix<4.{6}else{-1};s.translation_enabled=true;s.translation_tooltip=mix<4.;s.progress=0.38;s.component_count=1;r.render(&s,false)?;let name=format!("music-{family}-{label}.png");r.snapshot(&dir.join(&name))?;captures.push(name);}
 }
 a.music_mix=0.;a.settings["FontSize"]=json!(12.5);a.layout.card.h=layout::music_height(0.,12.5);
 for (mix,label)in [(0.,"paused"),(0.5,"transition"),(1.,"playing")]{let mut s=a.scene();s.panel=None;s.title="海边来信";s.artist="点屿 · 音乐与歌词";s.lyric="风把远方的故事轻轻送来";s.secondary="A quiet moment with your music";s.playing=mix>0.;s.play_shape=mix;s.animate=true;s.time_ms=600.;s.lyric_age_ms=1000.;s.progress=0.42;s.translation_tooltip=false;r.render(&s,false)?;let name=format!("music-transport-{label}.png");r.snapshot(&dir.join(&name))?;captures.push(name);}
 for(progress,completed,label)in [(0.43,false,"known"),(-1.,false,"unknown"),(1.,true,"ready")]{let mut s=a.scene();s.panel=None;s.frame=Frame::hidden(a.options());s.component_count=2;s.auxiliary_kind=3;s.auxiliary_running=!completed;s.auxiliary_update_progress=progress;s.auxiliary_update_completed=completed;s.animate=true;s.time_ms=700.;s.cover_background=false;s.translation_tooltip=false;s.layout.set_compact(260.,40.);r.render(&s,false)?;let name=format!("satellite-update-{label}.png");r.snapshot(&dir.join(&name))?;captures.push(name);}
 if measured.values().any(|w|!w.is_finite()||*w<=0.){return Err("invalid font metrics".into())}
 std::fs::write(dir.join("report.json"),serde_json::to_vec_pretty(&json!({"success":true,"captures":captures,"fontWidths":measured,"updateData":"synthetic visual preview; no download or install"}))?)?;Ok(())
}

pub unsafe fn benchmark(a:&mut App,out:&Path)->Result<(),Box<dyn std::error::Error>>{
 std::fs::create_dir_all(out)?;a.opacity=1.;a.lyric="晚风轻轻，音乐刚好 · 君に届け".into();a.frame=Frame::expanded();
 let mut r=a.renderer.take().ok_or("renderer unavailable")?;r.set_cover(Some(&std::fs::read(a.base.join("Assets/DemoCover.png"))?))?;
 let mut reports=vec![];
 for name in ["morph","music","timer","update","colour","immersive"]{
  let panel=Panel{kind:if name=="update"{3}else{1},title:"专注与更新预览".into(),value:"24:38".into(),subtitle:"生产渲染管线帧预算检查".into(),progress:0.43,running:true,tasks:vec![Task{title:"暂停".into(),..Default::default()}],..Default::default()};
  let mut submit=vec![];let mut completed=vec![];
  for i in 0..210{let mut s=a.scene();s.panel=if matches!(name,"timer"|"update"){Some(&panel)}else{None};s.animate=true;s.time_ms=i as f64*16.6667;s.lyric_age_ms=1000.;s.cover_age_ms=1000.;s.title="海边来信";s.artist="性能验收 · 合成场景";s.playing=true;s.play_shape=1.;s.progress=i as f64/210.;s.component_count=1;
   if name=="immersive"{s.music_mix=4.;s.layout.card.h=layout::music_height(4.,s.font_size);s.lyric="风把远方的故事轻轻送来，落在此刻的旋律里";}
   if name=="morph"{s.frame.card_reveal=((i%60)as f64/30.-1.).abs();}if name=="colour"{s.dynamic_source="Custom";s.dynamic_custom_seed=if i%60<30{0x536da5}else{0xa45c37};}
   let start=std::time::Instant::now();r.render(&s,false)?;let submitted=start.elapsed().as_secs_f64()*1000.;r.wait_idle();let done=start.elapsed().as_secs_f64()*1000.;if i>=30{submit.push(submitted);completed.push(done);}
  }
  submit.sort_by(f64::total_cmp);completed.sort_by(f64::total_cmp);let stat=|v:&Vec<f64>|json!({"medianMs":v[v.len()/2],"p95Ms":v[v.len()*95/100],"p99Ms":v[v.len()*99/100],"maxMs":v[v.len()-1]});reports.push(json!({"scene":name,"frames":submit.len(),"cpuSubmit":stat(&submit),"gpuCompleted":stat(&completed)}));
 }
 a.renderer=Some(r);std::fs::write(out.join("performance.json"),serde_json::to_vec_pretty(&json!({"scope":"production scene and GPU completion, offscreen synthetic benchmark; excludes OS scheduling and live media","scenes":reports}))?)?;Ok(())
}

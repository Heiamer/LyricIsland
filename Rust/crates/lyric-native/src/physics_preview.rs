//! Reversible production-renderer captures for the three main morphs.
use std::path::Path;use serde_json::json;use crate::{App,motion::{Frame,Options,Transition},renderer::Renderer};
use lyric_core::shared_motion::{Spring,Kind,Speed};
pub unsafe fn run(a:&mut App,out:&Path)->Result<(),Box<dyn std::error::Error>>{
 std::fs::create_dir_all(out)?;a.renderer=None;a.opacity=1.;a.layout=crate::layout::Layout::new(260.);a.layout.card.h=236.;a.expanded=true;a.music_mix=0.;let cover=std::fs::read(a.base.join("Assets/DemoCover.png"))?;let mut captures=vec![];
 let mut r=Renderer::new(a.visual_hwnd,a.layout.host.w as u32,a.layout.host.h as u32,96.,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;r.set_cover(Some(&cover))?;
 for kind in ["shell","play","notification"]{
  let options=Options::default();let mut transition=Transition::new(true,Frame::hidden(options),options,Frame::default(),false);let mut began=0.;let mut shape=Spring::spatial(0.,Speed::Fast);let mut press=Spring::spatial(0.,Speed::Fast);let mut banner=crate::banner::Banner::default();let base=a.layout.capsule;
  if kind=="notification"{let mut notice=crate::notifications::Notice::example(&a.base);notice.title="形状连续，状态清楚".into();notice.body="通知从当前胶囊接管，再自然收回。".into();banner.show(notice,base,20.,a.layout.host.w,80.,0.,true);}
  for frame in 0..96{let now=frame as f64/60.;
   if kind=="shell"&&(frame==24||frame==39){let at=(now-began)*1000.;transition=Transition::new(frame==39,transition.sample(at),options,transition.velocity(at),false);began=now;}
   a.frame=if kind=="shell"{transition.sample((now-began)*1000.)}else if kind=="notification"{Frame::hidden(options)}else{Frame::expanded()};
   shape.target=if frame<30||frame>=52{1.}else{0.};shape.tick(1./60.,false);press.target=if (25..31).contains(&frame)||(48..54).contains(&frame){1.}else{0.};press.tick(1./60.,false);
   if kind=="notification"{if frame==56{banner.dismiss(base,20.,now,true);}banner.tick(base,20.,now,false,true);}
   if frame%2!=0{continue}let mut s=a.scene();s.title="连续形变";s.artist="点屿 · M3E 物理弹簧";s.lyric="每次操作都从当前状态继续";s.lyric_age_ms=1000.;s.cover_age_ms=1000.;s.has_track=true;s.playing=shape.target>0.5;s.play_shape=if kind=="play"{shape.value}else{1.};s.press[4]=if kind=="play"{press.value as f32}else{0.};s.feedback[4]=if kind=="play"{(press.value.clamp(0.,1.)*0.12)as f32}else{0.};s.time_ms=now*1000.;s.controls_opacity=1.;s.glass_refraction=false;
   if kind=="notification"{let(rect,radius,mix)=banner.presentation(now);s.notification=banner.notice.as_ref();s.notification_rect=rect;s.notification_radius=radius;s.notification_mix=mix;}
   r.render(&s,false)?;let file=format!("physics-{kind}-{frame:03}.png");r.snapshot(&out.join(&file))?;captures.push(file);
  }
 }
 std::fs::write(out.join("report.json"),serde_json::to_vec_pretty(&json!({"success":true,"captures":captures,"scheme":"Expressive","scope":"Production renderer; real shared motion solver, in-flight shell reversal, play shape/press feedback and notification takeover. Isolated fixture, no OS input."}))?)?;Ok(())
}

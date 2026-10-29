//! Isolated production-scene regressions; no tray, pointer or player commands.
use std::path::Path;
use serde_json::json;
use crate::{model::{Model,Action,Page},draw::Text,gpu::Gpu,views::{self,ViewState},tutorial::{self,Kind}};
pub fn run(m:&mut Model,text:&mut Text,gpu:&mut Gpu,out:&Path)->Result<Vec<String>,String>{
 let saved=(m.native_state.clone(),m.prefs.clone(),m.page,m.island_tab,m.scroll);let snapshot=m.snapshot.clone();let mut captures=vec![];
 for mini in [false,true]{for floating in [false,true]{for click in [false,true]{for hover in [false,true]{for swipe in [false,true]{for hide in [false,true]{
  m.native_state["settings"]=json!({"IslandSizeMode":if mini{"Mini"}else{"Classic"},"Floating":floating,"ClickAnywhereExpand":click,"HoverExpand":hover,"SwipeActivities":swipe,"SwipeUpHide":hide});let steps=tutorial::steps(m);
  let has=|kind|steps.iter().any(|s|s.kind==kind);if has(Kind::Expand)!=(mini||click||hover)||has(Kind::Move)!=(!mini&&floating)||has(Kind::Swipe)!=swipe||has(Kind::Hide)!=hide||steps.last().map(|s|s.kind)!=Some(Kind::Recover){return Err("adaptive tutorial step selection differs from v2.2.3".into())}
 }}}}}}
 m.native_state["settings"]=json!({"IslandSizeMode":"Classic","Floating":true,"ClickAnywhereExpand":true,"HoverExpand":false,"SwipeActivities":true,"SwipeUpHide":true,"GestureTutorialSeen":false});
 m.tutorial_requested=true;crate::tutorial::sync(m);if m.tutorial.is_none(){return Err("explicit first-use request was lost".into())}m.tutorial.as_mut().unwrap().finishing=true;crate::tutorial::save_failed(m,"isolated failure");if m.tutorial.as_ref().is_some_and(|t|t.finishing||t.error.is_empty()){return Err("tutorial save error cannot be retried".into())}m.dispatch(Action::TutorialStart);if m.tutorial.as_ref().map(|t|t.steps.len())!=Some(5){return Err("complete gesture tutorial was not opened".into())}
 for dark in [false,true]{m.prefs.dark=dark;for (width,height)in [(900.,680.),(1200.,800.)]{for index in 0..5{m.tutorial.as_mut().unwrap().index=index;
  let view=views::render(m,text,width,height,&ViewState{tutorial_time:Some(1.5),..Default::default()});if view.hits.iter().any(|h|!tutorial::allowed(&h.action)||h.rect.x0<0.||h.rect.y0<0.||h.rect.x1>width||h.rect.y1>height)||view.max_scroll!=0.{return Err("tutorial leaks underlying actions or clips its controls".into())}
  if !view.hits.iter().any(|h|matches!(h.action,Action::TutorialNext)&&h.enabled){return Err("tutorial navigation is missing".into())}let name=format!("tutorial-{}-{}-{}.png",if dark{"dark"}else{"light"},width as u32,index);gpu.capture(&view.scene,width as u32,height as u32,&out.join(&name))?;captures.push(name);
 }}}
 m.prefs.dark=false;m.tutorial.as_mut().unwrap().index=4;for stage in [0,1,2]{m.dispatch(Action::TutorialDemo(stage));let view=views::render(m,text,900.,680.,&ViewState{tutorial_time:Some(0.),..Default::default()});if stage==1&&!view.hits.iter().any(|h|matches!(h.action,Action::TutorialDemo(2))){return Err("local tray menu does not restore the simulated island".into())}let name=format!("tutorial-tray-stage-{stage}.png");gpu.capture(&view.scene,900,680,&out.join(&name))?;captures.push(name);}
 m.dispatch(Action::TutorialReplay);if m.tutorial.as_ref().unwrap().demo!=0{return Err("tutorial replay did not reset demo".into())}m.dispatch(Action::TutorialBack);if m.tutorial.as_ref().unwrap().index!=3{return Err("tutorial back failed".into())}
 m.prefs.reduced_motion=true;let view=views::render(m,text,900.,680.,&ViewState::default());gpu.capture(&view.scene,900,680,&out.join("tutorial-reduced-motion.png"))?;captures.push("tutorial-reduced-motion.png".into());m.prefs.reduced_motion=false;
 m.dispatch(Action::TutorialFinish);if m.tutorial.is_some()||m.setting("GestureTutorialSeen")!=true{return Err("tutorial completion did not persist seen state".into())}m.dispatch(Action::TutorialStart);if m.tutorial.as_ref().map(|t|t.index)!=Some(0){return Err("seen tutorial cannot be replayed".into())}m.dispatch(Action::TutorialFinish);
 if m.snapshot!=snapshot{return Err("tutorial changed the real playback snapshot".into())}
 m.page=Page::Island;m.island_tab=0;for dark in [false,true]{m.prefs.dark=dark;for width in [900.,1200.]{m.scroll=0.;let top=views::render(m,text,width,800.,&ViewState::default());let anchor=top.hits.iter().find(|h|matches!(&h.action,Action::NativeEdit("MusicExpandedLayout",_))).map(|h|h.rect.y0);m.scroll=anchor.map(|y|(y-330.).max(0.)).unwrap_or(330.);let view=views::render(m,text,width,800.,&ViewState::default());let names=["SideBySide","Separated","LyricFocus"];for name in names{if !view.hits.iter().any(|h|matches!(&h.action,Action::NativeEdit("MusicExpandedLayout",value)if value==name)){return Err(format!("layout thumbnail is clipped: {name}/{width}"))}}
  let name=format!("music-previews-{}-{}.png",if dark{"dark"}else{"light"},width as u32);gpu.capture(&view.scene,width as u32,800,&out.join(&name))?;captures.push(name);
  if let Some(hit)=view.hits.iter().find(|h|matches!(&h.action,Action::NativeEdit("MusicExpandedLayout",v)if v=="LyricFocus")){let view=views::render(m,text,width,800.,&ViewState{pointer:(hit.rect.center().x,hit.rect.center().y),..Default::default()});let name=format!("music-previews-hover-{}-{}.png",if dark{"dark"}else{"light"},width as u32);gpu.capture(&view.scene,width as u32,800,&out.join(&name))?;captures.push(name);}
 }}
 m.native_state=saved.0;m.prefs=saved.1;m.page=saved.2;m.island_tab=saved.3;m.scroll=saved.4;m.tutorial=None;
 crate::shell::verify_tutorial_inputs(&out.join("tutorial-input"))?;Ok(captures)
}

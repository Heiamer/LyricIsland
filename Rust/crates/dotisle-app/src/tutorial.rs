//! The v2.2.3 gesture guide, rebuilt as local Material 3 Expressive scenes.
use std::time::Instant;
use serde_json::json;
use vello::kurbo::{Rect,Affine};
use crate::{draw::{self,rect,Icon},widgets::Ui,model::{Model,Action},views::ViewState};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Kind{Expand,Swipe,Move,Hide,Recover}
#[derive(Clone,Debug)]pub struct Step{pub kind:Kind,pub title:&'static str,pub detail:&'static str}
pub struct Tutorial{pub steps:Vec<Step>,pub index:usize,pub started:Instant,pub demo:u8,pub finishing:bool,pub error:String}
impl Tutorial{fn new(m:&Model)->Self{Self{steps:steps(m),index:0,started:Instant::now(),demo:0,finishing:false,error:String::new()}}}
fn flag(m:&Model,key:&str,default:bool)->bool{m.setting(key).as_bool().unwrap_or(default)}
pub fn steps(m:&Model)->Vec<Step>{
 let mut list=vec![];let mini=m.setting("IslandSizeMode")=="Mini";
 if mini||flag(m,"ClickAnywhereExpand",true){list.push(Step{kind:Kind::Expand,title:"轻点展开，再点收起",detail:"点击胶囊空白处展开，点击卡片外收起。按钮与进度条保留各自的操作。"});}
 else if flag(m,"HoverExpand",false){list.push(Step{kind:Kind::Expand,title:"悬停展开",detail:"将鼠标移到胶囊上展开，移开后收起。按钮与进度条保留各自的操作。"});}
 if flag(m,"SwipeActivities",true){list.push(Step{kind:Kind::Swipe,title:"左右拖动，切换活动",detail:"按住空白处左右拖动，在音乐、计时器与待办之间切换。有多个活动时可用。"});}
 if !mini&&flag(m,"Floating",false){list.push(Step{kind:Kind::Move,title:"拖动把手，调整位置",detail:"使用展开卡片左下角的移动按钮，再拖到你喜欢的位置。位置会自动保存。"});}
 if flag(m,"SwipeUpHide",true){list.push(Step{kind:Kind::Hide,title:"向上拖动，暂时隐藏",detail:"按住胶囊或卡片空白处向上拖动，松手隐藏。短距离会回弹，不会停止音乐和计时。"});}
 list.push(Step{kind:Kind::Recover,title:"随时找回点屿",detail:"右键任务栏托盘中的点屿图标，选择「显示 / 隐藏灵动岛」。点击演示中的图标和菜单试一试；以后也能从灵动岛设置重看教程。"});list
}
pub fn dispatch(m:&mut Model,a:&Action)->bool{match a{
 Action::TutorialStart=>{m.dialog=None;m.picker=None;m.palette=false;m.tutorial_checked=true;m.tutorial=Some(Tutorial::new(m));},
 Action::TutorialBack=>{if let Some(t)=&mut m.tutorial{if !t.finishing{t.index=t.index.saturating_sub(1);t.demo=0;t.started=Instant::now();}}},
 Action::TutorialNext=>{if let Some(t)=&mut m.tutorial{if t.finishing{return true}if t.index+1<t.steps.len(){t.index+=1;t.demo=0;t.started=Instant::now();}else{finish(m);}}},
 Action::TutorialReplay=>{if let Some(t)=&mut m.tutorial{t.demo=0;t.started=Instant::now();}},
 Action::TutorialDemo(value)=>{if let Some(t)=&mut m.tutorial{t.demo=*value;t.started=Instant::now();}},
 Action::TutorialFinish=>finish(m),_=>return false
 }true}
fn finish(m:&mut Model){if m.tutorial.as_ref().is_some_and(|t|t.finishing){return}if m.remote.is_some(){if let Some(t)=&mut m.tutorial{t.finishing=true;t.error.clear();}m.native_update("GestureTutorialSeen",json!(true));}else{m.native_state["settings"]["GestureTutorialSeen"]=json!(true);m.tutorial=None;}}
pub fn sync(m:&mut Model){
 if m.tutorial.as_ref().is_some_and(|t|t.finishing)&&m.setting("GestureTutorialSeen")==true{m.tutorial=None;}
 if m.tutorial_requested||(!m.tutorial_checked&&std::env::var_os("LYRICISLAND_TEST_PIPE").is_none()){
  let requested=m.tutorial_requested;m.tutorial_requested=false;m.tutorial_checked=true;
  if requested||m.setting("GestureTutorialSeen")!=true{dispatch(m,&Action::TutorialStart);}
 }
}
pub fn save_failed(m:&mut Model,message:&str){if let Some(t)=&mut m.tutorial{if t.finishing{t.finishing=false;t.error=format!("保存失败：{message}。请重试。");}}}
pub fn allowed(a:&Action)->bool{matches!(a,Action::TutorialStart|Action::TutorialBack|Action::TutorialNext|Action::TutorialReplay|Action::TutorialDemo(_)|Action::TutorialFinish)}
fn wrap(ui:&mut Ui,text:&str,x:f64,y:f64,w:f64,size:f32,rgb:u32){let mut line=String::new();let mut row=0;for ch in text.chars(){let next=format!("{line}{ch}");if !line.is_empty()&&ui.text.measure(&next,size,false)>w{ui.label(&line,x,y+row as f64*22.,size,rgb,false,w);row+=1;line.clear();}line.push(ch);}if !line.is_empty(){ui.label(&line,x,y+row as f64*22.,size,rgb,false,w);}}
fn ease(t:f64)->f64{if t>=1.{1.}else{lyric_core::shared_motion::entrance(t.max(0.)*0.35,lyric_core::shared_motion::Kind::Spatial,lyric_core::shared_motion::Speed::Fast).clamp(0.,1.)}}
fn pointer(ui:&mut Ui,x:f64,y:f64,pressed:bool){let s=ui.s;if pressed{draw::circle(ui.scene,x,y,12.,s.secondary_container);draw::circle(ui.scene,x,y,6.,s.primary);}else{draw::circle(ui.scene,x,y,7.,s.primary);draw::circle(ui.scene,x,y,3.,s.on_primary);}}
fn capsule(ui:&mut Ui,r:Rect,kind:Kind,open:f64){let s=ui.s;draw::fill(ui.scene,r,28.-8.*open,s.primary_container);let icon=match kind{Kind::Swipe=>Icon::Focus,_=>Icon::Music};draw::icon(ui.scene,icon,r.x0+18.,r.y0+15.,22.,s.on_primary_container);ui.label(if kind==Kind::Swipe{"专注时刻"}else{"让好音乐，陪你度过好时光"},r.x0+52.,r.y0+16.,13.,s.on_primary_container,true,r.width()-70.);if open>0.01{ui.scene.push_layer(vello::peniko::Mix::Normal,open as f32,Affine::IDENTITY,&r);ui.label("海边来信 · 点屿",r.x0+22.,r.y0+51.,11.,s.on_primary_container,false,r.width()-44.);let cy=r.y1-34.;draw::icon(ui.scene,Icon::Prev,r.center().x-62.,cy-10.,20.,s.on_primary_container);draw::fill(ui.scene,rect(r.center().x-22.,cy-19.,44.,38.),14.,s.primary);draw::icon(ui.scene,Icon::Pause,r.center().x-11.,cy-11.,22.,s.on_primary);draw::icon(ui.scene,Icon::Next,r.center().x+42.,cy-10.,20.,s.on_primary_container);ui.scene.pop_layer();}}
pub fn overlay(ui:&mut Ui,m:&Model,width:f64,height:f64,state:&ViewState){let Some(t)=&m.tutorial else{return};let s=ui.s;
 ui.hits.clear();ui.motion_targets.clear();ui.focus=state.focus;ui.pressed=state.pressed;
 ui.scene.fill(vello::peniko::Fill::NonZero,Affine::IDENTITY,draw::alpha(0x000000,0.42),None,&rect(0.,0.,width,height));
 let w=(width-64.).min(680.);let h=548.;let r=rect((width-w)/2.,((height-h)/2.+18.).max(64.),w,h);draw::fill(ui.scene,r,36.,s.surface);draw::outline(ui.scene,r,36.,s.outline_variant,1.);
 let step=&t.steps[t.index];ui.label("认识你的灵动岛",r.x0+30.,r.y0+24.,13.,s.primary,true,w-190.);ui.label(&format!("{:02} / {:02}",t.index+1,t.steps.len()),r.x0+30.,r.y0+50.,11.,s.on_surface_variant,false,90.);
 ui.button(rect(r.x1-104.,r.y0+21.,74.,36.),"跳过",None,Action::TutorialFinish,true,!t.finishing);
 ui.label(step.title,r.x0+30.,r.y0+79.,26.,s.on_surface,true,w-60.);
 let demo=rect(r.x0+24.,r.y0+128.,w-48.,218.);draw::fill(ui.scene,demo,26.,s.container);ui.scene.push_clip_layer(Affine::IDENTITY,&demo.to_rounded_rect(26.));
 let elapsed=state.tutorial_time.unwrap_or_else(||t.started.elapsed().as_secs_f64());let period=if step.kind==Kind::Recover{6.}else{3.2};let p=if m.prefs.reduced_motion{0.6}else{(elapsed/period)%1.};let cycle=ease((p-0.12)/0.24)*(1.-ease((p-0.7)/0.2));let cx=demo.center().x;
 let hint=match step.kind{Kind::Expand=>if step.title=="悬停展开"{"移入展开 · 移开收起"}else{"点击卡片试试 · 点击外侧收起"},Kind::Swipe=>"多个活动时 · 从空白处左右拖动",Kind::Move=>"从左下角的移动把手开始",Kind::Hide=>"向上拖动 · 短距离会回弹",Kind::Recover=>"点击模拟托盘图标，再选择显示"};ui.label(hint,demo.x0+18.,demo.y0+15.,11.,s.on_surface_variant,false,demo.width()-36.);
 match step.kind{
  Kind::Expand=>{let open=if t.demo==1{1.}else if t.demo==2{0.}else{cycle};let r=rect(cx-140.,demo.y0+64.-12.*open,280.,54.+84.*open);capsule(ui,r,step.kind,open);ui.hit(demo,"收起演示卡片",Action::TutorialDemo(2),true);ui.hit(r,"展开或收起演示卡片",Action::TutorialDemo(if open>0.5{2}else{1}),true);if t.demo==0{pointer(ui,cx+68.,demo.y0+102.,open>0.2&&open<0.9);}},
  Kind::Swipe=>{let dx=-62.*cycle;let card=rect(cx-140.+dx,demo.y0+70.,280.,58.);capsule(ui,card,if cycle>0.5{Kind::Swipe}else{Kind::Expand},0.);draw::icon(ui.scene,Icon::Back,demo.x0+22.,demo.y0+87.,20.,s.on_surface_variant);draw::icon(ui.scene,Icon::Arrow,demo.x1-42.,demo.y0+87.,20.,s.on_surface_variant);let active=usize::from(cycle>0.5);let mut indicator_x=cx-23.;for i in 0..3{let width=if i==active{18.}else{6.};draw::fill(ui.scene,rect(indicator_x,demo.y0+153.,width,6.),3.,if i==active{s.primary}else{s.outline_variant});indicator_x+=width+8.;}pointer(ui,cx+65.+dx,demo.y0+116.,true);},
  Kind::Move=>{let r=rect(cx-140.+48.*cycle,demo.y0+46.+22.*cycle,280.,132.);capsule(ui,r,step.kind,1.);draw::icon(ui.scene,Icon::Move,r.x0+15.,r.y1-38.,22.,s.primary);pointer(ui,r.x0+26.,r.y1-25.,true);},
  Kind::Hide=>{let amount=ease((p-0.22)/0.34);let alpha=if m.prefs.reduced_motion{1.}else{1.-ease((p-0.5)/0.14)};let r=rect(cx-140.,demo.y0+91.-amount*70.,280.,54.);ui.scene.push_layer(vello::peniko::Mix::Normal,alpha as f32,Affine::IDENTITY,&demo);capsule(ui,r,step.kind,0.);ui.scene.pop_layer();if m.prefs.reduced_motion{draw::icon(ui.scene,Icon::Arrow,cx-10.,demo.y0+58.,20.,s.primary);}else{pointer(ui,cx+65.,demo.y0+124.-amount*70.,p<0.55);}ui.center(if alpha<0.1{"已暂时隐藏 · 音乐与计时继续运行"}else{"↑ 从空白处向上拖动"},rect(demo.x0+20.,demo.y1-44.,demo.width()-40.,26.),12.,s.on_surface_variant,false);},
  Kind::Recover=>{let stage=if t.demo!=0{t.demo}else if p<0.28{0}else if p<0.58{1}else{2};let tray=rect(demo.x0+12.,demo.y1-43.,demo.width()-24.,31.);draw::fill(ui.scene,tray,12.,s.high);ui.label("任务栏",tray.x0+14.,tray.y0+8.,11.,s.on_surface_variant,false,100.);let icon=rect(tray.x1-60.,tray.y0+2.,44.,27.);draw::fill(ui.scene,icon,10.,if stage==1{s.secondary_container}else{s.high});draw::icon(ui.scene,Icon::Island,icon.center().x-11.,icon.center().y-11.,22.,s.primary);ui.hit(icon,"模拟右键点屿托盘图标",Action::TutorialDemo(1),true);
   if stage==1{let menu=rect(demo.x1-262.,tray.y0-88.,240.,78.);draw::fill(ui.scene,menu,18.,s.surface);draw::outline(ui.scene,menu,18.,s.outline_variant,1.);ui.label("点屿 · 托盘菜单",menu.x0+14.,menu.y0+10.,10.,s.on_surface_variant,false,200.);ui.button(rect(menu.x0+7.,menu.y0+32.,226.,38.),"显示 / 隐藏灵动岛",Some(Icon::Island),Action::TutorialDemo(2),true,true);}
   if stage==2{capsule(ui,rect(cx-140.,demo.y0+68.,280.,54.),Kind::Expand,0.);draw::icon(ui.scene,Icon::Check,cx-10.,demo.y0+135.,20.,s.primary);}
   if stage==0{ui.center("右键点屿图标 → 选择显示",rect(demo.x0+20.,demo.y0+83.,demo.width()-40.,30.),15.,s.on_surface,true);}
  }
 }
 ui.scene.pop_layer();wrap(ui,step.detail,r.x0+30.,r.y0+365.,w-60.,14.,s.on_surface_variant);
 let sw=(w-60.-(t.steps.len()-1)as f64*7.)/t.steps.len()as f64;for i in 0..t.steps.len(){draw::fill(ui.scene,rect(r.x0+30.+i as f64*(sw+7.),r.y0+445.,sw,5.),2.5,if i<=t.index{s.primary}else{s.outline_variant});}
 if !t.error.is_empty(){ui.label(&t.error,r.x0+30.,r.y0+426.,12.,s.error,false,w-60.);}
 ui.button(rect(r.x0+30.,r.y1-70.,116.,44.),"上一步",Some(Icon::Back),Action::TutorialBack,true,t.index>0&&!t.finishing);
 ui.button(rect(r.x0+160.,r.y1-70.,96.,44.),"重播",Some(Icon::Reset),Action::TutorialReplay,true,!t.finishing);
 ui.button(rect(r.x1-170.,r.y1-70.,140.,44.),if t.finishing{"正在保存…"}else if t.index+1==t.steps.len(){"开始使用"}else{"下一步"},Some(Icon::Arrow),Action::TutorialNext,false,!t.finishing);
}

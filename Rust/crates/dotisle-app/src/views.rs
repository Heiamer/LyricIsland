//! Material 3 Expressive settings rendered entirely with Vello.
use vello::{Scene,kurbo::{Rect,Affine,BezPath,Stroke},peniko::{Fill}};
use crate::{draw::{self,Icon,Text,rect},widgets::{Ui,Hit},model::{Model,Page,Action,DialogKind}};
pub const TITLEBAR_H:f64=56.;
#[derive(Default)]pub struct ViewState{pub tutorial_time:Option<f64>,pub selection_values:std::collections::HashMap<String,f64>,pub press_layers:Vec<f64>,pub rail_target:Option<f64>,pub switches:std::collections::HashMap<String,f64>,pub color_scheme:Option<dotisle_domain::monet::Scheme>,pub state_layers:Vec<f32>,pub page_enter:Option<f64>,pub media_position:Option<f64>,pub pointer:(f64,f64),pub focus:Option<usize>,pub pressed:Option<usize>,pub rail_y:f64,pub satellite:f64,pub cloud_open:f64,pub time:f64,pub swipe:f64,pub maximized:bool}
pub struct View{pub motion_targets:Vec<(String,f64)>,pub scene:Scene,pub hits:Vec<Hit>,pub max_scroll:f64}
fn page_icon(page:Page)->Icon{match page{Page::Home=>Icon::Home,Page::Music=>Icon::Music,Page::Focus=>Icon::Focus,Page::Todos=>Icon::Todo,Page::Island=>Icon::Island,Page::Settings=>Icon::Settings}}
fn caption_button(ui:&mut Ui,x:f64,kind:Icon,label:&'static str,action:Action,close:bool){
    let s=ui.s;let r=rect(x,0.,46.,TITLEBAR_H);let hovered=r.contains(ui.pointer);ui.hit(r,label,action,true);
    if hovered{draw::fill(ui.scene,r,0.,if close{0xC42B1C}else{s.high});}
    // Window chrome uses one shared 12px optical box and a 1.4px stroke.
    // Generic symbol viewBoxes have different ink bounds and a low minimize baseline.
    let(cx,cy)=(x+23.,TITLEBAR_H/2.);let mut path=BezPath::new();
    match kind{
      Icon::Minimize=>{path.move_to((cx-6.,cy));path.line_to((cx+6.,cy));},
      Icon::Maximize=>{path.move_to((cx-6.,cy-6.));path.line_to((cx+6.,cy-6.));path.line_to((cx+6.,cy+6.));path.line_to((cx-6.,cy+6.));path.close_path();},
      Icon::Restore=>{path.move_to((cx-3.,cy-3.));path.line_to((cx-3.,cy-6.));path.line_to((cx+6.,cy-6.));path.line_to((cx+6.,cy+3.));path.line_to((cx+3.,cy+3.));path.move_to((cx-6.,cy-3.));path.line_to((cx+3.,cy-3.));path.line_to((cx+3.,cy+6.));path.line_to((cx-6.,cy+6.));path.close_path();},
      _=>{path.move_to((cx-6.,cy-6.));path.line_to((cx+6.,cy+6.));path.move_to((cx+6.,cy-6.));path.line_to((cx-6.,cy+6.));}
    }
    ui.scene.stroke(&Stroke::new(1.4),Affine::IDENTITY,draw::color(if hovered&&close{0xFFFFFF}else{s.on_surface_variant}),None,&path);
}
pub fn render(model:&Model,text:&mut Text,width:f64,height:f64,state:&ViewState)->View{
    let mut scene=Scene::new();let s=state.color_scheme.unwrap_or_else(||model.scheme());let window=rect(0.,0.,width,height);
    let mut ui=Ui{selection_values:state.selection_values.clone(),motion_targets:vec![],press_layers:state.press_layers.clone(),switches:state.switches.clone(),scene:&mut scene,text,s,hits:vec![],state_layers:state.state_layers.clone(),pointer:state.pointer,focus:state.focus,pressed:state.pressed,clip:window};
    draw::fill(ui.scene,window,0.,s.surface);draw::fill(ui.scene,rect(0.,TITLEBAR_H,96.,height-TITLEBAR_H),0.,s.container);
    draw::fill(ui.scene,rect(16.,14.,28.,28.),8.,s.primary);draw::icon(ui.scene,Icon::Island,22.,20.,16.,s.on_primary);
    ui.label("点屿",52.,16.,16.,s.on_surface,true,44.);ui.label("/",94.,19.,13.,s.outline,false,12.);ui.label(model.page.label(),108.,19.,13.,s.on_surface_variant,false,86.);
    if model.demo{ui.label("设计预览 · 示例歌曲",196.,20.,11.,s.on_surface_variant,false,(width-600.).max(40.));}
    ui.icon_button(rect(width-378.,10.,36.,36.),Icon::Search,"搜索与跳转 · Ctrl+K",Action::Palette,false,true);
    ui.icon_button(rect(width-334.,10.,36.,36.),if model.prefs.dark{Icon::Sun}else{Icon::Moon},"切换明暗主题",Action::Pref("dark"),false,true);
    ui.button(rect(width-290.,10.,140.,36.),if model.prefs.island{"收起灵动岛"}else{"开启灵动岛"},Some(Icon::Island),Action::Pref("island"),true,true);
    caption_button(&mut ui,width-138.,Icon::Minimize,"最小化窗口",Action::WindowMinimize,false);
    caption_button(&mut ui,width-92.,if state.maximized{Icon::Restore}else{Icon::Maximize},if state.maximized{"还原窗口"}else{"最大化窗口"},Action::WindowMaximize,false);
    caption_button(&mut ui,width-46.,Icon::Close,"关闭窗口",Action::WindowClose,true);
    let rail_y=if state.rail_y>=TITLEBAR_H{state.rail_y}else{112.+Page::ALL.iter().position(|p|*p==model.page).unwrap_or(0)as f64*82.};
    let stretch=if model.prefs.reduced_motion{0.}else{state.rail_target.map(|target|((target-rail_y).abs()*0.12).min(14.)).unwrap_or(0.)};draw::fill(ui.scene,rect(18.+stretch*0.15,rail_y-stretch/2.,60.-stretch*0.3,36.+stretch),18.,s.secondary_container);
    for(i,page)in Page::ALL.iter().enumerate(){let y=112.+i as f64*82.;let r=rect(8.,y-8.,80.,72.);let index=ui.hit(r,page.label(),Action::Navigate(*page),true);ui.state(rect(18.,y,60.,36.),18.,index,s.on_surface);draw::icon(ui.scene,page_icon(*page),36.,y+6.,24.,if model.page==*page{s.on_secondary_container}else{s.on_surface_variant});ui.center(page.label(),rect(8.,y+38.,80.,26.),12.,s.on_surface_variant,model.page==*page);}
    ui.center("V3",rect(8.,height-44.,80.,26.),12.,s.on_surface_variant,true);
    let x=132.;let w=width-x-36.;
    ui.label(model.page.label(),x,101.,40.,s.on_surface,true,w-220.);ui.label(model.page.description(),x,159.,15.,s.on_surface_variant,false,w-220.);
    if model.page==Page::Todos{ui.button(rect(width-198.,118.,162.,52.),"新建待办",Some(Icon::Plus),Action::EditTodo(None),false,true);}
    let clip=Rect::new(x-4.,203.,width-24.,height-20.);ui.clip(clip);
    let enter=if model.prefs.reduced_motion{1.}else{state.page_enter.unwrap_or(1.)};let e=enter.clamp(0.,1.);ui.scene.push_layer(vello::peniko::Mix::Normal,(0.82+0.18*e)as f32,Affine::IDENTITY,&clip);let y=212.-model.scroll+12.*(1.-e);
    let bottom=match model.page{Page::Home=>home(&mut ui,model,x,y,w,state),Page::Music=>music(&mut ui,model,x,y,w,state),Page::Focus=>focus(&mut ui,model,x,y,w),Page::Todos=>todos(&mut ui,model,x,y,w),Page::Island=>crate::island_settings::render(&mut ui,model,x,y,w,state),Page::Settings=>settings(&mut ui,model,x,y,w)};
    ui.scene.pop_layer();ui.end_clip(window);
    let max_scroll=(bottom+model.scroll-(height-28.)).max(0.);
    if max_scroll>0.{let track=height-240.;let thumb=(track*track/(track+max_scroll)).max(40.);let pos=(model.scroll/max_scroll).clamp(0.,1.)*(track-thumb);draw::fill(ui.scene,rect(width-14.,212.+pos,4.,thumb),2.,s.outline);}
    if let Some((message,_))=&model.toast{let tw=(ui.text.measure(message,13.,false)+if model.undo.is_some(){136.}else{44.}).clamp(160.,w.min(480.));let r=rect(x+w-tw,height-76.,tw,48.);draw::fill(ui.scene,r,16.,s.high);draw::outline(ui.scene,r,16.,s.outline_variant,1.);ui.label(message,r.x0+18.,r.y0+13.,13.,s.on_surface,false,tw-if model.undo.is_some(){120.}else{36.});if model.undo.is_some(){let r=rect(r.x1-94.,r.y0+7.,84.,44.);let i=ui.hit(r,"撤销移除",Action::Undo,true);ui.state(r,20.,i,s.primary);ui.center("撤销",r,14.,s.primary,true);}}
    if model.palette{palette(&mut ui,model,width,height);}
    if model.dialog.is_some(){dialog(&mut ui,model,width,height);}
    if model.picker.is_some(){crate::picker::panel(&mut ui,model,width,height);}
    if model.tutorial.is_some(){crate::tutorial::overlay(&mut ui,model,width,height,state);}
    let max_scroll=if model.tutorial.is_some(){0.}else{max_scroll};let motion_targets=ui.motion_targets;let hits=ui.hits;View{motion_targets,scene,hits,max_scroll}
}
fn record(ui:&mut Ui,m:&Model,r:Rect,_flower:bool){if let Some(a)=&m.artwork{draw::image(ui.scene,&a.image,r,15.);}else{draw::fill(ui.scene,r,15.,ui.s.secondary_container);let size=r.width().min(r.height())*0.32;draw::icon(ui.scene,Icon::Music,r.center().x-size/2.,r.center().y-size/2.,size,ui.s.on_surface_variant);}}
fn player_controls(ui:&mut Ui,m:&Model,x:f64,y:f64,large:bool){
    let s=ui.s;let size=if large{64.}else{52.};let pad=(size-44.)/2.;let colors=dotisle_domain::music_controls::skip_colors(s.primary_container,s.on_primary_container);let disc=colors.background;
    // Pair skip ink with its actual disc, rather than the unrelated page surface role.
    for r in [rect(x-2.,y+pad-2.,48.,48.),rect(x+size+66.,y+pad-2.,48.,48.)]{draw::fill(ui.scene,r,24.,disc);}
    ui.s.on_surface_variant=colors.foreground;
    ui.icon_button(rect(x,y+pad,44.,44.),Icon::Prev,"上一首",Action::Media("previous"),false,m.track());
    ui.s=s;
    ui.icon_button(rect(x+56.,y,size,size),if m.playing(){Icon::Pause}else{Icon::Play},if m.playing(){"暂停播放"}else{"开始播放"},Action::Media("toggle"),true,m.track());
    ui.s.on_surface_variant=colors.foreground;
    ui.icon_button(rect(x+size+68.,y+pad,44.,44.),Icon::Next,"下一首",Action::Media("next"),false,m.track());
    ui.s=s;
}
fn progress(ui:&mut Ui,m:&Model,r:Rect,state:&ViewState){let duration=m.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.);let pos=state.media_position.unwrap_or_else(||m.snapshot["positionSeconds"].as_f64().unwrap_or(0.));ui.slider(r,if duration>0.{pos/duration}else{0.},m.track()&&m.snapshot["canSeek"].as_bool().unwrap_or(false),if m.playing()&&!m.prefs.reduced_motion{state.time*2.}else{0.});ui.label(&dotisle_domain::timers::format_time(pos.max(0.)as u64),r.x0,r.y1+2.,11.,ui.s.on_surface_variant,false,80.);let total=dotisle_domain::timers::format_time(duration.max(0.)as u64);ui.label(&total,r.x1-42.,r.y1+2.,11.,ui.s.on_surface_variant,false,60.);}
fn home(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64,state:&ViewState)->f64{
    let s=ui.s;let gap=20.;let left=(w-gap)*0.63;let right=w-left-gap;let rx=x+left+gap;
    ui.panel(rect(x,y,left,300.),s.primary_container);ui.label(if m.track(){"此刻的旋律"}else{"音乐，随时相伴"},x+26.,y+24.,13.,s.on_primary_container,true,left-52.);
    record(ui,m,rect(x+left-190.,y+62.,160.,160.),true);
    ui.label(m.title(),x+26.,y+65.,30.,s.on_primary_container,true,left-220.);ui.label(m.artist(),x+26.,y+111.,14.,s.on_primary_container,false,left-220.);
    ui.label(m.current_lyric(),x+26.,y+151.,12.,s.on_primary_container,false,left-220.);
    player_controls(ui,m,x+18.,y+191.,false);progress(ui,m,rect(x+26.,y+251.,left-52.,24.),state);
    ui.panel(rect(rx,y,right,300.),s.tertiary_container);draw::icon(ui.scene,Icon::Focus,rx+24.,y+24.,24.,s.on_tertiary_container);ui.label("留给自己的时间",rx+58.,y+26.,14.,s.on_tertiary_container,true,right-82.);
    let clock=m.current_timer().map(|t|t.clock()).unwrap_or("25:00".into());ui.label(&clock,rx+24.,y+78.,52.,s.on_tertiary_container,true,right-48.);
    let subtitle=m.current_timer().map(|t|t.subtitle()).unwrap_or("25 分钟专注 · 5 分钟休息".into());ui.label(&subtitle,rx+26.,y+158.,13.,s.on_tertiary_container,false,right-52.);
    ui.button(rect(rx+24.,y+220.,right-48.,52.),if m.current_timer().is_some(){"回到专注"}else{"开始专注"},Some(Icon::Arrow),Action::Navigate(Page::Focus),false,true);
    let y=y+320.;ui.panel(rect(x,y,left,216.),s.container);ui.label("接下来",x+24.,y+22.,22.,s.on_surface,true,150.);ui.label(&format!("{} 件待办",m.pending()),x+left-110.,y+30.,13.,s.on_surface_variant,false,90.);
    let now=dotisle_domain::todos::Now::local();let items=m.todos.items.iter().filter(|t|!t.done(&now)).take(2).collect::<Vec<_>>();
    if items.is_empty(){draw::icon(ui.scene,Icon::Leaf,x+28.,y+86.,32.,s.primary);ui.label("清爽的一天",x+76.,y+83.,18.,s.on_surface,true,left-100.);ui.label("记录一件小事，让心里少一点牵挂。",x+76.,y+114.,13.,s.on_surface_variant,false,left-100.);ui.button(rect(x+24.,y+158.,144.,40.),"添加待办",Some(Icon::Plus),Action::EditTodo(None),true,true);}
    else{for(i,t)in items.iter().enumerate(){let ty=y+68.+i as f64*60.;ui.icon_button(rect(x+20.,ty,44.,44.),Icon::Check,"完成待办",Action::ToggleTodo(t.id.clone()),false,true);ui.label(&t.title,x+76.,ty+2.,15.,s.on_surface,true,left-100.);ui.label(&format!("{} · {}",t.date,t.time),x+76.,ty+27.,12.,s.on_surface_variant,false,left-100.);}}
    ui.panel(rect(rx,y,right,216.),s.secondary_container);draw::icon(ui.scene,Icon::Island,rx+24.,y+24.,26.,s.on_secondary_container);ui.label("你的灵动岛",rx+24.,y+67.,23.,s.on_secondary_container,true,right-48.);ui.label("音乐、专注与小事，轻盈相伴。",rx+24.,y+108.,13.,s.on_secondary_container,false,right-48.);ui.button(rect(rx+24.,y+156.,right-48.,40.),if m.prefs.island{"灵动岛已开启"}else{"开启灵动岛"},None,Action::Pref("island"),true,true);
    y+216.
}
fn music(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64,state:&ViewState)->f64{
    let s=ui.s;ui.panel(rect(x,y,w,300.),s.primary_container);record(ui,m,rect(x+24.,y+24.,252.,252.),false);let px=x+310.;let avail=w-340.;
    ui.label(if m.demo{"音乐预览"}else if m.track(){"正在播放"}else{"等待播放器"},px,y+25.,12.,s.on_primary_container,true,avail);
    ui.label(m.title(),px,y+60.,36.,s.on_primary_container,true,avail);ui.label(m.artist(),px,y+113.,16.,s.on_primary_container,false,avail);
    progress(ui,m,rect(px,y+164.,avail,28.),state);player_controls(ui,m,px+avail/2.-88.,y+218.,true);
    let mut y=y+320.;
    if m.track()&&m.snapshot["canSeek"].as_bool()==Some(false){
        let status=m.snapshot["seekStatus"].as_str().unwrap_or("播放器尚未提供静默进度通道");
        ui.panel(rect(x,y,w,64.),s.container);ui.label("播放进度",x+24.,y+12.,15.,s.on_surface,true,200.);ui.label(status,x+24.,y+38.,12.,s.on_surface_variant,false,w-280.);
        if status=="播放器尚未提供静默进度通道"&&m.snapshot["sourceId"].as_str().is_some_and(|id|id.eq_ignore_ascii_case("cloudmusic.exe")){ui.button(rect(x+w-216.,y+10.,192.,44.),"重新连接进度",None,Action::NativeCommand("neteaseEnableSilent",serde_json::Value::Null),true,!m.demo);}
        y+=88.;
    }
    ui.panel(rect(x,y,w,194.),s.container);ui.label("歌词",x+26.,y+22.,13.,s.on_surface_variant,true,100.);
    let line=m.snapshot["frame"]["currentLine"].as_str().filter(|s|!s.is_empty()).unwrap_or("把耳朵交给音乐，把此刻留给自己。");
    ui.label(line,x+26.,y+56.,28.,s.on_surface,true,w-52.);
    if m.prefs.word_highlight&&m.track(){let p=m.snapshot["frame"]["wordScanProgress"].as_f64().unwrap_or(0.).clamp(0.,1.);ui.scene.push_clip_layer(Affine::IDENTITY,&rect(x+26.,y+54.,(w-52.)*p,45.));ui.label(line,x+26.,y+56.,28.,s.primary,true,w-52.);ui.scene.pop_layer();}
    if m.prefs.translation{let tr=m.snapshot["frame"]["currentTranslation"].as_str().unwrap_or("");ui.label(tr,x+28.,y+105.,14.,s.on_surface_variant,false,w-56.);}
    ui.label(m.snapshot["frame"]["nextLine"].as_str().unwrap_or("歌词会在歌曲播放后自动匹配"),x+28.,y+147.,18.,s.on_surface_variant,false,w-56.);
    let y=y+214.;ui.panel(rect(x,y,w,72.),s.high);ui.label("歌词同步",x+24.,y+13.,15.,s.on_surface,true,200.);ui.label("向左提前，向右延后 · 每次 100 ms",x+24.,y+39.,12.,s.on_surface_variant,false,w-300.);
    ui.icon_button(rect(x+w-238.,y+14.,44.,44.),Icon::Back,"歌词提前 100 毫秒",Action::Offset(-100),false,true);ui.center(&format!("{:+} ms",m.prefs.offset_ms),rect(x+w-187.,y+14.,120.,44.),15.,s.primary,true);ui.icon_button(rect(x+w-60.,y+14.,44.,44.),Icon::Arrow,"歌词延后 100 毫秒",Action::Offset(100),false,true);y+72.
}
fn focus(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{crate::timer_surface::render(ui,m,x,y,w)}
fn todos(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
    let s=ui.s;ui.label(&format!("{} 件未完成 · 共 {} 件",m.pending(),m.todos.items.len()),x+4.,y+4.,13.,s.on_surface_variant,false,w);let now=dotisle_domain::todos::Now::local();let mut items=m.todos.items.iter().collect::<Vec<_>>();items.sort_by_key(|t|(t.done(&now),t.date.clone(),t.time.clone()));
    if items.is_empty(){ui.panel(rect(x,y+45.,w,360.),s.container);draw::circle(ui.scene,x+w/2.,y+158.,42.,s.secondary_container);draw::icon(ui.scene,Icon::Leaf,x+w/2.-22.,y+136.,44.,s.primary);ui.center("心里，少一点牵挂",rect(x,y+234.,w,44.),28.,s.on_surface,true);ui.center("点击右上角的新建待办，记下第一件事。",rect(x,y+293.,w,28.),14.,s.on_surface_variant,false);return y+405.}
    let mut bottom=y+42.;for t in items{let done=t.done(&now);ui.panel(rect(x,bottom,w,96.),if done{s.container}else{s.high});ui.icon_button(rect(x+16.,bottom+26.,44.,44.),Icon::Check,if done{"恢复待办"}else{"完成待办"},Action::ToggleTodo(t.id.clone()),done,true);let title=if done{format!("已完成 · {}",t.title)}else{t.title.clone()};ui.label(&title,x+78.,bottom+17.,18.,if done{s.on_surface_variant}else{s.on_surface},!done,w-248.);ui.label(&format!("{}  {}{}{}",t.date,t.time,if t.daily{" · 每天"}else{""},if t.content.is_empty(){String::new()}else{format!("  /  {}",t.content)}),x+78.,bottom+54.,13.,s.on_surface_variant,false,w-248.);ui.button(rect(x+w-156.,bottom+26.,88.,44.),"编辑",None,Action::EditTodo(Some(t.id.clone())),true,true);ui.icon_button(rect(x+w-60.,bottom+26.,44.,44.),Icon::Trash,"移除待办",Action::DeleteTodo(t.id.clone()),false,true);bottom+=108.;}bottom
}
fn settings(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
    let s=ui.s;let tabs=[(4,"常规"),(0,"外观"),(1,"音乐与歌词"),(2,"通知与待办"),(5,"设备"),(6,"音效"),(7,"应用提醒"),(3,"关于与更新")];let tw=(w-24.)/4.;for(i,(index,label))in tabs.iter().enumerate(){ui.chip(rect(x+(i%4)as f64*(tw+8.),y+(i/4)as f64*52.,tw,44.),label,m.settings_tab==*index,Action::SettingsTab(*index));}
    let mut cy=y+120.;
    let mut group=|ui:&mut Ui,title:&str,rows:Vec<(&str,&str,bool,&'static str)>|{ui.label(title,x+4.,cy+5.,20.,s.on_surface,true,w);cy+=48.;let h=rows.len()as f64*80.+16.;ui.panel(rect(x,cy,w,h),s.container);for(i,(title,detail,on,key))in rows.iter().enumerate(){ui.toggle(rect(x+24.,cy+8.+i as f64*80.,w-48.,72.),title,detail,*on,Action::Pref(key));if i+1<rows.len(){draw::line(ui.scene,(x+24.,cy+88.+i as f64*80.),(x+w-24.,cy+88.+i as f64*80.),s.outline,0.5);}}cy+=h+28.;};
    match m.settings_tab{
        0=>{cy=crate::appearance::render(ui,m,x,cy,w);},
        1=>{group(ui,"聆听偏好",vec![("显示翻译","有译文时在当前歌词下方显示",m.prefs.translation,"translation"),("逐字高亮","歌词随歌曲进度渐次点亮",m.prefs.word_highlight,"words"),("在线歌曲补全","使用现有音乐服务补全歌曲信息和歌词",m.prefs.online,"online"),("接收浏览器媒体","将浏览器的媒体会话纳入播放来源",m.prefs.include_browser,"browser")]);
            ui.panel(rect(x,cy,w,88.),s.container);ui.label("本地歌词",x+24.,cy+16.,17.,s.on_surface,true,w-220.);ui.label(&format!("{} 个文件夹 · 支持本地歌词优先匹配",m.prefs.local_folders.len()),x+24.,cy+48.,12.,s.on_surface_variant,false,w-220.);ui.button(rect(x+w-172.,cy+22.,148.,44.),"管理文件夹",Some(Icon::Folder),Action::Paths,true,true);cy+=116.;
            ui.label("歌词偏移可在音乐页直接调整，播放时即可校准。",x+4.,cy,13.,s.on_surface_variant,false,w);cy+=30.;},
        2=>{},
        3=>{ui.panel(rect(x,cy,w,128.),s.high);draw::fill(ui.scene,rect(x+24.,cy+28.,64.,64.),22.,s.primary);draw::icon(ui.scene,Icon::Island,x+40.,cy+44.,32.,s.on_primary);ui.label("点屿",x+108.,cy+24.,28.,s.on_surface,true,w-300.);ui.label("为专注与聆听留一点空间",x+108.,cy+72.,15.,s.on_surface_variant,false,w-148.);let badge=rect(x+w-150.,cy+28.,126.,34.);draw::fill(ui.scene,badge,17.,s.secondary_container);ui.center(&format!("版本 {}",env!("CARGO_PKG_VERSION")),badge,12.,s.on_secondary_container,true);cy+=152.;
            cy=crate::software_update::render(ui,m,x,cy,w)+28.;
            ui.panel(rect(x,cy,w,112.),s.container);ui.label("从 2.x 迁移",x+24.,cy+20.,18.,s.on_surface,true,w-230.);ui.label("仅在 V3 数据目录为空时导入，旧版文件保持原样。",x+24.,cy+57.,13.,s.on_surface_variant,false,w-230.);ui.button(rect(x+w-180.,cy+32.,156.,48.),"导入旧数据",Some(Icon::Import),Action::Import,true,true);cy+=140.;
            ui.panel(rect(x,cy,w,112.),s.container);ui.label("数据保存在本机",x+24.,cy+18.,18.,s.on_surface,true,w-48.);ui.label(&m.data.to_string_lossy(),x+24.,cy+56.,12.,s.on_surface_variant,false,w-48.);ui.label("重启后计时器以暂停状态恢复。",x+24.,cy+82.,12.,s.on_surface_variant,false,w-48.);cy+=140.;
            if let Some(e)=m.prefs_error.as_ref().or(m.todos.error.as_ref()).or(m.timers.error.as_ref()){ui.label(e,x+4.,cy,14.,s.error,true,w);cy+=56.;}
            ui.button(rect(x,cy,168.,48.),"关闭设置",Some(Icon::Close),Action::Quit,true,true);cy+=60.;},_=>{}
    }crate::preferences_ui::extra(ui,m,x,cy,w,m.settings_tab)
}
fn dialog(ui:&mut Ui,m:&Model,width:f64,height:f64){
    let Some(d)=&m.dialog else{return};let s=ui.s;ui.hits.clear();ui.scene.fill(Fill::NonZero,Affine::IDENTITY,draw::alpha(0,0.38),None,&rect(0.,0.,width,height));
    if matches!(d.kind,DialogKind::WorkflowTemplates){templates_dialog(ui,m,width,height);return}if matches!(d.kind,DialogKind::WorkflowModules){crate::tool_preferences::modules_dialog(ui,m,width,height);return}
    let h=match d.kind{DialogKind::TimerPreferences=>496.,DialogKind::WorkflowModule(_)|DialogKind::WorkflowModules=>340.,DialogKind::QuickReminder=>380.,DialogKind::Rule(_)=>620.,DialogKind::Todo(_)=>552.,DialogKind::Paths=>300.,DialogKind::Import|DialogKind::Setting(_)=>320.,DialogKind::WorkflowStage(_)|DialogKind::WorkflowTemplate(_)=>360.,DialogKind::WorkflowTemplates=>360.};let w=560.;let x=(width-w)/2.;let y=(height-h)/2.;ui.panel(rect(x,y,w,h),s.surface);
    let title=match &d.kind{DialogKind::TimerPreferences=>"计时参数",DialogKind::WorkflowModules=>"流程模块库",DialogKind::WorkflowModule(index)=>if index.is_some(){"编辑模块"}else{"新建模块"},DialogKind::QuickReminder=>"一句话快速提醒",DialogKind::Rule(id)=>if id.is_some(){"编辑应用提醒"}else{"新建应用提醒"},DialogKind::Todo(id)=>if id.is_some(){"编辑待办"}else{"记下一件事"},DialogKind::Paths=>"本地歌词文件夹",DialogKind::Import=>"导入 2.x 数据",DialogKind::Setting(_)=>"调整设置",DialogKind::WorkflowStage(Some(_))=>"编辑阶段",DialogKind::WorkflowStage(None)=>"添加阶段",DialogKind::WorkflowTemplate(_)=>"保存流程模板",DialogKind::WorkflowTemplates=>"流程模板"};ui.label(title,x+28.,y+24.,28.,s.on_surface,true,w-110.);ui.icon_button(rect(x+w-68.,y+20.,44.,44.),Icon::Close,"取消",Action::CancelModal,false,true);
    if matches!(d.kind,DialogKind::Import){ui.label("从本机旧版点屿导入偏好、待办和计时器。",x+28.,y+94.,15.,s.on_surface,false,w-56.);ui.label("V3 已有数据时将拒绝导入。旧文件不会被修改。",x+28.,y+131.,13.,s.on_surface_variant,false,w-56.);ui.label("旧版材质、窗口布局和动画调参不会迁移。",x+28.,y+164.,13.,s.on_surface_variant,false,w-56.);}
    for(i,f)in d.fields.iter().enumerate(){let fy=y+88.+i as f64*if matches!(d.kind,DialogKind::Rule(_)){74.}else{84.};let r=rect(x+28.,fy,w-56.,68.);draw::fill(ui.scene,r,12.,s.high);if d.active==i{draw::outline(ui.scene,r,12.,s.primary,2.);}ui.label(f.label,r.x0+14.,fy+8.,11.,s.primary,true,w-88.);let caret=f.cursor.min(f.value.len());if f.selected&&d.active==i{draw::fill(ui.scene,rect(r.x0+10.,fy+27.,r.width()-20.,28.),6.,s.secondary_container);}let display=if d.active==i{format!("{}{}│{}",&f.value[..caret],d.preedit,&f.value[caret..])}else{f.value.clone()};let count=display.chars().count();let tail=if count>52{format!("…{}",display.chars().skip(count-52).collect::<String>())}else{display};ui.label(&tail,r.x0+14.,fy+28.,15.,s.on_surface,false,w-88.);ui.hit(r,f.label,Action::Field(i),true);}
    if matches!(d.kind,DialogKind::TimerPreferences){ui.chip(rect(x+28.,y+356.,196.,38.),"番茄钟正向计时",d.daily,Action::Daily);ui.label("时长参数用于下一次新建计时。",x+28.,y+404.,12.,s.on_surface_variant,false,w-56.);}
    if matches!(d.kind,DialogKind::QuickReminder){
        let q=&m.quick_preview;ui.label("支持几分钟后、今天 / 明天 HH:MM，以及指定日期。",x+28.,y+166.,12.,s.on_surface_variant,false,w-56.);
        if q["success"]==true{ui.label(&format!("{} {} · {}",q["date"].as_str().unwrap_or(""),q["time"].as_str().unwrap_or(""),q["title"].as_str().unwrap_or("")),x+28.,y+206.,17.,s.primary,true,w-56.);ui.button(rect(x+28.,y+250.,196.,44.),"确认创建提醒",Some(Icon::Check),Action::QuickCommit,false,true);}
    }
    if matches!(d.kind,DialogKind::Rule(_)){ui.chip(rect(x+28.,y+470.,232.,38.),"按前台累计时长触发",d.daily,Action::Daily);ui.label(if d.daily{"当前方式：每天累计前台使用时长"}else{"当前方式：进入应用时提醒"},x+280.,y+480.,12.,s.on_surface_variant,false,w-308.);}
    if matches!(d.kind,DialogKind::Todo(_)){ui.chip(rect(x+28.,y+432.,128.,40.),"每天重复",d.daily,Action::Daily);}
    if let DialogKind::WorkflowTemplate(Some(_))=&d.kind{ui.chip(rect(x+28.,y+176.,176.,40.),"覆盖已加载模板",d.daily,Action::Daily);ui.label(if d.daily{"保存后更新当前模板；正在运行的流程实例不受影响。"}else{"不覆盖时，会另存为一个新模板。"},x+28.,y+226.,12.,s.on_surface_variant,false,w-56.);}
    if !d.error.is_empty(){ui.label(&d.error,x+28.,y+h-104.,12.,s.error,true,w-56.);}
    let save=match &d.kind{DialogKind::QuickReminder=>"解析时间",DialogKind::Import=>"开始导入",DialogKind::WorkflowStage(_)=>"保存阶段",DialogKind::WorkflowTemplate(_)=>"保存模板",_=>"保存"};
    ui.button(rect(x+w-248.,y+h-70.,96.,44.),"取消",None,Action::CancelModal,true,true);ui.button(rect(x+w-140.,y+h-70.,112.,44.),save,None,Action::SaveModal,false,true);
}
fn templates_dialog(ui:&mut Ui,m:&Model,width:f64,height:f64){
    let s=ui.s;let templates=&m.workflow.templates;let shown=templates.len().min(16);let rows=shown.div_ceil(2);let w=620.;let h=(100.+rows as f64*58.+76.).max(250.).min(height-40.);let x=(width-w)/2.;let y=(height-h)/2.;
    ui.panel(rect(x,y,w,h),s.surface);
    ui.label("流程模板",x+28.,y+24.,28.,s.on_surface,true,w-110.);ui.icon_button(rect(x+w-68.,y+20.,44.,44.),Icon::Close,"关闭模板库",Action::CancelModal,false,true);
    ui.label(if templates.is_empty(){"还没有保存的模板。"}else{"加载会复制成当前草稿；删除不影响正在运行的流程。"},x+28.,y+74.,13.,s.on_surface_variant,false,w-56.);
    if templates.is_empty(){ui.label("编排好阶段后，点「保存为模板」即可在这里复用。",x+28.,y+126.,15.,s.on_surface,false,w-56.);}
    else{let cw=(w-64.)/2.;for(i,t)in templates.iter().take(16).enumerate(){let cx=x+28.+(i%2)as f64*(cw+8.);let cy=y+104.+(i/2)as f64*58.;let r=rect(cx,cy,cw,50.);draw::fill(ui.scene,r,14.,s.high);ui.label(&t.name,cx+14.,cy+6.,15.,s.on_surface,true,cw-160.);ui.label(&crate::workflow::summary(&t.stages),cx+14.,cy+28.,11.,s.on_surface_variant,false,cw-160.);ui.button(rect(cx+cw-136.,cy+7.,76.,36.),"加载",None,Action::WorkflowLoadTemplate(t.id.clone()),true,true);ui.icon_button(rect(cx+cw-52.,cy+7.,40.,36.),Icon::Trash,"删除模板",Action::WorkflowDeleteTemplate(t.id.clone()),false,true);}}
    ui.button(rect(x+w-140.,y+h-62.,112.,44.),"关闭",None,Action::CancelModal,true,true);
}
fn palette(ui:&mut Ui,m:&Model,width:f64,height:f64){let s=ui.s;ui.hits.clear();ui.scene.fill(Fill::NonZero,Affine::IDENTITY,draw::alpha(0,0.30),None,&rect(0.,0.,width,height));let w=560.;let x=(width-w)/2.;let y=height*0.18;ui.panel(rect(x,y,w,418.),s.surface);draw::icon(ui.scene,Icon::Search,x+24.,y+25.,24.,s.primary);ui.label(&format!("{}│",if m.query.is_empty(){"搜索页面或设置"}else{&m.query}),x+64.,y+23.,20.,s.on_surface,true,w-140.);ui.icon_button(rect(x+w-64.,y+16.,44.,44.),Icon::Close,"关闭搜索",Action::Palette,false,true);let mut cy=y+84.;for page in Page::ALL{if !m.query.is_empty()&&!page.label().contains(&m.query)&&!page.description().contains(&m.query){continue}let r=rect(x+16.,cy,w-32.,56.);let i=ui.hit(r,page.label(),Action::Navigate(page),true);ui.state(r,16.,i,s.on_surface);draw::icon(ui.scene,page_icon(page),x+30.,cy+16.,24.,s.primary);ui.label(page.label(),x+76.,cy+14.,17.,s.on_surface,false,w-120.);cy+=60.;}if cy==y+84.{ui.label("没有找到页面，试试“音乐”或“偏好”。",x+28.,cy+20.,14.,s.on_surface_variant,false,w-56.);}}

#![windows_subsystem = "windows"]
// The settings defaults are one large json! literal; the default macro recursion
// budget is not enough for it once the 1.3 keys are included.
#![recursion_limit = "256"]
mod bottom_osd;mod update_activity;mod activation;mod notification_store;mod cover_color;mod app_icon;mod motion;mod layout;mod renderer;mod config;mod font_probe;mod progress;mod notifications;mod banner;
mod devices;mod osd;mod timers;mod activities;
mod gestures;mod compact;mod sound;mod todos;mod history;mod quick_reminder;mod app_rules;mod foreground;mod diagnostics;
use std::{cell::RefCell,path::{Path,PathBuf},sync::{Arc,Mutex,OnceLock,mpsc},time::{Instant,Duration},ffi::{c_char,CStr}};
use serde_json::{Value,json};
use base64::Engine;
use windows::{core::{w,PCWSTR,Result},Win32::{Foundation::*,System::{Com::*,LibraryLoader::GetModuleHandleW,Threading::*,RemoteDesktop::{ProcessIdToSessionId,NOTIFY_FOR_THIS_SESSION,WTSRegisterSessionNotification,WTSUnRegisterSessionNotification},WindowsProgramming::QueryUnbiasedInterruptTime},Graphics::Gdi::*,UI::{WindowsAndMessaging::*,Shell::*,Input::KeyboardAndMouse::*,HiDpi::*}}};
use lyric_backend::{runtime::Backend,io,ipc::Server,shell};
use motion::{Frame,Options,Transition};use layout::{Layout,Rect};use renderer::{Renderer,Scene};
const WM_TRAY:u32=WM_APP+1;const WM_SETTINGS:u32=WM_APP+2;
/// Synthetic AUMID for reminder banners, so a click opens the todo page instead
/// of trying to activate an external application.
const TODO_APP_ID:&str="LyricIsland.Todo";
type Pending=(Value,mpsc::Sender<Value>);
/// One queued banner. Reminder banners are tracked by todo id so a completion or
/// deletion can cancel a banner that has not been presented yet.
struct Queued{pub notice:notifications::Notice,pub todo:Option<String>,pub chime:bool}
/// A parsed reminder awaiting an explicit commit. The token, not the original
/// text, is what commit accepts, so "明天"/"N分钟后" is never reinterpreted.
struct QuickPreview{pub token:String,pub title:String,pub date:String,pub time:String,pub expires:Instant}
/// Focus timers parked by the current lock. Kept after restart only as ordinary
/// paused state; nothing resumes automatically without a matching generation.
struct FocusResume{pub generation:u64,pub ids:Vec<String>,pub prompted:bool}
fn hotkey_code(name:&str)->Option<(HOT_KEY_MODIFIERS,u32)>{
    // One parser for the recorder and the registration, so a binding the settings window accepts
    // is never one this side quietly drops.
    let hotkey=lyric_core::hotkey::Hotkey::parse(name)?;
    let mut mods=HOT_KEY_MODIFIERS(0);
    if hotkey.ctrl{mods|=MOD_CONTROL}if hotkey.alt{mods|=MOD_ALT}if hotkey.shift{mods|=MOD_SHIFT}if hotkey.win{mods|=MOD_WIN}
    Some((mods,hotkey.vk))
}
fn awake_ms()->u64{let mut value=0u64;unsafe{let _=QueryUnbiasedInterruptTime(&mut value);}(value/10_000)as u64}
static REQUESTS:OnceLock<Mutex<Vec<Pending>>>=OnceLock::new();static WINDOW:OnceLock<isize>=OnceLock::new();
fn inside_round_rect(r:Rect,radius:f64,x:f64,y:f64)->bool{
    if !r.contains(x,y){return false}let radius=radius.min(r.w/2.).min(r.h/2.).max(0.);
    let cx=x.clamp(r.x+radius,r.x+r.w-radius);let cy=y.clamp(r.y+radius,r.y+r.h-radius);
    (x-cx).powi(2)+(y-cy).powi(2)<=radius*radius
}
/// "Appear when needed, retract once handled": decide whether the island must be
/// present from what is asking for attention right now. A feature merely being
/// switched on, or merely being selectable, is not a reason to stay on screen.
fn island_needed(valid:bool,playing:bool,hide_when_paused:bool,running_timer:bool,banner:bool,queue:bool,held:bool,manual:bool)->bool{
    valid&&(playing||!hide_when_paused)||running_timer||banner||queue||held||manual
}
unsafe extern "C" fn settings_callback(request:*const c_char)->*mut c_char {
    let response=std::panic::catch_unwind(||{
        let request=serde_json::from_slice::<Value>(CStr::from_ptr(request).to_bytes()).unwrap_or(Value::Null);
        let (tx,rx)=mpsc::channel();REQUESTS.get_or_init(Default::default).lock().unwrap().push((request,tx));
        if let Some(h)=WINDOW.get(){let _=PostMessageW(Some(HWND(*h as _)),WM_SETTINGS,WPARAM(0),LPARAM(0));}
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_|json!({"success":false,"message":"主窗口未响应"}))
    }).unwrap_or_else(|_|json!({"success":false,"message":"设置请求失败"}));
    let bytes=response.to_string();let p=CoTaskMemAlloc(bytes.len()+1)as *mut u8;if !p.is_null(){std::ptr::copy_nonoverlapping(bytes.as_ptr(),p,bytes.len());*p.add(bytes.len())=0;}p as _
}
struct App {
    banner:banner::Banner,osd_banner:banner::Banner,osd:osd::Hub,pending_osd:u32,notification_preview:bool,banner_todo:Option<String>,notification_listener:Option<notifications::Listener>,notification_state:Value,
    hwnd:HWND,visual_hwnd:HWND,input_region:Option<[i32;10]>,dragging:Option<(POINT,RECT)>,renderer:Option<Renderer>,base:PathBuf,data:PathBuf,settings:Value,backend:Option<Arc<Backend>>,
    _server:Option<Server>,snapshot:Value,cover_version:u64,demo:bool,
    layout:Layout,frame:Frame,transition:Option<Transition>,transition_start:Instant,sampled_ms:f64,
    expanded:bool,held:bool,hover:i32,pressed:i32,seeking:bool,seek_fraction:f64,command_busy:Arc<std::sync::atomic::AtomicBool>,
    start:Instant,last_poll:Instant,lyric:String,previous_lyric:String,compact_secondary:String,previous_compact_secondary:String,lyric_start:Instant,
    compact_size:compact::Resize,compact_key:String,last_compact:Instant,todos:todos::Store,todo_now:todos::Now,last_schedule:Instant,history:history::Store,last_history_prune:Instant,queue:std::collections::VecDeque<Queued>,pending_page:Option<String>,
    updates:update_activity::Hub,update_return:String,update_until:Option<Instant>,
    devices:devices::Hub,
    timers:timers::Store,selected:String,components:Vec<String>,panel:Option<activities::Panel>,previous_panel:Option<activities::Panel>,drag_panel:Option<activities::Panel>,drag_target:String,slide_outgoing:bool,switching:bool,slide:activities::Spring,slide_direction:f64,swipe:Option<activities::Swipe>,hide_drag:Option<gestures::HideDrag>,dismiss_offset:activities::Spring,last_timer:Instant,timer_fraction:u128,last_checkpoint:Instant,
    cursor_inside:bool,leave_at:Option<Instant>,last_left:bool,user_hidden:bool,display_wanted:bool,missing_since:Option<Instant>,manual_until:Option<Instant>,
    opacity:f64,fade_from:f64,fade_to:f64,fade_start:Instant,hide_at:Option<Instant>,dirty:bool,
    settings_process:Option<std::process::Child>,renderer_generation:u64,render_frames:u64,error:Option<String>,transparent:bool,activity:motion::Activity,cover_start:Instant,progress:progress::PlaybackProgress,outgoing_opacity:f64,outgoing_y:f64,
    quick:Vec<QuickPreview>,quick_commits:Vec<(String,String)>,pending_action:Option<String>,templates:timers::Templates,
    rules:app_rules::Store,foreground:foreground::Watcher,usage_date:String,last_awake:u64,last_rules_checkpoint:Instant,
    lock_generation:u64,session_locked:bool,focus_resume:Option<FocusResume>,
    bottom_osd:bottom_osd::Overlay,
    music_mix:f64,music_from:f64,music_start:Instant,
}
impl App {
    fn new(base:PathBuf,data:PathBuf,demo:bool)->Self{
        let settings=if demo{config::defaults()}else{config::load(&data.join("settings.json"))};let options=serde_json::from_value::<Options>(settings["Motion"].clone()).unwrap_or_default();
        let backend=if demo{None}else{Some(Backend::new(settings.clone(),data.clone(),PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()),data.join("Logs")))};
        let now=Instant::now();let snapshot=if demo{json!({"track":{"id":"demo","title":"晚风电台","artist":"DotIsle","sourceApp":"演示","durationSeconds":236.},"isPlaying":true,"positionSeconds":83.,"canSeek":true,"frame":{"currentLine":"景色は移ろい瞬く間に変わる","currentTranslation":"景色转瞬即逝","nextLine":"変わらないものはない","wordScanProgress":0.4}})}else{Value::Null};
        let notification_listener=if demo{None}else{Some(notifications::Listener::new(notifications::Policy::from_settings(&settings)))};
        // Demo runs must never touch the user's real todo file.
        let todos=todos::Store::new(if demo{PathBuf::new()}else{data.join("todos.json")});let todo_now=todos::Now::local();
        let timers=timers::Store::new(if demo{PathBuf::new()}else{data.join("timers.json")});
        let templates=timers::Templates::new(if demo{PathBuf::new()}else{data.join("workflows.json")});
        let foreground=foreground::Watcher::new();
        let mut rules=app_rules::Store::new(if demo{PathBuf::new()}else{data.join("rules.json")},if demo{PathBuf::new()}else{data.join("usage.json")},&todo_now.date);
        rules.baseline_now(foreground.path.as_deref(),foreground.segment);
        let usage_date=todo_now.date.clone();
        let mut history=history::Store::new(if demo{PathBuf::new()}else{data.join("history.json")});
        let history_days=||settings["NotificationHistoryDays"].as_u64().unwrap_or(7).clamp(1,30)as u32;
        let epoch=||std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()as i64;
        history.prune(history_days(),epoch());
        let compact_size=compact::Resize::new(settings["CompactWidth"].as_f64().unwrap_or(260.),40.);
        let music_mix=layout::MusicLayout::from_setting(settings["MusicExpandedLayout"].as_str().unwrap_or("")).position();
        let mut layout=Layout::new(settings["CompactWidth"].as_f64().unwrap_or(260.));layout.card.h=layout::music_height(music_mix,settings["FontSize"].as_f64().unwrap_or(12.5));
        let updates=update_activity::Hub::new(&data);
        Self{bottom_osd:bottom_osd::Overlay::new(),updates,update_return:"music".into(),update_until:None,banner:banner::Banner::default(),osd_banner:banner::Banner::default(),osd:osd::Hub::default(),pending_osd:0,notification_preview:false,banner_todo:None,notification_listener,notification_state:json!({"access":"disabled","message":"通知提醒已关闭","apps":[]}),hwnd:HWND::default(),visual_hwnd:HWND::default(),input_region:None,dragging:None,renderer:None,base,data,layout,settings,backend,_server:None,snapshot,cover_version:0,demo,frame:Frame::hidden(options),transition:None,transition_start:now,sampled_ms:0.,expanded:false,held:false,hover:0,pressed:0,seeking:false,seek_fraction:0.,command_busy:Arc::new(std::sync::atomic::AtomicBool::new(false)),start:now,last_poll:now-Duration::from_secs(1),lyric:"打开播放器，开始一首音乐".into(),previous_lyric:String::new(),compact_secondary:String::new(),previous_compact_secondary:String::new(),lyric_start:now-Duration::from_secs(2),compact_size,compact_key:String::new(),last_compact:now,todos,todo_now,last_schedule:now,history,last_history_prune:now,queue:std::collections::VecDeque::new(),pending_page:None,devices:devices::Hub::default(),timers,selected:"music".into(),components:vec!["music".into()],panel:None,previous_panel:None,drag_panel:None,drag_target:String::new(),slide_outgoing:false,switching:false,slide:activities::Spring::default(),slide_direction:1.,swipe:None,hide_drag:None,dismiss_offset:activities::Spring::default(),last_timer:now,timer_fraction:0,last_checkpoint:now,cursor_inside:false,leave_at:None,last_left:false,user_hidden:false,display_wanted:demo,missing_since:None,manual_until:None,opacity:if demo{1.}else{0.},fade_from:0.,fade_to:if demo{1.}else{0.},fade_start:now,hide_at:None,dirty:true,settings_process:None,renderer_generation:0,render_frames:0,error:None,transparent:false,activity:motion::Activity::new(),cover_start:now-Duration::from_secs(1),progress:progress::PlaybackProgress::default(),outgoing_opacity:1.,outgoing_y:0.,music_mix,music_from:music_mix,music_start:now,quick:vec![],quick_commits:vec![],pending_action:None,templates,rules,foreground,usage_date,last_awake:awake_ms(),last_rules_checkpoint:now,lock_generation:0,session_locked:false,focus_resume:None}
    }
    fn refresh_components(&mut self){
        let mut components=vec![];if self.updates.selectable(){components.push("update".into());}if self.demo||self.snapshot["track"].is_object(){components.push("music".to_string());}
        if self.flag("TodoIslandEnabled",true)&&self.todos.items.iter().any(|t|!t.done(&self.todo_now)){components.push("todos".into());}
        for t in &self.timers.items{components.push(format!("timer:{}",t.id));}if components.is_empty(){components.push("music".into());}
        if !components.contains(&self.selected){self.selected=components[0].clone();}self.components=components;
        self.panel=self.component_panel(&self.selected);
    }
    fn component_panel(&self,id:&str)->Option<activities::Panel>{
        if id=="update"{Some(self.updates.panel())}else if id=="todos"{let tasks=self.todos.items.iter().filter(|t|!t.done(&self.todo_now)).take(3).map(|t|activities::Task{id:t.id.clone(),title:t.title.clone(),detail:format!("{} {}{}",t.date,t.time,if t.daily{" · 每天"}else{""}),color:0xff000000|u32::from_str_radix(t.color.trim_start_matches('#'),16).unwrap_or(0xf3d79b)}).collect();Some(activities::Panel{kind:2,title:"待办清单".into(),value:format!("{} 项待办",self.todos.items.iter().filter(|t|!t.done(&self.todo_now)).count()),subtitle:"记住重要的小事".into(),accent:0xfff3d79b,tasks,..Default::default()})}
        else if let Some(id)=id.strip_prefix("timer:"){self.timers.items.iter().find(|t|t.id==id).map(|t|t.panel())}else{None}
    }
    fn select_component(&mut self,id:&str,direction:i32){
        if !self.components.iter().any(|c|c==id)||self.selected==id{return}
        self.hide_at=None;self.manual_until=Some(Instant::now()+Duration::from_secs(6));self.slide_outgoing=true;self.previous_panel=self.panel.clone();self.selected=id.into();self.refresh_components();self.compact_key.clear();
        self.slide_direction=if direction<0{-1.}else{1.};self.switching=self.flag("Animation",true);self.slide.set(if self.switching{self.slide_direction*if self.expanded{self.layout.card.w}else{self.layout.capsule.w}}else{0.});self.dirty=true;
    }
    fn cycle_component(&mut self,direction:i32){self.refresh_components();let n=self.components.len();if n<2{return}let i=self.components.iter().position(|c|c==&self.selected).unwrap_or(0);let next=(i as i32+direction).rem_euclid(n as i32)as usize;let id=self.components[next].clone();self.select_component(&id,direction);}
    fn can_hide(&self,hit:i32)->bool{self.flag("SwipeUpHide",true)&&!self.banner.visible()&&self.dragging.is_none()&&matches!(hit,0|1|13|40..=42)}
    fn hide_by_gesture(&mut self){self.user_hidden=true;self.held=false;self.leave_at=None;self.hide_at=None;self.seeking=false;self.pressed=0;self.drag_target.clear();self.drag_panel=None;self.swipe=None;self.fade(0.);self.dirty=true;}
    fn can_swipe(&self,hit:i32)->bool{self.flag("SwipeActivities",true)&&!self.banner.visible()&&self.components.len()>1&&matches!(hit,0|1|13|40..=42)}
    fn finish_swipe(&mut self,swipe:activities::Swipe)->bool{
        if !swipe.active{return false}
        let fluid=self.flag("IslandFluidMotion",true);
        // A flick is a decision even when the finger only travelled twenty pixels. Without
        // this a quick throw springs back to where it started, which is the first gesture
        // everyone tries and the one that reads as broken.
        let direction=if fluid{let flick=swipe.flick();if flick!=0{flick}else{swipe.direction()}}else{swipe.direction()};
        let width=if self.expanded{self.layout.card.w}else{self.layout.capsule.w};
        let offset=if fluid{motion::rubber(swipe.dx,width)}else{swipe.dx.clamp(-width,width)};
        if direction!=0{self.cycle_component(direction);if self.flag("Animation",true){self.slide.release(self.slide_direction*width+offset,swipe.velocity);}}
        else{self.slide.release(if self.flag("Animation",true){offset}else{0.},swipe.velocity);self.slide_outgoing=false;self.switching=self.flag("Animation",true);}
        self.drag_target.clear();self.drag_panel=None;self.pressed=0;self.dirty=true;true
    }
    fn active_timer_action(&mut self,action:&str){if let Some(id)=self.selected.strip_prefix("timer:"){let action=if action=="toggle"{self.timers.items.iter().find(|t|t.id==id).map(|t|t.primary_action()).unwrap_or(action)}else{action};if let Err(e)=self.timers.action(id,action){self.error=Some(e);}self.refresh_components();self.dirty=true;}}
    fn flag(&self,k:&str,default:bool)->bool{self.settings[k].as_bool().unwrap_or(default)}
    /// Prefer the single-file launcher when one extracted this copy: a versioned runtime folder
    /// changes on every update, so registering it would break login startup after the next release.
    fn startup_target(&self)->PathBuf{if self.base.join("Installed.json").exists()||self.base.join("Portable.json").is_file(){return self.base.join("LyricIsland.exe")}
        if let Ok(text)=std::fs::read_to_string(self.base.join("launcher.txt")){let path=PathBuf::from(text.trim());if path.is_file(){return path}}
        std::env::current_exe().unwrap_or_else(|_|self.base.join("LyricIsland.exe"))
    }
    fn apply_startup(&mut self)->std::result::Result<(),String>{
        shell::set_startup(self.flag("AutoStart",true),&self.startup_target().to_string_lossy(),&self.data.join("settings.json"),&self.settings,None)
    }
    fn music_target(&self)->f64{layout::MusicLayout::from_setting(self.settings["MusicExpandedLayout"].as_str().unwrap_or("")).position()}
    /// Height is part of the shared geometry, so the window keeps the taller host
    /// and only the card itself grows while the two layouts cross-fade.
    fn start_music_shift(&mut self){
        let target=self.music_target();if (target-self.music_mix).abs()<0.001{self.music_mix=target;return}
        if self.expanded&&self.flag("Animation",true){self.music_from=self.music_mix;self.music_start=Instant::now();}
        else{self.music_mix=target;self.music_from=target;}
        self.dirty=true;
    }
    fn shape(&self)->(Rect,f64){self.layout.morph_shape(self.frame.card_reveal,self.options().radius,self.flag("IslandFluidMotion",true))}
    fn options(&self)->Options{serde_json::from_value(self.settings["Motion"].clone()).unwrap_or_default()}
    unsafe fn position(&mut self)->Result<()> {
        let base=self.compact_base();self.layout=Layout::new(base);self.layout.card.h=layout::music_height(self.music_mix,self.settings["FontSize"].as_f64().unwrap_or(12.5));self.layout.reserve_compact(self.settings["AutoCompactMaxWidth"].as_f64().unwrap_or(420.).max(base)+64.);self.compact_key=String::new();
        let monitor=MonitorFromWindow(self.hwnd,MONITOR_DEFAULTTONEAREST);let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};let _=GetMonitorInfoW(monitor,&mut info);
        let dpi=GetDpiForWindow(self.hwnd).max(96)as f64;let scale=dpi/96.;let work=info.rcWork;let width=(self.layout.host.w*scale).ceil()as i32;let height=(self.layout.host.h*scale).ceil()as i32;
        let floating=self.flag("Floating",false);let center=if floating{self.settings["CenterX"].as_f64().unwrap_or(0.5)}else{0.5};
        let x=(work.left as f64+(work.right-work.left)as f64*center-width as f64/2.).round()as i32;let x=x.clamp(work.left,(work.right-width).max(work.left));
        let y=if floating{(work.top as f64+((work.bottom-work.top)as f64-40.*scale)*self.settings["TopFraction"].as_f64().unwrap_or(0.08)-24.*scale).round()as i32}else{work.top-(24.*scale).round()as i32};
        SetWindowPos(self.hwnd,Some(if self.flag("AlwaysOnTop",true){HWND_TOPMOST}else{HWND_NOTOPMOST}),x,y,width,height,SWP_NOACTIVATE)?;
        SetWindowPos(self.visual_hwnd,Some(if self.flag("AlwaysOnTop",true){HWND_TOPMOST}else{HWND_NOTOPMOST}),x,y,width,height,SWP_NOACTIVATE)?;
        let rebuild=!self.renderer.as_ref().is_some_and(|r|r.width==width as u32&&r.height==height as u32&&(r.dpi-dpi as f32).abs()<0.01);
        if rebuild{let font=self.base.join("Assets/Fonts/NotoSansSC.ttf");self.renderer=None;self.renderer=Some(Renderer::new(self.visual_hwnd,width as u32,height as u32,dpi as f32,&font)?);self.renderer_generation+=1;if let(Some(renderer),Some(notice))=(&mut self.renderer,&self.banner.notice){let _=renderer.set_notification_icon(notice.icon.as_deref());}self.cover_version=u64::MAX;self.last_poll=Instant::now()-Duration::from_secs(1);}
        self.input_region=None;self.sync_input_region()?;self.dirty=true;if rebuild{self.draw()?;}Ok(())
    }
    fn retarget(&mut self,expanded:bool){
        if self.expanded==expanded{return}let velocity=self.transition.as_ref().map(|t|t.velocity(self.sampled_ms)).unwrap_or_default();
        self.expanded=expanded;let o=self.options();
        if self.flag("Animation",true){self.transition=Some(Transition::styled(self.motion_style(),expanded,self.frame,o,velocity,false));self.transition_start=Instant::now();self.sampled_ms=0.;}
        else{self.transition=None;self.frame=if expanded{Frame::expanded()}else{Frame::hidden(o)};}
        self.dirty=true;
    }
    /// Which island curve this frame uses.
    ///
    /// 灵动岛流体动画 is its own switch, separate from 界面动画: the latter turns motion off
    /// entirely, this one chooses between the 2.0.0 curve and the derived-channel one. Both
    /// stay in the binary, so a user who preferred the old read keeps it exactly.
    fn motion_style(&self)->motion::Style{if self.flag("IslandFluidMotion",true){motion::Style::Fluid}else{motion::Style::Classic}}
    /// How far the dragged surface is drawn from its resting place.
    ///
    /// The classic curve hard-clamps at one width, so the surface stops dead under a finger
    /// that is still moving. The fluid curve damps the excess instead, which is the only
    /// difference the user can see at the boundary.
    fn swipe_offset(&self)->f64{
        let Some(swipe)=self.swipe.filter(|s|s.active)else{return 0.};
        let width=if self.expanded{self.layout.card.w}else{self.layout.capsule.w};
        if self.flag("IslandFluidMotion",true){motion::rubber(swipe.dx,width)}else{swipe.dx.clamp(-width,width)}
    }
    fn fade(&mut self,target:f64){if self.fade_to!=target{self.fade_from=self.opacity;self.fade_to=target;self.fade_start=Instant::now();self.dirty=true;}}
    fn command(&self,action:&str,seconds:f64){if let Some(backend)=self.backend.clone(){use std::sync::atomic::Ordering;if self.command_busy.swap(true,Ordering::AcqRel){return}let busy=self.command_busy.clone();let action=action.to_string();std::thread::spawn(move||{let _=backend.command(&action,seconds);busy.store(false,Ordering::Release);});}}
    fn show_settings(&mut self){self.show_settings_page("")}
    /// Open the settings window, optionally asking it to land on a named page.
    ///
    /// A window that is already running cannot be re-launched, so the request is
    /// also published as `pendingPage` in the state reply; the settings window
    /// reads it on its next refresh and switches itself.
    fn show_settings_page(&mut self,page:&str){
        if !page.is_empty(){self.pending_page=Some(page.to_owned());}
        if self.settings_process.as_mut().is_some_and(|p|p.try_wait().ok().flatten().is_none()){return}
        let exe=self.base.join("LyricIsland.Settings.exe");if exe.exists(){self.settings_process=std::process::Command::new(exe).env("LYRICISLAND_TEST_PIPE",std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into())).env("LYRICISLAND_NATIVE_DATA",&self.data).current_dir(&self.base).spawn().ok();}else{self.error=Some("设置组件缺失，请完整解压运行包".into());}
    }
    /// Register the configured quick-reminder hotkey. The previous binding is
    /// removed first; a conflict is reported so the caller can roll the setting back.
    unsafe fn register_hotkey(&self)->std::result::Result<(),String>{
        let _=UnregisterHotKey(Some(self.hwnd),1);
        let name=self.settings["QuickReminderHotkey"].as_str().unwrap_or("");
        let Some((mods,vk))=hotkey_code(name)else{return Ok(())};
        RegisterHotKey(Some(self.hwnd),1,mods|MOD_NOREPEAT,vk).map_err(|e|format!("快捷键注册失败，可能已被其他程序占用：{e}"))
    }
    fn quick_preview(&mut self,text:&str)->Value{
        if self.demo{return json!({"success":true,"previewToken":"demo","date":self.todo_now.date,"time":self.todo_now.time,"title":"演示","expiresAt":0,"minutePrecision":true})}
        let now=todos::Now::local();
        match quick_reminder::parse(text,&now){
            Ok(draft)=>{
                self.quick.retain(|p|p.expires>Instant::now());
                if self.quick.len()>=8{self.quick.remove(0);}
                let epoch=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                let token=format!("qr-{}-{}",std::process::id(),epoch*1000+self.quick.len()as u64);
                self.quick.push(QuickPreview{token:token.clone(),title:draft.title.clone(),date:draft.date.clone(),time:draft.time.clone(),expires:Instant::now()+Duration::from_secs(120)});
                json!({"success":true,"previewToken":token,"date":draft.date,"time":draft.time,"title":draft.title,"expiresAt":epoch+120,"minutePrecision":true,"message":format!("将在 {} {} 提醒",draft.date,draft.time)})
            }
            Err(e)=>json!({"success":false,"code":e.code,"message":e.message})
        }
    }
    fn quick_commit(&mut self,token:&str,request:&str)->Value{
        let request=request.trim();
        if request.is_empty()||request.len()>120{return json!({"success":false,"message":"缺少提交标识"})}
        if let Some((_,id))=self.quick_commits.iter().find(|(r,_)|r==request){return json!({"success":true,"id":id,"todos":self.todos.list(&self.todo_now),"message":"已创建提醒"})}
        if self.demo{return json!({"success":true,"id":"demo"})}
        let now=todos::Now::local();
        let Some(preview)=self.quick.iter().find(|p|p.token==token)else{return json!({"success":false,"message":"预览已过期，请重新解析"})};
        if preview.expires<=Instant::now(){return json!({"success":false,"message":"预览已过期，请重新解析"})}
        if format!("{} {}",preview.date,preview.time)<=format!("{} {}",now.date,now.time){return json!({"success":false,"message":"提醒时间已到，请重新解析"})}
        let (title,date,time)=(preview.title.clone(),preview.date.clone(),preview.time.clone());
        match self.todos.save(&json!({"title":title,"content":"","date":date,"time":time,"daily":false})){
            Ok(id)=>{
                self.quick.retain(|p|p.token!=token);self.quick_commits.push((request.into(),id.clone()));
                if self.quick_commits.len()>32{self.quick_commits.remove(0);}
                json!({"success":true,"id":id,"todos":self.todos.list(&self.todo_now),"message":"已创建提醒"})
            }
            Err(e)=>json!({"success":false,"message":e})
        }
    }
    fn on_lock(&mut self){
        if !self.flag("FocusPauseOnLock",false)||self.session_locked{return}
        self.session_locked=true;self.lock_generation=self.lock_generation.wrapping_add(1);
        match self.timers.pause_for_lock(self.lock_generation){
            Ok(ids)if !ids.is_empty()=>self.focus_resume=Some(FocusResume{generation:self.lock_generation,ids,prompted:false}),
            Ok(_)=>{},Err(e)=>self.error=Some(e)
        }
        self.last_awake=awake_ms();
    }
    fn on_unlock(&mut self){
        if !self.session_locked{return}
        self.session_locked=false;self.last_awake=awake_ms();
        if !self.flag("FocusPauseOnLock",false){self.focus_resume=None;return}
        let Some(mut resume)=self.focus_resume.take()else{return};
        let reason=format!("lock:{}",resume.generation);
        resume.ids.retain(|id|self.timers.items.iter().any(|t|&t.id==id&&!t.running&&!t.finished&&t.pause_reason.as_deref()==Some(reason.as_str())));
        if resume.ids.is_empty(){return}
        let notice=notifications::Notice{app_id:"LyricIsland.Focus".into(),app_name:"专注".into(),title:format!("继续刚才的 {} 项专注？",resume.ids.len()),body:"点击打开计时页，可逐项继续或暂不。".into(),icon:None};
        self.enqueue(Queued{notice,todo:Some(format!("focus:{}",resume.generation)),chime:false});
        resume.prompted=true;self.focus_resume=Some(resume);
    }
    fn fire_rule_hits(&mut self,hits:&[app_rules::Hit]){
        let (title,body)=if hits.len()==1{(hits[0].1.clone(),hits[0].2.clone())}else{(format!("{} 条应用提醒",hits.len()),hits.iter().map(|(_,name,message)|format!("{name}：{message}")).collect::<Vec<_>>().join("\n"))};
        let notice=notifications::Notice{app_id:"LyricIsland.AppRule".into(),app_name:"应用提醒".into(),title,body:body.clone(),icon:None};
        self.enqueue(Queued{notice:notice.clone(),todo:Some(format!("rule:{}",hits[0].0)),chime:true});
        if self.flag("NotificationHistoryEnabled",true){for (id,name,message) in hits{self.history.record("automation","LyricIsland.AppRule",name,name,message,Some(id.clone()));}}
    }
    /// One shared foreground listener for F05/F06. Enter events are consumed even
    /// while notifications are paused so they are never replayed later.
    unsafe fn poll_app_rules(&mut self){
        let entry=self.flag("AppEntryRemindersEnabled",false);let usage=self.flag("AppUsageRemindersEnabled",false);
        if !entry&&!usage{self.last_awake=awake_ms();return}
        let now=todos::Now::local();
        if now.date!=self.usage_date{self.rules.roll_day(&now.date);self.usage_date=now.date.clone();}
        let allowed=!self.flag("PauseNotifications",false);
        let mut hits:Vec<app_rules::Hit>=vec![];
        if let Some((path,segment))=self.foreground.poll(){if entry{hits.extend(self.rules.enter_hits(&path,segment,&now.date,&now,allowed));}}
        let awake=awake_ms();
        if usage{
            if let Some(path)=self.foreground.path.clone(){
                let delta=if self.session_locked||!allowed{0}else{awake.saturating_sub(self.last_awake).min(2000)};
                if delta>0{self.rules.accumulate(&path,delta,&now.date);}
                if allowed&&!self.session_locked{hits.extend(self.rules.due_usage(&path,&now.date));}
            }
        }
        self.last_awake=awake;
        if !hits.is_empty(){self.fire_rule_hits(&hits);}
        if self.last_rules_checkpoint.elapsed()>=Duration::from_secs(60){self.last_rules_checkpoint=Instant::now();if usage{if let Err(e)=self.rules.persist_usage(){self.error=Some(e);}}}
    }
    fn save(&mut self)->std::result::Result<(),String>{if self.demo{return Ok(())}io::save_settings(&self.data.join("settings.json"),&self.settings)}
    /// The collapsed second line: the translation when one exists, otherwise the
    /// next line for Chinese lyrics. Never invents an empty second row.
    fn compact_secondary_for(&self)->String{compact::secondary(&self.snapshot,self.flag("CompactTranslation",true),self.flag("ChineseTwoLine",false)).to_string()}
    fn compact_base(&self)->f64{self.settings["CompactWidth"].as_f64().unwrap_or(260.).clamp(140.,480.)}
    /// Size the collapsed capsule for the current lyric. The host surface stays
    /// reserved at the widest allowed width, so the window and the expanded card
    /// never move when the lyric changes; only the capsule inside it resizes.
    unsafe fn update_compact_layout(&mut self)->Result<()>{
        let base=self.compact_base();let font=self.settings["FontSize"].as_f64().unwrap_or(12.5);let double=!self.compact_secondary.is_empty();
        let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;
        let height=self.renderer.as_ref().map(|r|r.compact_height(font,double,scale)).unwrap_or(40.);
        let auto=self.flag("AutoCompactWidth",false);
        let target=if self.selected!="music"{260.}else if auto{
            let secondary_size=(font*0.78).clamp(9.,11.5);let mut widest=0.;
            if let Some(r)=self.renderer.as_mut(){
                widest=r.measure_line(&self.lyric,font)?;
                if double{widest=widest.max(r.measure_line(&self.compact_secondary,secondary_size)?);}
            }
            // Cover, activity bars and their gaps consume 76 layout units.
            (widest+76.).clamp(140.,self.settings["AutoCompactMaxWidth"].as_f64().unwrap_or(420.).clamp(140.,480.))
        }else{base};
        // Land the edges on the physical pixel grid so the final capsule is crisp.
        // Round up, never down: a hair narrower than the measured line would make
        // DirectWrite add an ellipsis. When the next line is only a fragment
        // narrower, keep the current target instead of breathing by a pixel.
        let round_up=|v:f64|(v*scale).ceil()/scale;
        let mut target=round_up(target);
        if auto&&target<self.compact_size.width_target&&self.compact_size.width_target-target<1.0{target=self.compact_size.width_target;}
        let height=round_up(height);
        self.compact_size.target(target,height,self.flag("Animation",true));
        self.layout.set_compact(self.compact_size.width,self.compact_size.height);Ok(())
    }
    /// Queue a banner. Reminders are never dropped silently: when the queue is
    /// full an older notification gives way, and a notification is refused
    /// rather than evicting a pending reminder.
    fn enqueue(&mut self,q:Queued){
        let limit=if q.todo.is_some(){32}else{8};
        while self.queue.len()>=limit{match self.queue.iter().position(|e|e.todo.is_none()){Some(i)=>{self.queue.remove(i);},None=>{if q.todo.is_none(){return}break}}}
        self.queue.push_back(q);
    }
    fn enqueue_todo(&mut self,t:&todos::Todo){let body=if t.content.trim().is_empty(){if t.daily{"每日待办提醒".into()}else{"待办提醒".into()}}else{t.content.clone()};self.enqueue(Queued{notice:notifications::Notice{app_id:TODO_APP_ID.into(),app_name:"待办".into(),title:t.title.clone(),body:body.clone(),icon:None},todo:Some(t.id.clone()),chime:true});if self.flag("NotificationHistoryEnabled",true){self.history.record("todo",TODO_APP_ID,"待办",&t.title,&body,Some(t.id.clone()));}}
    /// Only events that actually reached the user are kept: previews, paused
    /// notifications and whitelist rejects never get here.
    fn record_history(&mut self,kind:&str,notice:&notifications::Notice,todo_id:Option<String>){if self.flag("NotificationHistoryEnabled",true){self.history.record(kind,&notice.app_id,&notice.app_name,&notice.title,&notice.body,todo_id);}}
    /// Drop a reminder banner that has not been presented yet, and close it if it
    /// is on screen. Called after a todo is completed or deleted.
    fn cancel_todo(&mut self,id:&str){
        self.queue.retain(|e|e.todo.as_deref()!=Some(id));
        if self.banner_todo.as_deref()==Some(id){self.banner_todo=None;let (r,rad)=self.shape();self.banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}
    }
    fn poll(&mut self){
        let previous=self.snapshot.clone();self.last_poll=Instant::now();if let Some(b)=&self.backend{self.snapshot=b.snapshot(self.cover_version);}
        let version=self.snapshot["coverVersion"].as_u64().unwrap_or(0);
        if version!=self.cover_version{self.cover_start=Instant::now();self.cover_version=version;let cover=self.snapshot["cover"].as_str().and_then(|s|base64::engine::general_purpose::STANDARD.decode(s).ok());if let Some(r)=&mut self.renderer{if let Err(e)=unsafe{r.set_cover(cover.as_deref())}{self.error=Some(e.to_string());}}}
        let lyric=self.snapshot["frame"]["currentLine"].as_str().filter(|s|!s.is_empty()).unwrap_or("打开播放器，开始一首音乐").to_string();
        if lyric!=self.lyric{let t=(self.lyric_start.elapsed().as_secs_f64()/0.18).min(1.);self.outgoing_opacity=t;self.outgoing_y=4.*(1.-t).powi(3);self.previous_lyric=std::mem::replace(&mut self.lyric,lyric);self.lyric_start=Instant::now();}
        // A translation can arrive a beat after its line, so the second row is
        // tracked independently of the first.
        let compact_secondary=self.compact_secondary_for();
        if compact_secondary!=self.compact_secondary{self.previous_compact_secondary=std::mem::replace(&mut self.compact_secondary,compact_secondary);self.dirty=true;}
        let playing=self.snapshot["isPlaying"]==true;
        if playing!=self.activity.playing{self.activity.retarget(playing,self.start.elapsed().as_secs_f64()*1000.,self.flag("Animation",true));}
        let duration=self.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.);
        self.progress.update(self.start.elapsed().as_secs_f64(),self.snapshot["track"]["id"].as_str().unwrap_or(""),self.snapshot["positionSeconds"].as_f64().unwrap_or(0.),duration,playing,self.flag("Animation",true));
        if self.seeking&&previous["track"]["id"]!=self.snapshot["track"]["id"]{self.seeking=false;}
        if !self.demo {
            let valid=self.snapshot["track"]["title"].as_str().is_some_and(|s|!s.trim().is_empty())&&!self.snapshot["track"]["id"].as_str().unwrap_or("").ends_with("|ProcessFallback");
            let flag=island_needed(valid,self.snapshot["isPlaying"]==true,self.flag("HideWhenPaused",true),self.timers.items.iter().any(|t|t.running&&!t.finished)||self.updates.busy(),self.banner.visible()||self.osd_banner.visible()||self.updates.busy(),!self.queue.is_empty(),self.held,self.manual_until.is_some_and(|t|Instant::now()<t));
            let mut wanted=flag;
            if valid{self.missing_since=None;}
            else if !flag{let since=self.missing_since.get_or_insert(Instant::now());wanted=self.display_wanted&&since.elapsed()<Duration::from_millis(600);}
            if wanted!=self.display_wanted{
                self.display_wanted=wanted;
                if wanted{self.hide_at=None;if !self.user_hidden&&!self.banner.visible(){self.fade(1.);}}
                else{self.held=false;self.retarget(false);if !self.banner.visible(){self.hide_at=Some(Instant::now()+Duration::from_millis(if self.flag("Animation",true){220}else{0}));}}
            }
        }
        self.dirty |= previous["track"]!=self.snapshot["track"] || previous["isPlaying"]!=self.snapshot["isPlaying"] || previous["frame"]!=self.snapshot["frame"] || previous["coverVersion"]!=self.snapshot["coverVersion"];
    }
    fn hit(&self,x:f64,y:f64)->i32{
        if self.opacity<0.01{return -1}
        if self.banner.visible(){let (r,radius,_)=self.banner.presentation(self.start.elapsed().as_secs_f64());if !inside_round_rect(r,radius,x,y){return -1}return if Rect::new(r.x+r.w-32.,r.y+6.,26.,26.).contains(x,y){11}else{12}}
        if !self.expanded&&self.components.len()>1&&self.layout.satellite().contains(x,y){return 20}
        let (shape,radius)=self.shape();
        if !inside_round_rect(shape,radius,x,y){return -1}
        if self.selected!="music"{
            if self.expanded&&self.frame.card_reveal>0.7{let scale=(shape.w/self.layout.card.w).min(shape.h/self.layout.card.h);let x=(x-shape.x)/scale;let y=(y-shape.y)/scale-self.frame.content_offset_y;let w=self.layout.card.w;if self.selected=="update"{if Rect::new(24.,190.,w-48.,42.).contains(x,y){return 50}if Rect::new(w-48.,14.,30.,30.).contains(x,y){return 51}}else if self.selected=="todos"{for(i,r)in layout::todo_rows(w,self.panel.as_ref().map(|p|p.tasks.len()).unwrap_or(0)).enumerate(){if r.contains(x,y){return 40+i as i32;}}if layout::todo_footer(w).contains(x,y){return 7}}else{for(id,rect)in layout::timer_buttons(w){if rect.contains(x,y){return id}}}}
            return 0
        }
        if 1.-motion::blend(self.frame.card_reveal,0.06,0.46)>0.1&&self.layout.compact_cover(unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.}).contains(x,y){return 1}
        if self.expanded&&self.frame.content_opacity*motion::blend(self.frame.card_reveal,0.25,0.88)>0.01{
            let scale=(shape.w/self.layout.card.w).min(shape.h/self.layout.card.h);let tx=shape.x+(shape.w-self.layout.card.w*scale)/2.;let ty=shape.y+(shape.h-self.layout.card.h*scale)/2.;let x=(x-tx)/scale;let y=(y-ty)/scale-self.frame.content_offset_y;let w=self.layout.card.w;
            let g=layout::music_geometry(w,layout::MusicLayout::from_setting(self.settings["MusicExpandedLayout"].as_str().unwrap_or("")),self.music_mix,unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.},self.settings["FontSize"].as_f64().unwrap_or(12.5));
            if g.cover.contains(x,y){return 1}
            if self.flag("MusicClickOpenApp",false)&&g.text_hit.contains(x,y){return 13}
            if g.progress_hit.contains(x,y){return 10}
            for (id,r) in g.buttons{if r.contains(x,y){return id}}
        }0
    }
    unsafe fn sync_input_region(&mut self)->Result<()>{
        // The click-through visual HWND retains the complete GPU surface, including
        // shadows. This separate, undrawn HWND receives input only inside the
        // animated contour. No alpha readback or cursor polling is required.
        let (r,radius)=if self.banner.visible(){let (r,radius,_)=self.banner.presentation(self.start.elapsed().as_secs_f64());(r,radius)}else{self.shape()};
        let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;
        let satellite=if !self.expanded&&!self.banner.visible()&&self.components.len()>1{self.layout.satellite()}else{Rect::default()};
        let key=if self.opacity<0.01{[0;10]}else{[(r.x*scale).floor()as i32,(r.y*scale).floor()as i32,((r.x+r.w)*scale).ceil()as i32,((r.y+r.h)*scale).ceil()as i32,(radius*2.*scale).round()as i32,(radius*2.*scale).round()as i32,(satellite.x*scale)as i32,(satellite.y*scale)as i32,((satellite.x+satellite.w)*scale)as i32,((satellite.y+satellite.h)*scale)as i32]};
        if self.input_region==Some(key){return Ok(())}
        let region=if key[4]>0{CreateRoundRectRgn(key[0],key[1],key[2],key[3],key[4],key[5])}else{CreateRectRgn(key[0],key[1],key[2],key[3])};
        if key[8]>key[6]{let other=CreateRoundRectRgn(key[6],key[7],key[8],key[9],(satellite.w*scale)as i32,(satellite.h*scale)as i32);let _=CombineRgn(Some(region),Some(region),Some(other),RGN_OR);let _=DeleteObject(other.into());}
        if region.is_invalid(){return Err(windows::core::Error::from_thread())}
        if SetWindowRgn(self.hwnd,Some(region),false)==0{let _=DeleteObject(region.into());return Err(windows::core::Error::from_thread())}
        self.input_region=Some(key);Ok(())
    }
    unsafe fn remember_position(&mut self){let mut bounds=RECT::default();let _=GetWindowRect(self.hwnd,&mut bounds);let monitor=MonitorFromWindow(self.hwnd,MONITOR_DEFAULTTONEAREST);let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};let _=GetMonitorInfoW(monitor,&mut info);let w=info.rcWork;let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;self.settings["CenterX"]=json!(((bounds.left+bounds.right)as f64/2.-w.left as f64)/(w.right-w.left).max(1)as f64);self.settings["TopFraction"]=json!(((bounds.top-w.top)as f64+24.*scale)/((w.bottom-w.top)as f64-40.*scale).max(1.));let _=self.save();}
    unsafe fn activate(&mut self,id:i32){match id {
        50=>{if self.updates.phase()=="available"{self.request(json!({"version":1,"command":"updateDownload"}));}else if self.updates.phase()=="ready"{self.updates.action="apply".into();}else if self.updates.busy(){self.updates.action="cancel".into();}else{self.request(json!({"version":1,"command":"checkUpdate"}));}},
        51=>{let target=self.update_return.clone();self.select_component(&target,-1);self.held=false;self.retarget(false);self.manual_until=None;},
        20=>self.cycle_component(1),21=>self.cycle_component(-1),30=>self.active_timer_action("toggle"),31=>self.active_timer_action("reset"),40..=42=>{let task=self.panel.as_ref().and_then(|p|p.tasks.get((id-40)as usize)).cloned();if let Some(task)=task{let _=self.todos.complete(&task.id,true,&self.todo_now);self.cancel_todo(&task.id);self.refresh_components();}},
        11|12=>{
            let todo=self.banner_todo.clone();
            if id==12&&!self.notification_preview{
                if todo.as_ref().is_some_and(|id|id.starts_with("device:")){}else if todo.as_ref().is_some_and(|id|id.starts_with("timer:")){self.show_settings_page("timers");}else if todo.as_ref().is_some_and(|id|id.starts_with("focus:")){self.show_settings_page("timers");}else if todo.as_ref().is_some_and(|id|id.starts_with("rule:")){self.show_settings_page("notifications");}else if todo.is_some(){self.show_settings_page("todos");}
                else if self.flag("NotificationClickOpenApp",true){if let Some(n)=&self.banner.notice{if let Err(e)=activation::open(&n.app_id){self.error=Some(e);}}}
            }
            let (base,radius)=self.shape();self.banner.dismiss(base,radius,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));},
        0=>{if self.expanded{self.held=false;self.retarget(false)}else if self.flag("ClickAnywhereExpand",false){self.held=true;self.retarget(true)}},
        1|13=>{if self.flag("MusicClickOpenApp",false){if let Err(e)=activation::open(self.snapshot["sourceId"].as_str().unwrap_or("")){self.error=Some(e);}}else if id==1{self.held=!self.held;self.retarget(self.held)}},2=>{if self.flag("Floating",false){let mut cursor=POINT::default();let mut bounds=RECT::default();let _=GetCursorPos(&mut cursor);let _=GetWindowRect(self.hwnd,&mut bounds);self.dragging=Some((cursor,bounds));SetCapture(self.hwnd);}},
        3=>self.command("previous",0.),4=>{if self.demo{self.snapshot["isPlaying"]=json!(self.snapshot["isPlaying"]!=true)}else{self.command("toggle",0.)}},5=>self.command("next",0.),
        6=>{self.settings["Translation"]=json!(!self.flag("Translation",true));let _=self.save();},7=>{if self.selected=="todos"{self.show_settings_page("todos")}else if self.selected.starts_with("timer:"){self.show_settings_page("timers")}else{self.show_settings()}},
        9=>{let title=self.snapshot["track"]["title"].as_str().unwrap_or("").to_string();let source=self.snapshot["track"]["sourceApp"].as_str().unwrap_or("").to_string();std::thread::spawn(move||{shell::favorite(&title,&source,true);});},_=>{}}
        self.dirty=true;
    }
    fn update_seek(&mut self,x:f64){let (shape,_)=self.shape();let scale=(shape.w/self.layout.card.w).min(shape.h/self.layout.card.h);let tx=shape.x+(shape.w-self.layout.card.w*scale)/2.;let g=layout::music_geometry(self.layout.card.w,layout::MusicLayout::from_setting(self.settings["MusicExpandedLayout"].as_str().unwrap_or("")),self.music_mix,unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.},self.settings["FontSize"].as_f64().unwrap_or(12.5));self.seek_fraction=((x-tx-g.track.x*scale)/(g.track.w*scale)).clamp(0.,1.);self.dirty=true;}
    fn progress_value(&self)->f64{self.progress.fraction(self.start.elapsed().as_secs_f64())}
    fn scene(&self)->Scene<'_>{
        let elapsed=self.progress.seconds(self.start.elapsed().as_secs_f64());let duration=self.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.);let track=&self.snapshot["track"];let f=&self.snapshot["frame"];
        // Sampled once: two calls per frame meant the bars and their opacity were read at
        // two different instants, so the analyser could disagree with itself mid-fade.
        let activity=self.activity.sample(self.start.elapsed().as_secs_f64()*1000.,self.flag("Animation",true));
        let col=|k:&str,default:u32|self.settings[k].as_str().and_then(|s|u32::from_str_radix(s.trim_start_matches('#'),16).ok()).map(|v|0xff000000|v).unwrap_or(default);
        let (notice_rect,notice_radius,notice_mix)=self.banner.presentation(self.start.elapsed().as_secs_f64());
        let (osd_rect,osd_radius,osd_mix)=self.osd_banner.presentation(self.start.elapsed().as_secs_f64());
        Scene{glass_material:self.flag("IslandGlassMaterial",false),fluid_motion:self.flag("IslandFluidMotion",true),drag_panel:self.drag_panel.as_ref(),drag_active:self.swipe.is_some_and(|s|s.active),switch_outgoing:self.slide_outgoing,switch_direction:self.slide_direction,auxiliary_kind:self.components.iter().position(|c|c==&self.selected).and_then(|i|self.components.get((i+1)%self.components.len())).map(|s|if s=="todos"{2}else if s.starts_with("timer:"){1}else{0}).unwrap_or(0),panel:self.panel.as_ref(),previous_panel:self.previous_panel.as_ref(),component_count:self.components.len(),switching:self.switching,switch_offset:self.slide.value,swipe_offset:self.swipe_offset(),lyric_cover_color:self.flag("LyricColorFromCover",true),cover_background:self.flag("CoverBackground",true),demo:self.demo,animate:self.flag("Animation",true),notification:self.banner.notice.as_ref(),notification_rect:notice_rect,notification_radius:notice_radius,notification_mix:notice_mix,osd:if self.banner.visible(){None}else{self.osd_banner.notice.as_ref()},osd_rect,osd_radius,osd_mix,layout:self.layout,frame:self.frame,radius:self.options().radius,resizing:self.transition.is_some(),time_ms:self.start.elapsed().as_secs_f64()*1000.,lyric:&self.lyric,previous_lyric:&self.previous_lyric,compact_secondary:&self.compact_secondary,previous_compact_secondary:&self.previous_compact_secondary,lyric_age_ms:if self.flag("Animation",true){self.lyric_start.elapsed().as_secs_f64()*1000.}else{1000.},secondary:if self.flag("Translation",true){f["currentTranslation"].as_str().filter(|s|!s.is_empty()).or_else(||f["nextLine"].as_str()).unwrap_or("等待歌词")}else{f["nextLine"].as_str().unwrap_or("等待歌词")},title:track["title"].as_str().unwrap_or("暂无播放"),artist:track["artist"].as_str().unwrap_or("打开播放器，开始一首音乐"),playing:self.snapshot["isPlaying"]==true,highlight:if self.flag("WordHighlight",true){f["wordScanProgress"].as_f64().unwrap_or(0.).clamp(0.,1.)}else{0.},progress:if self.seeking{self.seek_fraction}else{self.progress_value()},elapsed:if self.seeking{self.seek_fraction*duration}else{elapsed},duration,font_size:self.settings["FontSize"].as_f64().unwrap_or(12.5),music_layout:layout::MusicLayout::from_setting(self.settings["MusicExpandedLayout"].as_str().unwrap_or("")),music_mix:self.music_mix,text_color:col("ExpandedTextColor",0xfff4f6f8),component_color:col("ExpandedComponentColor",0xffb1e8d1),progress_color:col("LyricProgressColor",0xffa9e6cd),hover:self.hover,pressed:self.pressed,opacity:self.opacity as f32,transparent:self.transparent,floating:self.flag("Floating",false),favorite_enabled:track["sourceApp"].as_str().is_some_and(|s|s.eq_ignore_ascii_case("Netease")),has_track:track.is_object(),layout_scale:unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.},activity_scales:activity.0,activity_opacity:activity.1,cover_opacity:1.,cover_age_ms:self.cover_start.elapsed().as_secs_f64()*1000.,outgoing_opacity:self.outgoing_opacity,outgoing_y:self.outgoing_y}
    }
    unsafe fn draw(&mut self)->Result<()>{// Take the renderer to split immutable scene data from mutable graphics state.
        if let Some(mut r)=self.renderer.take(){let result=r.render(&self.scene(),true);self.renderer=Some(r);result?;self.render_frames+=1;}self.dirty=false;Ok(())
    }
    fn show_notice(&mut self,notice:notifications::Notice,preview:bool,todo:Option<String>,chime:bool){
        if !preview&&todo.is_none(){let policy=notifications::Policy::from_settings(&self.settings);if !policy.should_show()||!policy.permits(&notice.app_id,&notice.app_name){return}}
        let device=notice.app_id.starts_with("LyricIsland.Device");
        self.notification_preview=preview;self.banner_todo=todo.clone();
        let now=self.start.elapsed().as_secs_f64();
        if self.osd_banner.visible(){let (r,rad)=self.shape();self.osd_banner.dismiss(r,rad,now,false);}
        let (base,radius)=self.shape();
        if let Some(r)=&mut self.renderer{let _=unsafe{r.set_notification_icon(notice.icon.as_deref())};}
        let width=440_f64.min(self.layout.host.w-48.);let height=self.renderer.as_mut().and_then(|r|unsafe{r.notification_height(&notice,width)}.ok()).unwrap_or(80.);
        self.banner.show(notice,base,radius,self.layout.host.w,height,now,self.flag("Animation",true));
        // The chime is bound to a banner actually appearing. Previews, paused
        // notifications, whitelist rejects and repeats never reach this point.
        if !preview&&chime{if let Err(e)=if device{sound::play_device(&self.settings,false)}else{sound::play(&self.settings,todo.is_some(),false)}{self.error=Some(e);}}
        self.hide_at=None;self.fade(1.);self.dirty=true;
    }
    /// System hints share the island surface but never the notification queue: they are
    /// ephemeral, so a newer state replaces the previous one instead of stacking.
    fn show_osd(&mut self,hint:osd::Hint,preview:bool){
        if !preview&&(!self.flag("SystemStateHintsEnabled",true)||self.flag("PauseNotifications",false)){return}
        if self.demo&&!preview{return}
        if self.settings["SystemStateHintPlacement"].as_str()==Some("Bottom"){
            let font=self.base.join("Assets/Fonts/NotoSansSC.ttf");let animate=self.flag("Animation",true);
            if let Err(e)=unsafe{self.bottom_osd.show(hint.notice(),self.hwnd,&font,animate)}{self.error=Some(e.to_string());}return
        }
        if self.user_hidden&&!preview{return}
        let now=self.start.elapsed().as_secs_f64();
        // A message the user is reading always wins; the hint is transient and is dropped
        // instead of cutting the notification short.
        if self.banner.visible(){
            if !preview{return}
            let (r,rad)=self.shape();self.banner.dismiss(r,rad,now,false);
        }
        let (base,radius)=self.shape();
        self.osd_banner.show_compact(hint.notice(),base,radius,self.layout.host.w,now,self.flag("Animation",true));
        self.hide_at=None;self.fade(1.);self.dirty=true;
    }
    fn flush_osd(&mut self){
        if !self.flag("SystemStateHintsEnabled",true){self.pending_osd=0;return}
        let pending=std::mem::take(&mut self.pending_osd);
        if pending==0{return}
        for hint in self.osd.flush(pending,true,true,true){self.show_osd(hint,false);}
    }
    fn apply_osd(&mut self){
        if self.demo{return}
        if self.flag("SystemStateHintsEnabled",true){unsafe{self.osd.install(self.hwnd);}}
        else{
            self.osd.uninstall();self.pending_osd=0;unsafe{self.bottom_osd.clear();}
            if self.osd_banner.visible(){let now=self.start.elapsed().as_secs_f64();let (r,rad)=self.shape();self.osd_banner.dismiss(r,rad,now,false);}
        }
    }
    unsafe fn tick(&mut self)->Result<()> {
        self.bottom_osd.tick()?;
        if self.updates.tick(self.flag("AutoCheckUpdatesEnabled",true),self.demo){self.dirty=true;}
        if self.updates.reminder{self.updates.reminder=false;if !self.flag("PauseNotifications",false){if self.selected!="update"{self.update_return=self.selected.clone();}self.refresh_components();self.select_component("update",1);self.update_until=Some(Instant::now()+Duration::from_secs(8));self.user_hidden=false;self.fade(1.);}}
        if self.update_until.is_some_and(|until|Instant::now()>until)&&!self.updates.busy()&&!self.held{self.update_until=None;let previous=self.update_return.clone();self.select_component(&previous,-1);self.manual_until=None;}
        let gates=devices::Gates{master:self.flag("DeviceRemindersEnabled",true)&&!self.flag("PauseNotifications",false),disconnect:self.flag("DeviceDisconnectRemindersEnabled",true),headphones:self.flag("HeadphoneConnectionRemindersEnabled",true),speaker:self.flag("SpeakerConnectionRemindersEnabled",true),gamepad:self.flag("GamepadConnectionRemindersEnabled",true),mouse:self.flag("MouseConnectionRemindersEnabled",true),keyboard:self.flag("KeyboardConnectionRemindersEnabled",true)};
        for event in self.devices.poll(&gates){let notice=event.notice();self.enqueue(Queued{notice:notice.clone(),todo:Some(format!("device:{}",event.key)),chime:true});self.record_history("device",&notice,None);}
        if self.flag("SystemStateHintsEnabled",true)&&!self.flag("PauseNotifications",false){for hint in self.osd.poll(true,true){self.show_osd(hint,false);}}else{self.osd.poll(false,false);}
        let now=Instant::now();let dt=now.duration_since(self.last_timer);self.last_timer=now;
        let before=self.dismiss_offset.value;
        if self.hide_drag.is_none(){if self.user_hidden&&self.opacity>0.001&&before<0.{self.dismiss_offset.set((before-dt.as_secs_f64()*180.).max(-150.));}else{self.dismiss_offset.tick(dt.as_secs_f64());}}
        if (before-self.dismiss_offset.value).abs()>0.01||self.hide_drag.is_some_and(|d|d.active){let mut rect=RECT::default();GetWindowRect(self.hwnd,&mut rect)?;let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;SetWindowPos(self.visual_hwnd,None,rect.left,rect.top+(self.dismiss_offset.value*scale).round()as i32,0,0,SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOSIZE)?;self.dirty=true;}
        let nanos=dt.as_nanos()+self.timer_fraction;self.timer_fraction=nanos%1_000_000;
        let events=self.timers.advance((nanos/1_000_000).min(u64::MAX as u128)as u64);
        if !events.is_empty(){for t in events{let notice=notifications::Notice{app_id:"LyricIsland.Timer".into(),app_name:"计时器".into(),title:t.name.clone(),body:if t.finished{"计时已完成".into()}else{t.subtitle()},icon:None};self.enqueue(Queued{notice:notice.clone(),todo:Some(format!("timer:{}",t.id)),chime:true});self.record_history("timer",&notice,None);}if let Err(e)=self.timers.persist(){self.error=Some(e);}}
        if self.last_checkpoint.elapsed()>=Duration::from_secs(15){if self.timers.items.iter().any(|t|t.running){let _=self.timers.persist();}self.last_checkpoint=Instant::now();}
        if self.last_history_prune.elapsed()>=Duration::from_secs(3600){self.last_history_prune=Instant::now();self.history.prune(self.settings["NotificationHistoryDays"].as_u64().unwrap_or(7).clamp(1,30)as u32,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()as i64);}
        let old_panel=self.panel.clone();self.refresh_components();if self.panel!=old_panel{self.dirty=true;}
        if self.switching&&self.slide.tick(dt.as_secs_f64()){self.dirty=true;}if self.slide.value==0.{self.switching=false;}

        let events=self.notification_listener.as_ref().map(|l|l.events.try_iter().collect::<Vec<_>>()).unwrap_or_default();
        for event in events{match event{
            notifications::Event::State(state)=>{self.notification_state=state;},
            // Banners are queued rather than shown directly: showing each one on
            // arrival would make a burst of messages overwrite the previous one.
            notifications::Event::Notice(notice)=>{let policy=notifications::Policy::from_settings(&self.settings);if policy.should_show()&&policy.permits(&notice.app_id,&notice.app_name){if !self.user_hidden{self.enqueue(Queued{notice:notice.clone(),todo:None,chime:true});}self.record_history("notice",&notice,None);}}
        }}
        if self.flag("PauseNotifications",false){
            if self.banner.visible(){let (r,rad)=self.shape();self.banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}
            if self.selected=="update"&&self.opacity>0.001&&self.panel.as_ref().is_some_and(|p|p.running&&p.progress<0.)&&self.flag("Animation",true){self.dirty=true;}
        if self.osd_banner.visible(){let (r,rad)=self.shape();self.osd_banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}self.bottom_osd.clear();
            // Nothing may be presented or chimed while paused. Reminders stay
            // unmarked, so they are re-checked once notifications resume.
            self.queue.retain(|e|e.todo.is_some());
        }
        if self.last_poll.elapsed()>=Duration::from_millis(90){self.poll()}
        if let Some(t)=self.transition{self.sampled_ms=self.transition_start.elapsed().as_secs_f64()*1000.;self.frame=t.sample(self.sampled_ms);self.dirty=true;if self.sampled_ms>=t.duration_ms{self.transition=None;self.frame=t.end;}}
        let target=self.music_target();
        if (self.music_mix-target).abs()>0.0001{let t=if self.flag("Animation",true){(self.music_start.elapsed().as_secs_f64()/0.23).min(1.)}else{1.};let e=t*t*(3.-2.*t);self.music_mix=self.music_from+(target-self.music_from)*e;if t>=1.{self.music_mix=target;}self.dirty=true;}
        let target_height=if self.selected=="update"{264.}else if self.panel.is_some(){200.}else{layout::music_height(self.music_mix,self.settings["FontSize"].as_f64().unwrap_or(12.5))};
        if (self.layout.card.h-target_height).abs()>0.01{let weight=if self.flag("Animation",true){1.-(-dt.as_secs_f64()*26.).exp()}else{1.};self.layout.card.h+=(target_height-self.layout.card.h)*weight;if (self.layout.card.h-target_height).abs()<0.1{self.layout.card.h=target_height;}self.dirty=true;}
        if !self.banner.visible()&&self.hide_at.is_some_and(|t|Instant::now()>=t){self.hide_at=None;self.fade(0.);}
        if (self.opacity-self.fade_to).abs()>0.00001{let t=if self.flag("Animation",true){(self.fade_start.elapsed().as_secs_f64()/0.2).min(1.)}else{1.};
            // A symmetric sine starts slow, and an appearing surface is exactly what the eye
            // is locked onto at that instant. The fluid curve decelerates into place instead,
            // so the island is already most of the way visible by the time it is noticed.
            let eased=if self.flag("IslandFluidMotion",true){motion::cubic_out(t)}else{(1.-(std::f64::consts::PI*t).cos())/2.};
            self.opacity=self.fade_from+(self.fade_to-self.fade_from)*eased;self.dirty=true;}
        let mut pt=POINT::default();let _=GetCursorPos(&mut pt);let mut local=pt;let _=ScreenToClient(self.hwnd,&mut local);let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;let x=local.x as f64/scale;let y=local.y as f64/scale;
        let hit=self.hit(x,y);let inside=hit>=0;
        if inside{if let Some(t)=self.manual_until.as_mut(){*t=Instant::now()+Duration::from_secs(6);}}
        if !self.banner.visible()&&inside&&!self.cursor_inside{self.leave_at=None;if self.flag("HoverExpand",false){self.retarget(true)}}
        if !inside&&self.cursor_inside&&!self.held{self.leave_at=Some(Instant::now());}
        if self.swipe.is_none()&&!self.banner.visible()&&self.leave_at.is_some_and(|t|t.elapsed()>=Duration::from_millis(1200)){self.leave_at=None;if !self.held{self.retarget(false)}}
        self.cursor_inside=inside;if self.hover!=hit{self.hover=hit;self.dirty=true;}
        let left=GetAsyncKeyState(VK_LBUTTON.0 as i32)<0;if !self.banner.visible()&&left&&!self.last_left&&!inside&&self.expanded{self.held=false;self.retarget(false)}self.last_left=left;
        if self.opacity>0.001&&(self.snapshot["isPlaying"]==true&&self.flag("Animation",true)||self.lyric_start.elapsed()<Duration::from_millis(220)||(self.expanded&&self.flag("Animation",true)&&self.cover_start.elapsed()<Duration::from_millis(300))){self.dirty=true;}
        if self.banner.visible(){
            let (r,rad)=self.shape();
            self.banner.tick(r,rad,self.start.elapsed().as_secs_f64(),inside,self.flag("Animation",true));self.dirty=true;
            if !self.banner.visible(){self.banner_todo=None;self.last_poll=Instant::now()-Duration::from_secs(1);}
        }
        if self.osd_banner.visible(){let (r,rad)=self.shape();self.osd_banner.tick(r,rad,self.start.elapsed().as_secs_f64(),false,self.flag("Animation",true));self.dirty=true;}
        // Collapsed capsule: retarget only when an input actually changed, then
        // advance the spring. Measurement is cached inside the renderer.
        let compact_key=format!("{}|{}|{}|{}|{}|{}|{}|{}",self.selected,self.lyric,self.compact_secondary,self.settings["FontSize"],self.settings["CompactWidth"],self.flag("AutoCompactWidth",false),self.settings["AutoCompactMaxWidth"],self.flag("Animation",true));
        if compact_key!=self.compact_key{self.compact_key=compact_key;if let Err(e)=self.update_compact_layout(){self.error=Some(e.to_string());}}
        let dt=self.last_compact.elapsed().as_secs_f64();self.last_compact=Instant::now();
        if self.compact_size.moving()&&self.compact_size.tick(dt,self.options().duration){self.layout.set_compact(self.compact_size.width,self.compact_size.height);self.dirty=true;}
        // Reminders are checked once a second, not per rendered frame.
        if !self.demo&&self.flag("TodoRemindersEnabled",true)&&!self.flag("PauseNotifications",false)&&self.last_schedule.elapsed()>=Duration::from_secs(1){
            self.last_schedule=Instant::now();self.todo_now=todos::Now::local();
            match self.todos.poll(&self.todo_now){Ok(due)=>{for t in due{self.enqueue_todo(&t);}},Err(e)=>{self.error=Some(e);}}
        }
        if !self.demo{self.poll_app_rules();}
        // One banner at a time; the next one starts once the previous is gone.
        if !self.banner.visible()&&!self.user_hidden&&!self.flag("PauseNotifications",false){if let Some(q)=self.queue.pop_front(){self.show_notice(q.notice,false,q.todo,q.chime);}}
        if let Some((start,bounds))=self.dragging{if left{let dx=pt.x-start.x;let dy=pt.y-start.y;for hwnd in [self.visual_hwnd,self.hwnd]{SetWindowPos(hwnd,None,bounds.left+dx,bounds.top+dy,0,0,SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOSIZE)?;}}else{self.dragging=None;let _=ReleaseCapture();self.remember_position();}}
        self.sync_input_region()?;if self.dirty{if self.opacity>0.001 || self.fade_from>0.001 {self.draw()?;if self.opacity<=0.001{self.fade_from=0.;}}else{self.dirty=false;}}Ok(())
    }
    unsafe fn request(&mut self,r:Value)->Value {
        if r["version"]!=1{return json!({"success":false,"message":"请求版本无效"})}let command=r["command"].as_str().unwrap_or("");
        match command {
            "state"=>{self.todo_now=todos::Now::local();let t=&self.snapshot["track"];json!({"success":true,"version":env!("CARGO_PKG_VERSION"),"settings":self.settings,"softwareUpdate":self.updates.state,"dataDirectory":self.data.to_string_lossy(),"title":t["title"],"artist":t["artist"],"player":t["sourceApp"],"position":self.snapshot["positionSeconds"],"duration":t["durationSeconds"],"timeline":self.snapshot["clockStatus"],"lyricSource":self.snapshot["lyricSource"],"isPlaying":self.snapshot["isPlaying"],"canSeek":self.snapshot["canSeek"],"expanded":self.expanded,"islandVisible":self.opacity>0.001,"processId":std::process::id(),"rendererGeneration":self.renderer_generation,"renderFrames":self.render_frames,"renderer":"Direct2D/DirectWrite/DirectComposition","lastError":self.error,"notificationState":self.notification_state,"notificationVisible":self.banner.visible(),"osdVisible":self.osd_banner.visible()||self.bottom_osd.visible(),"bottomOsdVisible":self.bottom_osd.visible(),"systemState":self.osd.status(),"todos":self.todos.list(&self.todo_now),"todoError":self.todos.error,"today":self.todo_now.date,"now":self.todo_now.time,"pendingPage":self.pending_page,"compactWidth":self.compact_size.width,"compactHeight":self.compact_size.height,"compactSecondary":self.compact_secondary,"timers":self.timers.list(),"timerError":self.timers.error,"activeComponent":self.selected,"components":self.components,"notificationHistory":self.history.list(),"historyError":self.history.error,"quickHotkey":self.settings["QuickReminderHotkey"],"pendingAction":self.pending_action,"sessionLocked":self.session_locked,"focusResume":self.focus_resume.as_ref().map(|r|json!({"generation":r.generation,"ids":r.ids,"prompted":r.prompted})),"templates":self.templates.list(),"workflowTemplateError":self.templates.error,"appRules":self.rules.list(&self.todo_now.date),"appRuleError":self.rules.error,"foreground":{"path":self.foreground.path,"name":self.foreground.name,"unreadable":self.foreground.unreadable},"lockWatch":true})},
            "updateTestPrepare" if std::env::var("LYRICISLAND_TEST_PIPE").is_ok_and(|p|p.starts_with("LyricIsland.Settings.test."))=>{self.updates.nonce="integration-test-nonce".into();self.updates.job="integration-test-job".into();self.updates.action.clear();self.updates.release=json!({"newer":true,"version":env!("CARGO_PKG_VERSION")});self.updates.state=json!({"phase":"downloading","seq":0});self.refresh_components();json!({"success":true,"nonce":self.updates.nonce,"jobId":self.updates.job})},
            "showUpdate"=>{self.refresh_components();if self.updates.selectable(){self.select_component("update",1);self.held=true;self.retarget(true);self.user_hidden=false;self.fade(1.);json!({"success":true})}else{json!({"success":false,"message":"当前没有更新任务，请先检查更新。"})}},
            "openInstaller"=>{match std::process::Command::new(self.base.join("DotIsle.Setup.exe")).arg(format!("/MIGRATEDATA={}",self.data.display())).spawn(){Ok(_)=>json!({"success":true}),Err(e)=>json!({"success":false,"message":e.to_string()})}},
            "checkUpdate"=>{self.updates.check(true);json!({"success":true,"message":"正在检查双源更新…"})},
            "updateDownload"|"installUpdate"=>{let pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into());match self.updates.begin(&self.base,&self.data,&pipe){Ok(())=>{if self.selected!="update"{self.update_return=self.selected.clone();}self.refresh_components();self.select_component("update",1);self.held=true;self.retarget(true);self.user_hidden=false;self.fade(1.);json!({"success":true,"message":"开始下载；校验后等待你确认安装。"})},Err(e)=>json!({"success":false,"message":e})}},
            "updateApply"=>{if self.updates.phase()!="ready"{json!({"success":false,"message":"更新尚未就绪"})}else{self.updates.action="apply".into();json!({"success":true})}},
            "updateCancel"=>{if matches!(self.updates.phase(),"applying"|"restarting"){json!({"success":false,"message":"安装期间不可取消"})}else{self.updates.action="cancel".into();json!({"success":true})}},
            "updateProgress"=>{match self.updates.progress(&r["value"]){Ok(reply)=>{if matches!(self.updates.phase(),"cancelled"|"failed"|"completed"){self.held=false;self.update_until=Some(Instant::now()+Duration::from_secs(5));}self.dirty=true;reply},Err(e)=>json!({"success":false,"message":e})}},
            "deviceSoundPreview"=>{match sound::play_device(&self.settings,true){Ok(())=>json!({"success":true}),Err(e)=>json!({"success":false,"message":e})}},
            "devicePreview"=>{let kind=match r["value"]["kind"].as_str().unwrap_or("usb"){"phone"=>devices::Kind::Phone,"volume"=>devices::Kind::Volume,"headphones"=>devices::Kind::Headphones,"speaker"=>devices::Kind::Speaker,"gamepad"=>devices::Kind::Gamepad,"mouse"=>devices::Kind::Mouse,"keyboard"=>devices::Kind::Keyboard,_=>devices::Kind::Usb};let connected=r["value"]["connected"].as_bool().unwrap_or(true);let label=match kind{devices::Kind::Phone=>"手机 · 示例设备",devices::Kind::Volume=>"移动磁盘 · 示例设备",devices::Kind::Headphones=>"耳机 · 示例设备",devices::Kind::Speaker=>"音响 · 示例设备",devices::Kind::Gamepad=>"手柄 · 示例设备",devices::Kind::Mouse=>"鼠标 · 示例设备",devices::Kind::Keyboard=>"键盘 · 示例设备",devices::Kind::Usb=>"U 盘 · 示例设备"};let e=devices::Event::new("preview".into(),kind,connected,label.into());self.show_notice(e.notice(),true,Some("device:preview".into()),true);json!({"success":true})},
            "osdPreview"=>{let kind=match r["value"]["kind"].as_str().unwrap_or("caps"){"num"=>osd::Kind::Num,"mic"=>osd::Kind::Mic,"touchpad"=>osd::Kind::Touchpad,_=>osd::Kind::Caps};let on=r["value"]["on"].as_bool().unwrap_or(true);self.show_osd(osd::Hint{kind,on},true);json!({"success":true})},
            "timerCreate"=>{let mut value=r["value"].clone();if value["name"].as_str().is_none_or(|s|s.trim().is_empty()){if let Some(title)=value["todoId"].as_str().and_then(|id|self.todos.items.iter().find(|t|t.id==id)).map(|t|t.title.clone()){value["name"]=json!(title);}}match self.timers.create(&value){Ok(id)=>{self.refresh_components();self.select_component(&format!("timer:{id}"),1);self.user_hidden=false;self.hide_at=None;self.fade(1.);self.held=true;self.retarget(true);json!({"success":true,"id":id})},Err(e)=>json!({"success":false,"message":e})}},
            "timerAction"=>{let id=r["value"]["id"].as_str().unwrap_or("");let action=r["value"]["action"].as_str().unwrap_or("");let result=if action=="direction"{r["value"]["countUp"].as_bool().ok_or_else(||"计时方向无效".to_owned()).and_then(|up|self.timers.set_direction(id,up,r["value"]["revision"].as_u64()))}else{self.timers.act(id,action,r["value"]["revision"].as_u64())};match result{Ok(())=>{self.refresh_components();self.compact_key.clear();json!({"success":true,"timers":self.timers.list()})},Err(e)=>json!({"success":false,"message":e})}},
            "quickReminderPreview"=>{let text=r["value"]["text"].as_str().unwrap_or("").to_owned();self.quick_preview(&text)},
            "quickReminderCommit"=>{let token=r["value"]["previewToken"].as_str().unwrap_or("");let request=r["value"]["clientRequestId"].as_str().unwrap_or("");self.quick_commit(token,request)},
            "pendingActionSeen"=>{self.pending_action=None;json!({"success":true})},
            "focusResume"=>{
                let ids=r["value"]["ids"].as_array().map(|a|a.iter().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default();let generation=r["value"]["generation"].as_u64().unwrap_or(0);
                if !self.flag("FocusPauseOnLock",false){return json!({"success":false,"message":"锁屏暂停专注已关闭"})}
                match self.timers.resume_focus(&ids,generation){
                    Ok(resumed)=>{if let Some(f)=&mut self.focus_resume{f.ids.retain(|id|!resumed.contains(id));}if self.focus_resume.as_ref().is_some_and(|f|f.ids.is_empty()){self.focus_resume=None;}self.refresh_components();json!({"success":true,"timers":self.timers.list(),"resumed":resumed})},
                    Err(e)=>json!({"success":false,"message":e})
                }
            },
            "focusResumeDismiss"=>{self.focus_resume=None;json!({"success":true})},
            "workflowTemplateSave"=>{match self.templates.save(&r["value"]){Ok(id)=>json!({"success":true,"id":id,"templates":self.templates.list()}),Err(e)=>json!({"success":false,"message":e})}},
            "workflowTemplateDelete"=>{match self.templates.delete(r["value"].as_str().unwrap_or("")){Ok(())=>json!({"success":true,"templates":self.templates.list()}),Err(e)=>json!({"success":false,"message":e})}},
            "appRuleSave"=>{
                self.todo_now=todos::Now::local();let today=self.todo_now.date.clone();
                match self.rules.save(&r["value"],&today,self.foreground.path.as_deref(),self.foreground.segment){Ok(id)=>json!({"success":true,"id":id,"rules":self.rules.list(&today)}),Err(e)=>json!({"success":false,"message":e})}
            },
            "appRuleAction"=>{
                self.todo_now=todos::Now::local();let today=self.todo_now.date.clone();let id=r["value"]["id"].as_str().unwrap_or("").to_owned();let action=r["value"]["action"].as_str().unwrap_or("").to_owned();
                match self.rules.action(&id,&action,r["value"]["revision"].as_u64(),&today){Ok(())=>{if action=="delete"{let key=format!("rule:{id}");self.queue.retain(|e|e.todo.as_deref()!=Some(key.as_str()));}json!({"success":true,"rules":self.rules.list(&today)})},Err(e)=>json!({"success":false,"message":e})}
            },
            "appRuleClearUsage"=>{match self.rules.clear_usage(){Ok(())=>json!({"success":true,"rules":self.rules.list(&self.todo_now.date)}),Err(e)=>json!({"success":false,"message":e})}},
            "foregroundInfo"=>json!({"success":true,"path":self.foreground.path,"name":self.foreground.name,"unreadable":self.foreground.unreadable,"segment":self.foreground.segment}),
            "islandSelect"=>{self.refresh_components();let id=r["value"].as_str().unwrap_or("");if !self.components.iter().any(|c|c==id){json!({"success":false,"message":"组件不存在"})}else{self.select_component(id,1);self.user_hidden=false;self.fade(1.);json!({"success":true})}},
            "islandCycle"=>{self.cycle_component(if r["value"]==json!(-1){-1}else{1});json!({"success":true})},
            "todos"=>json!({"success":true,"todos":self.todos.list(&self.todo_now),"today":self.todo_now.date,"message":self.todos.error}),
            "todoSave"=>{if self.demo{return json!({"success":true,"id":"demo"})}self.todo_now=todos::Now::local();match self.todos.save(&r["value"]){Ok(id)=>json!({"success":true,"id":id,"todos":self.todos.list(&self.todo_now),"message":"已保存待办"}),Err(e)=>json!({"success":false,"message":e})}},
            "todoComplete"=>{
                if self.demo{return json!({"success":true})}
                let id=r["value"]["id"].as_str().unwrap_or("").to_owned();let done=r["value"]["completed"].as_bool().unwrap_or(true);
                self.todo_now=todos::Now::local();match self.todos.complete(&id,done,&self.todo_now){Ok(())=>{if done{self.cancel_todo(&id);}json!({"success":true,"todos":self.todos.list(&self.todo_now),"message":if done{"已完成"}else{"已恢复待办"}})},Err(e)=>json!({"success":false,"message":e})}
            },
            "todoSnooze"=>{
                if self.demo{return json!({"success":true})}
                let id=r["value"]["id"].as_str().unwrap_or("").to_owned();let minutes=r["value"]["minutes"].as_u64().unwrap_or(0)as u32;
                self.todo_now=todos::Now::local();
                match self.todos.snooze(&id,minutes,&self.todo_now){Ok(())=>{self.cancel_todo(&id);json!({"success":true,"todos":self.todos.list(&self.todo_now),"message":if minutes>=60{"已推迟 1 小时后提醒"}else{"已推迟 10 分钟后提醒"}})},Err(e)=>json!({"success":false,"message":e})}
            },
            "todoDelete"=>{
                if self.demo{return json!({"success":true})}
                let id=r["value"]["id"].as_str().unwrap_or("").to_owned();
                match self.todos.delete(&id){Ok(())=>{self.cancel_todo(&id);json!({"success":true,"todos":self.todos.list(&self.todo_now),"message":"已删除"})},Err(e)=>json!({"success":false,"message":e})}
            },
            "historyClear"=>{
                let kind=r["value"]["kind"].as_str().filter(|k|!k.is_empty());
                if let Some(kind)=kind{if !history::kinds().contains(&kind){return json!({"success":false,"message":"未知的通知分类"})}}
                self.history.clear(kind);json!({"success":true,"history":self.history.list(),"message":"已清除通知历史"})
            },
            "historyOpen"=>{
                let id=r["value"].as_str().unwrap_or("");
                match self.history.items.iter().find(|e|e.id==id){
                    Some(event)if event.kind=="notice"&&!event.app_id.is_empty()=>match activation::open(&event.app_id){Ok(())=>json!({"success":true,"message":"已打开来源应用"}),Err(e)=>json!({"success":false,"message":e})},
                    Some(_)=>json!({"success":false,"message":"该记录没有可打开的来源应用"}),
                    None=>json!({"success":false,"message":"通知记录不存在"})
                }
            },
            "pageSeen"=>{self.pending_page=None;json!({"success":true})},
            "collectLogs"=>{match diagnostics::collect(&self.data,self.error.as_deref(),env!("CARGO_PKG_VERSION")){
                Ok(path)=>{if std::env::var_os("LYRICISLAND_TEST_PIPE").is_none(){diagnostics::reveal(&path);}json!({"success":true,"message":format!("诊断日志已生成：{}（已在资源管理器中选中）",path.file_name().map(|n|n.to_string_lossy().to_string()).unwrap_or_default()),"path":path.to_string_lossy()})},
                Err(e)=>json!({"success":false,"message":format!("生成诊断日志失败：{e}")})
            }},
            "soundPreview"=>{
                let todo=r["value"]["todo"].as_bool().unwrap_or(false);
                match sound::play(&self.settings,todo,true){Ok(())=>json!({"success":true,"message":"已播放试听"}),Err(e)=>json!({"success":false,"message":e})}
            },
            "expand"=>{self.user_hidden=false;self.fade(1.);self.held=true;self.retarget(true);json!({"success":true})},
            "toggleVisibility"=>{self.dismiss_offset.set(0.);let _=self.position();self.user_hidden=!self.user_hidden;if !self.user_hidden{self.manual_until=Some(Instant::now()+Duration::from_secs(6));}self.fade(if self.user_hidden{0.}else{1.});json!({"success":true})},
            "notificationPreview"=>{if self.flag("PauseNotifications",false){return json!({"success":false,"message":"通知已暂停，请先关闭“暂停通知”后预览"})}self.show_notice(notifications::Notice::example(&self.base),true,None,true);json!({"success":true,"message":"通知横幅预览"})},
            "toggleNotifications"=>{
                if !self.flag("NotificationsEnabled",true){return self.request(json!({"version":1,"command":"update","value":{"key":"NotificationsEnabled","value":true}}))}
                let paused=!self.flag("PauseNotifications",false);
                self.request(json!({"version":1,"command":"update","value":{"key":"PauseNotifications","value":paused}}))
            },
            "prepareUpdateExit"=>{if let Err(e)=self.save().and_then(|_|self.timers.persist()){return json!({"success":false,"message":e})}if let Some(mut child)=self.settings_process.take(){let _=child.kill();let _=child.wait();}let _=PostMessageW(Some(self.hwnd),WM_CLOSE,WPARAM(0),LPARAM(0));json!({"success":true})},
            "restart"=>{
                let result=(||->std::io::Result<()>{
                    let exe=std::env::current_exe()?;let mut command=std::process::Command::new(exe);
                    command.args(["--restart-after",&std::process::id().to_string(),"--data-dir"]).arg(&self.data);
                    if self.demo{command.arg("--demo");}
                    command.current_dir(&self.base).spawn()?;Ok(())
                })();
                match result{Ok(())=>{let _=self.save();if let Some(mut settings)=self.settings_process.take(){let _=settings.kill();let _=settings.wait();}let _=PostMessageW(Some(self.hwnd),WM_CLOSE,WPARAM(0),LPARAM(0));json!({"success":true,"message":"正在重启点屿"})},Err(e)=>json!({"success":false,"message":format!("重启失败：{e}")})}
            },
            "settings"=>{self.show_settings();json!({"success":true})},"exit"=>{let _=PostMessageW(Some(self.hwnd),WM_CLOSE,WPARAM(0),LPARAM(0));json!({"success":true})},
            "synchronize"=>{if let Some(n)=r["value"].as_f64().filter(|n|n.is_finite()&&(0.0..=86400.).contains(n)){self.command("synchronize",n);json!({"success":true})}else{json!({"success":false,"message":"请输入有效的播放秒数"})}},
            "update"|"resetAppearance"=>{
                let old=self.settings.clone();let next=if command=="resetAppearance"{let mut s=old.clone();s["CompactWidth"]=json!(260.);s["FontSize"]=json!(12.5);s["Floating"]=json!(false);Ok(s)}else{config::validate_setting(json!({"key":r["value"]["key"],"value":r["value"]["value"],"settings":old}))};
                let next=match next{Ok(v)=>v,Err(e)=>return json!({"success":false,"message":e})};self.settings=next;
                let saved=if !self.demo&&r["value"]["key"]=="AutoStart"{self.apply_startup()}else{self.save()};
                if let Err(e)=saved{self.settings=old;return json!({"success":false,"message":e})}
                if r["value"]["key"]=="NotificationHistoryDays"{self.history.prune(self.settings["NotificationHistoryDays"].as_u64().unwrap_or(7).clamp(1,30)as u32,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()as i64);}
                if let Some(listener)=&self.notification_listener{listener.configure(notifications::Policy::from_settings(&self.settings));}
                if r["value"]["key"]=="QuickReminderHotkey"&&!self.demo{if let Err(e)=unsafe{self.register_hotkey()}{self.settings=old.clone();let _=self.save();let _=unsafe{self.register_hotkey()};return json!({"success":false,"message":e})}}
                if r["value"]["key"]=="FocusPauseOnLock"&&!self.flag("FocusPauseOnLock",false){self.focus_resume=None;if let Err(e)=self.timers.clear_lock_pauses(){self.error=Some(e);}}
                if !self.notification_preview&&self.banner.visible()&&(self.flag("PauseNotifications",false)||(self.banner_todo.is_none()&&(!self.flag("NotificationsEnabled",true)||self.banner.notice.as_ref().is_some_and(|n|!notifications::Policy::from_settings(&self.settings).permits(&n.app_id,&n.app_name))))){let (r,rad)=self.shape();self.banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}
                if let Some(b)=&self.backend{b.apply(self.settings.clone());}if let Err(e)=self.position(){self.error=Some(e.to_string());return json!({"success":false,"message":e.to_string()})}
                if r["value"]["key"]=="SystemStateHintPlacement"{unsafe{self.bottom_osd.clear();}let now=self.start.elapsed().as_secs_f64();let(r,rad)=self.shape();self.osd_banner.dismiss(r,rad,now,false);}
                if r["value"]["key"]=="MusicExpandedLayout"{self.start_music_shift();}
                if ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"].contains(&r["value"]["key"].as_str().unwrap_or("")){self.apply_osd();}
                self.transition=None;self.frame=if self.expanded{Frame::expanded()}else{Frame::hidden(self.options())};json!({"success":true,"message":"已保存"})
            },_=>json!({"success":false,"message":"不支持的操作"})
        }
    }
}
unsafe extern "system" fn visual_wndproc(hwnd:HWND,msg:u32,wparam:WPARAM,lparam:LPARAM)->LRESULT{match msg{WM_ERASEBKGND=>LRESULT(1),WM_NCHITTEST=>LRESULT(HTTRANSPARENT as isize),WM_MOUSEACTIVATE=>LRESULT(MA_NOACTIVATE as isize),WM_PAINT=>{let mut paint=PAINTSTRUCT::default();let _=BeginPaint(hwnd,&mut paint);let _=EndPaint(hwnd,&paint);LRESULT(0)},_=>DefWindowProcW(hwnd,msg,wparam,lparam)}}
unsafe extern "system" fn wndproc(hwnd:HWND,msg:u32,wparam:WPARAM,lparam:LPARAM)->LRESULT{
    if msg==WM_NCCREATE{let create=&*(lparam.0 as *const CREATESTRUCTW);SetWindowLongPtrW(hwnd,GWLP_USERDATA,create.lpCreateParams as isize);return LRESULT(1)}
    if msg==WM_DESTROY{let mut icon=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:hwnd,uID:1,..Default::default()};let _=Shell_NotifyIconW(NIM_DELETE,&mut icon);PostQuitMessage(0);return LRESULT(0)} if msg==WM_NCDESTROY{SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);return DefWindowProcW(hwnd,msg,wparam,lparam)} let ptr=GetWindowLongPtrW(hwnd,GWLP_USERDATA)as *const RefCell<App>;
    if !ptr.is_null(){if let Ok(mut app)=(*ptr).try_borrow_mut(){let a=&mut *app;match msg{
        WM_NCHITTEST=>{let mut pt=POINT{x:(lparam.0 as u16 as i16)as i32,y:((lparam.0>>16)as u16 as i16)as i32};let _=ScreenToClient(hwnd,&mut pt);let s=GetDpiForWindow(hwnd).max(96)as f64/96.;return LRESULT(if a.hit(pt.x as f64/s,pt.y as f64/s)<0{HTTRANSPARENT as isize}else{HTCLIENT as isize})},
        WM_DEVICECHANGE=>{if a.flag("DeviceRemindersEnabled",true){a.devices.message(wparam.0,lparam.0);}return LRESULT(0)},
        WM_INPUT_DEVICE_CHANGE=>{if a.flag("DeviceRemindersEnabled",true){a.devices.input_change(wparam.0 as u32,lparam.0);}return LRESULT(0)},
        osd::WM_OSD=>{a.pending_osd|=wparam.0 as u32;let _=SetTimer(Some(hwnd),osd::OSD_TIMER,45,None);return LRESULT(0)},
        WM_TIMER=>{if wparam.0==osd::OSD_TIMER{let _=KillTimer(Some(hwnd),osd::OSD_TIMER);a.flush_osd();return LRESULT(0)}} ,
        WM_MOUSEACTIVATE=>return LRESULT(MA_NOACTIVATE as isize),WM_ERASEBKGND=>return LRESULT(1),
        WM_PAINT=>{let mut p=PAINTSTRUCT::default();let _=BeginPaint(hwnd,&mut p);let _=EndPaint(hwnd,&p);a.dirty=true;return LRESULT(0)},
        WM_LBUTTONDOWN=>{let s=GetDpiForWindow(hwnd).max(96)as f64/96.;let x=(lparam.0 as u16 as i16)as f64/s;let y=((lparam.0>>16)as u16 as i16)as f64/s;a.pressed=a.hit(x,y);if a.can_hide(a.pressed){a.hide_drag=Some(gestures::HideDrag::new(x,y));}if a.can_swipe(a.pressed){a.swipe=Some(activities::Swipe::new(x,y));}a.dirty=true;if a.pressed==2{a.activate(2);a.pressed=0;}else{SetCapture(hwnd);if a.pressed==10&&a.snapshot["canSeek"]==true{a.seeking=true;a.update_seek(x);}}return LRESULT(0)},
        WM_MOUSEMOVE=>{let scale=GetDpiForWindow(hwnd).max(96)as f64/96.;
            if a.swipe.is_some_and(|swipe|swipe.active){a.hide_drag=None;}
            if let Some(drag)=&mut a.hide_drag{let mut screen=POINT::default();let _=GetCursorPos(&mut screen);drag.update((lparam.0 as u16 as i16)as f64/scale,((lparam.0>>16)as u16 as i16)as f64/scale,screen.y<=2);if drag.active{a.swipe=None;a.dismiss_offset.set(drag.offset());a.pressed=-1;a.dirty=true;}}
            if let Some(swipe)=&mut a.swipe{let scale=GetDpiForWindow(hwnd).max(96)as f64/96.;swipe.update((lparam.0 as u16 as i16)as f64/scale,((lparam.0>>16)as u16 as i16)as f64/scale);if swipe.active{a.pressed=-1;a.dirty=true;} }
            if let Some(swipe)=a.swipe.filter(|s|s.active){let direction=if swipe.dx<0.{1}else{-1};let n=a.components.len();if n>1{let i=a.components.iter().position(|c|c==&a.selected).unwrap_or(0);let target=a.components[(i as i32+direction).rem_euclid(n as i32)as usize].clone();if target!=a.drag_target{a.drag_panel=a.component_panel(&target);a.drag_target=target;}}}if a.seeking{let s=GetDpiForWindow(hwnd).max(96)as f64/96.;a.update_seek((lparam.0 as u16 as i16)as f64/s);}return LRESULT(0)},
        WM_LBUTTONUP=>{let hide=a.hide_drag.take();let swipe=a.swipe.take();let _=ReleaseCapture();
            if let Some(drag)=hide.filter(|drag|drag.active){if drag.dismiss(){a.hide_by_gesture()}else{a.pressed=0;a.dirty=true;}return LRESULT(0)}
            a.swipe=swipe;let s=GetDpiForWindow(hwnd).max(96)as f64/96.;let x=(lparam.0 as u16 as i16)as f64/s;let y=((lparam.0>>16)as u16 as i16)as f64/s;let hit=a.hit(x,y);if let Some(swipe)=a.swipe.take(){if a.finish_swipe(swipe){return LRESULT(0)}}if a.seeking{a.seeking=false;a.command("seek",a.seek_fraction*a.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.));}else if hit==a.pressed&&hit>=0{a.activate(hit)}a.pressed=0;a.dirty=true;return LRESULT(0)},
        WM_CAPTURECHANGED|WM_CANCELMODE=>{a.hide_drag=None;if let Some(swipe)=a.swipe.take(){if swipe.active{a.slide.set(swipe.dx);a.slide_outgoing=false;a.switching=a.flag("Animation",true);}}a.drag_target.clear();a.drag_panel=None;a.seeking=false;a.pressed=0;a.dirty=true;return LRESULT(0)},
        WM_DPICHANGED|WM_DISPLAYCHANGE=>{if let Err(e)=a.position(){a.error=Some(e.to_string());}return LRESULT(0)},
        WM_WTSSESSION_CHANGE=>{let mut session=0u32;let _=ProcessIdToSessionId(GetCurrentProcessId(),&mut session);if lparam.0 as u32==session{if wparam.0 as u32==WTS_SESSION_LOCK{a.pending_osd=0;if a.osd_banner.visible(){let now=a.start.elapsed().as_secs_f64();let(r,rad)=a.shape();a.osd_banner.dismiss(r,rad,now,false);}a.on_lock();}else if wparam.0 as u32==WTS_SESSION_UNLOCK{a.osd.resync();a.on_unlock();}}return LRESULT(0)},
        WM_HOTKEY=>{if wparam.0==1{a.pending_action=Some("quickReminder".into());a.show_settings_page("overview");}return LRESULT(0)},
        WM_SETTINGS=>{let requests=REQUESTS.get_or_init(Default::default).lock().map(|mut q|std::mem::take(&mut *q)).unwrap_or_default();for (r,tx) in requests{let response=a.request(r);let _=tx.send(response);}return LRESULT(0)},
        WM_TRAY=>{if lparam.0 as u32==WM_RBUTTONUP{let mut p=POINT::default();let _=GetCursorPos(&mut p);let menu=CreatePopupMenu();if let Ok(menu)=menu{for (id,text) in [(1,w!("展开灵动岛")),(2,w!("显示 / 隐藏灵动岛")),(3,w!("设置")),(5,w!("重启点屿")),(4,w!("退出"))]{let _=AppendMenuW(menu,MF_STRING,id,text);}let active=a.flag("NotificationsEnabled",true)&&!a.flag("PauseNotifications",false);
            let label=if !a.flag("NotificationsEnabled",true){w!("开启通知")}else if a.flag("PauseNotifications",false){w!("恢复通知（已暂停）")}else{w!("暂停通知（已开启）")};
            let flags=if active{MF_STRING|MF_CHECKED}else{MF_STRING|MF_UNCHECKED};let _=InsertMenuW(menu,3,flags|MF_BYPOSITION,6,label);
            let startup_flags=MF_STRING|MF_BYPOSITION|if a.flag("AutoStart",true){MF_CHECKED}else{MF_UNCHECKED};
            let _=InsertMenuW(menu,3,startup_flags,7,w!("开机启动"));
            let _=SetForegroundWindow(hwnd);let selected=TrackPopupMenuEx(menu,(TPM_RETURNCMD|TPM_RIGHTBUTTON).0,p.x,p.y,hwnd,None).0;let _=DestroyMenu(menu);let command=match selected{1=>"expand",2=>"toggleVisibility",3=>"settings",5=>"restart",6=>"toggleNotifications",4=>"exit",_=>""};if selected==7{let enabled=!a.flag("AutoStart",true);let result=a.request(json!({"version":1,"command":"update","value":{"key":"AutoStart","value":enabled}}));if result["success"]!=true{a.error=Some(result["message"].as_str().unwrap_or("开机启动设置未保存").to_owned());}}else if !command.is_empty(){a.request(json!({"version":1,"command":command}));}}}else if lparam.0 as u32==WM_LBUTTONDBLCLK{a.request(json!({"version":1,"command":"expand"}));}return LRESULT(0)},
        WM_CLOSE=>{a.bottom_osd.clear();a.osd.uninstall();let _=a.timers.persist();let _=a.rules.persist_usage();let _=WTSUnRegisterSessionNotification(hwnd);let _=UnregisterHotKey(Some(hwnd),1);let _=DestroyWindow(hwnd);return LRESULT(0)},WM_DESTROY=>{let mut icon=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:hwnd,uID:1,..Default::default()};let _=Shell_NotifyIconW(NIM_DELETE,&mut icon);if let Some(b)=&a.backend{b.close();}PostQuitMessage(0);return LRESULT(0)},_=>{}
    }}}DefWindowProcW(hwnd,msg,wparam,lparam)
}
fn font_base()->PathBuf{let exe=std::env::current_exe().unwrap();let base=exe.parent().unwrap();if base.join("Assets/Fonts/NotoSansSC.ttf").exists(){return base.into()}Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")}
unsafe fn create(a:&mut App,owner:*const RefCell<App>,hidden:bool)->Result<()> {
    shell::ensure_per_monitor_dpi_awareness();CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;
    a.transparent=windows::UI::ViewManagement::UISettings::new().and_then(|s|s.AdvancedEffectsEnabled()).unwrap_or(false);let instance=GetModuleHandleW(None)?;let class=w!("LyricIsland.Native.DirectComposition.v1");let wc=WNDCLASSW{lpfnWndProc:Some(wndproc),hInstance:instance.into(),lpszClassName:class,hCursor:LoadCursorW(None,IDC_ARROW)?,..Default::default()};RegisterClassW(&wc);
    let visual_class=w!("LyricIsland.Native.Visual.v1");let visual_wc=WNDCLASSW{lpfnWndProc:Some(visual_wndproc),hInstance:instance.into(),lpszClassName:visual_class,..Default::default()};RegisterClassW(&visual_wc);
    let visual=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOREDIRECTIONBITMAP,visual_class,w!("DotIsle · Visual"),WS_POPUP,0,0,388,264,None,None,Some(instance.into()),None)?;a.visual_hwnd=visual;SetLayeredWindowAttributes(visual,COLORREF(0),255,LWA_ALPHA)?;
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP,class,w!("DotIsle · Native"),WS_POPUP,0,0,388,264,Some(visual),None,Some(instance.into()),Some(owner as _))?;a.hwnd=hwnd;a.devices.register(hwnd);a.apply_osd();let _=WINDOW.set(hwnd.0 as isize);let _=WTSRegisterSessionNotification(hwnd,NOTIFY_FOR_THIS_SESSION);if !a.demo{if let Err(e)=a.register_hotkey(){a.error=Some(e);}}a.rules.baseline_now(a.foreground.path.clone().as_deref(),a.foreground.segment);a.position()?;
    // Repair a stale Run entry: the launcher may have moved or a previous version registered
    // its extracted runtime path, which dies on the next update.
    if !a.demo&&!std::env::var("LYRICISLAND_TEST_PIPE").is_ok_and(|p|p.starts_with("LyricIsland.Settings.test."))&&a.flag("AutoStart",true){let _=a.apply_startup();}
    if !hidden{let _=ShowWindow(visual,SW_SHOWNOACTIVATE);let _=ShowWindow(hwnd,SW_SHOWNOACTIVATE);}
    Ok(())
}
unsafe fn tray(a:&App)->Result<()> {
    let path=a.base.join("Assets/LyricIsland-v2.ico").to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();let icon=LoadImageW(None,PCWSTR(path.as_ptr()),IMAGE_ICON,0,0,LR_LOADFROMFILE|LR_DEFAULTSIZE).ok().map(|i|HICON(i.0)).unwrap_or(LoadIconW(None,IDI_APPLICATION)?);
    let mut nid=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:a.hwnd,uID:1,uFlags:NIF_MESSAGE|NIF_ICON|NIF_TIP,uCallbackMessage:WM_TRAY,hIcon:icon,..Default::default()};let tip=format!("DotIsle · 点屿 {}",env!("CARGO_PKG_VERSION")).encode_utf16().collect::<Vec<_>>();let tip=&tip[..tip.len().min(127)];nid.szTip[..tip.len()].copy_from_slice(&tip);let _=Shell_NotifyIconW(NIM_ADD,&nid);Ok(())
}
fn traces(directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;let mut cases=vec![];
    for style in [motion::Style::Classic,motion::Style::Fluid]{
    let tag=if style==motion::Style::Classic{"Classic"}else{"Fluid"};
    for damping in [10.,30.,2.*(420_f64*0.8).sqrt(),80.]{let mut o=Options::default();o.spring.damping=damping;
        for expanded in [true,false]{let t=Transition::styled(style,expanded,if expanded{Frame::hidden(o)}else{Frame::expanded()},o,Frame::default(),false);
            for ms in (0..=1400).step_by(5){cases.push(json!({"style":tag,"damping":damping,"expanded":expanded,"time":ms,"sample":t.sample(ms as f64),"velocity":t.velocity(ms as f64)}));}
            for ms in [40.,90.,180.,320.]{let rev=Transition::styled(style,!expanded,t.sample(ms),o,t.velocity(ms),false);for after in [0.,1.,20.,80.,180.,400.]{cases.push(json!({"style":tag,"damping":damping,"expanded":!expanded,"reverseAt":ms,"time":after,"sample":rev.sample(after),"velocity":rev.velocity(after)}));}}
        }
    }}std::fs::write(directory.join("native-motion.json"),serde_json::to_vec(&cases)?)?;Ok(())
}
unsafe fn preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.renderer=None;a.renderer=Some(Renderer::new(a.visual_hwnd,a.layout.host.w as u32,a.layout.host.h as u32,96.,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?);a.poll();a.opacity=1.;let o=a.options();
    // Both curves, same instants, so the 灵动岛流体动画 switch can be judged side by side instead
    // of remembered. The classic frames keep their old names so an existing script still finds them.
    for (name,expanded,count) in [("opening",true,48),("closing",false,44)]{
        for (style,label) in [(motion::Style::Classic,"classic"),(motion::Style::Fluid,"fluid")]{
            let t=Transition::styled(style,expanded,if expanded{Frame::hidden(o)}else{Frame::expanded()},o,Frame::default(),false);a.transition=Some(t);a.expanded=expanded;
            for i in 0..count{let ms=i as f64*20.;if ms>t.duration_ms+20.{break}a.frame=t.sample(ms);let mut renderer=a.renderer.take().unwrap();let mut scene=a.scene();scene.time_ms=0.;scene.lyric_age_ms=1000.;scene.cover_opacity=1.;scene.progress=83./236.;let mut activity=motion::Activity::new();activity.retarget(true,0.,true);scene.activity_scales=activity.sample(330.+ms,true).0;scene.activity_opacity=0.9;renderer.render(&scene,false)?;
                let file=if style==motion::Style::Classic{format!("{name}-{:03}ms.png",i*20)}else{format!("{name}-{label}-{:03}ms.png",i*20)};
                renderer.snapshot(&directory.join(file))?;a.renderer=Some(renderer);}
        }
    }traces(directory)?;Ok(())
}
unsafe fn tools_preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.opacity=1.;a.frame=Frame::expanded();a.lyric="于是我拼命奔跑".into();
    let todo=activities::Panel{kind:2,title:"待办清单".into(),value:"3 项待办".into(),accent:0xfff3d79b,tasks:[("复盘今天的工作","2026-10-17 08:30",0xfff3d79b),("整理桌面与备份文件","2026-10-17 09:30",0xffb7dbcb),("背单词 20 分钟","2026-10-17 10:30 · 每天",0xffc9bce5)].iter().map(|(t,d,c)|activities::Task{title:(*t).into(),detail:(*d).into(),color:*c,..Default::default()}).collect(),..Default::default()};
    let timer=activities::Panel{kind:1,title:"番茄钟".into(),value:"24:38".into(),subtitle:"专注 · 第 1 / 4 轮".into(),progress:0.16,running:true,accent:0xffe7a185,..Default::default()};
    for dpi in [96.,144.,192.]{let scale=dpi/96.;a.renderer=None;let mut renderer=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;
        for (name,panel) in [("music",None),("todos",Some(&todo)),("timer",Some(&timer))]{let mut scene=a.scene();scene.panel=panel;scene.component_count=3;scene.previous_panel=None;scene.switching=false;scene.notification=None;scene.layout_scale=scale;scene.lyric_age_ms=1000.;scene.progress=0.37;scene.secondary="等待着夜晚的再次来临";scene.time_ms=0.;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("{name}-{dpi:.0}.png")))?;}
        {let mut scene=a.scene();scene.panel=Some(&timer);scene.frame=Frame::hidden(a.options());scene.component_count=3;scene.layout_scale=scale;scene.opacity=1.;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("capsule-{dpi:.0}.png")))?;}
        if dpi==144.{let mut spring=activities::Spring::default();spring.release(a.layout.card.w-70.,-650.);for i in 0..30{let mut scene=a.scene();scene.panel=Some(&timer);scene.previous_panel=Some(&todo);scene.switching=true;scene.switch_outgoing=true;scene.switch_direction=1.;scene.switch_offset=spring.value;scene.layout_scale=scale;scene.component_count=3;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("switch-{i:02}.png")))?;spring.tick(1./60.);}}
        a.renderer=Some(renderer);
    }Ok(())
}
unsafe fn notification_preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.frame=Frame::hidden(a.options());a.opacity=1.;a.lyric="景色は移ろい瞬く間に変わる".into();
    for dpi in [96.,120.,144.,192.]{
        a.renderer=None;let scale=dpi/96.;let mut renderer=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;
        for amount in [0.,0.5,1.]{let mut s=a.scene();s.notification=None;s.notification_mix=0.;s.layout_scale=scale;s.lyric_age_ms=1000.;s.highlight=amount;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("font-{dpi:.0}-{}.png",(amount*100.)as i32)))?;}
        {let mut s=a.scene();s.notification=None;s.notification_mix=0.;s.frame=Frame::expanded();s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("toolbar-{dpi:.0}.png")))?;}
        let notice=notifications::Notice::example(&a.base);renderer.set_notification_icon(notice.icon.as_deref())?;
        a.banner=banner::Banner::default();let (base,radius)=a.shape();let height=renderer.notification_height(&notice,440_f64.min(a.layout.host.w-48.))?;a.banner.show(notice,base,radius,a.layout.host.w,height,0.,true);
        for ms in [0.,40.,80.,120.,200.,320.,640.]{let (r,rad,mix)=a.banner.presentation(ms/1000.);let mut s=a.scene();s.notification_rect=r;s.notification_radius=rad;s.notification_mix=mix;s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("notification-{dpi:.0}-{ms:03.0}.png")))?;}
        a.banner.dismiss(base,radius,0.64,true);
        for ms in [640.,680.,720.,800.,960.,1280.]{let (r,rad,mix)=a.banner.presentation(ms/1000.);let mut s=a.scene();s.notification_rect=r;s.notification_radius=rad;s.notification_mix=mix;s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("dismiss-{dpi:.0}-{ms:04.0}.png")))?;}
        for (name,body) in [("short","这是一条简短消息。"),("empty",""),("long","这是一条用于检查多行通知排版的长消息。收到新消息后，点屿会根据正文内容调整横幅高度，保留清晰的文字间距，并在显示结束后平滑返回音乐界面。")]{
            let mut notice=notifications::Notice::example(&a.base);notice.body=body.into();
            if let Some(path)=std::env::var_os("LYRICISLAND_PREVIEW_ICON"){notice.icon=std::fs::read(path).ok();notice.app_name="QQ".into();}
            renderer.set_notification_icon(notice.icon.as_deref())?;
            let height=renderer.notification_height(&notice,440.)?;
            a.banner.show(notice,base,radius,a.layout.host.w,height,0.,false);
            let (r,rad,mix)=a.banner.presentation(0.);let mut scene=a.scene();scene.notification_rect=r;scene.notification_radius=rad;scene.notification_mix=mix;scene.layout_scale=scale;
            renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("notification-{name}-{dpi:.0}.png")))?;
            std::fs::write(directory.join(format!("height-{name}-{dpi:.0}.json")),serde_json::to_vec(&json!({"height":height,"dpi":dpi}))?)?;
        }
        a.renderer=Some(renderer);
    }Ok(())
}
unsafe fn icon_preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.poll();a.opacity=1.;a.frame=Frame::expanded();a.expanded=true;
    a.renderer=None;
    for dpi in [96.,120.,144.,192.]{let scale=dpi/96.;let mut renderer=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;
        let mut scene=a.scene();scene.layout_scale=scale;scene.lyric_age_ms=1000.;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("icons-{dpi:.0}.png")))?;std::fs::write(directory.join(format!("icons-{dpi:.0}.json")),serde_json::to_vec(&renderer.icon_bounds()?)?)?;
    }Ok(())
}
fn run()->std::result::Result<(),Box<dyn std::error::Error>>{unsafe{
    let args=std::env::args().collect::<Vec<_>>();

    if let Some(i)=args.iter().position(|s|s=="--restart-after"){
        let pid=args.get(i+1).and_then(|s|s.parse::<u32>().ok()).ok_or("invalid restart PID")?;
        if let Ok(parent)=OpenProcess(PROCESS_ACCESS_RIGHTS(0x00100000),false,pid){let result=WaitForSingleObject(parent,15000);let _=CloseHandle(parent);if result!=WAIT_OBJECT_0{return Err("等待旧实例退出超时".into())}}
    }
    if args.get(1).is_some_and(|s|s=="--check-update"){
        let report=lyric_backend::updates::check().unwrap_or_else(|e|json!({"success":false,"message":e}));std::fs::write(args.get(2).ok_or("missing report")?,serde_json::to_vec_pretty(&report)?)?;return Ok(())
    }
    if args.get(1).is_some_and(|s|s=="--app-icon"){
        let id=args.get(2).ok_or("missing app id")?;let path=args.get(3).ok_or("missing output path")?;
        std::thread::spawn({let id=id.clone();let path=path.clone();move||->std::result::Result<(),String>{
            let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
            let bytes=app_icon::shell_icon(&id).ok_or("Windows application icon unavailable")?;
            std::fs::write(path,bytes).map_err(|e|e.to_string())
        }}).join().map_err(|_|"icon worker failed")??;return Ok(())
    }

    if args.get(1).is_some_and(|s|s=="--activation-check"){
        let title=w!("LyricIsland activation test");let h=CreateWindowExW(WINDOW_EX_STYLE(0),w!("STATIC"),title,WS_OVERLAPPEDWINDOW|WS_VISIBLE,0,0,200,100,None,None,None,None)?;
        let _=ShowWindow(h,SW_MINIMIZE);let result=activation::open(&std::env::current_exe()?.to_string_lossy());
        let success=result.is_ok()&&!IsIconic(h).as_bool()&&GetForegroundWindow()==h;
        std::fs::write(args.get(2).ok_or("missing report")?,serde_json::to_vec_pretty(&json!({"success":success,"restoredMinimizedWindow":!IsIconic(h).as_bool(),"error":result.err()}))?)?;let _=DestroyWindow(h);return Ok(())
    }
    if args.get(1).is_some_and(|s|s=="--notification-info"){let report=std::thread::spawn(||{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);notifications::diagnostics()}).join().map_err(|_|"notification diagnostic failed")?;std::fs::write(args.get(2).ok_or("missing report path")?,serde_json::to_vec_pretty(&report)?)?;return Ok(())}if args.get(1).is_some_and(|s|s=="--motion-trace"){return traces(Path::new(args.get(2).ok_or("missing directory")?))}
    if args.get(1).is_some_and(|s|s=="--media-info"){let data=std::thread::spawn(||lyric_backend::media::control_diagnostics().map_err(|e|e.to_string())).join().map_err(|_|"media diagnostics worker failed")??;std::fs::write(args.get(2).ok_or("missing output path")?,serde_json::to_vec_pretty(&data)?)?;return Ok(())}
    let icon_mode=args.get(1).is_some_and(|s|s=="--icon-preview");let font_mode=args.get(1).is_some_and(|s|s=="--font-check");let notice_mode=args.get(1).is_some_and(|s|s=="--notification-preview");let cover_mode=args.get(1).is_some_and(|s|s=="--cover-preview");let tool_mode=args.get(1).is_some_and(|s|s=="--tools-preview");let preview_mode=args.get(1).is_some_and(|s|s=="--integration-preview")||tool_mode||cover_mode||icon_mode||font_mode||notice_mode||args.get(1).is_some_and(|s|s=="--animation-preview");let demo=preview_mode||args.iter().any(|s|s=="--demo");
    // One island per desktop, so a second copy must not start. It used to exit without a word,
    // which made a freshly built portable copy look broken: double-clicking it did nothing at all
    // while the installed version answered every click. The mutex is taken before any window is
    // created so the refused copy never flashes an island on its way out.
    let test_pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_default();
    let mutex_name=if test_pipe.starts_with("LyricIsland.Settings.test."){format!("Local.LyricIsland.Native.Test.{test_pipe}")}else{"Local.LyricIsland.Native.Direct2D.v1".into()};let mutex_name=mutex_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mutex=CreateMutexW(None,true,PCWSTR(mutex_name.as_ptr()))?;
    if GetLastError()==ERROR_ALREADY_EXISTS{
        let _=CloseHandle(mutex);
        // Not while logging in: a dialog nobody asked for, before the desktop has settled, is
        // worse than staying quiet.
        if !preview_mode&&!args.iter().any(|s|s=="--startup"){
            let current=io::pipe_request("LyricIsland.Native.Settings.v1",&json!({"version":1,"command":"state"})).unwrap_or_default();
            let message=format!("已有点屿 {} 正在运行。\r\n你正在启动的是此目录的 {}，不会跳转或改用安装版。\r\n\r\n请先从托盘退出正在运行的点屿，再启动这一份。",current["version"].as_str().unwrap_or("（版本未知）"),env!("CARGO_PKG_VERSION"));let text=message.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let title=format!("点屿 · DotIsle {}",env!("CARGO_PKG_VERSION")).encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            MessageBoxW(None,PCWSTR(text.as_ptr()),PCWSTR(title.as_ptr()),MB_OK|MB_ICONINFORMATION|MB_SETFOREGROUND);
        }
        return Ok(())
    }
let base=font_base();let data=if let Some(i)=args.iter().position(|s|s=="--data-dir"){PathBuf::from(args.get(i+1).ok_or("missing data directory")?)}else if !base.join("Portable.json").is_file()&&base.join("Installed.json").exists(){let marker=io::read_settings(&base.join("Installed.json"));marker["dataDirectory"].as_str().map(PathBuf::from).unwrap_or_else(||io::data_directory())}else{base.join("Data")};
    if !demo&&std::env::var_os("LYRICISLAND_TEST_PIPE").is_none()&&!base.join("Portable.json").is_file()&&base.join("Installed.json").is_file()&&base.join("Uninstall.exe").is_file(){let _=std::process::Command::new(base.join("Uninstall.exe")).arg("--repair-if-needed").arg(&base).spawn();}
    let owner=Box::new(RefCell::new(App::new(base,data,demo)));let owner_ptr=&*owner as *const RefCell<App>;let mut app=owner.borrow_mut();create(&mut app,owner_ptr,preview_mode)?;
    if cover_mode{
        let dir=Path::new(args.get(2).ok_or("missing directory")?);std::fs::create_dir_all(dir)?;
        app.poll();app.opacity=1.;app.frame=Frame::expanded();let mut r=app.renderer.take().unwrap();
        for name in ["red","blue","white"]{
            r.set_cover(Some(&std::fs::read(dir.join(format!("{name}.png")))?))?;
            for enabled in [false,true]{for glass in [false,true]{let mut scene=app.scene();scene.glass_material=glass;scene.cover_background=enabled;scene.lyric_age_ms=1000.;scene.cover_age_ms=1000.;r.render(&scene,false)?;let file=if glass{format!("glass-{name}-{enabled}.png")}else{format!("{name}-{enabled}.png")};r.snapshot(&dir.join(file))?;}}
        }
        let _=DestroyWindow(app.hwnd);return Ok(())
    }
    if args.get(1).is_some_and(|s|s=="--integration-preview"){
        let directory=Path::new(args.get(2).ok_or("missing directory")?);std::fs::create_dir_all(directory)?;
        app.opacity=1.;app.frame=Frame::expanded();app.lyric="最後の最後に答え合わせをしよう".into();
        let base=app.base.clone();app.renderer=None;
        for dpi in [96.,144.,192.]{let scale=dpi/96.;let mut renderer=Renderer::new(app.visual_hwnd,(app.layout.host.w*scale)as u32,(app.layout.host.h*scale)as u32,dpi as f32,&base.join("Assets/Fonts/NotoSansSC.ttf"))?;renderer.set_cover(Some(&std::fs::read(base.join("Assets/LyricIsland-v2.png"))?))?;
            for (name,mix) in [("side",0.),("separated",1.),("focus",2.)]{app.music_mix=mix;app.layout.card.h=layout::music_height(mix,12.5);let mut scene=app.scene();scene.music_layout=if mix>1.5{layout::MusicLayout::LyricFocus}else if mix>0.5{layout::MusicLayout::Separated}else{layout::MusicLayout::SideBySide};scene.layout_scale=scale;scene.lyric_age_ms=1000.;scene.title="ハウル";scene.artist="LiSA";scene.secondary="在最后的最后来核对答案吧";scene.highlight=0.55;scene.component_count=1;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("music-{name}-{dpi:.0}.png")))?;}
            renderer.set_cover(Some(&std::fs::read(base.join("Assets/DeviceIcons/headphones-128.png"))?))?;
            app.music_mix=1.;app.layout.card.h=layout::music_height(1.,12.5);
            for ms in [0.,60.,120.,140.,180.,240.,280.]{let mut scene=app.scene();scene.cover_age_ms=ms;scene.layout_scale=scale;scene.lyric_age_ms=1000.;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("cover-flip-{dpi:.0}-{ms:03.0}.png")))?;}
            app.layout.card.h=200.;let mut clocks=timers::Store::new(PathBuf::new());
            for (name,value) in [("timer",json!({"mode":"countdown","duration":300})),("stopwatch",json!({"mode":"stopwatch"})),("pomodoro",json!({"mode":"pomodoro","focus":1500})),("workflow",json!({"mode":"workflow","stages":[{"name":"专注","seconds":1500},{"name":"休息","seconds":300}]}))]{let id=clocks.create(&value).unwrap();let t=clocks.items.iter_mut().find(|t|t.id==id).unwrap();t.elapsed_ms=if name=="stopwatch"{3830}else{5000};let panel=t.panel();let mut scene=app.scene();scene.layout_scale=scale;scene.panel=Some(&panel);scene.component_count=1;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("island-{name}-{dpi:.0}.png")))?;}
            app.layout.card.h=264.;app.updates.state=json!({"phase":"downloading","version":"演示","percent":43,"receivedBytes":4300000,"totalBytes":10000000});let panel=app.updates.panel();let mut scene=app.scene();scene.layout_scale=scale;scene.panel=Some(&panel);scene.component_count=1;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("update-{dpi:.0}.png")))?;
            for kind in [devices::Kind::Usb,devices::Kind::Volume,devices::Kind::Phone,devices::Kind::Headphones,devices::Kind::Speaker,devices::Kind::Gamepad,devices::Kind::Mouse,devices::Kind::Keyboard]{let notice=devices::Event::new("preview".into(),kind,true,format!("{} · 示例设备",kind.label())).notice();let mut scene=app.scene();scene.layout_scale=scale;scene.notification=Some(&notice);scene.notification_rect=Rect::new(24.,24.,440.,110.);scene.notification_radius=26.;scene.notification_mix=1.;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("device-{}-{dpi:.0}.png",kind.key())))?;}
        }let _=DestroyWindow(app.hwnd);return Ok(())
    }
    if icon_mode{icon_preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if font_mode{let report=app.renderer.as_mut().unwrap().verify_font()?;std::fs::write(args.get(2).ok_or("missing font report path")?,serde_json::to_vec_pretty(&report)?)?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if tool_mode{tools_preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if notice_mode{notification_preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if preview_mode{preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
        if !app.demo&&std::env::var_os("LYRICISLAND_TEST_PIPE").is_none()&&!app.base.join("Portable.json").is_file()&&!app.base.join("Installed.json").exists()&&app.base.join("OtaInstall.json").exists()&&app.base.join("DotIsle.Setup.exe").exists(){
        let offered=app.data.join(format!("install-offer-{}.txt",env!("CARGO_PKG_VERSION")));
        if !offered.exists(){let _=std::fs::create_dir_all(&app.data);if std::process::Command::new(app.base.join("DotIsle.Setup.exe")).arg(format!("/MIGRATEDATA={}",app.data.display())).spawn().is_ok(){let _=std::fs::write(offered,b"offered");}}
    }
    tray(&app)?;let pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into());app._server=Some(Server::new(&pipe,settings_callback));
    if !app.demo&&std::env::var_os("LYRICISLAND_TEST_PIPE").is_none()&&!app.flag("GestureTutorialSeen",false){app.show_settings_page("island");}
    if args.iter().any(|s|s=="--expanded"){app.held=true;app.retarget(true)}
    let until=args.iter().position(|s|s=="--run-ms").and_then(|i|args.get(i+1)).and_then(|s|s.parse::<u64>().ok()).map(|ms|Instant::now()+Duration::from_millis(ms));
    drop(app);let mut msg=MSG::default();'events:loop{
        while PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool(){if msg.message==WM_QUIT{break 'events}let _=TranslateMessage(&msg);DispatchMessageW(&msg);}
        let mut app=owner.borrow_mut();if until.is_some_and(|t|Instant::now()>=t){let _=DestroyWindow(app.hwnd);break}
        if let Err(e)=app.tick(){app.error=Some(e.to_string());io::log(&app.data.join("Logs"),"NATIVE_RENDER",&e.to_string());if let Err(recovery)=app.position(){return Err(recovery.into())}}
        // Present(1) already synchronizes rendering to the display. Waiting for
        // DwmFlush here as well would introduce a second refresh interval.
        if !(app.switching||app.swipe.is_some_and(|s|s.active)||app.panel.as_ref().is_some_and(|p|p.running)||app.banner.visible()||app.opacity>0.001&&app.snapshot["isPlaying"]==true&&app.flag("Animation",true)||app.transition.is_some()||(app.opacity-app.fade_to).abs()>0.00001||app.lyric_start.elapsed()<Duration::from_millis(180)){let _=MsgWaitForMultipleObjectsEx(None,16,QS_ALLINPUT,MWMO_INPUTAVAILABLE);}
    }
    let app=owner.borrow();if let Some(b)=&app.backend{b.close();}if let Some(server)=&app._server{server.close();}let _=CloseHandle(mutex);CoUninitialize();Ok(())
}}
fn main(){if let Err(e)=run(){let base=std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)).unwrap_or_default();let _=std::fs::write(base.join("native-error.txt"),e.to_string());std::process::exit(1)}}

#[cfg(test)]mod interaction_tests{
    use super::*;
    fn app()->App{App::new(PathBuf::new(),PathBuf::new(),true)}
    #[test]fn swipes_work_on_both_views_but_leave_controls_alone(){let mut a=app();a.timers.create(&json!({"mode":"stopwatch"})).unwrap();a.refresh_components();a.settings["SwipeActivities"]=json!(false);assert!(!a.can_swipe(0));a.settings["SwipeActivities"]=json!(true);for expanded in [false,true]{a.expanded=expanded;for hit in [0,1,13,40,41,42]{assert!(a.can_swipe(hit));}for hit in [-1,2,3,4,5,6,7,10,30,31]{assert!(!a.can_swipe(hit));}}}
    #[test]fn releasing_a_swipe_keeps_the_rendered_position_continuous(){for expanded in [false,true]{let mut a=app();let id=a.timers.create(&json!({"mode":"stopwatch"})).unwrap();a.refresh_components();a.expanded=expanded;let width=if expanded{a.layout.card.w}else{a.layout.capsule.w};let mut swipe=activities::Swipe::new(100.,80.);swipe.update(30.,80.);let offset=swipe.dx;assert!(a.finish_swipe(swipe));assert_eq!(a.selected,format!("timer:{id}"));assert!((a.slide.value-width-offset).abs()<0.001);assert!(a.slide_outgoing);}}
    #[test]fn short_swipe_returns_without_changing_the_component(){let mut a=app();a.timers.create(&json!({"mode":"stopwatch"})).unwrap();a.refresh_components();let mut swipe=activities::Swipe::new(100.,80.);swipe.update(120.,80.);assert!(a.finish_swipe(swipe));assert_eq!(a.selected,"music");assert_eq!(a.slide.value,20.);assert!(!a.slide_outgoing);}
    #[test]fn disabled_body_click_does_not_expand_but_cover_does(){unsafe{
        let mut a=app();a.settings["ClickAnywhereExpand"]=json!(false);let c=a.layout.capsule;
        assert_eq!(a.hit(c.x+22.,c.y+20.),1);
        assert_eq!(a.hit(c.x+75.,c.y+20.),0);
        a.activate(0);assert!(!a.expanded);a.activate(1);assert!(a.expanded&&a.held);
    }}
    #[test]fn enabled_body_click_expands_without_changing_playback(){unsafe{
        let mut a=app();a.settings["ClickAnywhereExpand"]=json!(true);let before=a.snapshot["isPlaying"].clone();
        a.activate(0);assert!(a.expanded&&a.held);assert_eq!(a.snapshot["isPlaying"],before);
        a.frame=Frame::expanded();a.transition=None;
        let c=a.layout.card;let g=layout::music_geometry(c.w,layout::MusicLayout::SideBySide,0.,1.,12.5);let button=g.buttons[3].1;assert_eq!(a.hit(c.x+button.x+button.w/2.,c.y+button.y+button.h/2.),4);
        a.activate(4);assert!(a.expanded);assert_eq!(a.snapshot["isPlaying"],false);
    }}
    #[test]fn body_click_option_is_boolean_and_preserves_other_settings(){
        let original=config::defaults();assert_eq!(original["PauseNotifications"],false);assert!(original.get("FollowWindowsDnd").is_none());assert_eq!(original["ClickAnywhereExpand"],true);assert_eq!(original["CoverBackground"],true);assert_eq!(original["OnlineSongInfo"],true);
        let updated=config::validate_setting(json!({"key":"ClickAnywhereExpand","value":true,"settings":original})).unwrap();
        assert_eq!(updated["ClickAnywhereExpand"],true);assert_eq!(updated["Motion"],original["Motion"]);
        assert!(config::validate_setting(json!({"key":"ClickAnywhereExpand","value":"true","settings":original})).is_err());
    }
    #[test]fn island_fluid_motion_defaults_on_and_stays_switchable(){
        let s=config::defaults();
        assert_eq!(s["IslandFluidMotion"],json!(true));
        // Off must survive validation, because "关闭 = 回到 2.0.0" is the whole promise.
        let off=config::validate_setting(json!({"key":"IslandFluidMotion","value":false,"settings":s.clone()})).unwrap();
        assert_eq!(off["IslandFluidMotion"],json!(false));
        assert!(config::validate_setting(json!({"key":"IslandFluidMotion","value":"true","settings":s.clone()})).is_err());
        // An older settings.json has no such key; the shipped default decides, not the file.
        let loaded=config::load(std::path::Path::new("no-such-settings-from-an-older-build.json"));
        assert_eq!(loaded["IslandFluidMotion"],json!(true));
        // Both curves stay in the binary: the switch only picks between them.
        let o=Options::default();
        let classic=Transition::new(true,Frame::hidden(o),o,Frame::default(),false);
        let off_style=Transition::styled(motion::Style::Classic,true,Frame::hidden(o),o,Frame::default(),false);
        assert_eq!(classic.duration_ms,off_style.duration_ms);
        assert_eq!(classic.sample(120.).values(),off_style.sample(120.).values());
    }
    /// Mouse and keyboard reconnect several times a day on their own — a wireless mouse waking up,
    /// a KVM switching — so they are the two device reminders that turn into noise. Off by default,
    /// carried over once for installs that inherited 2.0.0's bundle default.
    #[test]fn mouse_and_keyboard_reminders_default_off(){
        let s=config::defaults();
        for key in ["MouseConnectionRemindersEnabled","KeyboardConnectionRemindersEnabled"]{
            assert_eq!(s[key],json!(false),"{key}");
            // Still a real switch: turning it on must validate and stick.
            assert_eq!(config::validate_setting(json!({"key":key,"value":true,"settings":s.clone()})).unwrap()[key],json!(true),"{key}");
            assert!(config::validate_setting(json!({"key":key,"value":1,"settings":s.clone()})).is_err(),"{key}");
        }
        // Everything else in the device bundle keeps its 2.0.0 default.
        for key in ["HeadphoneConnectionRemindersEnabled","SpeakerConnectionRemindersEnabled","GamepadConnectionRemindersEnabled","DeviceDisconnectRemindersEnabled"]{
            assert_eq!(s[key],json!(true),"{key}");
        }
        // A 2.0.0 install persisted `true` for both without anyone choosing it.
        let legacy=std::env::temp_dir().join(format!("dotisle-device-defaults-{}.json",std::process::id()));
        std::fs::write(&legacy,r#"{"DefaultsOnboardingVersion":1,"MouseConnectionRemindersEnabled":true,"KeyboardConnectionRemindersEnabled":true}"#).unwrap();
        let migrated=config::load(&legacy);
        assert_eq!(migrated["DeviceDefaultsVersion"],json!(1));
        assert_eq!(migrated["MouseConnectionRemindersEnabled"],json!(false));
        assert_eq!(migrated["KeyboardConnectionRemindersEnabled"],json!(false));
        // Once the marker is stamped, a user who turns them back on keeps that choice.
        std::fs::write(&legacy,r#"{"DefaultsOnboardingVersion":1,"DeviceDefaultsVersion":1,"MouseConnectionRemindersEnabled":true,"KeyboardConnectionRemindersEnabled":true}"#).unwrap();
        let chosen=config::load(&legacy);
        assert_eq!(chosen["MouseConnectionRemindersEnabled"],json!(true));
        assert_eq!(chosen["KeyboardConnectionRemindersEnabled"],json!(true));
        let _=std::fs::remove_file(&legacy);
    }
    #[test]fn swipe_notifications_and_devices_default_on(){
        let s=config::defaults();
        for key in ["SwipeActivities","NotificationsEnabled","NotificationClickOpenApp","DeviceRemindersEnabled"]{
            assert_eq!(s[key],json!(true),"{key}");
            assert!(config::validate_setting(json!({"key":key,"value":"yes","settings":s.clone()})).is_err(),"{key}");
            assert_eq!(config::validate_setting(json!({"key":key,"value":false,"settings":s.clone()})).unwrap()[key],json!(false),"{key}");
        }
        // A settings.json written by an older build has none of these keys, so the shipped
        // defaults — not the JSON — decide what a first run does.
        let loaded=config::load(std::path::Path::new("no-such-settings-from-an-older-build.json"));
        assert_eq!(loaded["SwipeActivities"],json!(true));assert_eq!(loaded["NotificationsEnabled"],json!(true));assert_eq!(loaded["DeviceRemindersEnabled"],json!(true));
        // A 1.4 install persisted `false` for all three and knows nothing about the marker, so the
        // one-time migration switches them on and records that it ran.
        let legacy=std::env::temp_dir().join(format!("dotisle-defaults-migration-{}.json",std::process::id()));
        std::fs::write(&legacy,r#"{"SwipeActivities":false,"NotificationsEnabled":false,"DeviceRemindersEnabled":false}"#).unwrap();
        let migrated=config::load(&legacy);
        assert_eq!(migrated["DefaultsOnboardingVersion"],json!(1));
        for key in ["SwipeActivities","NotificationsEnabled","DeviceRemindersEnabled"]{assert_eq!(migrated[key],json!(true),"{key}");}
        // Once the marker is stamped those same false values are a real opt-out and must survive.
        std::fs::write(&legacy,r#"{"DefaultsOnboardingVersion":1,"SwipeActivities":false,"NotificationsEnabled":false,"DeviceRemindersEnabled":false}"#).unwrap();
        let opted_out=config::load(&legacy);
        for key in ["SwipeActivities","NotificationsEnabled","DeviceRemindersEnabled"]{assert_eq!(opted_out[key],json!(false),"{key}");}
        let _=std::fs::remove_file(&legacy);
        assert!(notifications::Policy::from_settings(&json!({})).enabled);
        // Nothing opted in, so a first-run island already swipes between components.
        let mut a=app();a.timers.create(&json!({"mode":"stopwatch"})).unwrap();a.refresh_components();
        assert_eq!(a.components.len(),2);assert!(a.can_swipe(0));assert!(a.can_swipe(41));assert!(!a.can_swipe(2));
    }
}












#[cfg(test)]mod v121_tests{use super::*;
#[test]fn independent_click_defaults_and_retired_options(){let s=config::defaults();assert_eq!(s["NotificationClickOpenApp"],true);assert_eq!(s["MusicClickOpenApp"],false);for key in ["NotificationCompatibility","CardMaterial"]{assert!(s.get(key).is_none());assert!(config::validate_setting(json!({"key":key,"value":true,"settings":s})).is_err());}let s=config::validate_setting(json!({"key":"MusicClickOpenApp","value":true,"settings":s})).unwrap();assert_eq!(s["NotificationClickOpenApp"],true);assert_eq!(s["MusicClickOpenApp"],true);}
#[test]fn preview_and_close_do_not_launch_app(){let mut app=App::new(PathBuf::new(),PathBuf::new(),true);app.notification_preview=true;app.banner.notice=Some(notifications::Notice{app_id:"".into(),..Default::default()});unsafe{app.activate(12)}assert!(app.error.is_none());app.notification_preview=false;unsafe{app.activate(11)}assert!(app.error.is_none());unsafe{app.activate(12)}assert!(app.error.is_some());app.error=None;app.settings["NotificationClickOpenApp"]=json!(false);unsafe{app.activate(12)}assert!(app.error.is_none());}
}

#[cfg(test)]mod music_jump_tests{use super::*;
#[test]fn music_title_and_cover_follow_independent_switch(){unsafe{let mut a=App::new(PathBuf::new(),PathBuf::new(),true);a.opacity=1.;a.expanded=true;a.frame=Frame::expanded();a.transition=None;let c=a.layout.card;assert_eq!(a.hit(c.x+150.,c.y+40.),0);a.settings["MusicClickOpenApp"]=json!(true);assert_eq!(a.hit(c.x+150.,c.y+40.),13);assert_eq!(a.hit(c.x+30.,c.y+30.),1);let expanded=a.expanded;a.activate(13);assert_eq!(a.expanded,expanded);assert!(a.error.is_some());let before=a.snapshot["isPlaying"].clone();a.activate(4);assert_ne!(a.snapshot["isPlaying"],before);}}
}

#[cfg(test)]mod v13_tests{use super::*;
fn app()->App{App::new(PathBuf::new(),PathBuf::new(),true)}
fn reminder(id:&str)->todos::Todo{todos::Todo{color:"#F3D79B".into(),id:id.into(),title:format!("待办 {id}"),content:String::new(),date:"2026-10-16".into(),time:"09:00".into(),daily:false,completed:false,completed_date:None,last_reminded_date:None,snooze_until:None}}
fn notice(title:&str)->Queued{Queued{notice:notifications::Notice{title:title.into(),..Default::default()},todo:None,chime:true}}
/// The capsule may shrink for a short lyric, but the window and the expanded
/// card must stay exactly where they were.
#[test]fn collapsed_capsule_resizes_without_moving_host_or_card(){let mut a=app();let base=a.compact_base();a.layout.reserve_compact(base);let(host,card,card_x)=(a.layout.host.w,a.layout.card.w,a.layout.card.x);a.layout.set_compact(140.,40.);assert_eq!(a.layout.capsule.w,140.);assert_eq!(a.layout.host.w,host);assert_eq!(a.layout.card.w,card);a.layout.set_compact(base,48.);assert_eq!(a.layout.capsule.w,base);assert_eq!(a.layout.host.w,host);assert_eq!(a.layout.card.w,card);assert_eq!(a.layout.card.x,card_x);}
/// Auto width off is the 1.2 behaviour: the capsule is exactly the stored width.
#[test]fn auto_width_off_keeps_the_stored_capsule_width(){let mut a=app();unsafe{a.settings["CompactWidth"]=json!(300.);a.settings["AutoCompactWidth"]=json!(false);a.compact_size=compact::Resize::new(300.,40.);a.update_compact_layout().unwrap();assert_eq!(a.layout.capsule.w,300.);assert_eq!(a.compact_size.width_target,300.);a.settings["AutoCompactWidth"]=json!(true);a.update_compact_layout().unwrap();assert!(a.compact_size.width_target<300.&&a.compact_size.width_target>=140.);}}
#[test]fn compact_second_line_follows_both_switches(){let mut a=app();a.snapshot["frame"]=json!({"currentLine":"You are my sunshine","currentTranslation":"你是我的阳光","nextLine":"下一句"});
 a.settings["CompactTranslation"]=json!(true);a.settings["ChineseTwoLine"]=json!(false);assert_eq!(a.compact_secondary_for(),"你是我的阳光");
 a.settings["CompactTranslation"]=json!(false);assert_eq!(a.compact_secondary_for(),"");
 a.settings["ChineseTwoLine"]=json!(true);assert_eq!(a.compact_secondary_for(),"");
 a.snapshot["frame"]=json!({"currentLine":"晚风轻轻","currentTranslation":"","nextLine":"音乐刚好"});assert_eq!(a.compact_secondary_for(),"音乐刚好");
 a.settings["CompactTranslation"]=json!(true);assert_eq!(a.compact_secondary_for(),"音乐刚好");}
/// A burst of notifications must not overwrite each other, and a pending
/// reminder must never be evicted to make room for one.
#[test]fn notifications_queue_and_reminders_are_never_evicted(){let mut a=app();for i in 0..9{a.enqueue(notice(&format!("n{i}")));}assert_eq!(a.queue.len(),8);assert_eq!(a.queue.front().unwrap().notice.title,"n1");
 a.enqueue_todo(&reminder("t1"));assert_eq!(a.queue.len(),9);assert!(a.queue.iter().any(|e|e.todo.as_deref()==Some("t1")));
 for i in 0..40{a.enqueue(notice(&format!("m{i}")));}assert!(a.queue.iter().any(|e|e.todo.as_deref()==Some("t1")));}
#[test]fn completing_or_deleting_a_todo_cancels_its_pending_banner(){let mut a=app();a.enqueue_todo(&reminder("t1"));a.enqueue_todo(&reminder("t2"));a.cancel_todo("t1");assert_eq!(a.queue.len(),1);assert_eq!(a.queue[0].todo.as_deref(),Some("t2"));}
#[test]
fn state_exposes_todos_version_and_pending_page(){let mut a=app();a.pending_page=Some("todos".into());unsafe{let state=a.request(json!({"version":1,"command":"state"}));assert_eq!(state["version"],env!("CARGO_PKG_VERSION"));assert!(state["todos"].is_array());assert!(state["today"].as_str().is_some_and(|d|todos::valid_date(d)));assert_eq!(state["pendingPage"],"todos");assert_eq!(a.request(json!({"version":1,"command":"pageSeen"}))["success"],true);assert!(a.pending_page.is_none());}}
/// Demo runs are self-checks: they must never create or edit a todo file.
#[test]
fn demo_mode_leaves_the_todo_store_untouched(){let mut a=app();unsafe{let saved=a.request(json!({"version":1,"command":"todoSave","value":{"title":"x","date":"2026-10-16","time":"09:00"}}));assert_eq!(saved["success"],true);assert!(a.todos.items.is_empty());assert_eq!(a.request(json!({"version":1,"command":"todoComplete","value":{"id":"x","completed":true}}))["success"],true);assert_eq!(a.request(json!({"version":1,"command":"todoDelete","value":{"id":"x"}}))["success"],true);assert!(a.todos.items.is_empty());}}
/// Un-completing a reminder must not replay it: the delivery marker for that
/// date survives, so a finished item never chimes twice.
#[test]
fn uncompleting_a_reminder_does_not_replay_it(){let p=std::env::temp_dir().join(format!("LyricIsland.UncompleteTest.{}.{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));let mut store=todos::Store::new(p.clone());let now=todos::Now{date:"2026-10-16".into(),time:"09:00".into()};let id=store.save(&json!({"title":"喝水","date":"2026-10-16","time":"09:00"})).unwrap();assert_eq!(store.poll(&now).unwrap().len(),1);store.complete(&id,false,&now).unwrap();assert!(store.poll(&now).unwrap().is_empty());let _=std::fs::remove_file(p);}
#[test]
fn delivered_reminders_land_in_history_and_the_switch_silences_it(){let mut a=app();a.enqueue_todo(&reminder("t1"));assert_eq!(a.history.items.len(),1);assert_eq!(a.history.items[0].kind,"todo");assert_eq!(a.history.items[0].todo_id.as_deref(),Some("t1"));assert_eq!(a.history.list()[0]["title"],"待办 t1");a.settings["NotificationHistoryEnabled"]=json!(false);a.enqueue_todo(&reminder("t2"));assert_eq!(a.history.items.len(),1);}
#[test]
fn history_clear_rejects_unknown_kinds_and_can_target_one(){let mut a=app();a.enqueue_todo(&reminder("t1"));unsafe{assert_eq!(a.request(json!({"version":1,"command":"historyClear","value":{"kind":"bogus"}}))["success"],false);assert_eq!(a.request(json!({"version":1,"command":"historyClear"}))["success"],true);}assert!(a.history.items.is_empty());}
#[test]
fn island_presence_follows_need_not_switches(){assert!(island_needed(true,true,true,false,false,false,false,false));assert!(island_needed(true,false,false,false,false,false,false,false));assert!(!island_needed(true,false,true,false,false,false,false,false));assert!(island_needed(false,false,true,true,false,false,false,false));assert!(island_needed(false,false,true,false,false,true,false,false));assert!(island_needed(false,false,true,false,true,false,false,false));assert!(island_needed(false,false,true,false,false,false,true,false));assert!(island_needed(false,false,true,false,false,false,false,true));assert!(!island_needed(false,false,true,false,false,false,false,false));}
}

#[cfg(test)]mod hide_gesture_tests{use super::*;
 #[test]fn hide_is_available_with_one_component_and_preserves_timers(){let mut a=App::new(PathBuf::new(),PathBuf::new(),true);assert!(a.can_hide(0));assert!(a.can_hide(1));for hit in [2,3,4,5,6,7,9,10,11,12,30,31]{assert!(!a.can_hide(hit));}let id=a.timers.create(&json!({"mode":"stopwatch"})).unwrap();a.hide_by_gesture();assert!(a.user_hidden);assert!(!a.held);assert!(a.timers.items.iter().any(|t|t.id==id&&t.running));a.settings["SwipeUpHide"]=json!(false);assert!(!a.can_hide(0));}
}

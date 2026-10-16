#![windows_subsystem = "windows"]
mod notification_store;mod cover_color;mod app_icon;mod motion;mod layout;mod renderer;mod config;mod font_probe;mod progress;mod notifications;mod banner;
use std::{cell::RefCell,path::{Path,PathBuf},sync::{Arc,Mutex,OnceLock,mpsc},time::{Instant,Duration},ffi::{c_char,CStr}};
use serde_json::{Value,json};
use base64::Engine;
use windows::{core::{w,PCWSTR,Result},Win32::{Foundation::*,System::{Com::*,LibraryLoader::GetModuleHandleW,Threading::*},Graphics::Gdi::*,UI::{WindowsAndMessaging::*,Shell::*,Input::KeyboardAndMouse::*,HiDpi::*}}};
use lyric_backend::{runtime::Backend,io,ipc::Server,shell};
use motion::{Frame,Options,Transition};use layout::{Layout,Rect};use renderer::{Renderer,Scene};
const WM_TRAY:u32=WM_APP+1;const WM_SETTINGS:u32=WM_APP+2;
type Pending=(Value,mpsc::Sender<Value>);
static REQUESTS:OnceLock<Mutex<Vec<Pending>>>=OnceLock::new();static WINDOW:OnceLock<isize>=OnceLock::new();
fn inside_round_rect(r:Rect,radius:f64,x:f64,y:f64)->bool{
    if !r.contains(x,y){return false}let radius=radius.min(r.w/2.).min(r.h/2.).max(0.);
    let cx=x.clamp(r.x+radius,r.x+r.w-radius);let cy=y.clamp(r.y+radius,r.y+r.h-radius);
    (x-cx).powi(2)+(y-cy).powi(2)<=radius*radius
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
    banner:banner::Banner,notification_preview:bool,notification_listener:Option<notifications::Listener>,notification_state:Value,
    hwnd:HWND,visual_hwnd:HWND,input_region:Option<[i32;6]>,dragging:Option<(POINT,RECT)>,renderer:Option<Renderer>,base:PathBuf,data:PathBuf,settings:Value,backend:Option<Arc<Backend>>,
    _server:Option<Server>,snapshot:Value,cover_version:u64,demo:bool,
    layout:Layout,frame:Frame,transition:Option<Transition>,transition_start:Instant,sampled_ms:f64,
    expanded:bool,held:bool,hover:i32,pressed:i32,seeking:bool,seek_fraction:f64,command_busy:Arc<std::sync::atomic::AtomicBool>,
    start:Instant,last_poll:Instant,lyric:String,previous_lyric:String,lyric_start:Instant,
    cursor_inside:bool,leave_at:Option<Instant>,last_left:bool,user_hidden:bool,display_wanted:bool,missing_since:Option<Instant>,
    opacity:f64,fade_from:f64,fade_to:f64,fade_start:Instant,hide_at:Option<Instant>,dirty:bool,
    notification_setup:Option<std::process::Child>,notification_setup_message:String,
    settings_process:Option<std::process::Child>,render_frames:u64,error:Option<String>,transparent:bool,activity:motion::Activity,cover_start:Instant,progress:progress::PlaybackProgress,outgoing_opacity:f64,outgoing_y:f64,
}
impl App {
    fn new(base:PathBuf,data:PathBuf,demo:bool)->Self{
        let settings=if demo{config::defaults()}else{config::load(&data.join("settings.json"))};let options=serde_json::from_value::<Options>(settings["Motion"].clone()).unwrap_or_default();
        let backend=if demo{None}else{Some(Backend::new(settings.clone(),data.clone(),PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()),data.join("Logs")))};
        let now=Instant::now();let snapshot=if demo{json!({"track":{"id":"demo","title":"晚风电台","artist":"Lyric Island","sourceApp":"演示","durationSeconds":236.},"isPlaying":true,"positionSeconds":83.,"canSeek":true,"frame":{"currentLine":"晚风轻轻，音乐刚好","nextLine":"让这一刻慢下来","wordScanProgress":0.4}})}else{Value::Null};
        let notification_listener=if demo{None}else{Some(notifications::Listener::new(notifications::Policy::from_settings(&settings)))};
        Self{banner:banner::Banner::default(),notification_preview:false,notification_listener,notification_state:json!({"access":"disabled","message":"通知提醒已关闭","apps":[]}),hwnd:HWND::default(),visual_hwnd:HWND::default(),input_region:None,dragging:None,renderer:None,base,data,layout:Layout::new(settings["CompactWidth"].as_f64().unwrap_or(260.)),settings,backend,_server:None,snapshot,cover_version:0,demo,frame:Frame::hidden(options),transition:None,transition_start:now,sampled_ms:0.,expanded:false,held:false,hover:0,pressed:0,seeking:false,seek_fraction:0.,command_busy:Arc::new(std::sync::atomic::AtomicBool::new(false)),start:now,last_poll:now-Duration::from_secs(1),lyric:"打开播放器，开始一首音乐".into(),previous_lyric:String::new(),lyric_start:now-Duration::from_secs(2),cursor_inside:false,leave_at:None,last_left:false,user_hidden:false,display_wanted:demo,missing_since:None,opacity:if demo{1.}else{0.},fade_from:0.,fade_to:if demo{1.}else{0.},fade_start:now,hide_at:None,dirty:true,notification_setup:None,notification_setup_message:String::new(),settings_process:None,render_frames:0,error:None,transparent:false,activity:motion::Activity::new(),cover_start:now-Duration::from_secs(1),progress:progress::PlaybackProgress::default(),outgoing_opacity:1.,outgoing_y:0.}
    }
    fn flag(&self,k:&str,default:bool)->bool{self.settings[k].as_bool().unwrap_or(default)}
    fn options(&self)->Options{serde_json::from_value(self.settings["Motion"].clone()).unwrap_or_default()}
    unsafe fn position(&mut self)->Result<()> {
        self.layout=Layout::new(self.settings["CompactWidth"].as_f64().unwrap_or(260.));
        let monitor=MonitorFromWindow(self.hwnd,MONITOR_DEFAULTTONEAREST);let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};let _=GetMonitorInfoW(monitor,&mut info);
        let dpi=GetDpiForWindow(self.hwnd).max(96)as f64;let scale=dpi/96.;let work=info.rcWork;let width=(self.layout.host.w*scale).ceil()as i32;let height=(self.layout.host.h*scale).ceil()as i32;
        let floating=self.flag("Floating",false);let center=if floating{self.settings["CenterX"].as_f64().unwrap_or(0.5)}else{0.5};
        let x=(work.left as f64+(work.right-work.left)as f64*center-width as f64/2.).round()as i32;let x=x.clamp(work.left,(work.right-width).max(work.left));
        let y=if floating{(work.top as f64+((work.bottom-work.top)as f64-40.*scale)*self.settings["TopFraction"].as_f64().unwrap_or(0.08)-24.*scale).round()as i32}else{work.top-(24.*scale).round()as i32};
        SetWindowPos(self.hwnd,Some(if self.flag("AlwaysOnTop",true){HWND_TOPMOST}else{HWND_NOTOPMOST}),x,y,width,height,SWP_NOACTIVATE)?;
        SetWindowPos(self.visual_hwnd,Some(if self.flag("AlwaysOnTop",true){HWND_TOPMOST}else{HWND_NOTOPMOST}),x,y,width,height,SWP_NOACTIVATE)?;
        let font=self.base.join("Assets/Fonts/NotoSansSC.ttf");self.renderer=None;self.renderer=Some(Renderer::new(self.visual_hwnd,width as u32,height as u32,dpi as f32,&font)?);self.input_region=None;self.sync_input_region()?;self.cover_version=u64::MAX;self.last_poll=Instant::now()-Duration::from_secs(1);self.dirty=true;Ok(())
    }
    fn retarget(&mut self,expanded:bool){
        if self.expanded==expanded{return}let velocity=self.transition.as_ref().map(|t|t.velocity(self.sampled_ms)).unwrap_or_default();
        self.expanded=expanded;let o=self.options();
        if self.flag("Animation",true){self.transition=Some(Transition::new(expanded,self.frame,o,velocity,false));self.transition_start=Instant::now();self.sampled_ms=0.;}
        else{self.transition=None;self.frame=if expanded{Frame::expanded()}else{Frame::hidden(o)};}
        self.dirty=true;
    }
    fn fade(&mut self,target:f64){if self.fade_to!=target{self.fade_from=self.opacity;self.fade_to=target;self.fade_start=Instant::now();self.dirty=true;}}
    fn command(&self,action:&str,seconds:f64){if let Some(backend)=self.backend.clone(){use std::sync::atomic::Ordering;if self.command_busy.swap(true,Ordering::AcqRel){return}let busy=self.command_busy.clone();let action=action.to_string();std::thread::spawn(move||{let _=backend.command(&action,seconds);busy.store(false,Ordering::Release);});}}
    fn show_settings(&mut self){if self.settings_process.as_mut().is_some_and(|p|p.try_wait().ok().flatten().is_none()){return}let exe=self.base.join("LyricIsland.Settings.exe");if exe.exists(){self.settings_process=std::process::Command::new(exe).env("LYRICISLAND_TEST_PIPE",std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into())).env("LYRICISLAND_NATIVE_DATA",&self.data).current_dir(&self.base).spawn().ok();}else{self.error=Some("设置组件缺失，请完整解压运行包".into());}}
    fn save(&mut self)->std::result::Result<(),String>{if self.demo{return Ok(())}io::save_settings(&self.data.join("settings.json"),&self.settings)}
    fn poll(&mut self){
        let previous=self.snapshot.clone();self.last_poll=Instant::now();if let Some(b)=&self.backend{self.snapshot=b.snapshot(self.cover_version);}
        let version=self.snapshot["coverVersion"].as_u64().unwrap_or(0);
        if version!=self.cover_version{self.cover_start=Instant::now();self.cover_version=version;let cover=self.snapshot["cover"].as_str().and_then(|s|base64::engine::general_purpose::STANDARD.decode(s).ok());if let Some(r)=&mut self.renderer{if let Err(e)=unsafe{r.set_cover(cover.as_deref())}{self.error=Some(e.to_string());}}}
        let lyric=self.snapshot["frame"]["currentLine"].as_str().filter(|s|!s.is_empty()).unwrap_or("打开播放器，开始一首音乐").to_string();
        if lyric!=self.lyric{let t=(self.lyric_start.elapsed().as_secs_f64()/0.18).min(1.);self.outgoing_opacity=t;self.outgoing_y=4.*(1.-t).powi(3);self.previous_lyric=std::mem::replace(&mut self.lyric,lyric);self.lyric_start=Instant::now();}
        let playing=self.snapshot["isPlaying"]==true;
        if playing!=self.activity.playing{self.activity.retarget(playing,self.start.elapsed().as_secs_f64()*1000.,self.flag("Animation",true));}
        let duration=self.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.);
        self.progress.update(self.start.elapsed().as_secs_f64(),self.snapshot["track"]["id"].as_str().unwrap_or(""),self.snapshot["positionSeconds"].as_f64().unwrap_or(0.),duration,playing,self.flag("Animation",true));
        if self.seeking&&previous["track"]["id"]!=self.snapshot["track"]["id"]{self.seeking=false;}
        if !self.demo {
            let valid=self.snapshot["track"]["title"].as_str().is_some_and(|s|!s.trim().is_empty())&&!self.snapshot["track"]["id"].as_str().unwrap_or("").ends_with("|ProcessFallback");
            let wanted=if valid{self.missing_since=None;self.snapshot["isPlaying"]==true||!self.flag("HideWhenPaused",true)}else{let since=self.missing_since.get_or_insert(Instant::now());self.display_wanted&&since.elapsed()<Duration::from_millis(600)};
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
        let (shape,radius)=self.layout.shape(self.frame.card_reveal,self.options().radius);
        if !inside_round_rect(shape,radius,x,y){return -1}
        if 1.-motion::blend(self.frame.card_reveal,0.06,0.46)>0.1&&self.layout.compact_cover(unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.}).contains(x,y){return 1}
        if self.expanded&&self.frame.content_opacity*motion::blend(self.frame.card_reveal,0.25,0.88)>0.01{
            let scale=(shape.w/self.layout.card.w).min(shape.h/self.layout.card.h);let tx=shape.x+(shape.w-self.layout.card.w*scale)/2.;let ty=shape.y+(shape.h-self.layout.card.h*scale)/2.;let x=(x-tx)/scale;let y=(y-ty)/scale-self.frame.content_offset_y;let w=self.layout.card.w;
            if Rect::new(18.,16.,80.,80.).contains(x,y){return 1}
            if Rect::new(8.,115.,w-16.,20.).contains(x,y){return 10}
            for (id,r) in layout::buttons(w,unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.}){if r.contains(x,y){return id}}
        }0
    }
    unsafe fn sync_input_region(&mut self)->Result<()>{
        // The click-through visual HWND retains the complete GPU surface, including
        // shadows. This separate, undrawn HWND receives input only inside the
        // animated contour. No alpha readback or cursor polling is required.
        let (r,radius)=if self.banner.visible(){let (r,radius,_)=self.banner.presentation(self.start.elapsed().as_secs_f64());(r,radius)}else{self.layout.shape(self.frame.card_reveal,self.options().radius)};
        let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;
        let key=if self.opacity<0.01{[0;6]}else{[(r.x*scale).floor()as i32,(r.y*scale).floor()as i32,((r.x+r.w)*scale).ceil()as i32,((r.y+r.h)*scale).ceil()as i32,(radius*2.*scale).round()as i32,(radius*2.*scale).round()as i32]};
        if self.input_region==Some(key){return Ok(())}
        let region=if key[4]>0{CreateRoundRectRgn(key[0],key[1],key[2],key[3],key[4],key[5])}else{CreateRectRgn(key[0],key[1],key[2],key[3])};
        if region.is_invalid(){return Err(windows::core::Error::from_thread())}
        if SetWindowRgn(self.hwnd,Some(region),false)==0{let _=DeleteObject(region.into());return Err(windows::core::Error::from_thread())}
        self.input_region=Some(key);Ok(())
    }
    unsafe fn remember_position(&mut self){let mut bounds=RECT::default();let _=GetWindowRect(self.hwnd,&mut bounds);let monitor=MonitorFromWindow(self.hwnd,MONITOR_DEFAULTTONEAREST);let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};let _=GetMonitorInfoW(monitor,&mut info);let w=info.rcWork;let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;self.settings["CenterX"]=json!(((bounds.left+bounds.right)as f64/2.-w.left as f64)/(w.right-w.left).max(1)as f64);self.settings["TopFraction"]=json!(((bounds.top-w.top)as f64+24.*scale)/((w.bottom-w.top)as f64-40.*scale).max(1.));let _=self.save();}
    unsafe fn activate(&mut self,id:i32){match id {
        11|12=>{let (base,radius)=self.layout.shape(self.frame.card_reveal,self.options().radius);self.banner.dismiss(base,radius,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));},
        0=>{if self.expanded{self.held=false;self.retarget(false)}else if self.flag("ClickAnywhereExpand",false){self.held=true;self.retarget(true)}},
        1=>{self.held=!self.held;self.retarget(self.held)},2=>{if self.flag("Floating",false){let mut cursor=POINT::default();let mut bounds=RECT::default();let _=GetCursorPos(&mut cursor);let _=GetWindowRect(self.hwnd,&mut bounds);self.dragging=Some((cursor,bounds));SetCapture(self.hwnd);}},
        3=>self.command("previous",0.),4=>{if self.demo{self.snapshot["isPlaying"]=json!(self.snapshot["isPlaying"]!=true)}else{self.command("toggle",0.)}},5=>self.command("next",0.),
        6=>{self.settings["Translation"]=json!(!self.flag("Translation",true));let _=self.save();},7=>self.show_settings(),8=>{self.held=false;self.retarget(false)},
        9=>{let title=self.snapshot["track"]["title"].as_str().unwrap_or("").to_string();let source=self.snapshot["track"]["sourceApp"].as_str().unwrap_or("").to_string();std::thread::spawn(move||{shell::favorite(&title,&source,true);});},_=>{}}
        self.dirty=true;
    }
    fn update_seek(&mut self,x:f64){let (shape,_)=self.layout.shape(self.frame.card_reveal,self.options().radius);let scale=(shape.w/self.layout.card.w).min(shape.h/self.layout.card.h);let tx=shape.x+(shape.w-self.layout.card.w*scale)/2.;self.seek_fraction=((x-tx-18.*scale)/((self.layout.card.w-36.)*scale)).clamp(0.,1.);self.dirty=true;}
    fn progress_value(&self)->f64{self.progress.fraction(self.start.elapsed().as_secs_f64())}
    fn scene(&self)->Scene<'_>{
        let elapsed=self.progress.seconds(self.start.elapsed().as_secs_f64());let duration=self.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.);let track=&self.snapshot["track"];let f=&self.snapshot["frame"];
        let col=|k:&str,default:u32|self.settings[k].as_str().and_then(|s|u32::from_str_radix(s.trim_start_matches('#'),16).ok()).map(|v|0xff000000|v).unwrap_or(default);
        let (notice_rect,notice_radius,notice_mix)=self.banner.presentation(self.start.elapsed().as_secs_f64());
        Scene{cover_background:self.flag("CoverBackground",false),demo:self.demo,notification:self.banner.notice.as_ref(),notification_rect:notice_rect,notification_radius:notice_radius,notification_mix:notice_mix,layout:self.layout,frame:self.frame,radius:self.options().radius,resizing:self.transition.is_some(),time_ms:self.start.elapsed().as_secs_f64()*1000.,lyric:&self.lyric,previous_lyric:&self.previous_lyric,lyric_age_ms:if self.flag("Animation",true){self.lyric_start.elapsed().as_secs_f64()*1000.}else{1000.},secondary:if self.flag("Translation",true){f["currentTranslation"].as_str().filter(|s|!s.is_empty()).or_else(||f["nextLine"].as_str()).unwrap_or("等待歌词")}else{f["nextLine"].as_str().unwrap_or("等待歌词")},title:track["title"].as_str().unwrap_or("暂无播放"),artist:track["artist"].as_str().unwrap_or("打开播放器，开始一首音乐"),playing:self.snapshot["isPlaying"]==true,highlight:if self.flag("WordHighlight",true){f["wordScanProgress"].as_f64().unwrap_or(0.).clamp(0.,1.)}else{0.},progress:if self.seeking{self.seek_fraction}else{self.progress_value()},elapsed:if self.seeking{self.seek_fraction*duration}else{elapsed},duration,font_size:self.settings["FontSize"].as_f64().unwrap_or(12.5),text_color:col("ExpandedTextColor",0xfff4f6f8),component_color:col("ExpandedComponentColor",0xffb1e8d1),progress_color:col("LyricProgressColor",0xffa9e6cd),hover:self.hover,pressed:self.pressed,opacity:self.opacity as f32,transparent:self.transparent,floating:self.flag("Floating",false),favorite_enabled:track["sourceApp"].as_str().is_some_and(|s|s.eq_ignore_ascii_case("Netease")),has_track:track.is_object(),layout_scale:unsafe{GetDpiForWindow(self.hwnd).max(96)as f64/96.},activity_scales:self.activity.sample(self.start.elapsed().as_secs_f64()*1000.,self.flag("Animation",true)).0,activity_opacity:self.activity.sample(self.start.elapsed().as_secs_f64()*1000.,self.flag("Animation",true)).1,cover_opacity:1.,outgoing_opacity:self.outgoing_opacity,outgoing_y:self.outgoing_y}
    }
    unsafe fn draw(&mut self)->Result<()>{// Take the renderer to split immutable scene data from mutable graphics state.
        if let Some(mut r)=self.renderer.take(){let result=r.render(&self.scene(),true);self.renderer=Some(r);result?;self.render_frames+=1;}self.dirty=false;Ok(())
    }
    fn show_notice(&mut self,notice:notifications::Notice,preview:bool){
        self.notification_preview=preview;
        let (base,radius)=self.layout.shape(self.frame.card_reveal,self.options().radius);
        if let Some(r)=&mut self.renderer{let _=unsafe{r.set_notification_icon(notice.icon.as_deref())};}
        let width=440_f64.min(self.layout.host.w-48.);let height=self.renderer.as_mut().and_then(|r|unsafe{r.notification_height(&notice,width)}.ok()).unwrap_or(80.);
        self.banner.show(notice,base,radius,self.layout.host.w,height,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));
        self.hide_at=None;self.fade(1.);self.dirty=true;
    }
    unsafe fn tick(&mut self)->Result<()> {
        if let Some(status)=self.notification_setup.as_mut().and_then(|child|child.try_wait().ok().flatten()){
            self.notification_setup=None;
            if status.success(){self.notification_setup_message="通知组件已准备好，正在重启并请求 Windows 授权".into();self.request(json!({"version":1,"command":"restart","notificationOnboarding":true}));}
            else{self.notification_setup_message=std::fs::read_to_string(self.data.join("notification-setup.json")).ok().and_then(|s|serde_json::from_str::<Value>(s.trim_start_matches('\u{feff}')).ok()).and_then(|v|v["message"].as_str().map(str::to_owned)).unwrap_or_else(||"通知准备未完成，可点击一键启用重试".into());}
        }

        let events=self.notification_listener.as_ref().map(|l|l.events.try_iter().collect::<Vec<_>>()).unwrap_or_default();
        for event in events{match event{
            notifications::Event::State(state)=>{self.notification_state=state;},
            notifications::Event::Notice(notice)=>{let policy=notifications::Policy::from_settings(&self.settings);if !self.user_hidden&&policy.should_show()&&policy.permits(&notice.app_id,&notice.app_name){self.show_notice(notice,false);}}
        }}
        if self.banner.visible()&&self.flag("PauseNotifications",false) {let (r,rad)=self.layout.shape(self.frame.card_reveal,self.options().radius);self.banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}
        if self.last_poll.elapsed()>=Duration::from_millis(90){self.poll()}
        if let Some(t)=self.transition{self.sampled_ms=self.transition_start.elapsed().as_secs_f64()*1000.;self.frame=t.sample(self.sampled_ms);self.dirty=true;if self.sampled_ms>=t.duration_ms{self.transition=None;self.frame=t.end;}}
        if !self.banner.visible()&&self.hide_at.is_some_and(|t|Instant::now()>=t){self.hide_at=None;self.fade(0.);}
        if (self.opacity-self.fade_to).abs()>0.00001{let t=if self.flag("Animation",true){(self.fade_start.elapsed().as_secs_f64()/0.2).min(1.)}else{1.};self.opacity=self.fade_from+(self.fade_to-self.fade_from)*(1.-(std::f64::consts::PI*t).cos())/2.;self.dirty=true;}
        let mut pt=POINT::default();let _=GetCursorPos(&mut pt);let mut local=pt;let _=ScreenToClient(self.hwnd,&mut local);let scale=GetDpiForWindow(self.hwnd).max(96)as f64/96.;let x=local.x as f64/scale;let y=local.y as f64/scale;
        let hit=self.hit(x,y);let inside=hit>=0;
        if !self.banner.visible()&&inside&&!self.cursor_inside{self.leave_at=None;if self.flag("HoverExpand",false){self.retarget(true)}}
        if !inside&&self.cursor_inside&&!self.held{self.leave_at=Some(Instant::now());}
        if !self.banner.visible()&&self.leave_at.is_some_and(|t|t.elapsed()>=Duration::from_millis(1200)){self.leave_at=None;if !self.held{self.retarget(false)}}
        self.cursor_inside=inside;if self.hover!=hit{self.hover=hit;self.dirty=true;}
        let left=GetAsyncKeyState(VK_LBUTTON.0 as i32)<0;if !self.banner.visible()&&left&&!self.last_left&&!inside&&self.expanded{self.held=false;self.retarget(false)}self.last_left=left;
        if self.opacity>0.001&&(self.snapshot["isPlaying"]==true&&self.flag("Animation",true)||self.lyric_start.elapsed()<Duration::from_millis(220)){self.dirty=true;}
        if self.banner.visible(){
            let (r,rad)=self.layout.shape(self.frame.card_reveal,self.options().radius);
            self.banner.tick(r,rad,self.start.elapsed().as_secs_f64(),inside,self.flag("Animation",true));self.dirty=true;
            if !self.banner.visible(){self.last_poll=Instant::now()-Duration::from_secs(1);if !self.display_wanted&&!self.demo{self.fade(0.);}}
        }
        if let Some((start,bounds))=self.dragging{if left{let dx=pt.x-start.x;let dy=pt.y-start.y;for hwnd in [self.visual_hwnd,self.hwnd]{SetWindowPos(hwnd,None,bounds.left+dx,bounds.top+dy,0,0,SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOSIZE)?;}}else{self.dragging=None;let _=ReleaseCapture();self.remember_position();}}
        self.sync_input_region()?;if self.dirty{if self.opacity>0.001 || self.fade_from>0.001 {self.draw()?;if self.opacity<=0.001{self.fade_from=0.;}}else{self.dirty=false;}}Ok(())
    }
    unsafe fn request(&mut self,r:Value)->Value {
        if r["version"]!=1{return json!({"success":false,"message":"请求版本无效"})}let command=r["command"].as_str().unwrap_or("");
        match command {
            "state"=>{let t=&self.snapshot["track"];json!({"success":true,"version":"1.2","settings":self.settings,"title":t["title"],"artist":t["artist"],"player":t["sourceApp"],"position":self.snapshot["positionSeconds"],"duration":t["durationSeconds"],"timeline":self.snapshot["clockStatus"],"lyricSource":self.snapshot["lyricSource"],"isPlaying":self.snapshot["isPlaying"],"canSeek":self.snapshot["canSeek"],"expanded":self.expanded,"islandVisible":self.opacity>0.001,"processId":std::process::id(),"renderFrames":self.render_frames,"renderer":"Direct2D/DirectWrite/DirectComposition","lastError":self.error,"notificationState":self.notification_state,"notificationSetup":self.notification_setup_message,"notificationVisible":self.banner.visible()})},
            "expand"=>{self.user_hidden=false;self.fade(1.);self.held=true;self.retarget(true);json!({"success":true})},
            "toggleVisibility"=>{self.user_hidden=!self.user_hidden;self.fade(if self.user_hidden{0.}else{1.});json!({"success":true})},
            "notificationPreview"=>{if self.flag("PauseNotifications",false){return json!({"success":false,"message":"通知已暂停，请先关闭“暂停通知”后预览"})}self.show_notice(notifications::Notice::example(&self.base),true);json!({"success":true,"message":"通知横幅预览"})},
            "notificationSupport"=>{let script=self.base.join("notification-support/Install-Notifications.ps1");if !script.exists(){json!({"success":false,"message":"通知支持组件缺失，请完整解压新版运行包"})}else{use std::os::windows::process::CommandExt;match std::process::Command::new("powershell.exe").args(["-NoProfile","-ExecutionPolicy","Bypass","-File"]).arg(script).creation_flags(0x10).spawn(){Ok(_)=>json!({"success":true,"message":"请在安装窗口确认。安装完成后重新启动歌词岛，再授权通知访问。"}),Err(e)=>json!({"success":false,"message":e.to_string()})}}},
            "toggleNotifications"=>{
                if !self.flag("NotificationsEnabled",false){return self.request(json!({"version":1,"command":"notificationSetup"}))}
                let paused=!self.flag("PauseNotifications",false);
                self.request(json!({"version":1,"command":"update","value":{"key":"PauseNotifications","value":paused}}))
            },
            "notificationSetup"=>{
                if self.notification_setup.is_some(){return json!({"success":true,"message":"正在准备通知组件，请完成系统提示"})}
                let old=self.settings.clone();self.settings["NotificationsEnabled"]=json!(true);self.settings["PauseNotifications"]=json!(false);
                if let Err(e)=self.save(){self.settings=old;return json!({"success":false,"message":e})}
                if let Some(listener)=&self.notification_listener{listener.configure(notifications::Policy::from_settings(&self.settings));}
                if self.flag("NotificationCompatibility",true){self.notification_setup_message="免证书兼容通知已启用，无需安装证书或注册组件".into();return json!({"success":true,"message":self.notification_setup_message})}
                if notifications::access_status().is_ok(){return self.request(json!({"version":1,"command":"notificationAccess"}))}
                let script=self.base.join("notification-support/Setup-Notifications.ps1");
                if !script.is_file(){return json!({"success":false,"message":"通知组件缺失，请完整解压新版运行包"})}
                use std::os::windows::process::CommandExt;
                let _=std::fs::create_dir_all(&self.data);
                match std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-ExecutionPolicy","Bypass","-File"]).arg(script).arg("-ResultFile").arg(self.data.join("notification-setup.json")).creation_flags(0x08000000).spawn(){
                    Ok(child)=>{self.notification_setup=Some(child);self.notification_setup_message="正在自动准备通知组件，请确认 Windows 系统提示；完成后会自动重启".into();json!({"success":true,"message":self.notification_setup_message})},
                    Err(e)=>json!({"success":false,"message":format!("无法准备通知组件：{e}")})
                }
            },
            "notificationAccess"=>{if notifications::access_status().ok()==Some(windows::UI::Notifications::Management::UserNotificationListenerAccessStatus::Denied){return self.request(json!({"version":1,"command":"notificationSettings"}))}match notifications::request_access(){Ok(())=>json!({"success":true,"message":"请在 Windows 授权窗口允许通知访问"}),Err(e)=>json!({"success":false,"message":format!("无法申请通知权限（{}），请使用通知安装版",e.code())})}},
            "notificationSettings"=>{let uri=windows::core::HSTRING::from("ms-settings:privacy-notifications");let _=std::process::Command::new("explorer.exe").arg(uri.to_string()).spawn();json!({"success":true,"message":"已打开 Windows 通知访问设置"})},
            "restart"=>{
                let result=(||->std::io::Result<()>{
                    let exe=std::env::current_exe()?;let mut command=std::process::Command::new(exe);
                    command.args(["--restart-after",&std::process::id().to_string(),"--data-dir"]).arg(&self.data);
                    if r["notificationOnboarding"]==true{command.arg("--notification-onboarding");}
                    if self.demo{command.arg("--demo");}
                    command.current_dir(&self.base).spawn()?;Ok(())
                })();
                match result{Ok(())=>{let _=self.save();if let Some(mut settings)=self.settings_process.take(){let _=settings.kill();let _=settings.wait();}let _=PostMessageW(Some(self.hwnd),WM_CLOSE,WPARAM(0),LPARAM(0));json!({"success":true,"message":"正在重启歌词岛"})},Err(e)=>json!({"success":false,"message":format!("重启失败：{e}")})}
            },
            "settings"=>{self.show_settings();json!({"success":true})},"exit"=>{let _=PostMessageW(Some(self.hwnd),WM_CLOSE,WPARAM(0),LPARAM(0));json!({"success":true})},
            "synchronize"=>{if let Some(n)=r["value"].as_f64().filter(|n|n.is_finite()&&(0.0..=86400.).contains(n)){self.command("synchronize",n);json!({"success":true})}else{json!({"success":false,"message":"请输入有效的播放秒数"})}},
            "update"|"resetAppearance"=>{
                let old=self.settings.clone();let next=if command=="resetAppearance"{let mut s=old.clone();s["CompactWidth"]=json!(260.);s["FontSize"]=json!(12.5);s["Floating"]=json!(false);Ok(s)}else{config::validate_setting(json!({"key":r["value"]["key"],"value":r["value"]["value"],"settings":old}))};
                let next=match next{Ok(v)=>v,Err(e)=>return json!({"success":false,"message":e})};self.settings=next;
                let saved=if !self.demo&&r["value"]["key"]=="AutoStart"{shell::set_startup(self.flag("AutoStart",false),&self.base.join("LyricIsland.exe").to_string_lossy(),&self.data.join("settings.json"),&self.settings,None)}else{self.save()};
                if let Err(e)=saved{self.settings=old;return json!({"success":false,"message":e})}
                if let Some(listener)=&self.notification_listener{listener.configure(notifications::Policy::from_settings(&self.settings));}
                if !self.notification_preview&&self.banner.visible()&&(!self.flag("NotificationsEnabled",false)||self.flag("PauseNotifications",false)||self.banner.notice.as_ref().is_some_and(|n|!notifications::Policy::from_settings(&self.settings).permits(&n.app_id,&n.app_name))){let (r,rad)=self.layout.shape(self.frame.card_reveal,self.options().radius);self.banner.dismiss(r,rad,self.start.elapsed().as_secs_f64(),self.flag("Animation",true));}
                if let Some(b)=&self.backend{b.apply(self.settings.clone());}if let Err(e)=self.position(){self.error=Some(e.to_string());return json!({"success":false,"message":e.to_string()})}
                self.transition=None;self.frame=if self.expanded{Frame::expanded()}else{Frame::hidden(self.options())};json!({"success":true,"message":"已保存"})
            },_=>json!({"success":false,"message":"不支持的操作"})
        }
    }
}
unsafe extern "system" fn visual_wndproc(hwnd:HWND,msg:u32,wparam:WPARAM,lparam:LPARAM)->LRESULT{DefWindowProcW(hwnd,msg,wparam,lparam)}
unsafe extern "system" fn wndproc(hwnd:HWND,msg:u32,wparam:WPARAM,lparam:LPARAM)->LRESULT{
    if msg==WM_NCCREATE{let create=&*(lparam.0 as *const CREATESTRUCTW);SetWindowLongPtrW(hwnd,GWLP_USERDATA,create.lpCreateParams as isize);return LRESULT(1)}
    if msg==WM_DESTROY{let mut icon=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:hwnd,uID:1,..Default::default()};let _=Shell_NotifyIconW(NIM_DELETE,&mut icon);PostQuitMessage(0);return LRESULT(0)} if msg==WM_NCDESTROY{SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);return DefWindowProcW(hwnd,msg,wparam,lparam)} let ptr=GetWindowLongPtrW(hwnd,GWLP_USERDATA)as *const RefCell<App>;
    if !ptr.is_null(){if let Ok(mut app)=(*ptr).try_borrow_mut(){let a=&mut *app;match msg{
        WM_NCHITTEST=>{let mut pt=POINT{x:(lparam.0 as u16 as i16)as i32,y:((lparam.0>>16)as u16 as i16)as i32};let _=ScreenToClient(hwnd,&mut pt);let s=GetDpiForWindow(hwnd).max(96)as f64/96.;return LRESULT(if a.hit(pt.x as f64/s,pt.y as f64/s)<0{HTTRANSPARENT as isize}else{HTCLIENT as isize})},
        WM_MOUSEACTIVATE=>return LRESULT(MA_NOACTIVATE as isize),WM_ERASEBKGND=>return LRESULT(1),
        WM_PAINT=>{let mut p=PAINTSTRUCT::default();let _=BeginPaint(hwnd,&mut p);let _=EndPaint(hwnd,&p);a.dirty=true;return LRESULT(0)},
        WM_LBUTTONDOWN=>{let s=GetDpiForWindow(hwnd).max(96)as f64/96.;let x=(lparam.0 as u16 as i16)as f64/s;let y=((lparam.0>>16)as u16 as i16)as f64/s;a.pressed=a.hit(x,y);a.dirty=true;if a.pressed==2{a.activate(2);a.pressed=0;}else{SetCapture(hwnd);if a.pressed==10&&a.snapshot["canSeek"]==true{a.seeking=true;a.update_seek(x);}}return LRESULT(0)},
        WM_MOUSEMOVE=>{if a.seeking{let s=GetDpiForWindow(hwnd).max(96)as f64/96.;a.update_seek((lparam.0 as u16 as i16)as f64/s);}return LRESULT(0)},
        WM_LBUTTONUP=>{let _=ReleaseCapture();let s=GetDpiForWindow(hwnd).max(96)as f64/96.;let x=(lparam.0 as u16 as i16)as f64/s;let y=((lparam.0>>16)as u16 as i16)as f64/s;let hit=a.hit(x,y);if a.seeking{a.seeking=false;a.command("seek",a.seek_fraction*a.snapshot["track"]["durationSeconds"].as_f64().unwrap_or(0.));}else if hit==a.pressed&&hit>=0{a.activate(hit)}a.pressed=0;a.dirty=true;return LRESULT(0)},
        WM_CAPTURECHANGED=>{a.seeking=false;a.pressed=0;a.dirty=true;return LRESULT(0)},
        WM_DPICHANGED|WM_DISPLAYCHANGE=>{if let Err(e)=a.position(){a.error=Some(e.to_string());}return LRESULT(0)},
        WM_SETTINGS=>{let requests=REQUESTS.get_or_init(Default::default).lock().map(|mut q|std::mem::take(&mut *q)).unwrap_or_default();for (r,tx) in requests{let response=a.request(r);let _=tx.send(response);}return LRESULT(0)},
        WM_TRAY=>{if lparam.0 as u32==WM_RBUTTONUP{let mut p=POINT::default();let _=GetCursorPos(&mut p);let menu=CreatePopupMenu();if let Ok(menu)=menu{for (id,text) in [(1,w!("展开灵动岛")),(2,w!("显示 / 隐藏灵动岛")),(3,w!("设置")),(5,w!("重启歌词岛")),(4,w!("退出"))]{let _=AppendMenuW(menu,MF_STRING,id,text);}let active=a.flag("NotificationsEnabled",false)&&!a.flag("PauseNotifications",false);
            let label=if !a.flag("NotificationsEnabled",false){w!("开启通知")}else if a.flag("PauseNotifications",false){w!("恢复通知（已暂停）")}else{w!("暂停通知（已开启）")};
            let flags=if active{MF_STRING|MF_CHECKED}else{MF_STRING|MF_UNCHECKED};let _=InsertMenuW(menu,3,flags|MF_BYPOSITION,6,label);
            let _=SetForegroundWindow(hwnd);let selected=TrackPopupMenuEx(menu,(TPM_RETURNCMD|TPM_RIGHTBUTTON).0,p.x,p.y,hwnd,None).0;let _=DestroyMenu(menu);let command=match selected{1=>"expand",2=>"toggleVisibility",3=>"settings",5=>"restart",6=>"toggleNotifications",4=>"exit",_=>""};if !command.is_empty(){a.request(json!({"version":1,"command":command}));}}}else if lparam.0 as u32==WM_LBUTTONDBLCLK{a.request(json!({"version":1,"command":"expand"}));}return LRESULT(0)},
        WM_CLOSE=>{let _=DestroyWindow(hwnd);return LRESULT(0)},WM_DESTROY=>{let mut icon=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:hwnd,uID:1,..Default::default()};let _=Shell_NotifyIconW(NIM_DELETE,&mut icon);if let Some(b)=&a.backend{b.close();}PostQuitMessage(0);return LRESULT(0)},_=>{}
    }}}DefWindowProcW(hwnd,msg,wparam,lparam)
}
fn font_base()->PathBuf{let exe=std::env::current_exe().unwrap();let base=exe.parent().unwrap();if base.join("Assets/Fonts/NotoSansSC.ttf").exists(){return base.into()}Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")}
unsafe fn create(a:&mut App,owner:*const RefCell<App>,hidden:bool)->Result<()> {
    shell::ensure_per_monitor_dpi_awareness();CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;
    a.transparent=windows::UI::ViewManagement::UISettings::new().and_then(|s|s.AdvancedEffectsEnabled()).unwrap_or(false);let instance=GetModuleHandleW(None)?;let class=w!("LyricIsland.Native.DirectComposition.v1");let wc=WNDCLASSW{lpfnWndProc:Some(wndproc),hInstance:instance.into(),lpszClassName:class,hCursor:LoadCursorW(None,IDC_ARROW)?,..Default::default()};RegisterClassW(&wc);
    let visual_class=w!("LyricIsland.Native.Visual.v1");let visual_wc=WNDCLASSW{lpfnWndProc:Some(visual_wndproc),hInstance:instance.into(),lpszClassName:visual_class,..Default::default()};RegisterClassW(&visual_wc);
    let visual=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED|WS_EX_TRANSPARENT,visual_class,w!("Lyric Island · Visual"),WS_POPUP,0,0,388,248,None,None,Some(instance.into()),None)?;a.visual_hwnd=visual;SetLayeredWindowAttributes(visual,COLORREF(0),255,LWA_ALPHA)?;
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP,class,w!("Lyric Island · Native"),WS_POPUP,0,0,388,248,Some(visual),None,Some(instance.into()),Some(owner as _))?;a.hwnd=hwnd;let _=WINDOW.set(hwnd.0 as isize);a.position()?;
    if !hidden{let _=ShowWindow(visual,SW_SHOWNOACTIVATE);let _=ShowWindow(hwnd,SW_SHOWNOACTIVATE);}
    Ok(())
}
unsafe fn tray(a:&App)->Result<()> {
    let path=a.base.join("Assets/LyricIsland-v2.ico").to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();let icon=LoadImageW(None,PCWSTR(path.as_ptr()),IMAGE_ICON,0,0,LR_LOADFROMFILE|LR_DEFAULTSIZE).ok().map(|i|HICON(i.0)).unwrap_or(LoadIconW(None,IDI_APPLICATION)?);
    let mut nid=NOTIFYICONDATAW{cbSize:std::mem::size_of::<NOTIFYICONDATAW>()as u32,hWnd:a.hwnd,uID:1,uFlags:NIF_MESSAGE|NIF_ICON|NIF_TIP,uCallbackMessage:WM_TRAY,hIcon:icon,..Default::default()};let tip="Lyric Island · 歌词灵动岛".encode_utf16().collect::<Vec<_>>();nid.szTip[..tip.len()].copy_from_slice(&tip);let _=Shell_NotifyIconW(NIM_ADD,&nid);Ok(())
}
fn traces(directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;let mut cases=vec![];
    for damping in [10.,30.,2.*(420_f64*0.8).sqrt(),80.]{let mut o=Options::default();o.spring.damping=damping;
        for expanded in [true,false]{let t=Transition::new(expanded,if expanded{Frame::hidden(o)}else{Frame::expanded()},o,Frame::default(),false);
            for ms in (0..=1400).step_by(5){cases.push(json!({"damping":damping,"expanded":expanded,"time":ms,"sample":t.sample(ms as f64),"velocity":t.velocity(ms as f64)}));}
            for ms in [40.,90.,180.,320.]{let rev=Transition::new(!expanded,t.sample(ms),o,t.velocity(ms),false);for after in [0.,1.,20.,80.,180.,400.]{cases.push(json!({"damping":damping,"expanded":!expanded,"reverseAt":ms,"time":after,"sample":rev.sample(after),"velocity":rev.velocity(after)}));}}
        }
    }std::fs::write(directory.join("native-motion.json"),serde_json::to_vec(&cases)?)?;Ok(())
}
unsafe fn preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.renderer=None;a.renderer=Some(Renderer::new(a.visual_hwnd,a.layout.host.w as u32,a.layout.host.h as u32,96.,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?);a.poll();a.opacity=1.;let o=a.options();
    for (name,expanded,count) in [("opening",true,34),("closing",false,29)]{let t=Transition::new(expanded,if expanded{Frame::hidden(o)}else{Frame::expanded()},o,Frame::default(),false);a.transition=Some(t);a.expanded=expanded;
        for i in 0..count{let ms=i as f64*20.;a.frame=t.sample(ms);let mut renderer=a.renderer.take().unwrap();let mut scene=a.scene();scene.time_ms=0.;scene.lyric_age_ms=1000.;scene.cover_opacity=1.;scene.progress=83./236.;let mut activity=motion::Activity::new();activity.retarget(true,0.,true);scene.activity_scales=activity.sample(330.+ms,true).0;scene.activity_opacity=0.9;renderer.render(&scene,false)?;renderer.snapshot(&directory.join(format!("{name}-{:03}ms.png",i*20)))?;a.renderer=Some(renderer);}
    }traces(directory)?;Ok(())
}
unsafe fn notification_preview(a:&mut App,directory:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(directory)?;a.frame=Frame::hidden(a.options());a.opacity=1.;a.lyric="景色は移ろい瞬く間に変わる".into();
    for dpi in [96.,120.,144.,192.]{
        a.renderer=None;let scale=dpi/96.;let mut renderer=Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;
        for amount in [0.,0.5,1.]{let mut s=a.scene();s.notification=None;s.notification_mix=0.;s.layout_scale=scale;s.lyric_age_ms=1000.;s.highlight=amount;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("font-{dpi:.0}-{}.png",(amount*100.)as i32)))?;}
        {let mut s=a.scene();s.notification=None;s.notification_mix=0.;s.frame=Frame::expanded();s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("toolbar-{dpi:.0}.png")))?;}
        let notice=notifications::Notice::example(&a.base);renderer.set_notification_icon(notice.icon.as_deref())?;
        a.banner=banner::Banner::default();let (base,radius)=a.layout.shape(a.frame.card_reveal,a.options().radius);let height=renderer.notification_height(&notice,440_f64.min(a.layout.host.w-48.))?;a.banner.show(notice,base,radius,a.layout.host.w,height,0.,true);
        for ms in [0.,40.,80.,120.,200.,320.,640.]{let (r,rad,mix)=a.banner.presentation(ms/1000.);let mut s=a.scene();s.notification_rect=r;s.notification_radius=rad;s.notification_mix=mix;s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("notification-{dpi:.0}-{ms:03.0}.png")))?;}
        a.banner.dismiss(base,radius,0.64,true);
        for ms in [640.,680.,720.,800.,960.,1280.]{let (r,rad,mix)=a.banner.presentation(ms/1000.);let mut s=a.scene();s.notification_rect=r;s.notification_radius=rad;s.notification_mix=mix;s.layout_scale=scale;s.lyric_age_ms=1000.;renderer.render(&s,false)?;renderer.snapshot(&directory.join(format!("dismiss-{dpi:.0}-{ms:04.0}.png")))?;}
        for (name,body) in [("short","这是一条简短消息。"),("empty",""),("long","这是一条用于检查多行通知排版的长消息。收到新消息后，歌词岛会根据正文内容调整横幅高度，保留清晰的文字间距，并在显示结束后平滑返回音乐界面。")]{
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

    // The identity-bearing twin uses exactly this renderer and backend. Keep
    // the portable entry point runnable while optional identity is absent.
    let executable=std::env::current_exe()?;let directory=executable.parent().unwrap();
    let startup_data=args.iter().position(|s|s=="--data-dir").and_then(|i|args.get(i+1)).map(PathBuf::from).unwrap_or_else(||directory.join("Data"));
    let compatibility=io::read_settings(&startup_data.join("settings.json"))["NotificationCompatibility"].as_bool().unwrap_or(true);
    if !compatibility&&executable.file_name().is_some_and(|name|name.to_string_lossy().eq_ignore_ascii_case("LyricIsland.exe"))&&directory.join("notification-support/identity-enabled").is_file(){
        if std::process::Command::new(directory.join("LyricIsland.NotificationHost.exe")).args(args.iter().skip(1)).spawn().is_ok(){return Ok(())}
    }
    if args.get(1).is_some_and(|s|s=="--notification-info"){let report=std::thread::spawn(||{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);notifications::diagnostics()}).join().map_err(|_|"notification diagnostic failed")?;std::fs::write(args.get(2).ok_or("missing report path")?,serde_json::to_vec_pretty(&report)?)?;return Ok(())}if args.get(1).is_some_and(|s|s=="--motion-trace"){return traces(Path::new(args.get(2).ok_or("missing directory")?))}
    if args.get(1).is_some_and(|s|s=="--media-info"){let data=std::thread::spawn(||lyric_backend::media::control_diagnostics().map_err(|e|e.to_string())).join().map_err(|_|"media diagnostics worker failed")??;std::fs::write(args.get(2).ok_or("missing output path")?,serde_json::to_vec_pretty(&data)?)?;return Ok(())}
    let icon_mode=args.get(1).is_some_and(|s|s=="--icon-preview");let font_mode=args.get(1).is_some_and(|s|s=="--font-check");let notice_mode=args.get(1).is_some_and(|s|s=="--notification-preview");let cover_mode=args.get(1).is_some_and(|s|s=="--cover-preview");let preview_mode=cover_mode||icon_mode||font_mode||notice_mode||args.get(1).is_some_and(|s|s=="--animation-preview");let demo=preview_mode||args.iter().any(|s|s=="--demo");let base=font_base();let data=if let Some(i)=args.iter().position(|s|s=="--data-dir"){PathBuf::from(args.get(i+1).ok_or("missing data directory")?)}else{base.join("Data")};
    let owner=Box::new(RefCell::new(App::new(base,data,demo)));let owner_ptr=&*owner as *const RefCell<App>;let mut app=owner.borrow_mut();create(&mut app,owner_ptr,preview_mode)?;
    if cover_mode{
        let dir=Path::new(args.get(2).ok_or("missing directory")?);std::fs::create_dir_all(dir)?;
        app.poll();app.opacity=1.;app.frame=Frame::expanded();let mut r=app.renderer.take().unwrap();
        for name in ["red","blue","white"]{
            r.set_cover(Some(&std::fs::read(dir.join(format!("{name}.png")))?))?;
            for enabled in [false,true]{let mut scene=app.scene();scene.cover_background=enabled;scene.lyric_age_ms=1000.;r.render(&scene,false)?;r.snapshot(&dir.join(format!("{name}-{enabled}.png")))?;}
        }
        let _=DestroyWindow(app.hwnd);return Ok(())
    }
    if icon_mode{icon_preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if font_mode{let report=app.renderer.as_mut().unwrap().verify_font()?;std::fs::write(args.get(2).ok_or("missing font report path")?,serde_json::to_vec_pretty(&report)?)?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if notice_mode{notification_preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    if preview_mode{preview(&mut app,Path::new(args.get(2).ok_or("missing directory")?))?;let _=DestroyWindow(app.hwnd);return Ok(())}
    let test_pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_default();
    let mutex_name=if test_pipe.starts_with("LyricIsland.Settings.test."){format!("Local.LyricIsland.Native.Test.{test_pipe}")}else{"Local.LyricIsland.Native.Direct2D.v1".into()};let mutex_name=mutex_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mutex=CreateMutexW(None,true,PCWSTR(mutex_name.as_ptr()))?;if GetLastError()==ERROR_ALREADY_EXISTS{let _=CloseHandle(mutex);return Ok(())}
    tray(&app)?;let pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into());app._server=Some(Server::new(&pipe,settings_callback));
    if args.iter().any(|s|s=="--notification-onboarding"){app.show_settings();app.request(json!({"version":1,"command":"notificationAccess"}));}
    if args.iter().any(|s|s=="--expanded"){app.held=true;app.retarget(true)}
    let until=args.iter().position(|s|s=="--run-ms").and_then(|i|args.get(i+1)).and_then(|s|s.parse::<u64>().ok()).map(|ms|Instant::now()+Duration::from_millis(ms));
    drop(app);let mut msg=MSG::default();'events:loop{
        while PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool(){if msg.message==WM_QUIT{break 'events}let _=TranslateMessage(&msg);DispatchMessageW(&msg);}
        let mut app=owner.borrow_mut();if until.is_some_and(|t|Instant::now()>=t){let _=DestroyWindow(app.hwnd);break}
        if let Err(e)=app.tick(){app.error=Some(e.to_string());io::log(&app.data.join("Logs"),"NATIVE_RENDER",&e.to_string());if let Err(recovery)=app.position(){return Err(recovery.into())}}
        // Present(1) already synchronizes rendering to the display. Waiting for
        // DwmFlush here as well would introduce a second refresh interval.
        if !(app.banner.visible()||app.opacity>0.001&&app.snapshot["isPlaying"]==true&&app.flag("Animation",true)||app.transition.is_some()||(app.opacity-app.fade_to).abs()>0.00001||app.lyric_start.elapsed()<Duration::from_millis(180)){let _=MsgWaitForMultipleObjectsEx(None,16,QS_ALLINPUT,MWMO_INPUTAVAILABLE);}
    }
    let app=owner.borrow();if let Some(b)=&app.backend{b.close();}if let Some(server)=&app._server{server.close();}let _=CloseHandle(mutex);CoUninitialize();Ok(())
}}
fn main(){if let Err(e)=run(){let base=std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)).unwrap_or_default();let _=std::fs::write(base.join("native-error.txt"),e.to_string());std::process::exit(1)}}

#[cfg(test)]mod interaction_tests{
    use super::*;
    fn app()->App{App::new(PathBuf::new(),PathBuf::new(),true)}
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
        let c=a.layout.card;assert_eq!(a.hit(c.x+c.w/2.,c.y+164.),4);
        a.activate(4);assert!(a.expanded);assert_eq!(a.snapshot["isPlaying"],false);
    }}
    #[test]fn body_click_option_is_boolean_and_preserves_other_settings(){
        let original=config::defaults();assert_eq!(original["PauseNotifications"],false);assert!(original.get("FollowWindowsDnd").is_none());assert_eq!(original["ClickAnywhereExpand"],true);assert_eq!(original["CoverBackground"],false);assert_eq!(original["OnlineSongInfo"],true);
        let updated=config::validate_setting(json!({"key":"ClickAnywhereExpand","value":true,"settings":original})).unwrap();
        assert_eq!(updated["ClickAnywhereExpand"],true);assert_eq!(updated["Motion"],original["Motion"]);
        assert!(config::validate_setting(json!({"key":"ClickAnywhereExpand","value":"true","settings":original})).is_err());
    }
}












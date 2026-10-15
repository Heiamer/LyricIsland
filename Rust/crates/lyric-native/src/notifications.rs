//! Windows supplies notification data only. All presentation remains Direct2D.
use std::{collections::{HashSet,HashMap},ffi::c_void,sync::{Arc,Mutex,mpsc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use serde_json::{Value,json};
use windows::{core::{Result,HSTRING,PCSTR},UI::Notifications::{Management::*,NotificationKinds},Win32::System::{LibraryLoader::{GetModuleHandleW,GetProcAddress},WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED}}};

#[derive(Clone,Debug,Default)]
pub struct Notice {pub app_id:String,pub app_name:String,pub title:String,pub body:String,pub icon:Option<Vec<u8>>}
impl Notice {
    pub fn example(base:&std::path::Path)->Self {Self{app_id:"LyricIsland".into(),app_name:"歌词岛".into(),title:"通知预览".into(),body:"这是一条通知预览。音乐继续，重要消息也不会错过。".into(),icon:std::fs::read(base.join("Assets/LyricIsland-v2.png")).ok()}}
}
#[derive(Clone,Default)]
pub struct Policy {pub enabled:bool,pub follow_dnd:bool,pub allowed:Vec<String>}
impl Policy {
    pub fn from_settings(v:&Value)->Self {Self{enabled:v["NotificationsEnabled"].as_bool().unwrap_or(false),follow_dnd:v["FollowWindowsDnd"].as_bool().unwrap_or(true),allowed:v["NotificationAllowedApps"].as_array().map(|a|a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_else(||vec!["QQNT".into(),"QQ".into()])}}
    pub fn permits(&self,id:&str,name:&str)->bool {
        self.allowed.iter().any(|rule| {
            if rule.eq_ignore_ascii_case(id) {return true}
            let id=id.to_lowercase();let name=name.trim().to_lowercase();let r=rule.trim().to_lowercase();
            let tokens=id.split(['.','!','/','\\',':','_']).collect::<Vec<_>>();
            let nt=name=="qqnt"||name=="qq nt"||tokens.contains(&"qqnt")||(tokens.contains(&"qq")&&tokens.contains(&"nt"));
            if r=="qqnt" {return nt}
            if r=="qq" {return !nt&&(["qq","腾讯qq","腾讯 qq"].contains(&name.as_str())||tokens.contains(&"qq"))}
            false // Custom applications are matched by exact AUMID, never substring.
        })
    }
    pub fn should_show(&self,dnd:Option<bool>)->bool{self.enabled&&(!self.follow_dnd||dnd==Some(false))}
}
pub enum Event { Notice(Notice), State(Value) }
pub struct Listener {pub events:mpsc::Receiver<Event>,policy:Arc<Mutex<Policy>>,stop:Arc<AtomicBool>}
impl Drop for Listener {fn drop(&mut self){self.stop.store(true,Ordering::Release);}}
impl Listener {
    pub fn new(policy:Policy)->Self {
        let policy=Arc::new(Mutex::new(policy));let stop=Arc::new(AtomicBool::new(false));let (tx,events)=mpsc::channel();
        let shared=policy.clone();let stopping=stop.clone();
        std::thread::spawn(move||{
            let initialized=unsafe{RoInitialize(RO_INIT_MULTITHREADED)}.is_ok();
            let mut seen=HashSet::new();let mut icon_cache=HashMap::new();let mut seeded=false;let mut last_state=Value::Null;let mut last_enabled=false;
            let changed=Arc::new(AtomicBool::new(true));let mut subscription:Option<(UserNotificationListener,i64)>=None;let mut apps_cache=json!([]);
            while !stopping.load(Ordering::Acquire) {
                let p=shared.lock().map(|p|p.clone()).unwrap_or_default();
                if p.enabled!=last_enabled {seeded=false;seen.clear();last_enabled=p.enabled;if !p.enabled{if let Some((listener,token))=subscription.take(){let _=listener.RemoveNotificationChanged(token);}}}
                let dnd=dnd_active();
                let mut state=json!({"enabled":p.enabled,"dnd":dnd,"followDnd":p.follow_dnd,"access":"disabled","message":"通知提醒已关闭","apps":[]});
                if p.enabled {
                    match UserNotificationListener::Current() {
                        Err(e)=> {state["access"]=json!("unavailable");state["message"]=json!(format!("系统通知访问不可用（{}）。请使用带通知能力的安装版，并在此授权。",e.code()));seeded=false;},
                        Ok(listener)=>match listener.GetAccessStatus() {
                            Ok(s) if s==UserNotificationListenerAccessStatus::Allowed => {
                                if subscription.is_none(){let changed=changed.clone();let handler=windows::Foundation::TypedEventHandler::<UserNotificationListener,windows::UI::Notifications::UserNotificationChangedEventArgs>::new(move|_,_|{changed.store(true,Ordering::Release);Ok(())});if let Ok(token)=listener.NotificationChanged(&handler){subscription=Some((listener.clone(),token));}}
                                state["access"]=json!("allowed");
                                state["message"]=json!(if p.follow_dnd&&dnd==Some(true){"Windows 勿扰已开启，通知横幅暂停"}else if p.follow_dnd&&dnd.is_none(){"无法读取系统勿扰状态；跟随已开启，暂不弹出横幅"}else{"系统通知已连接，仅提醒白名单应用"});
                                let dirty=changed.swap(false,Ordering::AcqRel);
                                if !seeded||subscription.is_none()||dirty{
                                    match read_notifications(&listener,&p,&mut seen,&mut icon_cache,seeded,p.follow_dnd&&dnd!=Some(false)) {
                                        Ok((apps,notices))=>{apps_cache=apps;for notice in notices{let _=tx.send(Event::Notice(notice));}seeded=true;},
                                        Err(e)=>{state["message"]=json!(format!("读取系统通知失败（{}），稍后重试",e.code()));seeded=false;}
                                    }
                                }
                                state["apps"]=apps_cache.clone();
                            },
                            Ok(s)=>{state["access"]=json!(if s==UserNotificationListenerAccessStatus::Denied{"denied"}else{"unspecified"});state["message"]=json!("尚未授予通知访问权限，请点击“授权系统通知”");seeded=false;},
                            Err(e)=>{state["access"]=json!("unavailable");state["message"]=json!(format!("通知访问失败（{}），请使用通知安装版",e.code()));seeded=false;}
                        }
                    }
                }
                if state["access"]!="allowed"{if let Some((listener,token))=subscription.take(){let _=listener.RemoveNotificationChanged(token);}apps_cache=json!([]);}
                if state!=last_state {let _=tx.send(Event::State(state.clone()));last_state=state;}
                for _ in 0..8 {if stopping.load(Ordering::Acquire)||(p.enabled&&seeded&&subscription.is_some()&&changed.load(Ordering::Acquire)){break}std::thread::sleep(Duration::from_millis(50));}
            }
            if let Some((listener,token))=subscription{let _=listener.RemoveNotificationChanged(token);}
            if initialized {unsafe{RoUninitialize()}}
        });
        Self{events,policy,stop}
    }
    pub fn configure(&self,p:Policy){if let Ok(mut old)=self.policy.lock(){*old=p;}}
}

// Explicit bounded waits run only on this MTA worker, never the rendering thread.
macro_rules! wait {($op:expr)=>{{let op=$op?;let limit=Instant::now()+Duration::from_secs(2);while op.Status()?.0==0{if Instant::now()>limit{let _=op.Cancel();return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x800705B4u32 as i32)))}std::thread::sleep(Duration::from_millis(5));}op.GetResults()?}};}
fn read_notifications(listener:&UserNotificationListener,p:&Policy,seen:&mut HashSet<(String,u32,i64)>,icon_cache:&mut HashMap<String,Vec<u8>>,seeded:bool,suppressed:bool)->Result<(Value,Vec<Notice>)> {
    let items=wait!(listener.GetNotificationsAsync(NotificationKinds::Toast));let mut apps=vec![];let mut current=HashSet::new();let mut notices=vec![];
    for i in 0..items.Size()? {
        let Ok(n)=items.GetAt(i) else{continue};let Ok(app)=n.AppInfo() else{continue};let Ok(id)=app.AppUserModelId() else{continue};let id=id.to_string();
        let name=app.DisplayInfo().and_then(|d|d.DisplayName()).map(|n|n.to_string()).unwrap_or_else(|_|id.clone());
        if !apps.iter().any(|a:&Value|a["id"]==id) {apps.push(json!({"id":id,"name":name}));}
        let key=(id.clone(),n.Id()?,n.CreationTime()?.UniversalTime);current.insert(key.clone());
        if !seeded||suppressed||seen.contains(&key)||!p.permits(&id,&name)||notices.len()>=8 {continue}
        let content=(||->Result<Notice>{
            let visual=n.Notification()?.Visual()?;
            let binding=visual.GetBinding(&HSTRING::from("ToastGeneric")).or_else(|_|visual.Bindings()?.GetAt(0))?;
            let elements=binding.GetTextElements()?;let mut text=vec![];
            for j in 0..elements.Size()?.min(8){let s=elements.GetAt(j)?.Text()?.to_string();if !s.trim().is_empty(){text.push(s.chars().take(1024).collect::<String>());}}
            let title=text.first().cloned().unwrap_or_else(||name.clone());let body=text.into_iter().skip(1).collect::<Vec<_>>().join("\n");
            let icon=icon_cache.get(&id).cloned().or_else(||crate::app_icon::shell_icon(&id)).or_else(||(||->Result<Vec<u8>>{
                use windows::{Foundation::Size,Storage::Streams::{DataReader,InputStreamOptions}};
                let stream=wait!(app.DisplayInfo()?.GetLogo(Size{Width:48.,Height:48.})?.OpenReadAsync());
                let length=stream.Size()?;if length==0||length>8*1024*1024{return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32)))}let length=length as u32;let reader=DataReader::CreateDataReader(&stream)?;reader.SetInputStreamOptions(InputStreamOptions::None)?;
                let count=wait!(reader.LoadAsync(length));let mut bytes=vec![0;count as usize];reader.ReadBytes(&mut bytes)?;Ok(bytes)
            })().ok().filter(|bytes|image::load_from_memory(bytes).is_ok()));
            if let Some(bytes)=&icon{if icon_cache.len()>=64{icon_cache.clear();}icon_cache.insert(id.clone(),bytes.clone());}
            Ok(Notice{app_id:id.clone(),app_name:name.clone(),title,body,icon})
        })();
        if let Ok(notice)=content{notices.push(notice);}
    }
    // Retain at most currently present identities. Existing messages are never
    // replayed after enabling, revocation or while leaving Do Not Disturb.
    *seen=current;notices.truncate(8);Ok((json!(apps),notices))
}

/// Must be called by an explicit user action on the native UI thread.
pub fn request_access()->Result<()> {
    let op=UserNotificationListener::Current()?.RequestAccessAsync()?;
    std::thread::spawn(move||{let _=op.join();});Ok(())
}

/// Read-only compatibility adapter for the Windows shell's active quiet-hours
/// profile (not an official public contract). Unknown stays unknown; do not
/// equate SHQueryUserNotificationState/FocusSession with manual DND.
pub fn dnd_active()->Option<bool>{unsafe{
    type Query=unsafe extern "system" fn(*const u64,*const c_void,*const c_void,*mut u32,*mut u32,*mut u32)->i32;
    let module=GetModuleHandleW(windows::core::w!("ntdll.dll")).ok()?;
    let address=GetProcAddress(module,PCSTR(c"NtQueryWnfStateData".as_ptr().cast()))?;
    let query:Query=std::mem::transmute(address);let name=0x0D83063EA3BF1C75_u64;let mut stamp=0;let mut data=0;let mut size=4;
    if query(&name,std::ptr::null(),std::ptr::null(),&mut stamp,&mut data,&mut size)<0||size!=4{return None}
    match data{0=>Some(false),1|2=>Some(true),_=>None}
}}
pub fn diagnostics()->Value {let access=UserNotificationListener::Current().and_then(|l|l.GetAccessStatus());json!({"readOnly":true,"dnd":dnd_active(),"access":access.map(|a|a.0).map_err(|e|e.code().to_string()),"rendering":"Direct2D/DirectWrite/DirectComposition"})}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn qq_rules_do_not_admit_qq_music_or_lookalikes(){
        let p=Policy{allowed:vec!["QQ".into(),"QQNT".into()],..Default::default()};
        assert!(p.permits("Tencent.QQ.NT","QQ"));assert!(p.permits("QQNT","QQNT"));assert!(p.permits("C:\\QQ\\QQ.exe","QQ"));
        assert!(!p.permits("Tencent.QQMusic","QQ音乐"));assert!(!p.permits("FakeQQ","NotQQ"));
    }
    #[test] fn custom_apps_use_exact_identifiers(){let p=Policy{allowed:vec!["Example.App_123!Main".into()],..Default::default()};assert!(p.permits("Example.App_123!Main","Example"));assert!(!p.permits("Example.App_123!Other","Example"));}
    #[test] fn follow_dnd_is_independent_and_unknown_does_not_interrupt(){let mut p=Policy{enabled:true,follow_dnd:true,..Default::default()};assert!(p.should_show(Some(false)));assert!(!p.should_show(Some(true)));assert!(!p.should_show(None));p.follow_dnd=false;assert!(p.should_show(Some(true)));assert!(p.should_show(None));p.enabled=false;assert!(!p.should_show(Some(false)));}
    #[test] fn disabling_qqnt_does_not_leave_it_allowed_by_legacy_rule(){let p=Policy{allowed:vec!["QQ".into()],..Default::default()};assert!(!p.permits("Tencent.QQ.NT","QQ"));assert!(p.permits("Tencent.QQ","QQ"));}
}

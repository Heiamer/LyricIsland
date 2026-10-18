//! System state hints: Caps/Num read the lock bit after a real key event, microphone mute is
//! pushed by Core Audio, touchpad comes from the Precision Touchpad registry value.
//! Nothing here polls at frame rate or reads keyboard content.
use std::sync::atomic::{AtomicIsize,AtomicU32,Ordering};
use std::time::{Duration,Instant};
use serde_json::{json,Value};
use windows::core::{implement,w};
use windows::Win32::{
    Foundation::{HWND,LPARAM,LRESULT,WPARAM},
    Media::Audio::{eCapture,eConsole,AUDIO_VOLUME_NOTIFICATION_DATA,Endpoints::{IAudioEndpointVolume,IAudioEndpointVolumeCallback,IAudioEndpointVolumeCallback_Impl},IMMDeviceEnumerator,MMDeviceEnumerator},
    System::{Com::{CoCreateInstance,CLSCTX_ALL},Registry::{RegGetValueW,RRF_RT_REG_DWORD,HKEY_CURRENT_USER}},
    UI::{Input::KeyboardAndMouse::{GetKeyState,VK_CAPITAL,VK_NUMLOCK},WindowsAndMessaging::{CallNextHookEx,KBDLLHOOKSTRUCT,PostMessageW,SetWindowsHookExW,UnhookWindowsHookEx,WH_KEYBOARD_LL,HHOOK}},
};
pub const WM_OSD:u32=windows::Win32::UI::WindowsAndMessaging::WM_APP+3;
pub const OSD_TIMER:usize=2;
static TARGET:AtomicIsize=AtomicIsize::new(0);
static DOWN:AtomicU32=AtomicU32::new(0);

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Kind{Caps,Num,Mic,Touchpad}
impl Kind{
    pub fn key(self)->&'static str{match self{Self::Caps=>"Caps",Self::Num=>"Num",Self::Mic=>"Mic",Self::Touchpad=>"Touchpad"}}
    pub fn label(self)->&'static str{match self{Self::Caps=>"大写锁定",Self::Num=>"小键盘锁定",Self::Mic=>"麦克风",Self::Touchpad=>"触控板"}}
    pub fn state(self,on:bool)->&'static str{match self{Self::Mic=>if on{"已静音"}else{"已取消静音"},_=>if on{"已开启"}else{"已关闭"}}}
    pub fn bit(self)->u32{match self{Self::Caps=>1,Self::Num=>2,Self::Mic=>4,Self::Touchpad=>8}}
}
#[derive(Clone,Copy,Debug)]
pub struct Hint{pub kind:Kind,pub on:bool}
impl Hint{
    pub fn notice(self)->crate::notifications::Notice{crate::notifications::Notice{
        app_id:format!("LyricIsland.System.{}",self.kind.key()),app_name:"系统状态".into(),
        title:self.kind.label().into(),body:self.kind.state(self.on).into(),icon:None}}
}
#[derive(Clone,Copy,Default)]struct Probe{last:Option<bool>,supported:Option<bool>}
pub struct Hub{
    hook:Option<HHOOK>,caps:Option<bool>,num:Option<bool>,mic:Probe,touch:Probe,
    mic_id:Option<String>,audio:Option<(IAudioEndpointVolume,IAudioEndpointVolumeCallback)>,
    mic_active:bool,touch_active:bool,last_poll:Option<Instant>,
}
impl Default for Hub{fn default()->Self{Self{hook:None,caps:None,num:None,mic:Probe::default(),touch:Probe::default(),mic_id:None,audio:None,mic_active:false,touch_active:false,last_poll:None}}}
impl Drop for Hub{fn drop(&mut self){self.uninstall();}}
impl Hub{
    /// The hook only reports that Caps or Num was pressed; the lock bit is read back 45 ms later
    /// by the message pump, so auto-repeat and release timing can never fabricate a state.
    pub unsafe fn install(&mut self,hwnd:HWND){
        if self.hook.is_some()||hwnd.0.is_null(){return}
        TARGET.store(hwnd.0 as isize,Ordering::Relaxed);
        match SetWindowsHookExW(WH_KEYBOARD_LL,Some(keyboard_proc),None,0){
            Ok(hook)=>{self.hook=Some(hook);DOWN.store(0,Ordering::Relaxed);self.caps=Some(key(VK_CAPITAL.0 as i32));self.num=Some(key(VK_NUMLOCK.0 as i32));self.last_poll=Some(Instant::now());self.rebind();}
            Err(_)=>TARGET.store(0,Ordering::Relaxed),
        }
    }
    pub fn uninstall(&mut self){
        if let Some(hook)=self.hook.take(){unsafe{let _=UnhookWindowsHookEx(hook);}}
        TARGET.store(0,Ordering::Relaxed);DOWN.store(0,Ordering::Relaxed);self.caps=None;self.num=None;
        self.mic.last=None;self.mic_id=None;self.unbind();
    }
    /// Focus/session changes can hide transitions (secure desktop, lock), so the lock baseline
    /// is rebuilt without emitting a hint.
    pub fn resync(&mut self){
        self.caps=Some(unsafe{key(VK_CAPITAL.0 as i32)});self.num=Some(unsafe{key(VK_NUMLOCK.0 as i32)});
        DOWN.store(0,Ordering::Relaxed);
        if self.mic.last.is_some(){let state=unsafe{mic_state()};if state.is_some(){self.update_mic(state,false);}}
    }
    fn rebind(&mut self){self.unbind();if let Some(pair)=unsafe{bind_mic()}{self.audio=Some(pair);}}
    fn unbind(&mut self){if let Some((endpoint,callback))=self.audio.take(){unsafe{let _=endpoint.UnregisterControlChangeNotify(&callback);}}}
    /// Reads the lock bits for the keys that actually changed. The caller passes the pending
    /// mask collected from `WM_OSD`, so an unrelated key never triggers a read.
    pub fn flush(&mut self,pending:u32,caps_on:bool,num_on:bool,mic_on:bool)->Vec<Hint>{
        let mut out=vec![];
        if pending&Kind::Caps.bit()!=0{let on=unsafe{key(VK_CAPITAL.0 as i32)};if self.caps!=Some(on){self.caps=Some(on);if caps_on{out.push(Hint{kind:Kind::Caps,on});}}}
        if pending&Kind::Num.bit()!=0{let on=unsafe{key(VK_NUMLOCK.0 as i32)};if self.num!=Some(on){self.num=Some(on);if num_on{out.push(Hint{kind:Kind::Num,on});}}}
        if pending&Kind::Mic.bit()!=0{
            let state=unsafe{mic_state()};
            // The endpoint callback survives a default-device switch only until the next read;
            // rebinding here keeps later mute changes instant too.
            if state.as_ref().is_some_and(|(id,_)|self.mic_id.as_deref().is_some_and(|prior|prior!=id))||self.audio.is_none(){self.rebind();}
            if let Some(hint)=self.update_mic(state,mic_on){out.push(hint);}
        }
        out
    }
    fn update_mic(&mut self,state:Option<(String,bool)>,hint:bool)->Option<Hint>{
        let Some((id,on))=state else{self.mic.supported=Some(false);self.mic.last=None;self.mic_id=None;return None};
        self.mic.supported=Some(true);
        let switched=self.mic_id.as_deref().is_some_and(|prior|prior!=id);
        self.mic_id=Some(id);
        let previous=self.mic.last.replace(on);
        if hint&&!switched{if let Some(previous)=previous{if previous!=on{return Some(Hint{kind:Kind::Mic,on})}}}
        None
    }
    /// Microphone and touchpad are sampled every two seconds as a safety net; the mute callback
    /// normally delivers changes sooner. The first successful read only establishes a baseline,
    /// so enabling a switch or switching the default microphone is silent.
    pub fn poll(&mut self,mic_on:bool,touch_on:bool)->Vec<Hint>{
        if !mic_on{self.mic.last=None;}
        if !touch_on{self.touch.last=None;}
        let changed=(mic_on&&!self.mic_active)||(touch_on&&!self.touch_active);
        self.mic_active=mic_on;self.touch_active=touch_on;
        if !mic_on&&!touch_on{self.last_poll=None;return vec![]}
        if changed||self.last_poll.is_none(){self.last_poll=Some(Instant::now()-Duration::from_secs(2));}
        if self.last_poll.is_some_and(|t|t.elapsed()<Duration::from_secs(2)){return vec![]}
        self.last_poll=Some(Instant::now());let mut out=vec![];
        if mic_on{let state=unsafe{mic_state()};if let Some(hint)=self.update_mic(state,true){out.push(hint);}}
        if touch_on{
            let value=unsafe{touch_enabled()};self.touch.supported=Some(value.is_some());
            match value{
                Some(on)=>match self.touch.last{None=>self.touch.last=Some(on),Some(prev)if prev!=on=>{self.touch.last=Some(on);out.push(Hint{kind:Kind::Touchpad,on});},_=>{}},
                None=>self.touch.last=None,
            }
        }
        out
    }
    pub fn status(&self)->Value{json!({
        "caps":self.caps,"num":self.num,"mic":self.mic.last,"touch":self.touch.last,
        "micSupported":self.mic.supported,"touchSupported":self.touch.supported,
        "hooked":self.hook.is_some()
    })}
}
fn post(bit:u32){let target=TARGET.load(Ordering::Relaxed);if target!=0{unsafe{let _=PostMessageW(Some(HWND(target as *mut _)),WM_OSD,WPARAM(bit as usize),LPARAM(0));}}}
#[implement(IAudioEndpointVolumeCallback)]
struct MuteCallback;
impl IAudioEndpointVolumeCallback_Impl for MuteCallback_Impl{
    fn OnNotify(&self,_: *mut AUDIO_VOLUME_NOTIFICATION_DATA)->windows::core::Result<()>{post(Kind::Mic.bit());Ok(())}
}
unsafe extern "system" fn keyboard_proc(code:i32,wparam:WPARAM,lparam:LPARAM)->LRESULT{
    if code>=0{
        let key=&*(lparam.0 as *const KBDLLHOOKSTRUCT);let bit=if key.vkCode==VK_CAPITAL.0 as u32{Kind::Caps.bit()}else if key.vkCode==VK_NUMLOCK.0 as u32{Kind::Num.bit()}else{0};
        if bit!=0{
            let up=matches!(wparam.0 as u32,0x0101|0x0105);
            if up{DOWN.fetch_and(!bit,Ordering::Relaxed);}
            // Holding the key auto-repeats key-down without a release in between; only the
            // first edge is a real toggle.
            else if DOWN.fetch_or(bit,Ordering::Relaxed)&bit==0{post(bit);}
        }
    }
    CallNextHookEx(None,code,wparam,lparam)
}
unsafe fn key(vk:i32)->bool{GetKeyState(vk)&1!=0}
unsafe fn mic_state()->Option<(String,bool)>{
    let enumerator:IMMDeviceEnumerator=CoCreateInstance(&MMDeviceEnumerator,None,CLSCTX_ALL).ok()?;
    let device=enumerator.GetDefaultAudioEndpoint(eCapture,eConsole).ok()?;
    let id=device.GetId().ok()?.to_string().ok()?;
    let volume:IAudioEndpointVolume=device.Activate(CLSCTX_ALL,None).ok()?;
    Some((id,volume.GetMute().ok()?.as_bool()))
}
unsafe fn bind_mic()->Option<(IAudioEndpointVolume,IAudioEndpointVolumeCallback)>{
    let enumerator:IMMDeviceEnumerator=CoCreateInstance(&MMDeviceEnumerator,None,CLSCTX_ALL).ok()?;
    let device=enumerator.GetDefaultAudioEndpoint(eCapture,eConsole).ok()?;
    let endpoint:IAudioEndpointVolume=device.Activate(CLSCTX_ALL,None).ok()?;
    let callback:IAudioEndpointVolumeCallback=MuteCallback.into();
    endpoint.RegisterControlChangeNotify(&callback).ok()?;
    Some((endpoint,callback))
}
unsafe fn touch_enabled()->Option<bool>{
    let mut value=0u32;let mut size=std::mem::size_of::<u32>()as u32;
    let status=RegGetValueW(HKEY_CURRENT_USER,w!("Software\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad\\Status"),w!("Enabled"),RRF_RT_REG_DWORD,None,Some((&mut value as *mut u32).cast()),Some(&mut size));
    status.is_ok().then_some(value!=0)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn hint_copy_matches_the_state_and_never_claims_recording(){let muted=Hint{kind:Kind::Mic,on:true}.notice();assert_eq!(muted.title,"麦克风");assert_eq!(muted.body,"已静音");assert_eq!(muted.app_id,"LyricIsland.System.Mic");assert!(!muted.body.contains("录音"));let caps=Hint{kind:Kind::Caps,on:false}.notice();assert_eq!(caps.body,"已关闭");assert_eq!(caps.app_id,"LyricIsland.System.Caps");}
    #[test]fn key_mask_only_reads_changed_keys(){let mut hub=Hub::default();hub.caps=Some(false);hub.num=Some(true);assert!(hub.flush(0,true,true,true).is_empty());assert!(hub.poll(false,false).is_empty());}
    #[test]fn disabled_children_reset_their_baseline(){let mut hub=Hub::default();hub.mic=Probe{last:Some(true),supported:Some(true)};hub.touch=Probe{last:Some(false),supported:Some(true)};hub.poll(false,false);assert_eq!(hub.mic.last,None);assert_eq!(hub.touch.last,None);assert_eq!(hub.mic.supported,Some(true),"capability survives a switch cycle");}
    #[test]fn first_read_and_device_switch_are_silent(){let mut hub=Hub::default();assert!(hub.update_mic(Some(("a".into(),true)),true).is_none());assert_eq!(hub.update_mic(Some(("a".into(),false)),true).map(|hint|hint.on),Some(false));assert!(hub.update_mic(Some(("b".into(),true)),true).is_none(),"a default device change is not a mute change");assert!(hub.update_mic(None,true).is_none());assert_eq!(hub.mic.last,None);assert_eq!(hub.mic.supported,Some(false));}
}

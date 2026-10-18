//! Event-driven device notices: removable drives and phones through WM_DEVICECHANGE, audio
//! endpoints through Core Audio callbacks, gamepads and mice/keyboards through raw-input device
//! notifications. No polling, drivers or elevated permissions.
use std::{collections::HashMap,ffi::c_void,sync::mpsc::{self,Receiver,Sender},time::{Duration,Instant}};
use windows::{core::{implement,GUID,HSTRING,PCWSTR},Win32::{
    Devices::FunctionDiscovery::{PKEY_Device_ContainerId,PKEY_Device_FriendlyName},
    Foundation::{HANDLE,HWND},
    Media::Audio::{IMMDevice,IMMDeviceEnumerator,IMMNotificationClient,IMMNotificationClient_Impl,DEVICE_STATE,DEVICE_STATE_ACTIVE,EDataFlow,ERole,MMDeviceEnumerator,PKEY_AudioEndpoint_FormFactor,eRender},
    System::{Com::{CoCreateInstance,CLSCTX_ALL,STGM_READ,StructuredStorage::{PROPVARIANT,PropVariantClear}},Variant::{VT_CLSID,VT_I4,VT_LPWSTR,VT_UI4}},
    UI::{Input::{GetRawInputDeviceInfoW,RIDEV_DEVNOTIFY,RIDI_DEVICEINFO,RID_DEVICE_INFO,RIM_TYPEHID,RIM_TYPEKEYBOARD,RIM_TYPEMOUSE,RAWINPUTDEVICE,RegisterRawInputDevices},WindowsAndMessaging::{GIDC_ARRIVAL,GIDC_REMOVAL}},
}};
#[link(name="user32")]unsafe extern "system"{fn RegisterDeviceNotificationW(recipient:*mut c_void,filter:*const c_void,flags:u32)->*mut c_void;fn UnregisterDeviceNotification(handle:*mut c_void)->i32;}
#[link(name="kernel32")]unsafe extern "system"{fn GetDriveTypeW(root:*const u16)->u32;}
const USB:GUID=GUID::from_u128(0xa5dcbf10_6530_11d2_901f_00c04fb951ed);const WPD:GUID=GUID::from_u128(0x6ac27878_a6fa_4155_ba85_f98f491d4f33);
const AUDIO_RENDER:GUID=GUID::from_u128(0xe6327cad_dcec_4949_ae8a_991e976a79d2);
#[repr(C)]struct Filter{size:u32,kind:u32,reserved:u32,class:GUID,name:[u16;1]}
#[repr(C)]struct Header{size:u32,kind:u32,reserved:u32}
#[repr(C)]struct Volume{header:Header,mask:u32,flags:u16}

/// One alert family. A typed value instead of magic numbers keeps the setting map, the icon map
/// and the filter in one place.
#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum Kind{Usb,Volume,Phone,Headphones,Speaker,Gamepad,Mouse,Keyboard,Audio}
impl Kind{
    pub fn key(self)->&'static str{match self{Self::Usb=>"Usb",Self::Volume=>"Volume",Self::Phone=>"Phone",Self::Headphones=>"Headphones",Self::Speaker=>"Speaker",Self::Gamepad=>"Gamepad",Self::Mouse=>"Mouse",Self::Keyboard=>"Keyboard",Self::Audio=>"Audio"}}
    pub fn label(self)->&'static str{match self{Self::Usb=>"USB 设备",Self::Volume=>"移动磁盘",Self::Phone=>"手机 / 便携设备",Self::Headphones=>"耳机",Self::Speaker=>"音响",Self::Gamepad=>"手柄",Self::Mouse=>"鼠标",Self::Keyboard=>"键盘",Self::Audio=>"音频设备"}}
    /// The three families that existed before 2.0 keep their own filters and disconnect behaviour.
    pub fn legacy(self)->bool{matches!(self,Self::Usb|Self::Volume|Self::Phone)}
}
/// Which families may surface. Built once per frame from the settings snapshot.
pub struct Gates{pub master:bool,pub disconnect:bool,pub headphones:bool,pub speaker:bool,pub gamepad:bool,pub mouse:bool,pub keyboard:bool}
impl Gates{
    pub fn allows(&self,kind:Kind,connected:bool)->bool{
        if !self.master{return false}
        if !connected&&!kind.legacy()&&!self.disconnect{return false}
        match kind{Kind::Headphones=>self.headphones,Kind::Speaker=>self.speaker,Kind::Gamepad=>self.gamepad,Kind::Mouse=>self.mouse,Kind::Keyboard=>self.keyboard,
            // An unclassified endpoint is never folded into one category: it needs both audio
            // families to be on, which is the only choice that does not invent a type.
            Kind::Audio=>self.headphones&&self.speaker,_=>true}
    }
}
#[derive(Clone,Debug)]pub struct Event{pub key:String,pub kind:Kind,pub connected:bool,pub label:String,at:Instant}
impl Event{
    pub fn new(key:String,kind:Kind,connected:bool,label:String)->Self{Self{key,kind,connected,label,at:Instant::now()}}
    pub fn notice(&self)->crate::notifications::Notice{crate::notifications::Notice{
        app_id:format!("LyricIsland.Device.{}",self.kind.key()),app_name:"设备连接".into(),
        title:format!("{}{}",self.label,if self.connected{"已连接"}else if self.kind.legacy(){"已移除"}else{"已断开"}),
        body:if self.connected{"设备已就绪，可以继续使用。".into()}else{"设备连接已断开。".into()},icon:None}}
}

struct RawPending{kind:Kind,connected:bool,at:Instant}
struct AudioPending{id:String,connected:bool,at:Instant}
#[derive(Clone)]struct AudioInfo{kind:Kind,name:String,container:Option<String>}
enum AudioSignal{Added(String),Removed(String),State(String,DEVICE_STATE)}
#[implement(IMMNotificationClient)]
struct AudioCallback{tx:Sender<AudioSignal>}
impl IMMNotificationClient_Impl for AudioCallback_Impl{
    fn OnDeviceStateChanged(&self,id:&PCWSTR,state:DEVICE_STATE)->windows::core::Result<()>{let this:&AudioCallback=self;if let Ok(id)=unsafe{id.to_string()}{let _=this.tx.send(AudioSignal::State(id,state));}Ok(())}
    fn OnDeviceAdded(&self,id:&PCWSTR)->windows::core::Result<()>{let this:&AudioCallback=self;if let Ok(id)=unsafe{id.to_string()}{let _=this.tx.send(AudioSignal::Added(id));}Ok(())}
    fn OnDeviceRemoved(&self,id:&PCWSTR)->windows::core::Result<()>{let this:&AudioCallback=self;if let Ok(id)=unsafe{id.to_string()}{let _=this.tx.send(AudioSignal::Removed(id));}Ok(())}
    // A default-output switch is not a connection change.
    fn OnDefaultDeviceChanged(&self,_flow:EDataFlow,_role:ERole,_id:&PCWSTR)->windows::core::Result<()>{Ok(())}
    fn OnPropertyValueChanged(&self,_id:&PCWSTR,_key:&windows::Win32::Foundation::PROPERTYKEY)->windows::core::Result<()>{Ok(())}
}
struct AudioListener{enumerator:IMMDeviceEnumerator,client:IMMNotificationClient,rx:Receiver<AudioSignal>,known:HashMap<String,AudioInfo>}
impl Drop for AudioListener{fn drop(&mut self){unsafe{let _=self.enumerator.UnregisterEndpointNotificationCallback(&self.client);}}}

pub struct Hub{
    handles:Vec<*mut c_void>,pending:Vec<Event>,seen:HashMap<String,Instant>,
    raw:HashMap<isize,Kind>,raw_pending:Vec<RawPending>,audio_pending:Vec<AudioPending>,
    audio:Option<AudioListener>,endpoints:HashMap<String,AudioInfo>,
}
impl Default for Hub{fn default()->Self{Self{handles:vec![],pending:vec![],seen:HashMap::new(),raw:HashMap::new(),raw_pending:vec![],audio_pending:vec![],audio:None,endpoints:HashMap::new()}}}
impl Drop for Hub{fn drop(&mut self){for h in self.handles.drain(..){unsafe{UnregisterDeviceNotification(h);}}}}
impl Hub{
    pub unsafe fn register(&mut self,hwnd:HWND){
        for class in [USB,WPD,AUDIO_RENDER]{let f=Filter{size:std::mem::size_of::<Filter>()as u32,kind:5,reserved:0,class,name:[0]};let h=RegisterDeviceNotificationW(hwnd.0,(&f as *const Filter).cast(),0);if !h.is_null(){self.handles.push(h);}}
        let usages=[(0x01,0x02),(0x01,0x06),(0x01,0x04),(0x01,0x05),(0x01,0x08)];
        let devices=usages.map(|(page,usage)|RAWINPUTDEVICE{usUsagePage:page,usUsage:usage,dwFlags:RIDEV_DEVNOTIFY,hwndTarget:hwnd});
        let _=RegisterRawInputDevices(&devices,std::mem::size_of::<RAWINPUTDEVICE>()as u32);
        self.audio=AudioListener::listen();
        // The baseline is the set of endpoints that already exist; their removal still counts,
        // but their presence at startup must never be announced.
        if let Some(listener)=&self.audio{self.endpoints=listener.known.clone();}
    }
    /// WM_INPUT_DEVICE_CHANGE: only the device identity is read, never key or pointer traffic.
    pub unsafe fn input_change(&mut self,action:u32,handle:isize){
        if action==GIDC_ARRIVAL{
            let mut info=RID_DEVICE_INFO{cbSize:std::mem::size_of::<RID_DEVICE_INFO>()as u32,..Default::default()};let mut size=info.cbSize;
            let read=GetRawInputDeviceInfoW(Some(HANDLE(handle as *mut c_void)),RIDI_DEVICEINFO,Some((&mut info as *mut RID_DEVICE_INFO).cast()),&mut size);
            if read==u32::MAX{return}
            let kind=if info.dwType==RIM_TYPEMOUSE{Some(Kind::Mouse)}else if info.dwType==RIM_TYPEKEYBOARD{Some(Kind::Keyboard)}
                else if info.dwType==RIM_TYPEHID{let hid=info.Anonymous.hid;if hid.usUsagePage==0x01&&matches!(hid.usUsage,0x04|0x05|0x08){Some(Kind::Gamepad)}else{None}}else{None};
            if let Some(kind)=kind{self.raw.insert(handle,kind);self.raw_pending.push(RawPending{kind,connected:true,at:Instant::now()});}
        }else if action==GIDC_REMOVAL{if let Some(kind)=self.raw.remove(&handle){self.raw_pending.push(RawPending{kind,connected:false,at:Instant::now()});}}
    }
    pub unsafe fn message(&mut self,action:usize,data:isize){if ![0x8000,0x8004].contains(&action)||data==0{return}let h=&*(data as *const Header);let connected=action==0x8000;
        if h.kind==2&&h.size as usize>=std::mem::size_of::<Volume>(){let v=&*(data as *const Volume);for bit in 0..26{if v.mask&(1<<bit)!=0{let letter=(b'A'+bit as u8)as char;let root=format!("{letter}:\\").encode_utf16().chain(Some(0)).collect::<Vec<_>>();if connected&&GetDriveTypeW(root.as_ptr())!=2{continue}self.record(Event::new(format!("drive-{letter}"),Kind::Volume,connected,format!("移动磁盘 {letter}: ")));}}}
        else if h.kind==5&&h.size>=30&&h.size<8192{let class=std::ptr::read_unaligned((data as *const u8).add(12).cast::<GUID>());let bytes=(h.size as usize-28)/2;let raw=std::slice::from_raw_parts((data as *const u8).add(28).cast::<u16>(),bytes);let end=raw.iter().position(|v|*v==0).unwrap_or(raw.len());let name=String::from_utf16_lossy(&raw[..end]);
            let phone=class==WPD;if class==AUDIO_RENDER{self.audio_pending.push(AudioPending{id:name,connected,at:Instant::now()});}else{self.record(Event::new(name,if phone{Kind::Phone}else{Kind::Usb},connected,if phone{"手机 / 便携设备".into()}else{"USB 设备".into()}));}
        }
    }
    pub fn record(&mut self,event:Event){
        // A specific interface (drive letter, portable device, audio endpoint) describes the
        // same physical plug-in that also raises a generic USB event; only the specific one
        // may surface, whichever arrives first.
        if event.kind==Kind::Usb&&self.pending.iter().any(|e|e.kind!=Kind::Usb&&e.connected==event.connected){return}
        if self.fresh(&event){if event.kind!=Kind::Usb{self.pending.retain(|e|e.kind!=Kind::Usb||e.connected!=event.connected);}self.pending.push(event);}
    }
    fn fresh(&mut self,event:&Event)->bool{let key=format!("{}:{}:{}",event.kind.key(),event.key,event.connected);if self.seen.get(&key).is_some_and(|t|t.elapsed()<Duration::from_secs(3)){return false}if self.seen.len()>128{self.seen.clear();}self.seen.insert(key,Instant::now());true}
    pub fn poll(&mut self,gates:&Gates)->Vec<Event>{
        if !gates.master{self.pending.clear();self.raw_pending.clear();self.audio_pending.clear();return vec![]}
        self.pending.retain(|e|gates.allows(e.kind,e.connected));
        self.raw_pending.retain(|e|gates.allows(e.kind,e.connected));
        self.audio_pending.retain(|e|gates.allows(self.endpoints.get(&e.id).map(|i|i.kind).unwrap_or(Kind::Audio),e.connected));
        let mut ready=vec![];
        // Raw input: collapse multiple HID collections of one physical device into one notice.
        let mut keep_raw=Vec::new();
        for item in std::mem::take(&mut self.raw_pending){if item.at.elapsed()<Duration::from_millis(900){keep_raw.push(item);continue}
            let event=Event::new(format!("raw-{}",item.kind.key()),item.kind,item.connected,item.kind.label().into());
            if self.fresh(&event){ready.push(event);}
        }
        self.raw_pending=keep_raw;
        // Audio: Core Audio callbacks only report that an endpoint changed; classification and
        // the availability check happen once here, after the endpoint had time to settle.
        let signals:Vec<AudioSignal>=self.audio.as_ref().map(|a|{let mut out=vec![];while let Ok(s)=a.rx.try_recv(){out.push(s)}out}).unwrap_or_default();
        for signal in signals{
            match signal{
                AudioSignal::Added(id)|AudioSignal::State(id,DEVICE_STATE_ACTIVE)=>self.audio_pending.push(AudioPending{id,connected:true,at:Instant::now()}),
                AudioSignal::Removed(id)=>self.audio_pending.push(AudioPending{id,connected:false,at:Instant::now()}),
                AudioSignal::State(id,_)=>self.audio_pending.push(AudioPending{id,connected:false,at:Instant::now()}),
            }
        }
        let enumerator=self.audio.as_ref().map(|a|a.enumerator.clone());
        let mut keep_audio=Vec::new();
        for item in std::mem::take(&mut self.audio_pending){
            if item.at.elapsed()<Duration::from_millis(800){keep_audio.push(item);continue}
            if item.connected{
                let Some(enumerator)=&enumerator else{continue};
                let Some((info,state))=(unsafe{audio_info(&item.id,enumerator)}) else{continue};
                if state!=DEVICE_STATE_ACTIVE||self.endpoints.contains_key(&item.id){continue}
                self.endpoints.insert(item.id.clone(),info.clone());
                let event=Event::new(info.container.clone().unwrap_or(item.id.clone()),info.kind,true,info.name);
                if self.fresh(&event){ready.push(event);}
            }else{
                let Some(info)=self.endpoints.remove(&item.id) else{continue};
                let event=Event::new(info.container.clone().unwrap_or(item.id),info.kind,false,info.name);
                if self.fresh(&event){ready.push(event);}
            }
        }
        self.audio_pending=keep_audio;
        let mut settled=vec![];
        self.pending.retain(|e|{if e.at.elapsed()>=Duration::from_millis(if e.kind.legacy(){if e.kind==Kind::Usb{1500}else{250}}else{250}){settled.push(e.clone());false}else{true}});
        ready.extend(settled);
        ready
    }
}
impl AudioListener{
    fn listen()->Option<Self>{
        unsafe{
            let enumerator:IMMDeviceEnumerator=CoCreateInstance(&MMDeviceEnumerator,None,CLSCTX_ALL).ok()?;
            let (tx,rx)=mpsc::channel();let client:IMMNotificationClient=AudioCallback{tx}.into();
            enumerator.RegisterEndpointNotificationCallback(&client).ok()?;
            // Silent baseline: everything already present must not be announced.
            let mut known=HashMap::new();if let Ok(list)=enumerator.EnumAudioEndpoints(eRender,DEVICE_STATE_ACTIVE){let count=list.GetCount().unwrap_or(0);
                for index in 0..count{if let Ok(device)=list.Item(index){if let Ok(id)=device.GetId(){if let Ok(id)=id.to_string(){if let Some((info,_))=info_from(&device){known.insert(id,info);}}}}}}
            Some(Self{enumerator,client,rx,known})
        }
    }
}
unsafe fn audio_info(id:&str,enumerator:&IMMDeviceEnumerator)->Option<(AudioInfo,DEVICE_STATE)>{
    let device=enumerator.GetDevice(&HSTRING::from(id)).ok()?;let state=device.GetState().ok()?;let info=info_from(&device)?.0;Some((info,state))
}
unsafe fn info_from(device:&IMMDevice)->Option<(AudioInfo,DEVICE_STATE)>{
    let store=device.OpenPropertyStore(STGM_READ).ok()?;
    let kind=read_u32(store.GetValue(&PKEY_AudioEndpoint_FormFactor).ok()?).map(|form|match form{3|5=>Kind::Headphones,1=>Kind::Speaker,_=>Kind::Audio}).unwrap_or(Kind::Audio);
    let name=read_string(store.GetValue(&PKEY_Device_FriendlyName).ok()?).filter(|n|!n.trim().is_empty()).unwrap_or_else(||kind.label().into());
    let container=read_guid(store.GetValue(&PKEY_Device_ContainerId).ok()?);
    let state=device.GetState().ok()?;
    Some((AudioInfo{kind,name,container},state))
}
fn read_u32(mut value:PROPVARIANT)->Option<u32>{let v=unsafe{&value.Anonymous.Anonymous};let result=if v.vt.0==VT_UI4.0||v.vt.0==VT_I4.0{Some(unsafe{v.Anonymous.ulVal})}else{None};unsafe{let _=PropVariantClear(&mut value);}result}
fn read_string(mut value:PROPVARIANT)->Option<String>{let v=unsafe{&value.Anonymous.Anonymous};let result=if v.vt.0==VT_LPWSTR.0{let text=unsafe{v.Anonymous.pwszVal};if text.is_null(){None}else{unsafe{text.to_string()}.ok()}}else{None};unsafe{let _=PropVariantClear(&mut value);}result}
fn read_guid(mut value:PROPVARIANT)->Option<String>{let v=unsafe{&value.Anonymous.Anonymous};let result=if v.vt.0==VT_CLSID.0{let ptr=unsafe{v.Anonymous.puuid};if ptr.is_null(){None}else{Some(unsafe{format!("{:?}",*ptr)})}}else{None};unsafe{let _=PropVariantClear(&mut value);}result}
#[cfg(test)]mod tests{use super::*;
fn gates()->Gates{Gates{master:true,disconnect:false,headphones:true,speaker:false,gamepad:true,mouse:true,keyboard:true}}
#[test]fn duplicate_interface_events_and_generic_usb_are_coalesced(){let mut h=Hub::default();h.record(Event::new("usb-x".into(),Kind::Usb,true,"USB".into()));h.record(Event::new("usb-x".into(),Kind::Usb,true,"USB".into()));assert_eq!(h.pending.len(),1);h.record(Event::new("drive-E".into(),Kind::Volume,true,"E:".into()));assert_eq!(h.pending.len(),1);assert_eq!(h.pending[0].kind,Kind::Volume);assert!(h.poll(&Gates{master:false,disconnect:false,headphones:true,speaker:false,gamepad:false,mouse:false,keyboard:false}).is_empty());assert!(h.pending.is_empty());}
#[test]fn category_switches_filter_without_inventing_a_type(){let g=gates();assert!(g.allows(Kind::Headphones,true));assert!(!g.allows(Kind::Speaker,true));assert!(!g.allows(Kind::Audio,true),"an unclassified endpoint needs both audio families");
    assert!(!g.allows(Kind::Headphones,false),"disconnect reminders are opt-in");assert!(g.allows(Kind::Usb,false),"legacy removals keep working");
    let both=Gates{headphones:true,speaker:true,..gates()};assert!(both.allows(Kind::Audio,true));}
#[test]fn raw_collections_merge_and_disconnect_needs_opt_in(){let mut h=Hub::default();h.raw_pending.push(RawPending{kind:Kind::Mouse,connected:true,at:Instant::now()-Duration::from_millis(1000)});h.raw_pending.push(RawPending{kind:Kind::Mouse,connected:true,at:Instant::now()-Duration::from_millis(1000)});
    let events=h.poll(&gates());assert_eq!(events.len(),1,"two collections of one mouse must not stack notices");assert_eq!(events[0].kind,Kind::Mouse);
    let mut h=Hub::default();h.raw_pending.push(RawPending{kind:Kind::Keyboard,connected:false,at:Instant::now()-Duration::from_millis(1000)});assert!(h.poll(&gates()).is_empty());
    let mut h=Hub::default();h.raw_pending.push(RawPending{kind:Kind::Keyboard,connected:false,at:Instant::now()-Duration::from_millis(1000)});assert_eq!(h.poll(&Gates{disconnect:true,..gates()}).len(),1);}
}

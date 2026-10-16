//! Event-driven removable-device notices. No polling, drivers or elevated permissions.
use std::{collections::HashMap,ffi::c_void,time::{Duration,Instant}};use windows::{core::GUID,Win32::Foundation::HWND};
#[link(name="user32")]unsafe extern "system"{fn RegisterDeviceNotificationW(recipient:*mut c_void,filter:*const c_void,flags:u32)->*mut c_void;fn UnregisterDeviceNotification(handle:*mut c_void)->i32;}
#[link(name="kernel32")]unsafe extern "system"{fn GetDriveTypeW(root:*const u16)->u32;}
const USB:GUID=GUID::from_u128(0xa5dcbf10_6530_11d2_901f_00c04fb951ed);const WPD:GUID=GUID::from_u128(0x6ac27878_a6fa_4155_ba85_f98f491d4f33);
#[repr(C)]struct Filter{size:u32,kind:u32,reserved:u32,class:GUID,name:[u16;1]}
#[repr(C)]struct Header{size:u32,kind:u32,reserved:u32}
#[repr(C)]struct Volume{header:Header,mask:u32,flags:u16}
#[derive(Clone,Debug)]pub struct Event{pub key:String,pub kind:u8,pub connected:bool,pub label:String,at:Instant}
impl Event{pub fn new(key:String,kind:u8,connected:bool,label:String)->Self{Self{key,kind,connected,label,at:Instant::now()}}pub fn notice(&self)->crate::notifications::Notice{crate::notifications::Notice{app_id:format!("LyricIsland.Device.{}",if self.kind==2{"Phone"}else if self.kind==1{"Volume"}else{"USB"}),app_name:"设备连接".into(),title:format!("{}{}",self.label,if self.connected{"已连接"}else{"已移除"}),body:if self.connected{"设备已就绪，可以继续使用。".into()}else{"设备连接已断开。".into()},icon:None}}}
#[derive(Default)]pub struct Hub{handles:Vec<*mut c_void>,pending:Vec<Event>,seen:HashMap<String,Instant>}
impl Drop for Hub{fn drop(&mut self){for h in self.handles.drain(..){unsafe{UnregisterDeviceNotification(h);}}}}
impl Hub{
 pub unsafe fn register(&mut self,hwnd:HWND){for class in [USB,WPD]{let f=Filter{size:std::mem::size_of::<Filter>()as u32,kind:5,reserved:0,class,name:[0]};let h=RegisterDeviceNotificationW(hwnd.0,(&f as *const Filter).cast(),0);if !h.is_null(){self.handles.push(h);}}}
 pub unsafe fn message(&mut self,action:usize,data:isize){if ![0x8000,0x8004].contains(&action)||data==0{return}let h=&*(data as *const Header);let connected=action==0x8000;
  if h.kind==2&&h.size as usize>=std::mem::size_of::<Volume>(){let v=&*(data as *const Volume);for bit in 0..26{if v.mask&(1<<bit)!=0{let letter=(b'A'+bit as u8)as char;let root=format!("{letter}:\\").encode_utf16().chain(Some(0)).collect::<Vec<_>>();if connected&&GetDriveTypeW(root.as_ptr())!=2{continue}self.record(Event::new(format!("drive-{letter}"),1,connected,format!("移动磁盘 {letter}: ")));}}}
  else if h.kind==5&&h.size>=30&&h.size<8192{let class=std::ptr::read_unaligned((data as *const u8).add(12).cast::<GUID>());let bytes=(h.size as usize-28)/2;let raw=std::slice::from_raw_parts((data as *const u8).add(28).cast::<u16>(),bytes);let end=raw.iter().position(|v|*v==0).unwrap_or(raw.len());let name=String::from_utf16_lossy(&raw[..end]);let phone=class==WPD;self.record(Event::new(name,if phone{2}else{0},connected,if phone{"手机 / 便携设备".into()}else{"USB 设备".into()}));}
 }
 pub fn record(&mut self,event:Event){let key=format!("{}:{}",event.key,event.connected);if self.seen.get(&key).is_some_and(|t|t.elapsed()<Duration::from_secs(3)){return}if self.seen.len()>128{self.seen.clear();}self.seen.insert(key,Instant::now());if event.kind>0{self.pending.retain(|e|e.kind>0||e.connected!=event.connected);}if self.pending.len()<32{self.pending.push(event);}}
 pub fn poll(&mut self,enabled:bool)->Vec<Event>{if !enabled{self.pending.clear();return vec![]}let mut ready=vec![];self.pending.retain(|e|{if e.at.elapsed()>=Duration::from_millis(if e.kind==0{1500}else{250}){ready.push(e.clone());false}else{true}});ready}
}
#[cfg(test)]mod tests{use super::*;#[test]fn duplicate_interface_events_and_generic_usb_are_coalesced(){let mut h=Hub::default();h.record(Event::new("usb-x".into(),0,true,"USB".into()));h.record(Event::new("usb-x".into(),0,true,"USB".into()));assert_eq!(h.pending.len(),1);h.record(Event::new("drive-E".into(),1,true,"E:".into()));assert_eq!(h.pending.len(),1);assert_eq!(h.pending[0].kind,1);assert!(h.poll(false).is_empty());assert!(h.pending.is_empty());}}

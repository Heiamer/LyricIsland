//! Native player identity, local-only silent control, and real timeline acknowledgement.
use std::path::{Path,PathBuf};
use windows::Win32::System::Diagnostics::ToolHelp::*;
#[cfg(test)]use base64::{Engine,engine::general_purpose::STANDARD};
use windows::Win32::{Foundation::CloseHandle,System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION,QueryFullProcessImageNameW,PROCESS_NAME_WIN32,GetExitCodeProcess}};

#[derive(Clone,Debug,PartialEq,Eq)]pub struct Target{pub pid:u32,pub executable:PathBuf}
impl Target{
    pub fn from_pid(pid:u32)->Option<Self>{unsafe{
        let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid).ok()?;
        let mut path=[0u16;32768];let mut length=path.len()as u32;let mut code=0;
        let alive=GetExitCodeProcess(process,&mut code).is_ok()&&code==259;
        let queried=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,windows::core::PWSTR(path.as_mut_ptr()),&mut length).is_ok();let _=CloseHandle(process);
        if !alive||!queried{return None}
        let executable=PathBuf::from(String::from_utf16_lossy(&path[..length as usize]));
        if !is_player(&executable){return None}Some(Self{pid,executable})
    }}
    pub fn available(&self)->bool{Self::from_pid(self.pid).is_some_and(|target|target.executable==self.executable)}
    pub fn silent_available(&self,port:u16)->bool{crate::netease_window::available(self)||crate::netease_cdp::available(self.pid,port)}
    pub fn background_available(&self)->bool{crate::netease_window::available(self)}
    pub fn seek(&self,port:u16,seconds:f64,duration:f64,song_id:&str,valid:&dyn Fn()->bool)->Result<(&'static str,f64),String>{
        if !self.available(){return Err("网易云已退出或重启，请等待重新连接".into())}
        if self.background_available(){return crate::netease_window::seek(self,seconds,duration,valid).map(|actual|("NeteaseBackground",actual))}
        if !valid(){return Err("歌曲已变化，已取消进度操作".into())}
        crate::netease_cdp::seek(self.pid,port,song_id,seconds)?;Ok(("NeteaseCDP",seconds))
    }

}
/// The local CDP port is user-configurable; invalid or out-of-range values fall back to 9223.
pub fn cdp_port(settings:&serde_json::Value)->u16{
    let value=&settings["NeteaseCdpPort"];
    let parsed=value.as_u64().or_else(||value.as_str().and_then(|s|s.trim().parse::<u64>().ok()));
    parsed.filter(|p|(1024..=65535).contains(p)).map(|p|p as u16).unwrap_or(crate::netease_cdp::DEFAULT_PORT)
}
fn is_player(path:&Path)->bool{path.is_absolute()&&path.file_name().is_some_and(|name|name.to_string_lossy().eq_ignore_ascii_case("cloudmusic.exe"))&&path.parent().is_some_and(|p|p.join("cloudmusic.dll").is_file())}
pub fn bounded_target(seconds:f64,duration:f64)->Result<f64,String>{
    if !seconds.is_finite()||seconds<0.||!duration.is_finite()||duration<=0.||duration>86400.{return Err("播放进度或歌曲时长无效".into())}
    Ok(seconds.min((duration-0.05).max(0.)))
}
#[cfg(test)]fn seek_argument(seconds:f64)->Result<String,String>{Ok(format!("--webcmd={}",seek_uri(seconds)?))}
#[cfg(test)]fn seek_uri(seconds:f64)->Result<String,String>{
    if !seconds.is_finite()||!(0.0..=86400.).contains(&seconds){return Err("播放进度无效".into())}
    let message=serde_json::to_vec(&serde_json::json!({"cmd":"seek","value":seconds})).map_err(|e|e.to_string())?;
    Ok(format!("orpheus://{}",STANDARD.encode(message)))
}
pub fn acknowledged(target:f64,position:f64,playing:bool,sent_ms:i64,captured_ms:i64,now_ms:i64)->bool{
    if !target.is_finite()||!position.is_finite()||captured_ms<sent_ms||!(0..800).contains(&(now_ms-captured_ms)){return false}
    let advanced=if playing{((captured_ms-sent_ms)as f64/1000.).clamp(0.,5.)}else{0.};
    (position-target).abs()<=0.8||(position-target-advanced).abs()<=0.8
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn protocol_uses_absolute_seconds_and_numeric_json(){for seconds in [0.,0.01,61.26,86399.]{let uri=seek_uri(seconds).unwrap();let data=STANDARD.decode(uri.strip_prefix("orpheus://").unwrap()).unwrap();let json:serde_json::Value=serde_json::from_slice(&data).unwrap();assert_eq!(json["cmd"],"seek");assert_eq!(json["value"].as_f64(),Some(seconds));assert_eq!(json.as_object().unwrap().len(),2);}}
 #[test]fn launch_guard_receives_one_registered_switch(){let arg=seek_argument(12.5).unwrap();assert!(arg.starts_with("--webcmd=orpheus://"));assert!(!arg.contains([' ','"','\n','\r']));let data=STANDARD.decode(arg.strip_prefix("--webcmd=orpheus://").unwrap()).unwrap();assert_eq!(serde_json::from_slice::<serde_json::Value>(&data).unwrap()["value"],12.5);}
 #[test]fn malformed_targets_never_produce_commands(){for seconds in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,-1.,86401.]{assert!(seek_uri(seconds).is_err());}for duration in [0.,-1.,f64::NAN,f64::INFINITY,86401.]{assert!(bounded_target(10.,duration).is_err());}assert!(bounded_target(-1.,180.).is_err());assert!(bounded_target(f64::NAN,180.).is_err());assert_eq!(bounded_target(500.,180.).unwrap(),179.95);}
 #[test]fn stale_or_unconfirmed_positions_cannot_acknowledge_seek(){assert!(!acknowledged(80.,80.,false,1000,999,1100));assert!(!acknowledged(80.,10.,false,1000,1100,1200));assert!(!acknowledged(80.,80.,false,1000,1100,2000));assert!(!acknowledged(80.,80.,false,1000,1500,1400));assert!(acknowledged(80.,80.2,true,1000,1250,1300));assert!(acknowledged(80.,82.,true,1000,3000,3100));assert!(!acknowledged(80.,82.,false,1000,3000,3100));}
 #[test]fn configured_port_accepts_number_or_string_and_rejects_privileged_or_out_of_range(){use serde_json::json;assert_eq!(cdp_port(&json!({"NeteaseCdpPort":"9224"})),9224);assert_eq!(cdp_port(&json!({"NeteaseCdpPort":9225})),9225);assert_eq!(cdp_port(&json!({"NeteaseCdpPort":"80"})),9223);assert_eq!(cdp_port(&json!({"NeteaseCdpPort":"70000"})),9223);assert_eq!(cdp_port(&json!({})),9223);}
}

pub fn players()->Vec<Target>{
 let mut out=Vec::new();
 unsafe{
  let Ok(snapshot)=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0)else{return out};
  let mut entry=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>()as u32,..Default::default()};
  if Process32FirstW(snapshot,&mut entry).is_ok(){loop{
   let name=String::from_utf16_lossy(entry.szExeFile.split(|c|*c==0).next().unwrap_or(&[]));
   if name.eq_ignore_ascii_case("cloudmusic.exe"){if let Some(target)=Target::from_pid(entry.th32ProcessID){out.push(target);}}
   if Process32NextW(snapshot,&mut entry).is_err(){break}
  }}
  let _=CloseHandle(snapshot);
 }
 out
}

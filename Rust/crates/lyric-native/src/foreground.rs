//! One shared foreground watcher for F05/F06. Events are only reported after the
//! same executable has stayed in front for 500 ms; identity is the full path.
use crate::app_rules::same_path;
use std::time::{Duration,Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_NAME_WIN32,PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow,GetWindowThreadProcessId};
pub struct Watcher{pub path:Option<String>,pub name:String,pub segment:u64,pub unreadable:bool,candidate:Option<(String,String,Instant)>,last_poll:Instant}
fn process_path(pid:u32)->Option<String>{
 unsafe{
  let handle=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid).ok()?;
  let mut buffer=[0u16;1024];let mut length=buffer.len()as u32;
  let result=QueryFullProcessImageNameW(handle,PROCESS_NAME_WIN32,PWSTR(buffer.as_mut_ptr()),&mut length);
  let _=CloseHandle(handle);
  if result.is_err()||length==0{return None}
  Some(String::from_utf16_lossy(&buffer[..length as usize]))
 }
}
fn display_name(path:&str)->String{
 std::path::Path::new(path).file_stem().and_then(|s|s.to_str()).filter(|s|!s.is_empty()).unwrap_or("未知程序").to_owned()
}
fn read()->(Option<String>,String,bool){
 unsafe{
  let window=GetForegroundWindow();if window.0.is_null(){return(None,String::new(),false)}
  let mut pid=0u32;GetWindowThreadProcessId(window,Some(&mut pid));
  if pid==0{return(None,String::new(),false)}
  match process_path(pid){
   Some(path)=>{let name=display_name(&path);(Some(path),name,false)},
   None=>(None,"无法识别的程序".into(),true)
  }
 }
}
impl Watcher{
 pub fn new()->Self{
  let (path,name,_)=read();let mut w=Self{path:path.clone(),name,segment:1,unreadable:false,candidate:None,last_poll:Instant::now()};
  if w.path.is_some(){w.segment=1}
  w
 }
 /// Poll the foreground window. Returns a new stable path and its segment id once
 /// the 500 ms settle window has passed.
 pub fn poll(&mut self)->Option<(String,u64)>{
  if self.last_poll.elapsed()<Duration::from_millis(200){return None}
  self.last_poll=Instant::now();
  let (path,name,unreadable)=read();self.unreadable=unreadable;
  let Some(path)=path else{self.candidate=None;return None};
  match self.candidate.take(){
   Some((candidate,name,at)) if same_path(&candidate,&path)=>{if at.elapsed()>=Duration::from_millis(500){if self.path.as_deref()!=Some(path.as_str()){self.path=Some(path.clone());self.name=name;self.segment+=1;return Some((path,self.segment))}self.name=name;}else{self.candidate=Some((candidate,name,at));return None}},
   _=>{self.candidate=Some((path,name,Instant::now()));return None}
  }
  None
 }
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn display_names_and_path_identity(){assert_eq!(display_name("C:/Tools/Editor.exe"),"Editor");assert!(same_path("C:/Tools/Editor.exe","c:/tools/EDITOR.exe"));}
}

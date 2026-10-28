//! Background seek for the verified official client. No top-level show, focus, global
//! input, debug port, process launch or player-memory writes are involved.
use std::{path::PathBuf,sync::{Mutex,OnceLock},time::{Duration,Instant,SystemTime}};
use sha2::{Digest,Sha256};
use windows::{core::{BOOL,PCWSTR},Win32::{Foundation::{CloseHandle,HANDLE,HWND,LPARAM,WPARAM,RECT,WAIT_OBJECT_0,WAIT_ABANDONED},System::Threading::{CreateMutexW,ReleaseMutex,WaitForSingleObject},UI::{HiDpi::*,Input::KeyboardAndMouse::IsWindowEnabled,WindowsAndMessaging::*}}};
use crate::netease_control::Target;

// This package defines a full-width music slider 83 CSS px above the client bottom.
// Refuse changed resources rather than click an unverified player's interface.
const VERIFIED_PACKAGE:&str="170413b1675708e87a9e89c2d122a8643a51e86b5b5ae1c6260cfeaf25b19be4";
type PackageCache=(PathBuf,u64,SystemTime,bool);
fn supported(target:&Target)->bool{
 let Some(dir)=target.executable.parent()else{return false};let path=dir.join("package/orpheus.ntpk");
 let Ok(meta)=std::fs::metadata(&path)else{return false};let Ok(modified)=meta.modified()else{return false};
 static CACHE:OnceLock<Mutex<Option<PackageCache>>>=OnceLock::new();
 let Ok(mut cache)=CACHE.get_or_init(||Mutex::new(None)).lock()else{return false};
 if let Some((old,len,stamp,ok))=cache.as_ref(){if old==&path&&*len==meta.len()&&*stamp==modified{return *ok}}
 let ok=if meta.len()>64*1024*1024{false}else{std::fs::read(&path).is_ok_and(|bytes|format!("{:x}",Sha256::digest(bytes))==VERIFIED_PACKAGE)};
 *cache=Some((path,meta.len(),modified,ok));ok
}
fn class(window:HWND,name:&str)->bool{unsafe{let mut text=[0u16;256];let n=GetClassNameW(window,&mut text);n>0&&String::from_utf16_lossy(&text[..n as usize])==name}}
fn owned(window:HWND,pid:u32)->bool{unsafe{let mut owner=0;GetWindowThreadProcessId(window,Some(&mut owner));owner==pid&&IsWindow(Some(window)).as_bool()}}
#[derive(Default)]struct Windows{pid:u32,roots:Vec<HWND>,hosts:Vec<HWND>,renderers:Vec<HWND>}
unsafe extern "system" fn top(window:HWND,p:LPARAM)->BOOL{
 let found=&mut*(p.0 as*mut Windows);
 if owned(window,found.pid)&&class(window,"OrpheusBrowserHost")&&!GetWindow(window,GW_OWNER).is_ok_and(|h|!h.0.is_null()){found.roots.push(window);}BOOL(1)
}
unsafe extern "system" fn children(window:HWND,p:LPARAM)->BOOL{
 let found=&mut*(p.0 as*mut Windows);
 if owned(window,found.pid){if class(window,"CefBrowserWindow"){found.hosts.push(window);}if class(window,"Chrome_RenderWidgetHostHWND"){found.renderers.push(window);}}BOOL(1)
}
fn windows(pid:u32)->Option<(HWND,HWND,Option<HWND>)>{unsafe{
 let mut found=Windows{pid,..Default::default()};let p=LPARAM(&mut found as*mut _ as isize);let _=EnumWindows(Some(top),p);
 if found.roots.len()!=1{return None}let root=found.roots[0];if !IsWindowEnabled(root).as_bool(){return None}
 let _=EnumChildWindows(Some(root),Some(children),p);if found.hosts.len()!=1||found.renderers.len()>1{return None}
 Some((root,found.hosts[0],found.renderers.first().copied()))
}}
struct Dpi(DPI_AWARENESS_CONTEXT);
impl Dpi{fn enter()->Self{Self(unsafe{SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)})}}
impl Drop for Dpi{fn drop(&mut self){unsafe{let _=SetThreadDpiAwarenessContext(self.0);}}}
struct Serial(HANDLE);
impl Serial{fn enter()->Result<Self,String>{unsafe{
 let name="Local\\DotIsle.NetEase.BackgroundSeek.v1".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
 let handle=CreateMutexW(None,false,PCWSTR(name.as_ptr())).map_err(|e|e.to_string())?;
 let status=WaitForSingleObject(handle,0);if status!=WAIT_OBJECT_0&&status!=WAIT_ABANDONED{let _=CloseHandle(handle);return Err("网易云正在处理另一条进度操作，请稍后重试".into())}Ok(Self(handle))
}}}
impl Drop for Serial{fn drop(&mut self){unsafe{let _=ReleaseMutex(self.0);let _=CloseHandle(self.0);}}}
/// Restores only a temporarily resized child, and yields if the user changes the main window.
struct ChildSize{pid:u32,root:HWND,host:HWND,width:i32,height:i32,iconic:bool,visible:bool}
impl Drop for ChildSize{fn drop(&mut self){unsafe{
 if owned(self.root,self.pid)&&owned(self.host,self.pid)&&IsIconic(self.root).as_bool()==self.iconic&&IsWindowVisible(self.root).as_bool()==self.visible{
  let _=SetWindowPos(self.host,None,0,0,self.width,self.height,SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_ASYNCWINDOWPOS);
 }
}}}
fn bounds(width:i32,height:i32,dpi:u32,fraction:f64)->Option<(i32,i32)>{
 let scale=dpi as f64/96.;if !fraction.is_finite()||!(0.0..=1.).contains(&fraction)||!(700.0..=6000.0).contains(&(width as f64/scale))||!(400.0..=3000.0).contains(&(height as f64/scale)){return None}
 let x=(fraction*width as f64).round().clamp(1.,(width-2)as f64)as i32;
 Some((x,(height as f64-81.5*scale).round()as i32))
}
pub fn available(target:&Target)->bool{target.available()&&supported(target)&&windows(target.pid).is_some()}

pub fn seek(target:&Target,seconds:f64,duration:f64,valid:&dyn Fn()->bool)->Result<f64,String>{
 let seconds=crate::netease_control::bounded_target(seconds,duration)?;
 if !available(target){return Err("当前网易云界面版本尚未通过后台进度适配".into())}
 let _serial=Serial::enter()?;let _dpi=Dpi::enter();
 let (root,host,mut renderer)=windows(target.pid).ok_or("网易云窗口已变化，已取消进度操作")?;
 let mut original=RECT::default();unsafe{GetClientRect(host,&mut original)}.map_err(|e|e.to_string())?;
 let mut resize=None;
 if original.right==0||original.bottom==0{
  let mut placement=WINDOWPLACEMENT{length:std::mem::size_of::<WINDOWPLACEMENT>()as u32,..Default::default()};
  unsafe{GetWindowPlacement(root,&mut placement)}.map_err(|e|e.to_string())?;
  let width=placement.rcNormalPosition.right-placement.rcNormalPosition.left;let height=placement.rcNormalPosition.bottom-placement.rcNormalPosition.top;
  if bounds(width,height,unsafe{GetDpiForWindow(root)}.max(96),seconds/duration).is_none(){return Err("网易云保存的窗口尺寸无效，已取消后台操作".into())}
  resize=Some(ChildSize{pid:target.pid,root,host,width:original.right,height:original.bottom,iconic:unsafe{IsIconic(root)}.as_bool(),visible:unsafe{IsWindowVisible(root)}.as_bool()});
  // A minimized CEF host collapses to 0x0. Resize only that child; leave the
  // main window minimized/hidden. This recreates the renderer without showing it.
  unsafe{SetWindowPos(host,None,0,0,width,height,SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_ASYNCWINDOWPOS)}.map_err(|e|e.to_string())?;
  let until=Instant::now()+Duration::from_millis(900);
  while Instant::now()<until{
   if !valid(){return Err("歌曲已变化，已取消后台进度操作".into())}
   let Some((same,_,next))=windows(target.pid)else{return Err("网易云窗口已变化，已取消后台操作".into())};
   if same!=root{return Err("网易云窗口已变化，已取消后台操作".into())}
   if let Some(next)=next{let mut r=RECT::default();if unsafe{GetClientRect(next,&mut r)}.is_ok()&&r.right>width/2&&r.bottom>height/2{renderer=Some(next);break}}
   std::thread::sleep(Duration::from_millis(20));
  }
 }
 let renderer=renderer.filter(|w|owned(*w,target.pid)&&class(*w,"Chrome_RenderWidgetHostHWND")).ok_or("网易云后台播放控件未就绪，请稍后重试")?;
 let mut rect=RECT::default();unsafe{GetClientRect(renderer,&mut rect)}.map_err(|e|e.to_string())?;
 let (x,y)=bounds(rect.right,rect.bottom,unsafe{GetDpiForWindow(root)}.max(96),seconds/duration).ok_or("网易云进度控件尺寸不受支持")?;
 let pos=LPARAM(((y as u32)<<16|x as u32)as isize);
 if !valid(){return Err("歌曲已变化，已取消后台进度操作".into())}
 unsafe{
  PostMessageW(Some(renderer),WM_MOUSEMOVE,WPARAM(0),pos).map_err(|e|e.to_string())?;
  PostMessageW(Some(renderer),WM_LBUTTONDOWN,WPARAM(1),pos).map_err(|e|e.to_string())?;
  // Always enqueue button-up before waiting, even when the player is busy.
  PostMessageW(Some(renderer),WM_LBUTTONUP,WPARAM(0),pos).map_err(|e|e.to_string())?;
 }
 // Let the CEF event queue consume the paired messages before restoring 0x0.
 // The caller then confirms against a fresh native timeline, never a local clock.
 if resize.is_some(){std::thread::sleep(Duration::from_millis(180));}
 drop(resize);Ok(x as f64/rect.right as f64*duration)
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn physical_slider_coordinates_follow_dpi_and_client_size(){assert_eq!(bounds(1056,752,96,0.5),Some((528,671)));assert_eq!(bounds(1584,1128,144,0.5),Some((792,1006)));assert_eq!(bounds(1056,752,96,0.),Some((1,671)));assert_eq!(bounds(1056,752,96,1.),Some((1054,671)));}
 #[test]fn invalid_or_mini_player_geometry_never_produces_input(){for(w,h,d,f)in[(0,0,96,0.5),(300,100,96,0.5),(1000,30,96,0.5),(1000,800,96,f64::NAN),(1000,800,96,-0.1)]{assert!(bounds(w,h,d,f).is_none());}}
}

use windows::{core::BOOL,Win32::{Foundation::{HWND,LPARAM,RECT},UI::{WindowsAndMessaging::*,HiDpi::GetDpiForWindow}}};
unsafe extern "system" fn visit(h:HWND,p:LPARAM)->BOOL{
 let mut pid=0;GetWindowThreadProcessId(h,Some(&mut pid));if p.0!=0&&pid!=p.0 as u32{return BOOL(1)}
 let mut class=[0u16;256];let n=GetClassNameW(h,&mut class);let class=String::from_utf16_lossy(&class[..n.max(0)as usize]);
 if p.0!=0||class=="OrpheusBrowserHost"{
  let mut rect=RECT::default();let _=GetClientRect(h,&mut rect);let mut placement=WINDOWPLACEMENT{length:std::mem::size_of::<WINDOWPLACEMENT>()as u32,..Default::default()};let _=GetWindowPlacement(h,&mut placement);println!("{}",serde_json::json!({"hwnd":h.0 as usize,"class":class,"pid":pid,"w":rect.right,"h":rect.bottom,"normal":[placement.rcNormalPosition.left,placement.rcNormalPosition.top,placement.rcNormalPosition.right,placement.rcNormalPosition.bottom],"visible":IsWindowVisible(h).as_bool(),"minimized":IsIconic(h).as_bool(),"dpi":GetDpiForWindow(h)}));
  if p.0==0{let _=EnumChildWindows(Some(h),Some(visit),LPARAM(pid as isize));}
 }BOOL(1)
}
fn main(){unsafe{let _=windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);let _=EnumWindows(Some(visit),LPARAM(0));}}

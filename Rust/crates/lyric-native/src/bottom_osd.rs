//! A separate, non-activating system hint surface. It never replaces the island activity.
use std::{path::Path,time::{Instant,Duration}};
use windows::{core::{w,Result},Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,UI::{WindowsAndMessaging::*,HiDpi::*}}};
use crate::{renderer::{Renderer,MaterialTheme,Material},notifications::Notice};
pub struct Overlay{theme:MaterialTheme,hwnd:HWND,renderer:Option<Renderer>,notice:Option<Notice>,opacity:f64,from:f64,target:f64,began:Instant,until:Instant,animate:bool,dirty:bool,idle:Instant}
impl Overlay{
 pub fn new()->Self{let now=Instant::now();Self{theme:MaterialTheme::new(Material::Acrylic,false),hwnd:HWND::default(),renderer:None,notice:None,opacity:0.,from:0.,target:0.,began:now,until:now,animate:true,dirty:false,idle:now}}
 pub fn visible(&self)->bool{self.notice.is_some()&&(self.opacity>0.001||self.target>0.)}
 pub unsafe fn show(&mut self,notice:Notice,anchor:HWND,font:&Path,animate:bool,theme:MaterialTheme)->Result<()>{
  let dpi=GetDpiForWindow(anchor).max(96)as f32;let scale=dpi as f64/96.;let width=(272.*scale).round()as i32;let height=(80.*scale).round()as i32;
  let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};GetMonitorInfoW(MonitorFromWindow(anchor,MONITOR_DEFAULTTONEAREST),&mut info).ok()?;
  let work=info.rcWork;let x=work.left+(work.right-work.left-width)/2;let y=work.bottom-height-(40.*scale).round()as i32;
  if self.hwnd==HWND::default(){let instance=GetModuleHandleW(None)?;let class=w!("DotIsle.BottomOSD.v201");let wc=WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance.into(),lpszClassName:class,..Default::default()};RegisterClassW(&wc);
   self.hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP|WS_EX_TRANSPARENT,class,w!("点屿 · 系统状态"),WS_POPUP,x,y,width,height,None,None,Some(instance.into()),None)?;
  }
  SetWindowPos(self.hwnd,Some(HWND_TOPMOST),x,y,width,height,SWP_NOACTIVATE|SWP_SHOWWINDOW)?;
  if !self.renderer.as_ref().is_some_and(|r|r.width==width as u32&&r.height==height as u32&&(r.dpi-dpi).abs()<0.01){self.renderer=None;self.renderer=Some(Renderer::new(self.hwnd,width as u32,height as u32,dpi,font)?);if let Some(renderer)=&mut self.renderer{renderer.set_material_theme(&self.theme)?;}}
  self.set_theme(theme)?;
  self.notice=Some(notice);self.animate=animate;self.from=self.opacity;self.target=1.;self.began=Instant::now();self.until=self.began+Duration::from_millis(1600);self.dirty=true;self.tick()
 }
 pub unsafe fn set_theme(&mut self,theme:MaterialTheme)->Result<()>{
  if self.theme!=theme{if let Some(renderer)=&mut self.renderer{renderer.set_material_theme(&theme)?;}self.theme=theme;self.dirty=true;}Ok(())
 }
 pub unsafe fn clear(&mut self){self.notice=None;self.opacity=0.;self.target=0.;self.dirty=false;self.idle=Instant::now();if self.hwnd!=HWND::default(){let _=ShowWindow(self.hwnd,SW_HIDE);}}
 pub unsafe fn tick(&mut self)->Result<()>{
  if self.notice.is_none(){
   // The overlay keeps its own full wgpu + Vello renderer. Hints are rare, so it
   // is retired after half a hidden minute instead of holding a second GPU
   // context alive for the rest of the session; the next hint rebuilds it.
   if self.renderer.is_some()&&self.idle.elapsed()>=Duration::from_secs(30){self.renderer=None;}
   return Ok(())}let now=Instant::now();
  if now>=self.until&&self.target>0.{self.from=self.opacity;self.target=0.;self.began=now;}
  let t=if self.animate{(now.duration_since(self.began).as_secs_f64()/if self.target>0.{0.16}else{0.12}).min(1.)}else{1.};
  let value=self.from+(self.target-self.from)*(1.-(1.-t).powi(3));
  if (value-self.opacity).abs()>0.0001||self.dirty{self.opacity=value;self.dirty=false;if let(Some(renderer),Some(notice))=(&mut self.renderer,&self.notice){renderer.render_bottom_osd(notice,value,if self.animate{8.*(1.-value)}else{0.},crate::surface::Surface::expanded(self.theme.material,self.theme.cover_background),true)?;}}
  if t>=1.{self.opacity=self.target;if self.target==0.{self.clear();}}
  Ok(())
 }
}
impl Drop for Overlay{fn drop(&mut self){self.renderer=None;if self.hwnd!=HWND::default(){unsafe{let _=DestroyWindow(self.hwnd);}}}}
unsafe extern "system" fn proc(hwnd:HWND,message:u32,w:WPARAM,l:LPARAM)->LRESULT{match message{WM_NCHITTEST=>LRESULT(HTTRANSPARENT as isize),WM_MOUSEACTIVATE=>LRESULT(MA_NOACTIVATE as isize),WM_ERASEBKGND=>LRESULT(1),_=>DefWindowProcW(hwnd,message,w,l)}}

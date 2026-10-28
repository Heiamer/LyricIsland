//! The visible window and headless verification use the same hybrid backend.
use std::{sync::Arc,path::Path};
use vello::Scene;
use winit::window::Window;
pub struct Gpu{capture:Option<dotisle_render::surface::Offscreen>}
impl Gpu{
 pub fn new()->Self{Self{capture:None}}
 pub fn surface(&mut self,window:Arc<Window>)->Result<dotisle_render::surface::Surface,String>{use raw_window_handle::{HasWindowHandle,RawWindowHandle};let handle=window.window_handle().map_err(|e|e.to_string())?;let RawWindowHandle::Win32(h)=handle.as_raw()else{return Err("需要 Windows 窗口".into())};let size=window.inner_size();unsafe{dotisle_render::surface::Surface::new(windows::Win32::Foundation::HWND(h.hwnd.get()as _),size.width,size.height)}}
 pub fn draw(&mut self,surface:&mut dotisle_render::surface::Surface,scene:&Scene,scale:f64)->Result<(),String>{surface.draw(scene,scale,true)}
 pub fn capture(&mut self,scene:&Scene,w:u32,h:u32,path:&Path)->Result<(),String>{
  if self.capture.is_none(){self.capture=Some(dotisle_render::surface::Offscreen::new(w,h)?);}
  self.capture.as_mut().unwrap().capture(scene,w,h,path)
 }
}

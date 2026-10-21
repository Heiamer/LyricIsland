//! Ephemeral luminance sampling of two tiny transparent gutters, never stored.
use std::time::{Duration,Instant};
use windows::Win32::{Foundation::{HWND,POINT},Graphics::Gdi::*,UI::HiDpi::GetDpiForWindow};
use crate::layout::Rect;

fn luminance(r:u8,g:u8,b:u8)->f64{let linear=|v:u8|{let s=v as f64/255.;if s<=0.04045{s/12.92}else{((s+0.055)/1.055).powf(2.4)}};0.2126*linear(r)+0.7152*linear(g)+0.0722*linear(b)}
fn dark_text(luma:f64,current:bool)->bool{if luma>=0.22{true}else if luma<=0.13{false}else{current}}

pub struct Adaptive{
    checked:Instant,pending:Option<bool>,pub sampled:Option<f64>,pub dark:bool,mix:f64,
}
impl Default for Adaptive{fn default()->Self{Self{checked:Instant::now()-Duration::from_secs(1),pending:None,sampled:None,dark:false,mix:0.}}}
impl Adaptive{
    pub fn ink(&self)->u32{0xff000000|crate::cover_color::mix(0xf4f6f8,0x10141c,self.mix)}
    pub fn outline(&self)->u32{if self.mix>0.5{0xc0ffffff}else{0xe0000000}}
    pub fn moving(&self)->bool{(self.mix-if self.dark{1.}else{0.}).abs()>0.002}
    pub fn reset(&mut self){self.dark=false;self.pending=None;self.sampled=None;}
    fn accept(&mut self,luma:f64){
        let next=dark_text(luma,self.dark);
        if self.sampled.is_none()||self.pending==Some(next){self.dark=next;self.pending=None;}else if next!=self.dark{self.pending=Some(next);}else{self.pending=None;}
        self.sampled=Some(luma);
    }
    pub unsafe fn refresh(&mut self,hwnd:HWND,capsule:Rect){
        if self.checked.elapsed()<Duration::from_millis(250){return}self.checked=Instant::now();
        if let Some(luma)=sample_gutters(hwnd,capsule){self.accept(luma);}
    }
    pub fn tick(&mut self,seconds:f64,animate:bool)->bool{
        let target=if self.dark{1.}else{0.};let before=self.mix;
        self.mix=if animate{self.mix+(target-self.mix)*(1.-(-seconds.min(0.1)*28.).exp())}else{target};
        if (self.mix-target).abs()<0.002{self.mix=target;}(before-self.mix).abs()>0.00001
    }
}

unsafe fn sample_gutters(hwnd:HWND,r:Rect)->Option<f64>{
    if r.w<60.||r.h<12.{return None}
    let scale=GetDpiForWindow(hwnd).max(96)as f64/96.;let mut origin=POINT::default();if !ClientToScreen(hwnd,&mut origin).as_bool(){return None}
    let pw=(3.*scale).ceil()as i32;let h=(8.*scale).ceil()as i32;let w=pw*2;
    let top=origin.y+((r.y+(r.h-8.)/2.)*scale).round()as i32;
    let left=origin.x+((r.x+17.)*scale).round()as i32;
    let right=origin.x+((r.x+r.w-20.)*scale).round()as i32;
    let screen=GetDC(None);if screen.is_invalid(){return None}
    let memory=CreateCompatibleDC(Some(screen));if memory.is_invalid(){ReleaseDC(None,screen);return None}
    let info=BITMAPINFO{bmiHeader:BITMAPINFOHEADER{biSize:std::mem::size_of::<BITMAPINFOHEADER>()as u32,biWidth:w,biHeight:-h,biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()},..Default::default()};
    let mut bits=std::ptr::null_mut();let bitmap=match CreateDIBSection(Some(screen),&info,DIB_RGB_COLORS,&mut bits,None,0){Ok(v)=>v,Err(_)=>{let _=DeleteDC(memory);ReleaseDC(None,screen);return None}};
    let old=SelectObject(memory,bitmap.into());
    // Text and the dot never occupy these symmetric 3-DIP gutters. This avoids
    // sampling our own foreground even when the compositor includes layered HWNDs.
    let result=if !bits.is_null()&&BitBlt(memory,0,0,pw,h,Some(screen),left,top,SRCCOPY).is_ok()&&BitBlt(memory,pw,0,pw,h,Some(screen),right,top,SRCCOPY).is_ok()&&GdiFlush().as_bool(){
        let bytes=std::slice::from_raw_parts(bits as *const u8,(w*h*4)as usize);
        let mut values=bytes.chunks_exact(4).map(|v|luminance(v[2],v[1],v[0])).collect::<Vec<_>>();values.sort_by(f64::total_cmp);Some(values[values.len()/2])
    }else{None};
    SelectObject(memory,old);let _=DeleteObject(bitmap.into());let _=DeleteDC(memory);ReleaseDC(None,screen);result
}

#[cfg(test)]mod tests{use super::*;
    #[test]fn black_and_white_backgrounds_select_opposite_ink(){assert!(dark_text(luminance(255,255,255),false));assert!(!dark_text(luminance(0,0,0),true));for current in [false,true]{assert_eq!(dark_text(0.17,current),current);}}
    #[test]fn changing_background_requires_confirmation_and_settles_without_flicker(){let mut a=Adaptive::default();a.accept(0.95);assert!(a.dark);a.tick(1.,false);assert_eq!(a.ink(),0xff10141c);a.accept(0.01);assert!(a.dark);a.accept(0.9);assert!(a.dark);a.accept(0.01);a.accept(0.01);assert!(!a.dark);for _ in 0..120{a.tick(1./120.,true);}assert_eq!(a.ink(),0xfff4f6f8);assert!(!a.moving());}
}

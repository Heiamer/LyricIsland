//! Local edge-only background sampling for glass refraction. No screen image is saved.
//! Only samples outside the glass outline contribute; foreground controls are never warped.
use std::{sync::{Arc,mpsc},time::{Duration,Instant}};
use windows::Win32::{Foundation::{HWND,POINT},Graphics::Gdi::*};
use vello::peniko::{ImageData,ImageFormat,ImageAlphaType,Blob};
use crate::layout::Rect;
#[derive(Clone,Copy)]struct Request{x:i32,y:i32,w:u32,h:u32,pad:f64,width:f64,height:f64,radius:f64,scale:f64}
struct Raster{pixels:Vec<u8>,w:u32,h:u32}
fn image(r:Raster)->ImageData{ImageData{data:Blob::new(Arc::new(r.pixels)),format:ImageFormat::Rgba8,alpha_type:ImageAlphaType::Alpha,width:r.w,height:r.h}}
fn sdf(x:f64,y:f64,w:f64,h:f64,r:f64)->f64{let r=r.max(0.).min(w.min(h)/2.);let qx=(x-w/2.).abs()-(w/2.-r);let qy=(y-h/2.).abs()-(h/2.-r);qx.max(0.).hypot(qy.max(0.))+qx.max(qy).min(0.)-r}
/// Returns an exterior source point and opacity for an interior edge pixel.
fn lens(x:f64,y:f64,w:f64,h:f64,r:f64,scale:f64)->Option<(f64,f64,f64,f64,f64)>{
 let distance=sdf(x,y,w,h,r);let rim=(1.2*scale).max(1.).min(w.min(h)*0.10);if distance>0.||distance < -rim{return None}
 let nx=sdf(x+0.5,y,w,h,r)-sdf(x-0.5,y,w,h,r);let ny=sdf(x,y+0.5,w,h,r)-sdf(x,y-0.5,w,h,r);let length=nx.hypot(ny).max(1e-8);let(nx,ny)=(nx/length,ny/length);
 let u=(-distance/rim).clamp(0.,1.);let bend=(1.2+1.8*(1.-u).powi(2))*scale;let travel=-distance+bend;
 let opacity=(std::f64::consts::PI*u).sin().max(0.).powf(0.65)*0.32;
 Some((x+nx*travel,y+ny*travel,opacity,nx,ny))
}
fn refract(req:Request,source:&[u8])->Raster{
 let w=req.width.ceil().max(1.)as u32;let h=req.height.ceil().max(1.)as u32;let mut out=vec![0u8;(w*h*4)as usize];
 let sample=|x:f64,y:f64,c:usize|{let x=(x+req.pad).round().clamp(0.,req.w as f64-1.)as usize;let y=(y+req.pad).round().clamp(0.,req.h as f64-1.)as usize;source[(y*req.w as usize+x)*4+c]as f64};
 for y in 0..h{for x in 0..w{let Some((sx,sy,alpha,nx,ny))=lens(x as f64+0.5,y as f64+0.5,req.width,req.height,req.radius,req.scale)else{continue};let index=((y*w+x)*4)as usize;
  // A fraction of a physical pixel separates wavelengths; the middle remains transparent.
  let fringe=0.12*req.scale;let light=1.+(-ny*0.035+nx*0.012)*(1.-alpha);
  out[index]=(sample(sx+nx*fringe,sy+ny*fringe,2)*light).clamp(0.,255.)as u8;
  out[index+1]=(sample(sx,sy,1)*light).clamp(0.,255.)as u8;
  out[index+2]=(sample(sx-nx*fringe,sy-ny*fringe,0)*light).clamp(0.,255.)as u8;
  out[index+3]=(alpha*255.).round()as u8;
 }}Raster{pixels:out,w,h}
}
unsafe fn capture(req:Request)->Option<Raster>{
 let screen=GetDC(None);if screen.is_invalid(){return None}let memory=CreateCompatibleDC(Some(screen));if memory.is_invalid(){ReleaseDC(None,screen);return None}
 let info=BITMAPINFO{bmiHeader:BITMAPINFOHEADER{biSize:std::mem::size_of::<BITMAPINFOHEADER>()as u32,biWidth:req.w as i32,biHeight:-(req.h as i32),biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()},..Default::default()};let mut bits=std::ptr::null_mut();
 let bitmap=match CreateDIBSection(Some(screen),&info,DIB_RGB_COLORS,&mut bits,None,0){Ok(b)=>b,Err(_)=>{let _=DeleteDC(memory);ReleaseDC(None,screen);return None}};let old=SelectObject(memory,bitmap.into());
 let result=if !bits.is_null()&&BitBlt(memory,0,0,req.w as i32,req.h as i32,Some(screen),req.x,req.y,SRCCOPY).is_ok()&&GdiFlush().as_bool(){Some(refract(req,std::slice::from_raw_parts(bits as *const u8,(req.w*req.h*4)as usize)))}else{None};
 SelectObject(memory,old);let _=DeleteObject(bitmap.into());let _=DeleteDC(memory);ReleaseDC(None,screen);result
}
fn pattern(x:i32,y:i32)->[u8;4]{let stripe=if(x/28+y/36).rem_euclid(2)==0{1.}else{0.};let t=(x as f64/900.).clamp(0.,1.);[(65.+85.*t+30.*stripe)as u8,(100.+65.*stripe)as u8,(190.-95.*t+25.*stripe)as u8,255]}
fn fixture(req:Request)->Raster{let mut source=vec![0;(req.w*req.h*4)as usize];for y in 0..req.h{for x in 0..req.w{let p=pattern(req.x+x as i32,req.y+y as i32);let i=((y*req.w+x)*4)as usize;source[i..i+4].copy_from_slice(&[p[2],p[1],p[0],255]);}}refract(req,&source)}
pub struct Sampler{pub captures:u64,tx:Option<mpsc::SyncSender<Request>>,rx:Option<mpsc::Receiver<Option<Raster>>>,at:Instant,busy:bool,texture:Option<ImageData>,fixture:Option<ImageData>}
impl Default for Sampler{fn default()->Self{Self{captures:0,tx:None,rx:None,at:Instant::now()-Duration::from_secs(1),busy:false,texture:None,fixture:None}}}
impl Sampler{
 pub fn enable_fixture(&mut self,width:u32,height:u32){let mut pixels=Vec::with_capacity((width*height*4)as usize);for y in 0..height{for x in 0..width{pixels.extend_from_slice(&pattern(x as i32,y as i32));}}self.fixture=Some(image(Raster{pixels,w:width,h:height}));}
 pub fn background(&self)->Option<ImageData>{self.fixture.clone()}
 pub fn poll(&mut self)->bool{let mut changed=false;if let Some(rx)=&self.rx{while let Ok(result)=rx.try_recv(){self.busy=false;if let Some(r)=result{self.texture=Some(image(r));self.captures+=1;changed=true;}}}changed}
 pub fn due(&self)->bool{!self.busy&&self.at.elapsed()>=Duration::from_millis(125)}
 pub fn image(&mut self,hwnd:HWND,r:Rect,radius:f64,dpi:f32,live:bool)->Option<ImageData>{
  self.poll();
  let scale=dpi as f64/96.;let width=(r.w*scale).max(1.);let height=(r.h*scale).max(1.);if width>1600.||height>1000.{return None}let pad=(16.*scale).ceil();
  let mut origin=POINT::default();if live&&unsafe{!ClientToScreen(hwnd,&mut origin).as_bool()}{return self.texture.clone()}
  let request=Request{x:origin.x+(r.x*scale).floor()as i32-pad as i32,y:origin.y+(r.y*scale).floor()as i32-pad as i32,w:width.ceil()as u32+pad as u32*2,h:height.ceil()as u32+pad as u32*2,pad,width,height,radius:radius*scale,scale};
  if self.fixture.is_some(){return Some(image(fixture(request)))}
  // Capture is bounded and off the UI thread. During morphs the last edge texture stretches
  // with the shell; a stable outline gets a fresh exterior-only sample at up to 8 Hz.
  if live&&!self.busy&&self.at.elapsed()>=Duration::from_millis(125){
   if self.tx.is_none(){let(tx,requests)=mpsc::sync_channel::<Request>(1);let(results,rx)=mpsc::channel();std::thread::Builder::new().name("glass-edge".into()).spawn(move||{while let Ok(req)=requests.recv(){let result=unsafe{capture(req)};if results.send(result).is_err(){break}}}).ok()?;self.tx=Some(tx);self.rx=Some(rx);}
   if self.tx.as_ref().unwrap().try_send(request).is_ok(){self.busy=true;self.at=Instant::now();}
  }
  self.texture.clone()
 }
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn lens_only_samples_outside_and_keeps_the_content_center_clear(){for(w,h,r)in[(400.,240.,30.),(260.,40.,20.),(260.,20.,10.)]{assert!(lens(w/2.,h/2.,w,h,r,1.).is_none());for y in 0..h as i32{for x in 0..w as i32{if let Some((sx,sy,alpha,_,_))=lens(x as f64+0.5,y as f64+0.5,w,h,r,1.){assert!(sdf(sx,sy,w,h,r)>0.8);assert!((0. ..=0.53).contains(&alpha));}}}}}
 #[test]fn deterministic_fixture_has_transparent_center_and_visible_refracted_rim(){let req=Request{x:0,y:0,w:432,h:272,pad:16.,width:400.,height:240.,radius:30.,scale:1.};let image=fixture(req);assert_eq!(image.pixels[((120*400+200)*4+3)as usize],0);assert!(image.pixels.chunks_exact(4).any(|p|p[3]>60));assert!(image.pixels.chunks_exact(4).filter(|p|p[3]>0).count()<400*240/3);}
}

pub unsafe fn preview(a:&mut crate::App,out:&std::path::Path)->Result<(),Box<dyn std::error::Error>>{
 use serde_json::json;std::fs::create_dir_all(out)?;a.renderer=None;a.opacity=1.;a.expanded=true;a.frame=crate::motion::Frame::expanded();a.layout=crate::layout::Layout::new(260.);a.layout.card.h=236.;a.music_mix=0.;a.settings["IslandMaterial"]=json!("Glass");a.settings["ExpandedCoverOverlay"]=json!(false);let mut captures=vec![];
 for dpi in [96.,144.,192.]{let scale=dpi/96.;let mut r=crate::renderer::Renderer::new(a.visual_hwnd,(a.layout.host.w*scale)as u32,(a.layout.host.h*scale)as u32,dpi as f32,&a.base.join("Assets/Fonts/NotoSansSC.ttf"))?;r.set_cover(Some(&std::fs::read(a.base.join("Assets/DemoCover.png"))?))?;r.glass_fixture();
  for enabled in [false,true]{let mut s=a.scene();s.layout_scale=scale;s.glass_refraction=enabled;s.title="玻璃边缘折射";s.artist="背景纹理沿外缘偏折";s.lyric="中央文字与按钮保持清晰";s.lyric_age_ms=1000.;s.time_ms=0.;s.play_shape=1.;r.render(&s,false)?;let file=format!("glass-{dpi:.0}-{}.png",if enabled{"on"}else{"off"});r.snapshot(&out.join(&file))?;captures.push(file);}
 }
 std::fs::write(out.join("report.json"),serde_json::to_vec_pretty(&json!({"success":true,"captures":captures,"scope":"Production optical compositor with deterministic exterior texture; no desktop capture in fixture mode. Live GDI sampling is verified separately."}))?)?;Ok(())
}

//! Windows native HTTP, always WINHTTP_ACCESS_TYPE_NO_PROXY. No PAC/environment fallback.
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration,Instant};

type Handle = *mut c_void;
#[link(name="winhttp")]
extern "system" {
    fn WinHttpOpen(agent:*const u16,access:u32,proxy:*const u16,bypass:*const u16,flags:u32)->Handle;
    fn WinHttpConnect(session:Handle,server:*const u16,port:u16,reserved:u32)->Handle;
    fn WinHttpOpenRequest(connection:Handle,verb:*const u16,path:*const u16,version:*const u16,refer:*const u16,accept:*const *const u16,flags:u32)->Handle;
    fn WinHttpSetTimeouts(handle:Handle,resolve:i32,connect:i32,send:i32,receive:i32)->i32;
    fn WinHttpSetOption(handle:Handle,option:u32,value:*const c_void,length:u32)->i32;
    fn WinHttpSendRequest(handle:Handle,headers:*const u16,header_len:u32,body:*mut c_void,body_len:u32,total:u32,context:usize)->i32;
    fn WinHttpReceiveResponse(handle:Handle,reserved:*mut c_void)->i32;
    fn WinHttpQueryHeaders(handle:Handle,level:u32,name:*const u16,buffer:*mut c_void,len:*mut u32,index:*mut u32)->i32;
    fn WinHttpReadData(handle:Handle,buffer:*mut c_void,len:u32,read:*mut u32)->i32;
    fn WinHttpCloseHandle(handle:Handle)->i32;
}
struct Owned(Handle);
impl Owned { fn new(h:Handle)->Result<Self,String> { if h.is_null(){Err(std::io::Error::last_os_error().to_string())}else{Ok(Self(h))} } }
impl Drop for Owned { fn drop(&mut self){unsafe{WinHttpCloseHandle(self.0);}} }
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
#[derive(Clone)]
pub struct RequestContext { pub cancelled:Arc<AtomicBool>, pub deadline:Instant }
impl RequestContext {
    pub fn new(timeout:Duration)->Self{Self{cancelled:Arc::new(AtomicBool::new(false)),deadline:Instant::now()+timeout}}
    pub fn check(&self)->Result<(),String>{if self.cancelled.load(Ordering::Acquire){Err("cancelled".into())}else if Instant::now()>self.deadline{Err("timeout".into())}else{Ok(())}}
}
pub fn encode(s:&str)->String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {(b as char).to_string()}else{format!("%{b:02X}")}).collect() }
pub fn request(url:&str,method:&str,body:&[u8],content_type:&str,refer:&str,ctx:&RequestContext,limit:usize)->Result<Vec<u8>,String>{
    ctx.check()?;
    let (scheme,rest)=url.split_once("://").ok_or("invalid URL")?;
    if scheme!="https" && scheme!="http"{return Err("unsupported URL".into())}
    let (authority,path)=rest.split_once('/').map(|(a,p)|(a,format!("/{p}"))).unwrap_or((rest,"/".into()));
    if authority.contains('@') || authority.is_empty(){return Err("invalid host".into())}
    let (host,port)=authority.split_once(':').map(|(h,p)|Ok::<_,String>((h,p.parse::<u16>().map_err(|_|"invalid port")?))).transpose()?.unwrap_or((authority,if scheme=="https"{443}else{80}));
    let agent=wide("Mozilla/5.0 (Windows NT 10.0; Win64; x64) LyricIsland/1.0");
    unsafe{
        let session=Owned::new(WinHttpOpen(agent.as_ptr(),1,null(),null(),0))?; // NO_PROXY = 1
        let remaining=ctx.deadline.saturating_duration_since(Instant::now()).as_millis().clamp(1,5000) as i32;
        WinHttpSetTimeouts(session.0,remaining.min(2000),remaining.min(2000),remaining.min(2000),remaining.min(1500));
        let decompression=3u32;WinHttpSetOption(session.0,118,&decompression as *const _ as _,4);
        let host=wide(host);let connection=Owned::new(WinHttpConnect(session.0,host.as_ptr(),port,0))?;
        let path=wide(&path);let method=wide(method);let refer_w=wide(refer);
        let request=Owned::new(WinHttpOpenRequest(connection.0,method.as_ptr(),path.as_ptr(),null(),if refer.is_empty(){null()}else{refer_w.as_ptr()},null(),if scheme=="https"{0x800000}else{0}))?;
        let headers=wide(&format!("Content-Type: {content_type}\r\nAccept: */*\r\n"));
        if WinHttpSendRequest(request.0,headers.as_ptr(),u32::MAX,if body.is_empty(){null_mut()}else{body.as_ptr() as _},body.len() as u32,body.len() as u32,0)==0{return Err(std::io::Error::last_os_error().to_string())}
        ctx.check()?;
        if WinHttpReceiveResponse(request.0,null_mut())==0{return Err(std::io::Error::last_os_error().to_string())}
        let mut status=0u32;let mut len=4;
        if WinHttpQueryHeaders(request.0,19|0x20000000,null(),&mut status as *mut _ as _,&mut len,null_mut())==0 || !(200..300).contains(&status){return Err(format!("HTTP {status}"))}
        let mut output=Vec::new();let mut buffer=[0u8;65536];
        loop{
            ctx.check()?;let mut count=0;
            if WinHttpReadData(request.0,buffer.as_mut_ptr() as _,buffer.len() as u32,&mut count)==0{return Err(std::io::Error::last_os_error().to_string())}
            if count==0{break}if output.len()+count as usize>limit{return Err("response exceeds limit".into())}
            output.extend_from_slice(&buffer[..count as usize]);
        }
        Ok(output)
    }
}
pub fn get(url:&str,ctx:&RequestContext)->Result<serde_json::Value,String>{serde_json::from_slice(&request(url,"GET",&[],"application/json","",ctx,8*1024*1024)?).map_err(|e|e.to_string())}
pub fn post(url:&str,body:&str,content_type:&str,refer:&str,ctx:&RequestContext)->Result<serde_json::Value,String>{serde_json::from_slice(&request(url,"POST",body.as_bytes(),content_type,refer,ctx,8*1024*1024)?).map_err(|e|e.to_string())}
#[cfg(test)] mod tests{use super::*;#[test]fn url_encoding_is_utf8_and_never_injects_query(){assert_eq!(encode("a&中 "),"a%26%E4%B8%AD%20");}#[test]fn expired_and_cancelled_never_open_network(){let c=RequestContext::new(Duration::ZERO);assert!(request("https://invalid.test","GET",&[],"","",&c,10).is_err());}}


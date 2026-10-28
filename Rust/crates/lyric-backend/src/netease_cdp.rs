//! Local-only NetEase seek. Uses existing React provider state; no hooks or activation commands.
use std::{ffi::c_void,ptr::{null,null_mut},sync::{Arc,atomic::{AtomicUsize,Ordering},mpsc},time::{Duration,Instant}};
use serde_json::{Value,json};

pub const DEFAULT_PORT:u16=9223;
const PAGE:&str="orpheus://orpheus/pub/app.html";
const SCRIPT:&str=include_str!("netease_seek.js");
type Handle=*mut c_void;
#[link(name="iphlpapi")]extern "system"{fn GetExtendedTcpTable(table:*mut c_void,size:*mut u32,ordered:i32,family:u32,class:u32,reserved:u32)->u32;}
#[link(name="winhttp")]extern "system"{
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
 fn WinHttpWebSocketCompleteUpgrade(request:Handle,context:usize)->Handle;
 fn WinHttpWebSocketSend(socket:Handle,kind:u32,buffer:*const c_void,length:u32)->u32;
 fn WinHttpWebSocketReceive(socket:Handle,buffer:*mut c_void,length:u32,read:*mut u32,kind:*mut u32)->u32;
}
struct Owned(Arc<AtomicUsize>);
impl Owned{
 fn new(handle:Handle)->Result<Self,String>{if handle.is_null(){Err(native_error())}else{Ok(Self(Arc::new(AtomicUsize::new(handle as usize))))}}
 fn handle(&self)->Handle{self.0.load(Ordering::Acquire)as Handle}
}
fn close(handle:&AtomicUsize){let raw=handle.swap(0,Ordering::AcqRel);if raw!=0{unsafe{WinHttpCloseHandle(raw as Handle);}}}
impl Drop for Owned{fn drop(&mut self){close(&self.0)}}
struct Deadline{stop:Option<mpsc::Sender<()>>,thread:Option<std::thread::JoinHandle<()>>}
impl Deadline{
 fn new(handle:&Owned,timeout:Duration)->Self{
  let(raw_stop,stopped)=mpsc::channel();let raw=handle.0.clone();
  let thread=std::thread::spawn(move||{if stopped.recv_timeout(timeout)==Err(mpsc::RecvTimeoutError::Timeout){close(&raw)}});
  Self{stop:Some(raw_stop),thread:Some(thread)}
 }
}
impl Drop for Deadline{fn drop(&mut self){if let Some(stop)=self.stop.take(){let _=stop.send(());}if let Some(thread)=self.thread.take(){let _=thread.join();}}}
fn native_error()->String{format!("本机静默通道不可用：{}",std::io::Error::last_os_error())}
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}

/// Refuse a foreign process or a CDP listener exposed beyond IPv4 loopback.
pub fn available(pid:u32,port:u16)->bool{
 let Some((ipv4,bytes))=tcp_table(2)else{return false};
 let Some((ipv6,bytes6))=tcp_table(23)else{return false};
 listener_matches(&ipv4,bytes,pid,port)&&ipv6_port_absent(&ipv6,bytes6,port)
}
/// Any IPv4 process listening on the configured loopback port, for a clear conflict message.
pub fn ipv4_listener_pid(port:u16)->Option<u32>{
 let (table,bytes)=tcp_table(2)?;let count=*table.first()? as usize;if bytes>table.len()*4||count>bytes.saturating_sub(4)/24{return None}
 for row in table[1..].chunks_exact(6).take(count){if u16::from_be(row[2]as u16)==port{return Some(row[5])}}
 None
}
fn tcp_table(family:u32)->Option<(Vec<u32>,usize)>{unsafe{
 let mut size=0;let status=GetExtendedTcpTable(null_mut(),&mut size,0,family,3,0);
 if status!=122||!(4..=1024*1024).contains(&size){return None}
 let mut table=vec![0u32;(size as usize).div_ceil(4)];
 if GetExtendedTcpTable(table.as_mut_ptr()as _,&mut size,0,family,3,0)!=0{return None}
 Some((table,size as usize))
}}
fn ipv6_port_absent(table:&[u32],bytes:usize,port:u16)->bool{
 let Some(&count)=table.first()else{return false};if bytes>table.len()*4||count as usize>bytes.saturating_sub(4)/56{return false}
 table[1..].chunks_exact(14).take(count as usize).all(|row|u16::from_be(row[5]as u16)!=port)
}
fn listener_matches(table:&[u32],bytes:usize,pid:u32,port:u16)->bool{
 let Some(&count)=table.first()else{return false};if bytes>table.len()*4||count as usize>(bytes.saturating_sub(4))/24{return false}
 let mut matched=false;
 for row in table[1..].chunks_exact(6).take(count as usize){if u16::from_be(row[2]as u16)!=port{continue}if row[1]!=u32::from_ne_bytes([127,0,0,1])||row[5]!=pid{return false}matched=true;}
 matched
}
fn websocket_path(url:&str,port:u16)->Result<&str,String>{
 let prefix=format!("ws://127.0.0.1:{port}");let path=url.strip_prefix(&prefix).ok_or("静默通道返回了非本机地址")?;
 let id=path.strip_prefix("/devtools/page/").ok_or("静默通道页面地址无效")?;
 if id.is_empty()||id.len()>128||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err("静默通道页面标识无效".into())}Ok(path)
}
fn select_target(list:&Value,port:u16)->Result<&str,String>{
 let pages=list.as_array().filter(|p|p.len()<=64).ok_or("静默通道页面列表无效")?;
 let mut targets=pages.iter().filter(|p|p["type"]=="page"&&p["url"].as_str().is_some_and(|url|url.split(['?','#']).next()==Some(PAGE)));
 let first=targets.next().ok_or("未找到网易云播放页面，请等待客户端初始化")?;
 if targets.next().is_some(){return Err("存在多个网易云播放页面，已取消进度操作".into())}
 websocket_path(first["webSocketDebuggerUrl"].as_str().ok_or("静默通道缺少页面地址")?,port)
}
fn expression(song_id:&str,seconds:f64)->Result<String,String>{
 if song_id.is_empty()||song_id.len()>32||!song_id.bytes().all(|b|b.is_ascii_digit())||!seconds.is_finite()||!(0.0..=86400.).contains(&seconds){return Err("网易云歌曲或进度无效".into())}
 Ok(format!("{SCRIPT}({}, {}, true)",serde_json::to_string(song_id).map_err(|e|e.to_string())?,seconds))
}
fn parse_reply(value:&Value,song_id:&str)->Result<(),String>{
 if value.get("error").is_some()||value["result"].get("exceptionDetails").is_some(){
  let detail=value["result"]["exceptionDetails"]["exception"]["description"].as_str().unwrap_or("");
  return Err(if detail.contains("track_changed"){"歌曲已变化，已取消旧歌曲的进度操作"}else{"网易云播放页面尚未就绪或客户端不兼容，请稍后重试"}.into())
 }
 let result=&value["result"]["result"];
 if result["type"]!="object"||result["value"]["dispatched"]!=true||result["value"]["trackId"]!=song_id||result["value"]["playId"].as_str().is_none_or(str::is_empty){return Err("网易云静默进度响应无效".into())}Ok(())
}
unsafe fn open_request(connection:&Owned,path:&str,upgrade:bool)->Result<Owned,String>{
 let get=wide("GET");let path=wide(path);
 let request=Owned::new(WinHttpOpenRequest(connection.handle(),get.as_ptr(),path.as_ptr(),null(),null(),null(),0))?;
 let never_redirect=0u32;
 if WinHttpSetOption(request.handle(),88,&never_redirect as *const _ as _,4)==0{return Err(native_error())}
 if upgrade&&WinHttpSetOption(request.handle(),114,null(),0)==0{return Err(native_error())}
 if WinHttpSendRequest(request.handle(),null(),0,null_mut(),0,0,0)==0||WinHttpReceiveResponse(request.handle(),null_mut())==0{return Err(native_error())}
 let mut status=0u32;let mut size=4;
 if WinHttpQueryHeaders(request.handle(),19|0x20000000,null(),&mut status as *mut _ as _,&mut size,null_mut())==0||status!=if upgrade{101}else{200}{return Err(format!("本机静默通道 HTTP {status}"))}Ok(request)
}
/// True only when the loopback listener belongs to pid and its player page is exposed.
pub fn page_ready(pid:u32,port:u16)->bool{
 if !available(pid,port){return false}
 unsafe{
  let agent=wide("DotIsle/3.1.3 LocalMusicControl");let host=wide("127.0.0.1");
  let Ok(session)=Owned::new(WinHttpOpen(agent.as_ptr(),1,null(),null(),0))else{return false};
  if WinHttpSetTimeouts(session.handle(),300,300,300,300)==0{return false}
  let Ok(connection)=Owned::new(WinHttpConnect(session.handle(),host.as_ptr(),port,0))else{return false};
  let Ok(list)=open_request(&connection,"/json/list",false)else{return false};
  let mut body=Vec::new();
  for _ in 0..16{let mut data=[0u8;4096];let mut read=0;if WinHttpReadData(list.handle(),data.as_mut_ptr()as _,4096,&mut read)==0{return false}if read==0{break}if body.len()+read as usize>65536{return false}body.extend_from_slice(&data[..read as usize]);}
  let Ok(targets)=serde_json::from_slice::<Value>(&body)else{return false};
  select_target(&targets,port).is_ok()
 }
}
pub fn seek(pid:u32,port:u16,song_id:&str,seconds:f64)->Result<(),String>{
 let command=expression(song_id,seconds)?;parse_reply(&evaluate(pid,port,&command)?,song_id)
}
/// Read the actual player store. An open TCP port alone is not seek capability.
pub fn probe(pid:u32,port:u16,song_id:&str)->bool{
 let Ok(command)=expression(song_id,0.)else{return false};
 let command=command.trim_end_matches(", true)").to_owned()+", false)";
 evaluate(pid,port,&command).is_ok_and(|v|v.get("error").is_none()&&v["result"].get("exceptionDetails").is_none()&&v["result"]["result"]["value"]["dispatched"]==false&&v["result"]["result"]["value"]["trackId"]==song_id&&v["result"]["result"]["value"]["playId"].as_str().is_some_and(|p|!p.is_empty()))
}
fn evaluate(pid:u32,port:u16,expression:&str)->Result<Value,String>{
 if !available(pid,port){return Err(format!("网易云静默控制未开启；请在设置中确认端口 {port}，不会回退到弹窗命令"))}
 let until=Instant::now()+Duration::from_millis(2200);
 unsafe{
  let agent=wide("DotIsle/3.1.1 LocalMusicControl");let host=wide("127.0.0.1");
  let session=Owned::new(WinHttpOpen(agent.as_ptr(),1,null(),null(),0))?;
  if WinHttpSetTimeouts(session.handle(),500,500,500,500)==0{return Err(native_error())}
  let connection=Owned::new(WinHttpConnect(session.handle(),host.as_ptr(),port,0))?;
  let list=open_request(&connection,"/json/list",false)?;let mut body=Vec::new();
  loop{if Instant::now()>=until{return Err("本机静默通道超时".into())}let mut data=[0u8;4096];let mut read=0;if WinHttpReadData(list.handle(),data.as_mut_ptr()as _,4096,&mut read)==0{return Err(native_error())}if read==0{break}if body.len()+read as usize>65536{return Err("本机页面列表过大".into())}body.extend_from_slice(&data[..read as usize]);}
  let targets:Value=serde_json::from_slice(&body).map_err(|_|"本机页面列表无效")?;let path=select_target(&targets,port)?;
  if !available(pid,port){return Err("网易云控制进程已变化，已取消操作".into())}
  let request=open_request(&connection,path,true)?;
  let socket=Owned::new(WinHttpWebSocketCompleteUpgrade(request.handle(),0))?;drop(request);
  let remaining=until.saturating_duration_since(Instant::now());if remaining.is_zero(){return Err("本机静默通道超时".into())}
  let _deadline=Deadline::new(&socket,remaining);
  let command=serde_json::to_vec(&json!({"id":1,"method":"Runtime.evaluate","params":{"expression":expression,"returnByValue":true,"awaitPromise":false,"timeout":800}})).map_err(|e|e.to_string())?;
  let status=WinHttpWebSocketSend(socket.handle(),2,command.as_ptr()as _,command.len()as u32);if status!=0{return Err(format!("静默进度发送失败：{status}"))}
  let mut message=Vec::new();let mut unrelated=0;
  loop{
   if Instant::now()>=until{return Err("本机静默通道超时".into())}
   let mut buffer=[0u8;8192];let mut read=0;let mut kind=0;
   let status=WinHttpWebSocketReceive(socket.handle(),buffer.as_mut_ptr()as _,buffer.len()as u32,&mut read,&mut kind);
   if status!=0{return Err(format!("静默进度响应失败或超时：{status}"))}
   if !matches!(kind,2|3)||message.len()+read as usize>65536{return Err("静默进度响应无效或过大".into())}
   message.extend_from_slice(&buffer[..read as usize]);if kind==3{continue}
   let value:Value=serde_json::from_slice(&message).map_err(|_|"静默进度响应 JSON 无效")?;
   if value["id"].as_u64()==Some(1){return Ok(value)}
   unrelated+=1;if unrelated>8{return Err("静默进度响应数量超限".into())}message.clear();
  }
 }
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn rejects_nonlocal_or_injected_websocket_endpoints(){for url in ["ws://localhost:9223/devtools/page/x","ws://127.0.0.1:9224/devtools/page/x","ws://127.0.0.1:9223@other/devtools/page/x","ws://127.0.0.1:9223/devtools/page/x?y","ws://127.0.0.1:9223/devtools/page/../x","ws://127.0.0.1:9223/devtools/browser/x"]{assert!(websocket_path(url,9223).is_err());}assert_eq!(websocket_path("ws://127.0.0.1:9223/devtools/page/ab-12",9223).unwrap(),"/devtools/page/ab-12");}
 #[test]fn matches_only_the_expected_process_and_loopback_listener(){let row=[1,2,u32::from_ne_bytes([127,0,0,1]),(9223u16.to_be())as u32,0,0,42];assert!(listener_matches(&row,28,42,9223));assert!(!listener_matches(&row,28,43,9223));let mut exposed=row;exposed[2]=0;assert!(!listener_matches(&exposed,28,42,9223));assert!(!listener_matches(&row,5,42,9223));assert!(!listener_matches(&row,28,42,9224));}
 #[test]fn refuses_an_additional_ipv6_listener_on_the_control_port(){let mut row=[0u32;15];row[0]=1;assert!(ipv6_port_absent(&row,60,9223));row[6]=9223u16.to_be()as u32;assert!(!ipv6_port_absent(&row,60,9223));assert!(ipv6_port_absent(&row,60,9224));assert!(!ipv6_port_absent(&row,4,9223));}
 #[test]fn native_deadline_closes_a_stalled_handle_and_can_be_cancelled(){unsafe{let agent=wide("DotIsle offline deadline test");let handle=Owned::new(WinHttpOpen(agent.as_ptr(),1,null(),null(),0)).unwrap();{let _guard=Deadline::new(&handle,Duration::from_millis(10));std::thread::sleep(Duration::from_millis(40));assert!(handle.handle().is_null());}let handle=Owned::new(WinHttpOpen(agent.as_ptr(),1,null(),null(),0)).unwrap();{let _guard=Deadline::new(&handle,Duration::from_millis(10));}std::thread::sleep(Duration::from_millis(40));assert!(!handle.handle().is_null());}}
 #[test]fn ambiguous_and_foreign_pages_cannot_be_controlled(){let p=json!({"type":"page","url":PAGE,"webSocketDebuggerUrl":"ws://127.0.0.1:9223/devtools/page/a"});assert!(select_target(&json!([p]),9223).is_ok());assert!(select_target(&json!([p,p]),9223).is_err());assert!(select_target(&json!([{"type":"page","url":"https://music.163.com/"}]),9223).is_err());}
 #[test]fn only_numeric_song_ids_and_finite_seconds_form_expressions(){for id in ["","a","1);alert(1)"]{assert!(expression(id,2.).is_err());}for seconds in [-1.,f64::NAN,f64::INFINITY,86401.]{assert!(expression("123",seconds).is_err());}assert!(expression("123",0.).unwrap().ends_with("(\"123\", 0, true)"));}
 #[test]fn dispatch_ack_requires_same_song_and_playback_instance(){let ok=json!({"result":{"result":{"type":"object","value":{"dispatched":true,"trackId":"123","playId":"123_x"}}}});assert!(parse_reply(&ok,"123").is_ok());assert!(parse_reply(&ok,"124").is_err());assert!(parse_reply(&json!({"result":{"exceptionDetails":{},"result":ok["result"]["result"]}}),"123").is_err());assert!(parse_reply(&json!({"id":1,"result":{}}),"123").is_err());}
 #[test]fn a_stalled_local_http_peer_cannot_hold_the_control_worker(){
  use std::{net::TcpListener,io::{Read,Write}};
  for partial_body in [false,true]{
   let listener=TcpListener::bind(("127.0.0.1",0)).unwrap();let port=listener.local_addr().unwrap().port();
   let server=std::thread::spawn(move||{
    listener.set_nonblocking(true).unwrap();let until=Instant::now()+Duration::from_secs(3);
    let mut stream=loop{if let Ok((s,_))=listener.accept(){break s}if Instant::now()>until{return}std::thread::sleep(Duration::from_millis(5));};
    stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();let mut input=[0;4096];let _=stream.read(&mut input);
    if partial_body{let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n[");}
    std::thread::sleep(Duration::from_millis(1200));
   });
   let started=Instant::now();assert!(evaluate(std::process::id(),port,"0").is_err());assert!(started.elapsed()<Duration::from_secs(2));server.join().unwrap();
  }
 }

}

//! User-confirmed OTA with authenticated helper progress and quiet background checks.
use serde_json::{Value,json};
use std::{path::{Path,PathBuf},sync::mpsc,time::{Instant,Duration}};
fn epoch()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()}
pub struct Hub{pub state:Value,pub release:Value,pub job:String,pub nonce:String,pub action:String,pub reminder:bool,pub error:Option<String>,rx:Option<mpsc::Receiver<Result<Value,String>>>,manual:bool,last:Instant,path:PathBuf,meta:Value,last_progress:Instant}
impl Hub{
 pub fn new(data:&Path)->Self{let path=data.join("update-state.json");let meta=lyric_backend::io::read_settings(&path);Self{state:json!({"phase":"idle"}),release:Value::Null,job:String::new(),nonce:String::new(),action:String::new(),reminder:false,error:None,rx:None,manual:false,last:Instant::now(),path,meta,last_progress:Instant::now()}}
 pub fn busy(&self)->bool{matches!(self.phase(),"downloading"|"switching"|"verifying"|"ready"|"applying"|"restarting")}
 pub fn phase(&self)->&str{self.state["phase"].as_str().unwrap_or("idle")}
 pub fn selectable(&self)->bool{self.busy()||matches!(self.phase(),"available"|"failed"|"completed")}
 pub fn check(&mut self,manual:bool){if self.rx.is_some()||self.busy(){return}self.manual=manual;self.last=Instant::now();self.meta["lastAttempt"]=json!(epoch());let _=lyric_backend::io::save_settings(&self.path,&self.meta);if manual{self.state=json!({"phase":"checking","message":"正在检查双源更新…"});}let(tx,rx)=mpsc::channel();self.rx=Some(rx);std::thread::spawn(move||{let _=tx.send(lyric_backend::updates::check());});}
 pub fn tick(&mut self,enabled:bool,demo:bool)->bool{
  let result=self.rx.as_ref().and_then(|rx|rx.try_recv().ok());let mut changed=false;
  if let Some(result)=result{self.rx=None;changed=true;match result{Ok(release)=>{self.release=release.clone();let newer=release["newer"]==true;self.state=json!({"phase":if newer{"available"}else{"idle"},"version":release["version"],"message":release["message"]});if newer{let version=release["version"].as_str().unwrap_or("");if self.manual||(self.meta["lastNotifiedVersion"].as_str()!=Some(version)||epoch().saturating_sub(self.meta["lastNotifiedAt"].as_u64().unwrap_or(0))>=86400){self.reminder=true;self.meta["lastNotifiedVersion"]=json!(version);self.meta["lastNotifiedAt"]=json!(epoch());}}self.meta["lastSuccess"]=json!(epoch());let _=lyric_backend::io::save_settings(&self.path,&self.meta);},Err(e)=>{self.error=Some(e.clone());lyric_backend::io::log(&self.path.parent().unwrap().join("Logs"),"UPDATE_ERROR",&e);if self.manual{self.state=json!({"phase":"failed","message":e});self.reminder=true;}}}}
  if enabled&&!demo&&self.rx.is_none()&&!self.busy()&&self.last.elapsed()>Duration::from_secs(30)&&epoch().saturating_sub(self.meta["lastAttempt"].as_u64().unwrap_or(0))>=43200{self.check(false);}
  if self.busy()&&!matches!(self.phase(),"ready"|"applying"|"restarting")&&self.last_progress.elapsed()>Duration::from_secs(150){self.state=json!({"phase":"failed","message":"更新助手长时间未响应，原程序保持不变。"});changed=true;}
  changed
 }
 pub fn begin(&mut self,base:&Path,data:&Path,pipe:&str)->Result<(),String>{
  if self.busy(){return Err("已有更新任务正在进行".into())}if self.release["newer"]!=true{return Err("请先检查更新，再选择下载。".into())}
  self.nonce=format!("{:?}",unsafe{windows::Win32::System::Com::CoCreateGuid()}.map_err(|e|e.to_string())?);self.job=self.nonce.clone();self.action.clear();self.last_progress=Instant::now();
  lyric_backend::updates::launch_candidate(&self.release,base,data,pipe,&self.job,&self.nonce)?;
  self.state=json!({"phase":"downloading","version":self.release["version"],"receivedBytes":0,"totalBytes":null,"seq":0});Ok(())
 }
 pub fn progress(&mut self,value:&Value)->Result<Value,String>{
  if self.nonce.is_empty()||value["nonce"].as_str()!=Some(&self.nonce)||value["jobId"].as_str()!=Some(&self.job){return Err("更新任务标识无效".into())}
  let seq=value["seq"].as_u64().ok_or("进度序号无效")?;
  if seq>self.state["seq"].as_u64().unwrap_or(0){let phase=value["phase"].as_str().ok_or("进度阶段无效")?;if !["downloading","switching","verifying","ready","applying","restarting","completed","failed","cancelled"].contains(&phase){return Err("进度阶段无效".into())}
   let received=value["receivedBytes"].as_u64().unwrap_or(0).min(256*1024*1024);let total=value["totalBytes"].as_u64().filter(|t|*t>0&&*t<=256*1024*1024&&*t>=received);
   self.state=json!({"phase":phase,"seq":seq,"version":self.release["version"],"receivedBytes":received,"totalBytes":total,"percent":total.map(|t|(received*100/t).min(100)),"message":value["message"].as_str().unwrap_or("").chars().take(300).collect::<String>()});self.last_progress=Instant::now();
  }Ok(json!({"success":true,"action":self.action}))
 }
 pub fn panel(&self)->crate::activities::Panel{
  let(phase,label)=match self.phase(){"available"=>("发现新版本","下载更新"),"downloading"=>("正在下载","取消下载"),"switching"=>("正在切换更新源","取消下载"),"verifying"=>("正在校验","取消"),"ready"=>("更新已就绪","重启并安装"),"applying"=>("正在安装",""),"failed"=>("更新未完成","重新检查"),"completed"=>("更新完成","关闭"),_=>("软件更新","")};
  let percent=self.state["percent"].as_u64();let progress=if self.phase()=="downloading"{percent.map(|p|p as f64/100.).unwrap_or(-1.)}else if matches!(self.phase(),"ready"|"completed"){1.}else if self.busy(){-1.}else{0.};
  crate::activities::Panel{kind:3,title:format!("{} {}",phase,self.state["version"].as_str().unwrap_or("")),value:percent.filter(|_|self.phase()=="downloading").map(|p|format!("{p}%")).unwrap_or_else(||"···".into()),subtitle:if !self.state["message"].as_str().unwrap_or("").is_empty(){self.state["message"].as_str().unwrap_or("").into()}else{format!("已下载 {:.1} MB",self.state["receivedBytes"].as_u64().unwrap_or(0)as f64/1048576.)},progress,running:self.busy(),accent:0xff8dd6ad,tasks:vec![crate::activities::Task{title:label.into(),..Default::default()}],..Default::default()}
 }
}
#[cfg(test)]mod tests{use super::*;#[test]fn rejects_forged_and_stale_progress(){let mut h=Hub::new(Path::new(""));h.nonce="secret".into();h.job="job".into();assert!(h.progress(&json!({"nonce":"bad","jobId":"job","seq":1,"phase":"ready"})).is_err());h.progress(&json!({"nonce":"secret","jobId":"job","seq":2,"phase":"downloading","receivedBytes":30,"totalBytes":100})).unwrap();assert_eq!(h.state["percent"],30);h.progress(&json!({"nonce":"secret","jobId":"job","seq":1,"phase":"ready"})).unwrap();assert_eq!(h.phase(),"downloading");h.progress(&json!({"nonce":"secret","jobId":"job","seq":3,"phase":"downloading","receivedBytes":40})).unwrap();assert!(h.state["percent"].is_null());}}

//! User-confirmed OTA with authenticated helper progress and quiet background checks.
use serde_json::{Value,json};
use std::{path::{Path,PathBuf},sync::mpsc,time::{Instant,Duration}};
fn epoch()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()}
/// The helper restarts the app without touching update-state.json, so a marker written before
/// exit is the only evidence the new process has that an install was requested.
fn same_version(a:&str,b:&str)->bool{a.trim().trim_start_matches('v')==b.trim().trim_start_matches('v')}
pub struct Hub{pub state:Value,pub release:Value,pub job:String,pub nonce:String,pub action:String,pub reminder:bool,pub error:Option<String>,rx:Option<mpsc::Receiver<Result<Value,String>>>,manual:bool,last:Instant,path:PathBuf,meta:Value,last_progress:Instant,speed:f64,speed_at:Option<(u64,Instant)>}
impl Hub{
 pub fn new(data:&Path)->Self{
  let path=data.join("update-state.json");let mut meta=lyric_backend::io::read_settings(&path);let mut state=json!({"phase":"idle"});
  if let Some(pending)=meta["applyingVersion"].as_str().map(str::to_owned).filter(|v|!v.is_empty()){
   // The helper was asked to install this version. Matching the running version means it
   // succeeded; anything else means it rolled back and the old build is what is running now.
   state=if same_version(&pending,crate::DISPLAY_VERSION){json!({"phase":"completed","version":pending,"message":"新版已启动，设置已保留。"})}else{json!({"phase":"failed","version":pending,"message":"更新未完成，当前仍为原版本。"})};
   if let Some(fields)=meta.as_object_mut(){fields.remove("applyingVersion");fields.remove("applyingAt");}
   if path.parent().is_some_and(|parent|!parent.as_os_str().is_empty()){let _=lyric_backend::io::save_settings(&path,&meta);}
  }
  Self{state,release:Value::Null,job:String::new(),nonce:String::new(),action:String::new(),reminder:false,error:None,rx:None,manual:false,last:Instant::now(),path,meta,last_progress:Instant::now(),speed:0.,speed_at:None}
 }
 fn persist(&self){if self.path.parent().is_none_or(|parent|parent.as_os_str().is_empty()){return}let _=lyric_backend::io::save_settings(&self.path,&self.meta);}
 pub fn phase(&self)->&str{self.state["phase"].as_str().unwrap_or("idle")}
 /// Work happening right now: it keeps the island on screen and owns the indeterminate ring.
 pub fn active(&self)->bool{matches!(self.phase(),"checking"|"downloading"|"switching"|"verifying"|"applying"|"restarting")}
 /// Kept as the name the island presence and the auto-check scheduler already use.
 pub fn busy(&self)->bool{self.active()}
 /// A prepared (`ready`) or finished (`completed`) update stays selectable for the user, but it
 /// never counts as running work, otherwise the island could never retract again.
 pub fn selectable(&self)->bool{self.active()||matches!(self.phase(),"available"|"ready"|"failed"|"completed")}
 pub fn check(&mut self,manual:bool){if self.rx.is_some()||self.active()||matches!(self.phase(),"ready"){return}self.manual=manual;self.last=Instant::now();self.meta["lastAttempt"]=json!(epoch());let _=lyric_backend::io::save_settings(&self.path,&self.meta);if manual{self.state=json!({"phase":"checking","message":"正在检查双源更新…"});}let(tx,rx)=mpsc::channel();self.rx=Some(rx);std::thread::spawn(move||{let _=tx.send(lyric_backend::updates::check());});}
 pub fn tick(&mut self,enabled:bool,demo:bool)->bool{
  let result=self.rx.as_ref().and_then(|rx|rx.try_recv().ok());let mut changed=false;
  if let Some(result)=result{self.rx=None;changed=true;match result{Ok(release)=>{self.release=release.clone();let newer=release["newer"]==true;self.state=json!({"phase":if newer{"available"}else{"idle"},"version":release["version"],"message":release["message"]});if newer{let version=release["version"].as_str().unwrap_or("");if self.manual||(self.meta["lastNotifiedVersion"].as_str()!=Some(version)||epoch().saturating_sub(self.meta["lastNotifiedAt"].as_u64().unwrap_or(0))>=86400){self.reminder=true;self.meta["lastNotifiedVersion"]=json!(version);self.meta["lastNotifiedAt"]=json!(epoch());}}self.meta["lastSuccess"]=json!(epoch());let _=lyric_backend::io::save_settings(&self.path,&self.meta);},Err(e)=>{self.error=Some(e.clone());lyric_backend::io::log(&self.path.parent().unwrap().join("Logs"),"UPDATE_ERROR",&e);if self.manual{self.state=json!({"phase":"failed","message":e});self.reminder=true;}}}}
  // A background check stays silent: only a prepared task or a manual check may run while idle.
  if enabled&&!demo&&self.rx.is_none()&&!self.active()&&self.phase()!="ready"&&self.last.elapsed()>Duration::from_secs(30)&&epoch().saturating_sub(self.meta["lastAttempt"].as_u64().unwrap_or(0))>=43200{self.check(false);}
  // Installing cannot be cancelled and the helper owns the surface after exit, so it must not
  // be reported as stalled while it is deliberately waiting for or replacing the app.
  if self.active()&&!matches!(self.phase(),"applying"|"restarting")&&self.last_progress.elapsed()>Duration::from_secs(150){self.state=json!({"phase":"failed","message":"更新助手长时间未响应，原程序保持不变。"});self.action.clear();changed=true;}
  // "重启并安装" was clicked but the helper never started applying: report it instead of
  // leaving a ready card that silently does nothing forever.
  if self.phase()=="ready"&&self.action=="apply"&&self.last_progress.elapsed()>Duration::from_secs(90){self.state=json!({"phase":"failed","message":"更新助手未响应，原程序保持不变。"});self.action.clear();changed=true;}
  changed
 }
 pub fn begin(&mut self,base:&Path,data:&Path,pipe:&str)->Result<(),String>{
  if self.active(){return Err("已有更新任务正在进行".into())}if self.release["newer"]!=true{return Err("请先检查更新，再选择下载。".into())}
  self.nonce=format!("{:?}",unsafe{windows::Win32::System::Com::CoCreateGuid()}.map_err(|e|e.to_string())?);self.job=self.nonce.clone();self.action.clear();self.error=None;self.speed=0.;self.speed_at=None;self.last_progress=Instant::now();
  lyric_backend::updates::launch_candidate(&self.release,base,data,pipe,&self.job,&self.nonce)?;
  self.state=json!({"phase":"downloading","version":self.release["version"],"receivedBytes":0,"totalBytes":null,"seq":0});Ok(())
 }
 /// Ask the helper to install the prepared update. The helper polls the action field.
 pub fn ask_apply(&mut self)->bool{if self.phase()!="ready"{return false}self.action="apply".into();self.meta["applyingVersion"]=json!(self.state["version"].as_str().or(self.release["version"].as_str()).unwrap_or(""));self.meta["applyingAt"]=json!(epoch());self.persist();self.last_progress=Instant::now();true}
 /// Cancel is only meaningful while the download, its verification or the prepared wait runs.
 pub fn cancel(&mut self)->Result<(),String>{if !matches!(self.phase(),"downloading"|"switching"|"verifying"|"ready"){return Err("当前阶段不能取消更新".into())}self.action="cancel".into();self.last_progress=Instant::now();Ok(())}
 /// Closing a finished reminder returns the hub to idle so the update activity leaves the cycle.
 /// A prepared (`ready`) update is user work and survives closing unless it was cancelled.
 pub fn dismiss(&mut self)->bool{
  // An apply request is already with the helper; closing the card must not cancel it.
  if self.action=="apply"{return false}
  if matches!(self.phase(),"completed"|"failed"|"cancelled"){self.state=json!({"phase":"idle"});self.error=None;self.reminder=false;self.action.clear();self.meta["lastPhase"]=json!("idle");self.persist();true}
  else{self.action.clear();false}
 }
 pub fn progress(&mut self,value:&Value)->Result<Value,String>{
  if self.nonce.is_empty()||value["nonce"].as_str()!=Some(&self.nonce)||value["jobId"].as_str()!=Some(&self.job){return Err("更新任务标识无效".into())}
  let seq=value["seq"].as_u64().ok_or("进度序号无效")?;
  if seq>self.state["seq"].as_u64().unwrap_or(0){let phase=value["phase"].as_str().ok_or("进度阶段无效")?;if !["downloading","switching","verifying","ready","applying","restarting","completed","failed","cancelled"].contains(&phase){return Err("进度阶段无效".into())}
   let mut received=value["receivedBytes"].as_u64().unwrap_or(0).min(256*1024*1024);let mut total=value["totalBytes"].as_u64().filter(|t|*t>0&&*t<=256*1024*1024&&*t>=received);
   // The helper reports `ready` without byte counters after the download finished; the last
   // real counters from this same response stream are still the only truthful percentage.
   if matches!(phase,"ready"|"completed")&&received==0&&total.is_none(){received=self.state["receivedBytes"].as_u64().unwrap_or(0);total=self.state["totalBytes"].as_u64();}
   if phase=="downloading"{let now=Instant::now();match self.speed_at{Some((last,at))=>{let dt=now.duration_since(at).as_secs_f64();if dt>=0.2{let sample=received.saturating_sub(last)as f64/dt;self.speed=if self.speed<=0.{sample}else{self.speed*0.6+sample*0.4};self.speed_at=Some((received,now));}else if received<last{self.speed_at=Some((received,now));}},None=>self.speed_at=Some((received,now))}self.speed=self.speed.clamp(0.,100.*1048576.);}else{self.speed=0.;self.speed_at=None;}
   let percent=total.map(|t|(received*100/t).min(100));let previous=self.phase().to_owned();
   self.state=json!({"phase":phase,"seq":seq,"version":self.release["version"],"receivedBytes":received,"totalBytes":total,"percent":percent,"speedBytesPerSecond":self.speed.round()as u64,"message":value["message"].as_str().unwrap_or("").chars().take(300).collect::<String>()});self.last_progress=Instant::now();
   if previous!=phase{self.meta["lastPhase"]=json!(phase);self.meta["lastVersion"]=self.state["version"].clone();self.persist();}
  }Ok(json!({"success":true,"action":self.action}))
 }
 pub fn panel(&self)->crate::activities::Panel{
  let phase=self.phase();let version=self.state["version"].as_str().or_else(||self.release["version"].as_str()).unwrap_or("");
  let(label,action)=match phase{"available"=>("发现新版本","下载更新"),"checking"=>("正在检查更新",""),"downloading"=>("正在下载更新","取消下载"),"switching"=>("正在切换更新源","取消下载"),"verifying"=>("正在校验更新包","取消"),"ready"=>("更新已就绪","重启并安装"),"applying"=>("正在安装更新",""),"restarting"=>("正在重启点屿",""),"failed"=>("更新未完成","重新检查"),"completed"=>("更新完成","关闭"),_=>("软件更新","")};
  let percent=self.state["percent"].as_u64();
  // Determinate only when both byte counters exist; verifying/switching/applying are genuinely
  // unknown-length stages and must animate instead of inventing a percentage.
  let progress=if phase=="downloading"{percent.map(|p|p as f64/100.).unwrap_or(-1.)}else if self.active(){-1.}else if matches!(phase,"ready"|"completed"){1.}else{0.};
  let value=if matches!(phase,"downloading"|"ready"|"completed"){percent.map(|p|format!("{p}%")).unwrap_or_default()}else{String::new()};
  let received=self.state["receivedBytes"].as_u64().unwrap_or(0);let total=self.state["totalBytes"].as_u64();let speed=self.state["speedBytesPerSecond"].as_u64().unwrap_or(0);
  let amount=|bytes:u64|if bytes>=1048576{format!("{:.1} MB",bytes as f64/1048576.)}else{format!("{:.0} KB",bytes as f64/1024.)};
  let subtitle=match phase{
   "checking"=>"正在检查双源更新…".into(),
   "downloading"=>{let mut text=if let Some(total)=total{format!("已下载 {} / {}",amount(received),amount(total))}else{format!("已下载 {} · 总大小未知",amount(received))};if speed>=1048576{text.push_str(&format!(" · {:.1} MB/s",speed as f64/1048576.))}else if speed>0{text.push_str(&format!(" · {:.0} KB/s",speed as f64/1024.))}text},
   "switching"=>"正在切换到备用更新源…".into(),
   "verifying"=>"下载完成，正在核对更新包…".into(),
   "applying"=>"正在保存数据并安装，请勿关闭…".into(),
   "restarting"=>"正在启动新版并确认…".into(),
   "available"=>self.state["message"].as_str().filter(|m|!m.is_empty()).map(str::to_owned).unwrap_or_else(||"下载后由你确认安装；未确认不会重启。".into()),
   "ready"|"completed"|"failed"=>self.state["message"].as_str().or_else(||self.error.as_deref()).filter(|m|!m.is_empty()).unwrap_or(match phase{"ready"=>"校验已完成，重启后安装。","failed"=>"更新未完成，原程序保持不变。",_=>"新版已启动，设置已保留。"}).to_owned(),
   _=>self.state["message"].as_str().unwrap_or("").to_owned(),
  };
  let title=if version.is_empty(){label.to_owned()}else{format!("{label} {version}")};
  crate::activities::Panel{kind:3,title,value,subtitle,progress,running:self.active(),accent:0xff8dd6ad,tasks:if action.is_empty(){vec![]}else{vec![crate::activities::Task{title:action.into(),..Default::default()}]},..Default::default()}
 }
}
#[cfg(test)]mod tests{use super::*;
 fn hub()->Hub{let mut h=Hub::new(Path::new(""));h.nonce="secret".into();h.job="job".into();h.release=json!({"newer":true,"version":"v1.5.1"});h}
 fn report(h:&mut Hub,seq:u64,phase:&str,received:u64,total:Option<u64>)->crate::activities::Panel{h.progress(&json!({"nonce":"secret","jobId":"job","seq":seq,"phase":phase,"receivedBytes":received,"totalBytes":total})).unwrap();h.panel()}
 #[test]fn rejects_forged_and_stale_progress(){let mut h=hub();assert!(h.progress(&json!({"nonce":"bad","jobId":"job","seq":1,"phase":"ready"})).is_err());h.progress(&json!({"nonce":"secret","jobId":"job","seq":2,"phase":"downloading","receivedBytes":30,"totalBytes":100})).unwrap();assert_eq!(h.state["percent"],30);h.progress(&json!({"nonce":"secret","jobId":"job","seq":1,"phase":"ready"})).unwrap();assert_eq!(h.phase(),"downloading");h.progress(&json!({"nonce":"secret","jobId":"job","seq":3,"phase":"downloading","receivedBytes":40})).unwrap();assert!(h.state["percent"].is_null());}
 #[test]fn percent_comes_only_from_byte_counts_and_unknown_lengths_stay_indeterminate(){
  let mut h=hub();let panel=report(&mut h,1,"downloading",0,Some(100));assert_eq!(panel.value,"0%");assert_eq!(panel.progress,0.);
  let panel=report(&mut h,2,"downloading",50,Some(100));assert_eq!(panel.value,"50%");assert_eq!(panel.progress,0.5);assert!(panel.subtitle.contains("已下载"));
  let panel=report(&mut h,3,"downloading",700000,None);assert_eq!(panel.value,"","no fake percentage without a reliable length");assert_eq!(panel.progress,-1.,"unknown length must animate indefinitely");
  let panel=report(&mut h,4,"verifying",700000,None);assert_eq!(panel.value,"");assert_eq!(panel.progress,-1.,"verifying has no reliable total");
  let panel=report(&mut h,5,"switching",0,None);assert_eq!(panel.progress,-1.);
  // ready keeps the last real byte counters of the same response stream, so 100% is truthful.
  h.state["percent"]=json!(100);h.state["receivedBytes"]=json!(100);h.state["totalBytes"]=json!(100);
  let panel=report(&mut h,6,"ready",0,None);assert_eq!(panel.value,"100%");assert_eq!(panel.progress,1.);assert!(panel.running==false);
 }
 #[test]fn only_active_work_is_busy_and_ready_never_holds_the_island(){
  let mut h=hub();assert!(!h.busy()&&!h.selectable());
  h.state=json!({"phase":"available","version":"v1.5.1"});assert!(!h.busy());assert!(h.selectable());
  h.state=json!({"phase":"ready","version":"v1.5.1"});assert!(!h.busy(),"a prepared update must not keep the island on screen");assert!(h.selectable());
  for phase in ["downloading","switching","verifying","applying","restarting"]{h.state=json!({"phase":phase});assert!(h.busy(),"{phase}");}
  h.manual=true;h.state=json!({"phase":"checking"});assert!(h.busy()&&h.selectable(),"a manual check is user-visible work");
 }
 #[test]fn closing_a_finished_reminder_leaves_the_activity_cycle(){
  for phase in ["completed","failed","cancelled"]{let mut h=hub();h.state=json!({"phase":phase,"version":"v1.5.1"});assert!(h.dismiss());assert_eq!(h.phase(),"idle");assert!(!h.selectable());assert!(h.release["newer"]==true,"dismiss keeps the known release");}
  let mut ready=hub();ready.state=json!({"phase":"ready","version":"v1.5.1"});assert!(!ready.dismiss());assert_eq!(ready.phase(),"ready","latering a prepared update keeps it installed");
  assert!(ready.selectable());
 }
 #[test]fn cancel_is_rejected_while_installing_and_accepted_while_downloading(){
  let mut h=hub();h.state=json!({"phase":"downloading"});assert!(h.cancel().is_ok());assert_eq!(h.action,"cancel");
  for phase in ["applying","restarting","completed","failed","available","idle"]{let mut h=hub();h.state=json!({"phase":phase});assert!(h.cancel().is_err(),"{phase}");}
 }
 #[test]fn a_prepared_update_leaves_the_island_when_the_main_window_closes(){
  let mut h=hub();h.state=json!({"phase":"ready","version":"v1.5.1"});assert!(h.ask_apply());assert_eq!(h.action,"apply");assert_eq!(h.meta["applyingVersion"],"v1.5.1");
 }
 #[test]fn a_restart_confirms_or_rejects_the_install_marker(){
  let dir=std::env::temp_dir().join(format!("dotisle-update-confirm-{}",std::process::id()));let _=std::fs::create_dir_all(&dir);
  for (stored,expected) in [(crate::DISPLAY_VERSION.to_owned(),"completed"),("9.9.9".to_owned(),"failed")]{
   std::fs::write(dir.join("update-state.json"),serde_json::to_vec(&json!({"applyingVersion":stored,"applyingAt":1,"lastSuccess":2})).unwrap()).unwrap();
   let hub=Hub::new(&dir);assert_eq!(hub.phase(),expected,"{stored}");
   let meta=lyric_backend::io::read_settings(&dir.join("update-state.json"));assert!(meta["applyingVersion"].is_null(),"the marker is consumed once");
  }
  let _=std::fs::remove_dir_all(&dir);
 }
}

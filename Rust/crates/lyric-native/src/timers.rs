//! Multiple monotonic clocks plus multi-stage workflows. Persisted sessions resume paused,
//! never fabricate elapsed time.
use serde::{Serialize,Deserialize};use serde_json::{Value,json};use std::path::PathBuf;
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub struct Stage{pub name:String,pub seconds:u64}
#[derive(Clone,Debug,Serialize,Deserialize,Default)]#[serde(rename_all="camelCase")]
pub struct Workflow{
 #[serde(default)]pub stages:Vec<Stage>,#[serde(default)]pub stage_index:usize,#[serde(default)]pub waiting:bool,#[serde(default)]pub template_id:Option<String>,
}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub struct Timer{
 pub id:String,pub name:String,pub mode:String,pub duration:u64,pub focus:u64,pub rest:u64,pub rounds:u32,pub round:u32,pub phase:String,pub elapsed_ms:u64,pub running:bool,pub finished:bool,
 #[serde(default)]pub todo_id:Option<String>,#[serde(default)]pub workflow:Option<Workflow>,#[serde(default)]pub pause_reason:Option<String>,#[serde(default)]pub revision:u64,#[serde(default)]pub laps:Vec<u64>,#[serde(default)]pub count_up:bool,
}
impl Timer{
 pub fn primary_action(&self)->&'static str{if self.finished{"restart"}else if self.mode=="workflow"&&self.workflow.as_ref().is_some_and(|w|w.waiting){"next"}else if self.running{"pause"}else{"resume"}}
 pub fn panel(&self)->crate::activities::Panel{
  let stopwatch=self.mode=="stopwatch";let waiting=self.workflow.as_ref().is_some_and(|w|w.waiting);
  let primary=match self.primary_action(){"restart"=>"重新开始","next"=>"下一阶段","pause"=>"暂停",_=>"继续"};
  crate::activities::Panel{kind:1,title:self.name.clone(),value:self.clock(),subtitle:self.subtitle(),progress:self.progress(),running:self.running,accent:0xffa9e6cd,
   timer:Some(crate::activities::TimerPresentation{label:if stopwatch{"累计时间"}else if self.count_up{"已过时间"}else{"剩余时间"}.into(),elapsed:if stopwatch{"精确到百分秒".into()}else{format!("已过 {}",format_time(self.elapsed_ms/1000))},footnote:if stopwatch{format!("{} 次计次",self.laps.len())}else if self.finished{"已完成".into()}else if waiting{"等待确认".into()}else{"到时提醒".into()},primary:primary.into(),expanded_value:if stopwatch{format!("{}.{:02}",format_time(self.elapsed_ms/1000),self.elapsed_ms/10%100)}else{self.clock()},indefinite:stopwatch}),..Default::default()}
 }
 fn stage(&self)->Option<&Stage>{self.workflow.as_ref().and_then(|w|w.stages.get(w.stage_index))}
 pub fn limit_ms(&self)->u64{1000*if self.mode=="workflow"{self.stage().map(|s|s.seconds).unwrap_or(0).max(1)}else if self.mode=="pomodoro"{if self.phase=="rest"{self.rest}else{self.focus}}else{self.duration}}
 pub fn remaining_ms(&self)->u64{self.limit_ms().saturating_sub(self.elapsed_ms)}
 pub fn clock(&self)->String{format_time(if self.mode=="stopwatch"||self.count_up{self.elapsed_ms/1000}else{self.remaining_ms().div_ceil(1000)})}
 pub fn progress(&self)->f64{if self.mode=="stopwatch"{(self.elapsed_ms%60000)as f64/60000.}else{(self.elapsed_ms as f64/self.limit_ms().max(1)as f64).clamp(0.,1.)}}
 pub fn subtitle(&self)->String{
  if self.finished{return "已完成".into()}
  if self.mode=="workflow"{
   let (index,total,waiting)=self.workflow.as_ref().map(|w|(w.stage_index,w.stages.len(),w.waiting)).unwrap_or((0,0,false));
   let stage=self.stage().map(|s|s.name.as_str()).unwrap_or("阶段");
   let state=if waiting{"等待下一阶段".into()}else if self.running{stage.into()}else{format!("{stage} · 已暂停")};
   return format!("流程 · 第 {} / {} 阶段 · {state}",index+1,total);
  }
  let phase=if self.mode=="pomodoro"{format!("{} · 第 {} / {} 轮",if self.phase=="focus"{"专注"}else{"休息"},self.round,self.rounds)}else if self.mode=="stopwatch"{"正向计时".into()}else{"倒计时".into()};
  format!("{phase}{}",if self.running{""}else{" · 已暂停"})
 }
 fn advance(&mut self,ms:u64)->bool{
  if !self.running||self.finished{return false}
  if self.mode=="workflow"{
   // A workflow stage waits for an explicit "next"; even a sleep-sized gap only
   // completes the stage that was actually running.
   self.elapsed_ms=self.elapsed_ms.saturating_add(ms);let limit=self.limit_ms();
   if self.elapsed_ms>=limit{self.elapsed_ms=limit;self.running=false;if let Some(w)=&mut self.workflow{w.waiting=true;}self.revision+=1;return true}
   return false
  }
  self.elapsed_ms=self.elapsed_ms.saturating_add(ms);if self.mode=="stopwatch"{return false}let mut changed=false;
  while self.elapsed_ms>=self.limit_ms(){let limit=self.limit_ms();self.elapsed_ms-=limit;changed=true;if self.mode!="pomodoro"||(self.phase=="focus"&&self.round>=self.rounds){self.running=false;self.finished=true;self.elapsed_ms=limit;break}if self.phase=="focus"{self.phase="rest".into()}else{self.phase="focus".into();self.round+=1;}}
  if changed{self.revision+=1;}changed
 }
 fn finish_workflow(&mut self){self.finished=true;self.running=false;self.pause_reason=None;if let Some(w)=&mut self.workflow{w.waiting=false;}self.elapsed_ms=self.limit_ms();self.revision+=1}
}
pub fn format_time(s:u64)->String{if s>=3600{format!("{:02}:{:02}:{:02}",s/3600,s/60%60,s%60)}else{format!("{:02}:{:02}",s/60,s%60)}}
/// Validate and normalise a stages array shared by timer instances and templates.
/// Names default to "阶段 N"; each stage lasts 1..=240 minutes and the whole flow
/// fits in 24 hours.
pub fn stages_from(value:&Value)->Result<Vec<Stage>,String>{
 let stages=value.as_array().ok_or("流程至少需要 2 个阶段")?;
 if !(2..=12).contains(&stages.len()){return Err("流程需要 2 到 12 个阶段".into())}
 let mut out=Vec::with_capacity(stages.len());let mut total=0u64;
 for (index,stage) in stages.iter().enumerate(){
  let seconds=stage["seconds"].as_u64().ok_or("阶段时长无效")?;
  if !(60..=14400).contains(&seconds){return Err("每个阶段需要 1 到 240 分钟".into())}
  total=total.saturating_add(seconds);if total>86400{return Err("流程总时长不能超过 24 小时".into())}
  let name=stage["name"].as_str().unwrap_or("").trim();
  if name.chars().count()>40||name.chars().any(char::is_control){return Err("阶段名称过长或包含无效字符".into())}
  out.push(Stage{name:if name.is_empty(){format!("阶段 {}",index+1)}else{name.into()},seconds});
 }
 Ok(out)
}
pub fn stages_ok(stages:&[Stage])->bool{
 (2..=12).contains(&stages.len())&&stages.iter().all(|s|(60..=14400).contains(&s.seconds)&&!s.name.is_empty()&&s.name.chars().count()<=40&&!s.name.chars().any(char::is_control))&&stages.iter().map(|s|s.seconds).sum::<u64>()<=86400
}
fn valid(t:&Timer)->bool{
 if !matches!(t.mode.as_str(),"countdown"|"pomodoro"|"stopwatch"|"workflow"){return false}
 if t.mode=="workflow"{return t.workflow.as_ref().is_some_and(|w|stages_ok(&w.stages)&&w.stage_index<w.stages.len())}
 t.duration>0&&t.focus>0&&t.rest>0&&(1..=16).contains(&t.rounds)&&t.laps.len()<=200&&t.laps.windows(2).all(|p|p[0]<p[1])&&t.laps.last().map_or(true,|last|*last<=t.elapsed_ms)
}
pub struct Store{path:PathBuf,pub items:Vec<Timer>,pub error:Option<String>}
impl Store{
 pub fn new(path:PathBuf)->Self{let mut s=Self{path,items:vec![],error:None};if s.path.is_file(){match std::fs::read(&s.path).map_err(|e|e.to_string()).and_then(|b|serde_json::from_slice::<Vec<Timer>>(&b).map_err(|e|e.to_string())){Ok(mut items)=>{for t in &mut items{if t.running{t.running=false;t.pause_reason.get_or_insert_with(||"manual".into());}t.revision=t.revision.max(1);}if items.len()<=8&&items.iter().all(valid){s.items=items}else{s.error=Some("计时数据无效，原文件已保留".into())}},Err(e)=>s.error=Some(e)}}s}
 pub fn persist(&self)->Result<(),String>{if let Some(e)=&self.error{return Err(e.clone())}if self.path.as_os_str().is_empty(){return Ok(())}lyric_backend::io::save_settings(&self.path,&json!(self.items))}
 pub fn list(&self)->Value{json!(self.items.iter().map(|t|{let mut v=json!(t);v["clock"]=json!(t.clock());v["subtitle"]=json!(t.subtitle());v["progress"]=json!(t.progress());v}).collect::<Vec<_>>())}
 pub fn create(&mut self,v:&Value)->Result<String,String>{
  if self.items.len()>=8{return Err("最多同时保留 8 个计时器，请先移除不用的项目".into())}
  let mode=v["mode"].as_str().unwrap_or("countdown");if !matches!(mode,"countdown"|"pomodoro"|"stopwatch"|"workflow"){return Err("未知计时模式".into())}
  let count_up=match v.get("countUp"){Some(value)=>value.as_bool().ok_or("计时方向无效")?,None=>false};if count_up&&!matches!(mode,"pomodoro"|"workflow"){return Err("只有番茄钟可切换计时方向".into())}
  let number=|key,default,max|->Result<u64,String>{match v.get(key){Some(value)=>value.as_u64().filter(|n|*n>=1&&*n<=max).ok_or_else(||format!("{key} 超出有效范围")),None=>Ok(default)}};
  let (duration,focus,rest,rounds,workflow)=if mode=="workflow"{(0,0,0,0,Some(Workflow{stages:stages_from(&v["stages"])?,stage_index:0,waiting:false,template_id:v["templateId"].as_str().filter(|s|!s.is_empty()).map(str::to_owned)}))}else{(number("duration",300,86400)?,number("focus",1500,14400)?,number("rest",300,3600)?,number("rounds",4,16)?as u32,None)};
  let name=v["name"].as_str().unwrap_or("").trim();if name.chars().count()>60||name.chars().any(char::is_control){return Err("计时器名称过长或包含无效字符".into())}
  let name=if name.is_empty(){match mode{"pomodoro"=>"番茄钟","stopwatch"=>"正向计时","workflow"=>"流程",_=>"计时器"}}else{name};
  let todo_id=match v["todoId"].as_str(){Some(s)if !s.is_empty()=>{if s.len()>80||!s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err("关联待办标识无效".into())}Some(s.to_owned())},_=>None};
  let id=format!("clock-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
  self.items.push(Timer{id:id.clone(),name:name.into(),mode:mode.into(),duration,focus,rest,rounds,round:1,phase:"focus".into(),elapsed_ms:0,running:true,finished:false,todo_id,workflow,pause_reason:None,revision:1,laps:vec![],count_up});
  if let Err(e)=self.persist(){self.items.pop();return Err(e)}Ok(id)
 }
 /// Legacy toggle/reset/delete entry point kept for callers that do not track revisions.
 pub fn action(&mut self,id:&str,action:&str)->Result<(),String>{self.act(id,action,None)}
 pub fn act(&mut self,id:&str,action:&str,revision:Option<u64>)->Result<(),String>{
  let old=self.items.clone();
  if action=="delete"{if !self.items.iter().any(|t|t.id==id){return Err("计时器不存在".into())}self.items.retain(|t|t.id!=id);if let Err(e)=self.persist(){self.items=old;return Err(e)}return Ok(())}
  let t=self.items.iter_mut().find(|t|t.id==id).ok_or("计时器不存在")?;
  if matches!(action,"next"|"skip"|"extend5"|"end")&&t.mode!="workflow"{return Err("该操作仅适用于流程计时".into())}
  if let Some(expected)=revision{if expected!=t.revision{return Err("计时状态已变化，请刷新后重试".into())}}
  match action{
   "toggle"=>{if t.finished{return Err("计时已结束，请使用重新开始".into())}if t.mode=="workflow"{let Some(w)=&t.workflow else{return Err("流程数据缺失".into())};if w.waiting{return Err("当前阶段已完成，请点击下一阶段".into())}}t.running=!t.running;t.pause_reason=if t.running{None}else{Some("manual".into())};},
   "pause"=>{if t.finished{return Err("计时已结束".into())}if t.mode=="workflow"&&t.workflow.as_ref().is_some_and(|w|w.waiting){return Err("当前阶段已完成，请点击下一阶段".into())}t.running=false;t.pause_reason=Some("manual".into());},
   "resume"=>{if t.finished{return Err("计时已结束".into())}if t.mode=="workflow"&&t.workflow.as_ref().is_some_and(|w|w.waiting){return Err("当前阶段已完成，请点击下一阶段".into())}t.running=true;t.pause_reason=None;},
   "lap"=>{if t.mode!="stopwatch"||!t.running||t.finished{return Err("仅运行中的秒表可以计次".into())}if t.laps.len()>=200{return Err("最多记录200次".into())}if t.elapsed_ms==0||t.laps.last().is_some_and(|last|*last>=t.elapsed_ms){return Err("本次尚未产生新的计时".into())}t.laps.push(t.elapsed_ms);},
   "reset"=>{t.running=false;t.finished=false;t.elapsed_ms=0;t.laps.clear();t.round=1;t.phase="focus".into();t.pause_reason=None;if let Some(w)=&mut t.workflow{w.stage_index=0;w.waiting=false;}},
   "restart"=>{t.running=true;t.finished=false;t.elapsed_ms=0;t.laps.clear();t.round=1;t.phase="focus".into();t.pause_reason=None;if let Some(w)=&mut t.workflow{w.stage_index=0;w.waiting=false;}},
   "next"=>{let Some(w)=&mut t.workflow else{return Err("流程数据缺失".into())};if t.finished{return Err("流程已结束".into())}if !w.waiting{return Err("当前阶段尚未完成".into())}if w.stage_index+1>=w.stages.len(){t.finish_workflow()}else{w.stage_index+=1;w.waiting=false;t.elapsed_ms=0;t.running=true;t.pause_reason=None;}},
   "skip"=>{let Some(w)=&mut t.workflow else{return Err("流程数据缺失".into())};if t.finished{return Err("流程已结束".into())}if w.stage_index+1>=w.stages.len(){t.finish_workflow()}else{w.stage_index+=1;w.waiting=false;t.elapsed_ms=0;t.running=true;t.pause_reason=None;}},
   "extend5"=>{let Some(w)=&mut t.workflow else{return Err("流程数据缺失".into())};if t.finished{return Err("流程已结束".into())}let index=w.stage_index;let current=w.stages[index].seconds;let next=(current+300).min(14400);if next==current{return Err("当前阶段已达 240 分钟上限".into())}let total=w.stages.iter().map(|s|s.seconds).sum::<u64>()+(next-current);if total>86400{return Err("流程总时长不能超过 24 小时".into())}w.stages[index].seconds=next;w.waiting=false;t.running=true;t.pause_reason=None;},
   "end"=>{if t.finished{return Err("流程已结束".into())}t.finish_workflow()},
   _=>return Err("未知计时操作".into())
  }
  t.revision=t.revision.saturating_add(1);
  if let Err(e)=self.persist(){self.items=old;return Err(e)}Ok(())
 }
 /// Change only the displayed direction; the monotonic elapsed clock and deadlines stay intact.
 pub fn set_direction(&mut self,id:&str,count_up:bool,revision:Option<u64>)->Result<(),String>{
  let old=self.items.clone();let t=self.items.iter_mut().find(|t|t.id==id).ok_or("计时器不存在")?;
  if !matches!(t.mode.as_str(),"pomodoro"|"workflow"){return Err("只有番茄钟可切换计时方向".into())}
  if revision.is_some_and(|expected|expected!=t.revision){return Err("计时状态已变化，请刷新后重试".into())}
  if t.count_up==count_up{return Ok(())}t.count_up=count_up;t.revision+=1;if let Err(e)=self.persist(){self.items=old;return Err(e)}Ok(())
 }
 pub fn advance(&mut self,ms:u64)->Vec<Timer>{let mut events=vec![];for t in &mut self.items{if t.advance(ms){events.push(t.clone());}}events}
 /// Pause every running pomodoro focus timer for the current lock generation.
 pub fn pause_for_lock(&mut self,generation:u64)->Result<Vec<String>,String>{
  let reason=format!("lock:{generation}");let old=self.items.clone();let mut ids=vec![];
  for t in &mut self.items{if t.mode=="pomodoro"&&t.phase=="focus"&&t.running&&!t.finished{t.running=false;t.pause_reason=Some(reason.clone());t.revision+=1;ids.push(t.id.clone());}}
  if !ids.is_empty(){if let Err(e)=self.persist(){self.items=old;return Err(e)}}Ok(ids)
 }
 /// Resume lock-paused focus timers only when id, generation and pause reason all match.
 pub fn resume_focus(&mut self,ids:&[String],generation:u64)->Result<Vec<String>,String>{
  let reason=format!("lock:{generation}");let old=self.items.clone();let mut resumed=vec![];
  for t in &mut self.items{if !ids.contains(&t.id){continue}if t.mode=="pomodoro"&&t.phase=="focus"&&!t.finished&&!t.running&&t.pause_reason.as_deref()==Some(reason.as_str()){t.running=true;t.pause_reason=None;t.revision+=1;resumed.push(t.id.clone());}}
  if resumed.is_empty(){return Err("没有可继续的专注计时".into())}
  if let Err(e)=self.persist(){self.items=old;return Err(e)}Ok(resumed)
 }
 /// Drop automatic-resume eligibility without changing the paused state.
 pub fn clear_lock_pauses(&mut self)->Result<(),String>{
  let old=self.items.clone();let mut changed=false;
  for t in &mut self.items{if t.pause_reason.as_deref().is_some_and(|r|r.starts_with("lock:")){t.pause_reason=Some("manual".into());t.revision+=1;changed=true;}}
  if changed{if let Err(e)=self.persist(){self.items=old;return Err(e)}}Ok(())
 }
}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub struct Template{pub id:String,pub name:String,pub stages:Vec<Stage>}
pub struct Templates{path:PathBuf,pub items:Vec<Template>,pub error:Option<String>}
impl Templates{
 pub fn new(path:PathBuf)->Self{let mut s=Self{path,items:vec![],error:None};if s.path.is_file(){match std::fs::read(&s.path).map_err(|e|e.to_string()).and_then(|b|serde_json::from_slice::<Vec<Template>>(&b).map_err(|e|e.to_string())){Ok(items)=>{if items.len()<=16&&items.iter().all(|t|!t.name.is_empty()&&t.name.chars().count()<=60&&stages_ok(&t.stages)){s.items=items}else{s.error=Some("流程模板数据无效，原文件已保留".into())}},Err(e)=>s.error=Some(e)}}s}
 pub fn persist(&self)->Result<(),String>{if let Some(e)=&self.error{return Err(e.clone())}if self.path.as_os_str().is_empty(){return Ok(())}lyric_backend::io::save_settings(&self.path,&json!(self.items))}
 pub fn list(&self)->Value{json!(self.items)}
 pub fn save(&mut self,v:&Value)->Result<String,String>{
  let name=v["name"].as_str().unwrap_or("").trim();if name.is_empty(){return Err("请填写流程名称".into())}if name.chars().count()>60||name.chars().any(char::is_control){return Err("流程名称过长或包含无效字符".into())}
  let stages=stages_from(&v["stages"])?;let requested=v["id"].as_str().unwrap_or("");let old=self.items.clone();
  if !requested.is_empty(){let t=self.items.iter_mut().find(|t|t.id==requested).ok_or("流程模板不存在")?;t.name=name.into();t.stages=stages;if let Err(e)=self.persist(){self.items=old;return Err(e)}return Ok(requested.into())}
  if self.items.len()>=16{return Err("最多保存 16 个流程模板".into())}
  let id=format!("flow-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
  self.items.push(Template{id:id.clone(),name:name.into(),stages});if let Err(e)=self.persist(){self.items=old;return Err(e)}Ok(id)
 }
 pub fn delete(&mut self,id:&str)->Result<(),String>{let old=self.items.clone();if !self.items.iter().any(|t|t.id==id){return Err("流程模板不存在".into())}self.items.retain(|t|t.id!=id);if let Err(e)=self.persist(){self.items=old;return Err(e)}Ok(())}
}
#[cfg(test)]mod tests{use super::*;
 fn store()->Store{Store::new(PathBuf::new())}
 fn stages()->Value{json!([{"name":"整理","seconds":600},{"name":"学习","seconds":1500},{"name":"休息","seconds":300}])}
 #[test]fn pause_resume_and_countdown_complete_exactly_once(){let mut s=store();let id=s.create(&json!({"mode":"countdown","duration":2})).unwrap();s.advance(900);s.action(&id,"toggle").unwrap();s.advance(5000);assert_eq!(s.items[0].clock(),"00:02");s.action(&id,"toggle").unwrap();assert_eq!(s.advance(1100).len(),1);assert!(s.advance(5000).is_empty());assert_eq!(s.items[0].clock(),"00:00");}
 #[test]fn pomodoro_handles_a_sleep_sized_gap_without_alert_flood(){let mut s=store();s.create(&json!({"mode":"pomodoro","focus":2,"rest":1,"rounds":3})).unwrap();assert_eq!(s.advance(7000).len(),1);assert_eq!(s.items[0].round,3);assert_eq!(s.items[0].phase,"focus");assert!(!s.items[0].finished);assert_eq!(s.advance(1000).len(),1);assert!(s.items[0].finished);}
 #[test]fn concurrent_clocks_keep_independent_state_and_validate_input(){let mut s=store();let id=s.create(&json!({"mode":"stopwatch"})).unwrap();s.create(&json!({"duration":5})).unwrap();s.advance(5000);assert_eq!(s.items[0].clock(),"00:05");assert!(s.items[1].finished);s.action(&id,"reset").unwrap();assert_eq!(s.items[0].clock(),"00:00");assert!(s.create(&json!({"duration":0})).is_err());assert!(s.create(&json!({"rounds":17})).is_err());}
 #[test]fn todo_link_persists_and_invalid_ids_are_rejected(){let p=std::env::temp_dir().join(format!("LyricIsland.TimerLink.{}.{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));let mut s=Store::new(p.clone());let id=s.create(&json!({"mode":"pomodoro","focus":2,"name":"整理产品方案","todoId":"todo-123"})).unwrap();assert!(s.create(&json!({"todoId":"bad id!"})).is_err());let s=Store::new(p.clone());assert_eq!(s.items.iter().find(|t|t.id==id).and_then(|t|t.todo_id.as_deref()),Some("todo-123"));let _=std::fs::remove_file(p);}
 #[test]fn workflow_waits_after_each_stage_even_across_a_sleep_gap(){let mut s=store();let id=s.create(&json!({"mode":"workflow","name":"学习流程","stages":stages()})).unwrap();assert_eq!(s.items[0].clock(),"10:00");assert!(s.advance(599000).is_empty());assert_eq!(s.advance(1000).len(),1);assert!(s.items[0].workflow.as_ref().unwrap().waiting);assert!(!s.items[0].running);assert_eq!(s.items[0].clock(),"00:00");assert!(s.advance(24*60*60*1000).is_empty());assert_eq!(s.items[0].workflow.as_ref().unwrap().stage_index,0);let revision=s.items[0].revision;s.act(&id,"next",Some(revision)).unwrap();assert_eq!(s.items[0].workflow.as_ref().unwrap().stage_index,1);assert_eq!(s.items[0].clock(),"25:00");assert!(s.items[0].running);}
 #[test]fn stale_revision_prevents_a_double_next(){let mut s=store();let id=s.create(&json!({"mode":"workflow","stages":stages()})).unwrap();s.advance(600000);let revision=s.items[0].revision;s.act(&id,"next",Some(revision)).unwrap();assert!(s.act(&id,"next",Some(revision)).is_err());assert_eq!(s.items[0].workflow.as_ref().unwrap().stage_index,1);}
 #[test]fn skip_last_stage_ends_and_extend_is_capped(){let mut s=store();let id=s.create(&json!({"mode":"workflow","stages":stages()})).unwrap();for _ in 0..3{s.act(&id,"skip",None).unwrap();}assert!(s.items[0].finished);assert!(s.act(&id,"skip",None).is_err());let id=s.create(&json!({"mode":"workflow","stages":[{"name":"长","seconds":60},{"name":"末","seconds":60}]})).unwrap();s.advance(60000);for _ in 0..48{s.act(&id,"extend5",None).unwrap();}assert_eq!(s.items[1].workflow.as_ref().unwrap().stages[0].seconds,14400);assert!(s.act(&id,"extend5",None).is_err());}
 #[test]fn workflow_restarts_paused_at_the_same_stage_and_templates_are_copied(){let p=std::env::temp_dir().join(format!("LyricIsland.Workflow.{}.{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));let mut s=Store::new(p.clone());let id=s.create(&json!({"mode":"workflow","stages":stages(),"templateId":"flow-1"})).unwrap();s.act(&id,"skip",None).unwrap();let s=Store::new(p.clone());let t=s.items.iter().find(|t|t.id==id).unwrap();assert!(!t.running);assert_eq!(t.workflow.as_ref().unwrap().stage_index,1);assert_eq!(s.error,None);let instance=t.workflow.as_ref().unwrap().stages.clone();let mut templates=Templates::new(PathBuf::new());let template=templates.save(&json!({"name":"学习流程","stages":stages()})).unwrap();templates.save(&json!({"id":template,"name":"改名","stages":[{"name":"a","seconds":60},{"name":"b","seconds":60}]})).unwrap();assert_eq!(s.items[0].workflow.as_ref().unwrap().stages,instance);assert!(templates.delete(&template).is_ok());let _=std::fs::remove_file(p);}
 #[test]fn lock_pauses_only_running_focus_and_resume_needs_the_generation(){let mut s=store();let focus=s.create(&json!({"mode":"pomodoro","focus":60})).unwrap();let rest=s.create(&json!({"mode":"pomodoro","focus":60,"rest":30})).unwrap();let countdown=s.create(&json!({"mode":"countdown","duration":60})).unwrap();s.items.iter_mut().find(|t|t.id==rest).unwrap().phase="rest".into();assert_eq!(s.pause_for_lock(3).unwrap(),vec![focus.clone()]);assert!(s.items.iter().find(|t|t.id==rest).unwrap().running);assert!(s.items.iter().find(|t|t.id==countdown).unwrap().running);assert!(s.resume_focus(&[focus.clone()],2).is_err());assert_eq!(s.resume_focus(&[focus.clone()],3).unwrap(),vec![focus.clone()]);assert!(s.resume_focus(&[focus.clone()],3).is_err());s.pause_for_lock(4).unwrap();s.clear_lock_pauses().unwrap();assert!(s.resume_focus(&[focus],4).is_err());}
 #[test]fn old_three_mode_files_still_load(){let p=std::env::temp_dir().join(format!("LyricIsland.OldTimers.{}.{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::write(&p,r#"[{"id":"clock-1","name":"倒计时","mode":"countdown","duration":300,"focus":1500,"rest":300,"rounds":4,"round":1,"phase":"focus","elapsedMs":0,"running":true,"finished":false}]"#).unwrap();let s=Store::new(p.clone());assert_eq!(s.error,None);assert_eq!(s.items.len(),1);assert!(!s.items[0].running);assert!(s.items[0].pause_reason.is_some());let _=std::fs::remove_file(p);}
}

#[cfg(test)]mod lap_tests{use super::*;
#[test]fn stopwatch_laps_are_authoritative_independent_and_resettable(){let mut s=Store::new(PathBuf::new());let id=s.create(&json!({"mode":"stopwatch"})).unwrap();s.advance(3180);s.act(&id,"lap",Some(1)).unwrap();s.advance(650);let rev=s.items[0].revision;s.act(&id,"lap",Some(rev)).unwrap();assert_eq!(s.items[0].laps,vec![3180,3830]);s.action(&id,"pause").unwrap();assert!(s.action(&id,"lap").is_err());s.action(&id,"reset").unwrap();assert!(s.items[0].laps.is_empty());assert_eq!(s.items[0].elapsed_ms,0);let other=s.create(&json!({"mode":"countdown"})).unwrap();assert!(s.action(&other,"lap").is_err());}
}

#[cfg(test)]mod direction_tests{use super::*;
#[test]fn reversing_display_keeps_phase_and_completion_deadline(){let mut s=Store::new(PathBuf::new());let id=s.create(&json!({"mode":"pomodoro","focus":10,"rounds":1})).unwrap();s.advance(3100);assert_eq!(s.items[0].clock(),"00:07");s.set_direction(&id,true,Some(1)).unwrap();assert_eq!(s.items[0].clock(),"00:03");assert_eq!(s.items[0].elapsed_ms,3100);assert!(s.items[0].running);assert!(s.set_direction(&id,false,Some(1)).is_err());assert_eq!(s.advance(6900).len(),1);assert!(s.items[0].finished);assert_eq!(s.items[0].clock(),"00:10");assert!(s.advance(5000).is_empty());}
#[test]fn count_up_workflow_still_waits_and_direction_survives_restart(){let p=std::env::temp_dir().join(format!("DotIsle.Direction.{}.json",std::process::id()));let mut s=Store::new(p.clone());let id=s.create(&json!({"mode":"workflow","countUp":true,"stages":[{"name":"专注","seconds":60},{"name":"休息","seconds":60}]})).unwrap();s.advance(60000);assert_eq!(s.items[0].clock(),"01:00");assert!(s.items[0].workflow.as_ref().unwrap().waiting);s.set_direction(&id,false,None).unwrap();assert_eq!(s.items[0].clock(),"00:00");assert!(s.items[0].workflow.as_ref().unwrap().waiting);let copy=Store::new(p.clone());assert_eq!(copy.error,None);assert!(!copy.items[0].count_up);assert!(copy.items[0].workflow.as_ref().unwrap().waiting);let _=std::fs::remove_file(p);assert!(s.create(&json!({"mode":"countdown","countUp":true})).is_err());assert!(s.create(&json!({"mode":"pomodoro","countUp":"true"})).is_err());}
}

#[cfg(test)]mod island_presentation_tests{use super::*;
#[test]fn island_actions_match_finished_and_waiting_states(){let mut s=Store::new(PathBuf::new());let id=s.create(&json!({"duration":1})).unwrap();s.advance(1000);assert_eq!(s.items[0].primary_action(),"restart");s.action(&id,"restart").unwrap();assert!(s.items[0].running&&!s.items[0].finished);assert_eq!(s.items[0].elapsed_ms,0);let flow=s.create(&json!({"mode":"workflow","stages":[{"name":"a","seconds":60},{"name":"b","seconds":60}]})).unwrap();s.advance(60000);assert_eq!(s.items[1].primary_action(),"next");s.action(&flow,"next").unwrap();assert_eq!(s.items[1].workflow.as_ref().unwrap().stage_index,1);}
#[test]fn stopwatch_expansion_shares_a_single_fractional_readout(){let mut s=Store::new(PathBuf::new());s.create(&json!({"mode":"stopwatch"})).unwrap();s.advance(3830);let panel=s.items[0].panel();assert_eq!(panel.value,"00:03");let detail=panel.timer.unwrap();assert_eq!(detail.expanded_value,"00:03.83");assert!(detail.indefinite);assert_eq!(detail.primary,"暂停");}
}

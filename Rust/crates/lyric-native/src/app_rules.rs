//! Application rules shared by "entered the app" (F05) and "foreground minutes" (F06)
//! reminders. Rules and daily usage are separate files so an edited rule can never
//! corrupt the accumulated counters.
use serde::{Serialize,Deserialize};use serde_json::{Value,json};use std::{collections::HashMap,path::{Path,PathBuf}};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub struct Rule{
 pub id:String,pub name:String,pub executable_path:String,pub trigger:String,pub message:String,
 #[serde(default)]pub cooldown_minutes:u32,#[serde(default)]pub threshold_minutes:u32,#[serde(default)]pub enabled:bool,
 #[serde(default)]pub last_fired:Option<String>,#[serde(default)]pub disabled_date:Option<String>,#[serde(default)]pub revision:u64,
}
#[derive(Clone,Debug,Serialize,Deserialize,Default)]#[serde(rename_all="camelCase")]
pub struct Usage{
 #[serde(default)]pub accumulated_ms:u64,#[serde(default)]pub snooze_target_ms:u64,#[serde(default)]pub fired_date:Option<String>,
}
#[derive(Serialize,Deserialize)]#[serde(rename_all="camelCase")]
struct UsageFile{#[serde(default)]date:String,#[serde(default)]rules:HashMap<String,Usage>}
pub type Hit=(String,String,String);
/// Case-insensitive Windows path identity. Both sides are trimmed and separators
/// are unified; the displayed path is never rewritten.
pub fn same_path(a:&str,b:&str)->bool{
 let clean=|s:&str|s.trim().trim_matches('"').replace('/',"\\").trim_end_matches('\\').to_lowercase();
 clean(a)==clean(b)
}
fn stamp(date:&str,time:&str)->i64{
 if date.len()!=10||time.len()!=5{return 0}
 let y=date[..4].parse::<i64>().unwrap_or(0);let m=date[5..7].parse::<i64>().unwrap_or(1).clamp(1,12);
 let d=date[8..10].parse::<i64>().unwrap_or(1);let h=time[..2].parse::<i64>().unwrap_or(0);let min=time[3..].parse::<i64>().unwrap_or(0);
 let mut days=0i64;for year in 2000..y{days+=if year%400==0||(year%4==0&&year%100!=0){366}else{365}}
 let leap=|year:i64|year%400==0||(year%4==0&&year%100!=0);
 for month in 1..m{days+=match month{1|3|5|7|8|10|12=>31,4|6|9|11=>30,_=>if leap(y){29}else{28}}}
 (days+d-1)*1440+h*60+min
}
pub fn valid_path(path:&str)->bool{
 let p=Path::new(path);p.is_absolute()&&p.extension().is_some_and(|e|e.eq_ignore_ascii_case("exe"))
}
pub struct Store{
 rules_path:PathBuf,usage_path:PathBuf,pub items:Vec<Rule>,pub usage:HashMap<String,Usage>,pub usage_date:String,
 pub baseline:HashMap<String,u64>,seen:HashMap<String,u64>,pub error:Option<String>,
}
fn rule_ok(r:&Rule)->bool{
 !r.id.is_empty()&&r.id.len()<=80&&r.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')&&!r.name.is_empty()&&r.name.chars().count()<=60&&!r.message.is_empty()&&r.message.chars().count()<=200&&valid_path(&r.executable_path)&&matches!(r.trigger.as_str(),"enter"|"foregroundDaily")&&(10..=240).contains(&r.threshold_minutes)&&[5,15,30,60].contains(&r.cooldown_minutes)
}
impl Store{
 pub fn new(rules_path:PathBuf,usage_path:PathBuf,today:&str)->Self{
  let mut s=Self{rules_path,usage_path,items:vec![],usage:HashMap::new(),usage_date:today.into(),baseline:HashMap::new(),seen:HashMap::new(),error:None};
  if s.rules_path.is_file(){match std::fs::read(&s.rules_path).map_err(|e|e.to_string()).and_then(|b|serde_json::from_slice::<Vec<Rule>>(&b).map_err(|e|e.to_string())){Ok(items)=>if items.len()<=32&&items.iter().all(rule_ok){s.items=items}else{s.error=Some("应用提醒规则无效，原文件已保留".into())},Err(e)=>s.error=Some(e)}}
  if s.usage_path.is_file(){if let Ok(file)=std::fs::read(&s.usage_path).map_err(|e|e.to_string()).and_then(|b|serde_json::from_slice::<UsageFile>(&b).map_err(|e|e.to_string())){s.usage_date=if file.date==today{today.into()}else{today.into()};if file.date==today{s.usage=file.rules;}}}
  s
 }
 fn persist_rules(&self)->Result<(),String>{if let Some(e)=&self.error{return Err(e.clone())}if self.rules_path.as_os_str().is_empty(){return Ok(())}lyric_backend::io::save_settings(&self.rules_path,&json!(self.items))}
 pub fn persist_usage(&self)->Result<(),String>{if self.usage_path.as_os_str().is_empty(){return Ok(())}lyric_backend::io::save_settings(&self.usage_path,&json!({"date":self.usage_date,"rules":self.usage}))}
 pub fn list(&self,today:&str)->Value{
  json!(self.items.iter().map(|r|{let u=self.usage.get(&r.id).cloned().unwrap_or_default();json!({
   "id":r.id,"name":r.name,"executablePath":r.executable_path,"trigger":r.trigger,"message":r.message,"cooldownMinutes":r.cooldown_minutes,
   "thresholdMinutes":r.threshold_minutes,"enabled":r.enabled,"lastFired":r.last_fired,"revision":r.revision,
   "todayMs":u.accumulated_ms,"snoozeTargetMs":u.snooze_target_ms,"firedToday":u.fired_date.as_deref()==Some(today),"disabledToday":r.disabled_date.as_deref()==Some(today)
  })}).collect::<Vec<_>>())
 }
 pub fn save(&mut self,v:&Value,_today:&str,current:Option<&str>,segment:u64)->Result<String,String>{
  let text=|key:&str,max:usize|->Result<String,String>{let s=v[key].as_str().unwrap_or("").trim().to_owned();if s.is_empty(){return Err(if key=="name"{"请填写规则名称"}else if key=="message"{"请填写提醒内容"}else{"请选择要提醒的软件"}.into())}if s.chars().count()>max||s.chars().any(char::is_control){return Err("规则内容过长或包含无效字符".into())}Ok(s)};
  let name=text("name",60)?;let message=text("message",200)?;let path=text("executablePath",260)?;if !valid_path(&path){return Err("请选择一个本地 .exe 程序".into())}
  let trigger=v["trigger"].as_str().unwrap_or("enter");if !matches!(trigger,"enter"|"foregroundDaily"){return Err("未知的提醒触发方式".into())}
  let cooldown=v["cooldownMinutes"].as_u64().unwrap_or(30)as u32;if !matches!(cooldown,5|15|30|60){return Err("冷却时间仅支持 5 / 15 / 30 / 60 分钟".into())}
  let threshold=v["thresholdMinutes"].as_u64().unwrap_or(60)as u32;if !(10..=240).contains(&threshold){return Err("前台累计阈值需要 10 到 240 分钟".into())}
  let enabled=v["enabled"].as_bool().unwrap_or(true);let requested=v["id"].as_str().unwrap_or("");let old=self.items.clone();
  let id=if requested.is_empty(){
   if self.items.len()>=32{return Err("最多保存 32 条应用提醒规则".into())}
   let id=format!("rule-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
   self.items.push(Rule{id:id.clone(),name,executable_path:path.clone(),trigger:trigger.into(),message,cooldown_minutes:cooldown,threshold_minutes:threshold,enabled,last_fired:None,disabled_date:None,revision:1});id
  }else{
   let rule=self.items.iter_mut().find(|r|r.id==requested).ok_or("应用提醒规则不存在")?;
   if !same_path(&rule.executable_path,&path)||rule.trigger!=trigger{self.usage.remove(requested);rule.last_fired=None;rule.disabled_date=None;}
   rule.name=name;rule.executable_path=path.clone();rule.trigger=trigger.into();rule.message=message;rule.cooldown_minutes=cooldown;rule.threshold_minutes=threshold;rule.enabled=enabled;rule.revision+=1;requested.into()
  };
  if current.is_some_and(|p|same_path(p,&path)){self.baseline.insert(id.clone(),segment);}
  if let Err(e)=self.persist_rules(){self.items=old;return Err(e)}Ok(id)
 }
 /// Advance the stored local day, discarding yesterday's counters.
 pub fn roll_day(&mut self,today:&str){if self.usage_date!=today{self.usage_date=today.into();self.usage.clear();self.seen.clear();}}
 pub fn baseline_now(&mut self,path:Option<&str>,segment:u64){let Some(path)=path else{return};for r in &self.items{if r.trigger=="enter"&&same_path(&r.executable_path,path){self.baseline.insert(r.id.clone(),segment);}}}
 pub fn action(&mut self,id:&str,action:&str,revision:Option<u64>,today:&str)->Result<(),String>{
  let old=self.items.clone();
  if action=="delete"{if !self.items.iter().any(|r|r.id==id){return Err("应用提醒规则不存在".into())}self.items.retain(|r|r.id!=id);self.usage.remove(id);if let Err(e)=self.persist_rules(){self.items=old;return Err(e)}let _=self.persist_usage();return Ok(())}
  let rule=self.items.iter_mut().find(|r|r.id==id).ok_or("应用提醒规则不存在")?;
  if let Some(expected)=revision{if expected!=rule.revision{return Err("规则已变化，请刷新后重试".into())}}
  match action{
   "enable"=>rule.enabled=true,"disable"=>rule.enabled=false,"disableToday"=>{if rule.trigger!="enter"{return Err("该操作仅适用于进入软件提醒".into())}rule.disabled_date=Some(today.into());},
   "snoozeForeground10"=>{if rule.trigger!="foregroundDaily"{return Err("稍后提醒仅适用于前台时长提醒".into())}let usage=self.usage.entry(id.into()).or_default();usage.snooze_target_ms=usage.accumulated_ms.saturating_add(600000);usage.fired_date=None;},
   _=>return Err("未知的规则操作".into())
  }
  rule.revision+=1;if let Err(e)=self.persist_rules(){self.items=old;return Err(e)}let _=self.persist_usage();Ok(())
 }
 pub fn accumulate(&mut self,path:&str,delta_ms:u64,today:&str){
  if delta_ms==0{return}
  for r in &self.items{if r.trigger=="foregroundDaily"&&r.enabled&&r.disabled_date.as_deref()!=Some(today)&&same_path(&r.executable_path,path){let usage=self.usage.entry(r.id.clone()).or_default();usage.accumulated_ms=usage.accumulated_ms.saturating_add(delta_ms);}}
 }
 /// Fire every daily rule whose target has been reached; once per local day, and
 /// only while the target is still in the foreground.
 pub fn due_usage(&mut self,path:&str,today:&str)->Vec<Hit>{
  let due=self.items.iter().filter(|r|r.trigger=="foregroundDaily"&&r.enabled&&r.disabled_date.as_deref()!=Some(today)&&same_path(&r.executable_path,path)).map(|r|(r.id.clone(),r.name.clone(),r.message.clone(),r.threshold_minutes as u64*60000)).collect::<Vec<_>>();
  let mut hits=vec![];let mut changed=false;
  for (id,name,message,threshold) in due{let usage=self.usage.entry(id.clone()).or_default();let target=threshold.max(usage.snooze_target_ms);if usage.accumulated_ms>=target&&usage.fired_date.as_deref()!=Some(today){usage.fired_date=Some(today.into());hits.push((id,name,message));changed=true;}}
  if changed{let _=self.persist_usage();}hits
 }
 /// Report rules whose application just entered the foreground. `allow` is false
 /// while notifications are paused: the segment still gets consumed so the event
 /// is never replayed later.
 pub fn enter_hits(&mut self,path:&str,segment:u64,today:&str,now:&crate::todos::Now,allow:bool)->Vec<Hit>{
  let candidates=self.items.iter().filter(|r|r.trigger=="enter"&&r.enabled&&r.disabled_date.as_deref()!=Some(today)&&same_path(&r.executable_path,path)).map(|r|(r.id.clone(),r.name.clone(),r.message.clone(),r.cooldown_minutes,r.last_fired.clone())).collect::<Vec<_>>();
  let now_stamp=stamp(&now.date,&now.time);let mut hits=vec![];let mut fired=vec![];
  for (id,name,message,cooldown,last) in candidates{
   if self.baseline.get(&id)==Some(&segment){continue}
   if self.seen.get(&id)==Some(&segment){continue}
   self.seen.insert(id.clone(),segment);
   if !allow{continue}
   if let Some(last)=&last{let diff=now_stamp-stamp(&last[..10.min(last.len())],last.get(11..16).unwrap_or("00:00"));if diff<0{fired.push(id.clone());continue}if diff<(cooldown as i64){continue}}
   fired.push(id.clone());hits.push((id,name,message));
  }
  for id in fired{if let Some(rule)=self.items.iter_mut().find(|r|r.id==id){rule.last_fired=Some(format!("{} {}",now.date,now.time));}}
  if !hits.is_empty(){let changed=hits.len();if let Err(e)=self.persist_rules(){self.error=Some(e);}if changed>0{let _=self.persist_usage();}}
  hits
 }
 pub fn clear_usage(&mut self)->Result<(),String>{self.usage.clear();self.persist_usage()}
}
#[cfg(test)]mod tests{use super::*;
 fn dir()->PathBuf{let p=std::env::temp_dir().join(format!("LyricIsland.Rules.{}.{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(&p).unwrap();p}
 fn store()->(Store,PathBuf){let d=dir();(Store::new(d.join("rules.json"),d.join("usage.json"),"2026-10-17"),d)}
 fn now()->crate::todos::Now{crate::todos::Now{date:"2026-10-17".into(),time:"09:00".into()}}
 fn rule(path:&str,trigger:&str)->Value{json!({"name":"导出前检查","executablePath":path,"trigger":trigger,"message":"检查导出目录","cooldownMinutes":30,"thresholdMinutes":10,"enabled":true})}
 #[test]fn paths_are_matched_case_insensitively_and_by_full_path(){assert!(same_path("C:/Tools/Editor.exe","c:/tools/editor.EXE"));assert!(!same_path("C:/A/Editor.exe","C:/B/Editor.exe"));assert!(valid_path("C:/Tools/Editor.exe"));assert!(!valid_path("Editor.exe"));assert!(!valid_path("C:/Tools/Editor.txt"));}
 #[test]fn enter_fires_once_per_segment_and_respects_cooldown(){let(mut s,d)=store();let id=s.save(&rule("C:/Tools/Editor.exe","enter"),"2026-10-17",None,0).unwrap();assert_eq!(s.enter_hits("C:/Tools/Editor.exe",2,"2026-10-17",&now(),true).len(),1);assert!(s.enter_hits("C:/Tools/Editor.exe",2,"2026-10-17",&now(),true).is_empty());assert!(s.enter_hits("C:/Tools/Editor.exe",3,"2026-10-17",&now(),true).is_empty());s.items[0].last_fired=Some("2026-10-17 08:00".into());assert_eq!(s.enter_hits("C:/Tools/Editor.exe",4,"2026-10-17",&now(),true).len(),1);assert!(s.action(&id,"delete",None,"2026-10-17").is_ok());assert!(s.items.is_empty());let _=std::fs::remove_dir_all(d);}
 #[test]fn creating_a_rule_while_the_app_is_foreground_only_sets_a_baseline(){let(mut s,d)=store();s.save(&rule("C:/Tools/Editor.exe","enter"),"2026-10-17",Some("C:/Tools/Editor.exe"),7).unwrap();assert!(s.enter_hits("C:/Tools/Editor.exe",7,"2026-10-17",&now(),true).is_empty());assert_eq!(s.enter_hits("C:/Tools/Editor.exe",8,"2026-10-17",&now(),true).len(),1);let _=std::fs::remove_dir_all(d);}
 #[test]fn paused_enters_are_consumed_not_replayed(){let(mut s,d)=store();s.save(&rule("C:/Tools/Editor.exe","enter"),"2026-10-17",None,0).unwrap();assert!(s.enter_hits("C:/Tools/Editor.exe",2,"2026-10-17",&now(),false).is_empty());assert!(s.enter_hits("C:/Tools/Editor.exe",2,"2026-10-17",&now(),true).is_empty());assert_eq!(s.enter_hits("C:/Tools/Editor.exe",3,"2026-10-17",&now(),true).len(),1);let _=std::fs::remove_dir_all(d);}
 #[test]fn daily_usage_fires_once_then_snooze_requires_ten_more_minutes(){let(mut s,d)=store();let id=s.save(&rule("C:/Tools/Editor.exe","foregroundDaily"),"2026-10-17",None,0).unwrap();s.accumulate("C:/Tools/Editor.exe",599999,"2026-10-17");assert!(s.due_usage("C:/Tools/Editor.exe","2026-10-17").is_empty());s.accumulate("C:/Tools/Editor.exe",1,"2026-10-17");assert_eq!(s.due_usage("C:/Tools/Editor.exe","2026-10-17").len(),1);s.accumulate("C:/Tools/Editor.exe",3600000,"2026-10-17");assert!(s.due_usage("C:/Tools/Editor.exe","2026-10-17").is_empty());s.action(&id,"snoozeForeground10",None,"2026-10-17").unwrap();assert!(s.due_usage("C:/Tools/Editor.exe","2026-10-17").is_empty());s.accumulate("C:/Tools/Editor.exe",600000,"2026-10-17");assert_eq!(s.due_usage("C:/Tools/Editor.exe","2026-10-17").len(),1);let _=std::fs::remove_dir_all(d);}
 #[test]fn usage_rolls_over_and_restores_todays_checkpoint(){let d=dir();let rules=d.join("rules.json");let usage=d.join("usage.json");let id={let mut s=Store::new(rules.clone(),usage.clone(),"2026-10-17");s.save(&rule("C:/Tools/Editor.exe","foregroundDaily"),"2026-10-17",None,0).unwrap()};{let mut s=Store::new(rules.clone(),usage.clone(),"2026-10-17");s.accumulate("C:/Tools/Editor.exe",120000,"2026-10-17");s.persist_usage().unwrap();}let restored=Store::new(rules.clone(),usage.clone(),"2026-10-17");assert_eq!(restored.usage.get(&id).unwrap().accumulated_ms,120000);let next=Store::new(rules,usage,"2026-10-18");assert!(next.usage.get(&id).is_none());let _=std::fs::remove_dir_all(d);}
 #[test]fn rules_validate_and_share_the_thirty_two_entry_cap(){let(mut s,d)=store();assert!(s.save(&json!({"name":"x","executablePath":"relative.exe","trigger":"enter","message":"x"}),"2026-10-17",None,0).is_err());assert!(s.save(&json!({"name":"x","executablePath":"C:/a.exe","trigger":"enter","message":"x","cooldownMinutes":7}),"2026-10-17",None,0).is_err());assert!(s.save(&json!({"name":"x","executablePath":"C:/a.exe","trigger":"foregroundDaily","message":"x","thresholdMinutes":4}),"2026-10-17",None,0).is_err());for _ in 0..32{s.save(&rule("C:/Tools/Editor.exe","enter"),"2026-10-17",None,0).unwrap();}assert!(s.save(&rule("C:/Tools/Editor.exe","enter"),"2026-10-17",None,0).is_err());let _=std::fs::remove_dir_all(d);}
}

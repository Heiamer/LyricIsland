//! Bounded notification history for the control center. Content is transient:
//! it is never logged or uploaded, and only events that reached the user are kept.
use serde::{Serialize,Deserialize};use serde_json::{Value,json};use std::path::PathBuf;
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub struct Event{pub id:String,pub kind:String,pub app_id:String,pub app_name:String,pub title:String,pub body:String,pub date:String,pub clock:String,pub t:i64,#[serde(default)]pub todo_id:Option<String>}
pub const LIMIT:usize=200;
pub fn kinds()->[&'static str;5]{["notice","todo","device","timer","automation"]}
pub struct Store{path:PathBuf,pub items:Vec<Event>,pub error:Option<String>}
impl Store{
 pub fn new(path:PathBuf)->Self{let (items,error)=if path.is_file(){match std::fs::read_to_string(&path).map_err(|e|e.to_string()).and_then(|s|serde_json::from_str::<Vec<Event>>(&s).map_err(|e|e.to_string())){Ok(mut items)=>{items.truncate(LIMIT);(items,None)},Err(e)=>(vec![],Some(format!("通知历史读取失败，将重新开始记录：{e}")))}}else{(vec![],None)};Self{path,items,error}}
 fn persist(&mut self){if self.path.as_os_str().is_empty(){return}if let Err(e)=lyric_backend::io::save_settings(&self.path,&json!(self.items)){self.error=Some(e);}}
 /// Record one delivered event. Callers only pass events that actually reached the user.
 pub fn record(&mut self,kind:&str,app_id:&str,app_name:&str,title:&str,body:&str,todo_id:Option<String>){
  let nanos=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();let now=crate::todos::Now::local();
  let cut=|s:&str,max:usize|if s.chars().count()>max{s.chars().take(max).collect::<String>()}else{s.to_owned()};
  if self.items.len()>=LIMIT{self.items.remove(0);}
  self.items.push(Event{id:format!("h-{nanos}"),kind:kind.into(),app_id:cut(app_id,300),app_name:cut(app_name,80),title:cut(title,300),body:cut(body,600),date:now.date,clock:now.time,t:(nanos/1_000_000_000)as i64,todo_id});self.persist();
 }
 pub fn prune(&mut self,days:u32,now:i64){let cutoff=now-(days as i64).max(1)*86400;let before=self.items.len();self.items.retain(|e|e.t>=cutoff);if self.items.len()!=before{self.persist();}}
 pub fn clear(&mut self,kind:Option<&str>){match kind{Some(k)=>self.items.retain(|e|e.kind!=k),None=>self.items.clear()}self.persist();}
 pub fn list(&self)->Value{json!(self.items.iter().rev().collect::<Vec<_>>())}
}
#[cfg(test)]mod tests{use super::*;fn fixture()->(Store,PathBuf){let p=std::env::temp_dir().join(format!("LyricIsland.History.{}.{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));(Store::new(p.clone()),p)}
 #[test]fn newest_first_and_bounded(){let (mut s,p)=fixture();for i in 0..(LIMIT+5){s.record("notice","Example.App","Example",&format!("t{i}"),"body",None);}assert_eq!(s.items.len(),LIMIT);let list=s.list();let rows=list.as_array().unwrap();assert_eq!(rows[0]["title"],format!("t{}",LIMIT+4));assert_eq!(rows[LIMIT-1]["title"],"t5");let _=std::fs::remove_file(p);}
 #[test]fn survives_restart_and_prunes_by_age(){let (mut s,p)=fixture();s.record("todo","LyricIsland.Todo","待办","交稿","今天 09:00",Some("todo-1".into()));let mut s=Store::new(p.clone());assert_eq!(s.items.len(),1);assert_eq!(s.items[0].todo_id.as_deref(),Some("todo-1"));let old=s.items[0].t;let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()as i64;s.prune(7,now);assert_eq!(s.items.len(),1);s.prune(7,old+8*86400);assert!(s.items.is_empty());let _=std::fs::remove_file(p);}
 #[test]fn clear_targets_one_kind_or_everything(){let (mut s,p)=fixture();s.record("notice","a","a","n","",None);s.record("device","b","b","d","",None);s.clear(Some("notice"));assert_eq!(s.items.len(),1);assert_eq!(s.items[0].kind,"device");s.clear(None);assert!(s.items.is_empty());let _=std::fs::remove_file(p);}
 #[test]fn corrupt_file_restarts_and_content_is_capped(){let p=std::env::temp_dir().join(format!("LyricIsland.HistoryCorrupt.{}.json",std::process::id()));std::fs::write(&p,b"broken").unwrap();let mut s=Store::new(p.clone());assert!(s.error.is_some());s.record("notice","a","a","t",&"x".repeat(700),None);assert_eq!(s.items[0].body.chars().count(),600);assert_eq!(s.items[0].kind,"notice");let _=std::fs::remove_file(p);}
}

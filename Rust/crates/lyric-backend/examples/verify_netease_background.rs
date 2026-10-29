//! Production queue/native-timeline stress test, with position and playback restoration.
use std::{path::PathBuf,time::{Instant,Duration},sync::Arc};
use serde_json::{json,Value};
use lyric_backend::runtime::Backend;
fn command(backend:&Backend,id:u64)->Value{
 let until=Instant::now()+Duration::from_secs(5);
 loop{let s=backend.snapshot(u64::MAX);let c=&s["mediaCommand"];if c["id"]==id&&matches!(c["state"].as_str(),Some("confirmed"|"failed")){return json!({"command":c,"position":s["positionSeconds"],"playing":s["isPlaying"],"songId":s["track"]["songId"]})}if Instant::now()>until{return json!({"timeout":true,"command":c})}std::thread::sleep(Duration::from_millis(20));}
}
struct Restore{backend:Arc<Backend>,id:Value,position:f64,playing:bool,started:Instant,armed:bool}
impl Restore{fn finish(&mut self)->Value{
 if !self.armed{return json!({"notRequired":true})}self.armed=false;
 if self.backend.snapshot(u64::MAX)["track"]["songId"]!=self.id{return json!({"skipped":"song_changed"})}
 let mut result=json!({});
 result["playbackRestored"]=json!(self.backend.command(if self.playing{"play"}else{"pause"},0.).unwrap_or(false));
 let until=Instant::now()+Duration::from_secs(2);while self.backend.snapshot(u64::MAX)["isPlaying"]!=self.playing&&Instant::now()<until{std::thread::sleep(Duration::from_millis(30));}
 let target=self.position+if self.playing{self.started.elapsed().as_secs_f64()}else{0.};
 result["position"]=match self.backend.enqueue("seek",target){Ok(id)=>command(&self.backend,id),Err(e)=>json!({"error":e})};result
}}
impl Drop for Restore{fn drop(&mut self){if self.armed{eprintln!("Emergency restoration: {}",self.finish());}}}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<String>=std::env::args().collect();if args.get(1).map(String::as_str)!=Some("--authorized-and-restore"){return Err("Authorization flag required".into())}
 let dir=PathBuf::from(args.get(2).ok_or("Missing evidence directory")?);std::fs::create_dir_all(&dir)?;
 let local=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
 // Deliberately choose an unopened port: all successful commands must use the background route.
 let backend=Backend::new(json!({"AllowedSources":["Netease"],"IgnoredSources":[],"OnlineSongInfo":false,"LyricSourceOrder":[],"NeteaseCdpPort":65534}),dir.join("Data"),local,dir.join("Logs"));
 let until=Instant::now()+Duration::from_secs(10);
 let mut before=loop{let s=backend.snapshot(u64::MAX);if s["canSeek"]==true&&s["rawSeconds"].as_f64().is_some(){break s}if Instant::now()>until{std::fs::write(dir.join("precheck.json"),serde_json::to_vec_pretty(&s)?)?;backend.close();return Err("No background capability; no command sent".into())}std::thread::sleep(Duration::from_millis(80));};
 if args.iter().any(|s|s=="--play-first"){let _=backend.command("play",0.);let until=Instant::now()+Duration::from_secs(2);while backend.snapshot(u64::MAX)["isPlaying"]!=true&&Instant::now()<until{std::thread::sleep(Duration::from_millis(30));}before=backend.snapshot(u64::MAX);}
 let position=before["positionSeconds"].as_f64().unwrap();let duration=before["track"]["durationSeconds"].as_f64().unwrap();
 let mut restore=Restore{backend:backend.clone(),id:before["track"]["songId"].clone(),position,playing:before["isPlaying"]==true,started:Instant::now(),armed:true};
 let mut results=vec![];
 for offset in [8.,-6.,14.,-10.,3.,-3.,10.,-8.,6.,-5.]{
  let target=(position+offset).clamp(1.,duration-2.);let r=match backend.enqueue("seek",target){Ok(id)=>command(&backend,id),Err(e)=>json!({"error":e})};let ok=r["command"]["state"]=="confirmed"&&r["command"]["route"]=="NeteaseBackground";results.push(r);if !ok{break}
 }
 let pause=backend.command("pause",0.).unwrap_or(false);
 if pause{let until=Instant::now()+Duration::from_secs(2);while backend.snapshot(u64::MAX)["isPlaying"]!=false&&Instant::now()<until{std::thread::sleep(Duration::from_millis(30));}}
 let paused=if pause{let target=(position+4.).clamp(1.,duration-2.);match backend.enqueue("seek",target){Ok(id)=>command(&backend,id),Err(e)=>json!({"error":e})}}else{json!({"error":"SMTC pause unavailable"})};
 let restored=restore.finish();backend.close();
 let success=results.len()==10&&results.iter().all(|r|r["command"]["state"]=="confirmed"&&r["command"]["route"]=="NeteaseBackground")&&paused["command"]["state"]=="confirmed"&&paused["playing"]==false&&restored["position"]["command"]["state"]=="confirmed";
 let report=json!({"attention":lyric_backend::netease_control::attention_diagnostics(),"success":success,"scope":"production Backend with intentionally unused CDP port, 10 alternating seeks, paused seek and restoration","before":{"songId":restore.id,"position":position,"playing":restore.playing},"results":results,"pauseConfirmed":pause,"pausedSeek":paused,"restored":restored});
 std::fs::write(dir.join("report.json"),serde_json::to_vec_pretty(&report)?)?;println!("{}",json!({"success":success,"count":results.len(),"pausedSeek":paused,"restored":restored}));
 if !success{return Err("Background acceptance failed; see report".into())}Ok(())
}

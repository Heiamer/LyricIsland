//! Explicitly authorized one-seek-and-restore acceptance. Uses the production backend.
use std::{path::PathBuf,time::{Instant,Duration},sync::Arc};
use serde_json::{Value,json};
use lyric_backend::runtime::Backend;
fn wait(backend:&Backend,id:u64)->Value{
    let until=Instant::now()+Duration::from_secs(6);
    loop{let s=backend.snapshot(u64::MAX);let command=&s["mediaCommand"];if command["id"].as_u64()==Some(id)&&matches!(command["state"].as_str(),Some("confirmed"|"failed")){return json!({"command":command,"position":s["positionSeconds"],"playing":s["isPlaying"],"track":s["track"]})}if Instant::now()>=until{return json!({"timeout":true,"command":command,"position":s["positionSeconds"]})}std::thread::sleep(Duration::from_millis(40));}
}
struct Restore{backend:Arc<Backend>,track:Value,position:f64,playing:bool,started:Instant,armed:bool}
impl Restore{
 fn restore(&mut self)->Value{
    if !self.armed{return json!({"notRequired":true})}self.armed=false;
    let current=self.backend.snapshot(u64::MAX);
    if current["track"]["id"]!=self.track["id"]||current["track"]["sourceApp"]!=self.track["sourceApp"]{return json!({"skipped":"track_changed","track":current["track"]})}
    let target=self.position+if self.playing{self.started.elapsed().as_secs_f64()}else{0.};
    match self.backend.enqueue("seek",target){Ok(id)=>wait(&self.backend,id),Err(error)=>json!({"error":error})}
 }
}
impl Drop for Restore{fn drop(&mut self){if self.armed{let result=self.restore();eprintln!("Emergency restore: {}",result);}}}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<String>=std::env::args().collect();if args.get(1).map(String::as_str)!=Some("--authorized-once-and-restore"){return Err("Explicit authorization flag required".into())}
 let directory=PathBuf::from(args.get(2).ok_or("missing output directory")?);std::fs::create_dir_all(&directory)?;
 let local=PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?);
 let backend=Backend::new(json!({"AllowedSources":["Netease"],"IgnoredSources":[],"OnlineSongInfo":false,"LyricSourceOrder":[]}),directory.join("Data"),local,directory.join("Logs"));
 let until=Instant::now()+Duration::from_secs(8);
 let before=loop{let s=backend.snapshot(u64::MAX);if s["canSeek"]==true&&s["track"]["sourceApp"]=="Netease"{break s}if Instant::now()>=until{backend.close();std::fs::write(directory.join("report.json"),serde_json::to_vec_pretty(&json!({"success":false,"readOnlyPrecheckFailed":true,"state":s}))?)?;return Err("NetEase seek capability unavailable; no playback command sent".into())}std::thread::sleep(Duration::from_millis(100));};
 let duration=before["track"]["durationSeconds"].as_f64().ok_or("missing duration")?;let position=before["positionSeconds"].as_f64().ok_or("missing position")?;
 let target=if position+12.<duration-5.{position+12.}else{(position-12.).max(0.)};
 let mut restore=Restore{backend:backend.clone(),track:before["track"].clone(),position,playing:before["isPlaying"]==true,started:Instant::now(),armed:true};
 println!("Seeking once: {:.3} -> {:.3} seconds",position,target);
 let jump=match backend.enqueue("seek",target){Ok(id)=>wait(&backend,id),Err(error)=>json!({"error":error})};
 println!("Jump result: {}",jump);
 let restored=restore.restore();println!("Restore result: {}",restored);backend.close();
 let success=jump["command"]["state"]=="confirmed"&&restored["command"]["state"]=="confirmed"&&jump["playing"]==before["isPlaying"]&&restored["playing"]==before["isPlaying"];
 let report=json!({"success":success,"scope":"one real loopback CDP seek and restore, production Backend queue and real native timeline acknowledgements; no OS pointer or keyboard input","before":{"track":before["track"],"position":position,"playing":before["isPlaying"],"canSeek":before["canSeek"]},"requested":target,"jump":jump,"restore":restored});
 std::fs::write(directory.join("report.json"),serde_json::to_vec_pretty(&report)?)?;
 if !success{return Err("Live seek/restore did not both confirm; inspect report".into())}Ok(())
}

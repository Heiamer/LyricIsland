//! Settings transport. The island process remains the sole owner of clocks, reminders and media.
use std::{sync::mpsc,time::Duration};
use serde_json::{Value,json};
pub struct Packet{pub state:Value,pub reply:Option<(String,Value)>}
pub struct Remote{wake:Option<std::sync::Arc<lyric_backend::io::SettingsWake>>,pub send:mpsc::Sender<(String,Value)>,pub recv:mpsc::Receiver<Result<Packet,String>>}
pub fn request(command:&str,value:Value)->Result<Value,String>{let pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into());lyric_backend::io::pipe_request(&pipe,&json!({"version":1,"command":command,"value":value}))}
impl Remote{
    pub fn new(data:std::path::PathBuf)->Self{let(send,commands)=mpsc::channel::<(String,Value)>();let(out,recv)=mpsc::channel();let wake=lyric_backend::io::SettingsWake::new().map(std::sync::Arc::new);let waiter=wake.clone();std::thread::spawn(move||{
        // Connect immediately. A missing native process is started once on this worker.
        if let Err(_) = request("state",Value::Null){
            if let Some(native)=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("LyricIsland.exe"))).filter(|p|p.is_file()){
                use std::os::windows::process::CommandExt;
                let _=std::process::Command::new(native).arg("--from-settings").arg("--data-dir").arg(data).creation_flags(0x08000000).spawn();
            }
        }
        if let Ok(state)=request("state",Value::Null){if out.send(Ok(Packet{state,reply:None})).is_err(){return}}
        crate::shell::wake();
        loop{
        // Drain queued commands before waiting. Auto-reset events coalesce signals,
        // so waiting once per command adds 240 ms for every queued slider change.
        let command=match commands.try_recv(){Ok(v)=>Some(v),Err(mpsc::TryRecvError::Disconnected)=>break,Err(mpsc::TryRecvError::Empty)=>{
            if let Some(w)=&waiter{w.wait(240);}else{std::thread::sleep(Duration::from_millis(240));}
            commands.try_recv().ok()
        }};
        let reply=match command.ok_or(mpsc::TryRecvError::Empty){Ok((command,value))=>match request(&command,value){Ok(reply)=>Some((command,reply)),Err(e)=>{if out.send(Err(e)).is_err(){break}continue}},Err(mpsc::TryRecvError::Disconnected)=>break,_=>None};
        let packet=request("state",Value::Null).map(|state|Packet{state,reply});if out.send(packet).is_err(){break}crate::shell::wake();
    }});Self{send,recv,wake}}
    pub fn command(&self,name:&str,value:Value){let _=self.send.send((name.into(),value));if let Some(w)=&self.wake{w.signal();}}
}

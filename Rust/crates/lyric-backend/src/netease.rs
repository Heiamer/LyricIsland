//! Read-only NetEase timeline port. Signature/field sources: AF-Media-Bar and Kxnrl (MIT).
use std::{path::PathBuf,time::{Instant,Duration},sync::mpsc};
use windows::{Win32::{Foundation::{HANDLE,CloseHandle},System::{Diagnostics::{ToolHelp::*,Debug::ReadProcessMemory},Threading::{OpenProcess,PROCESS_VM_READ}}}};
use crate::{http::{self,RequestContext},media::unix_ms,providers::text};

const PLAYER:&str="48 8D 0D ? ? ? ? E8 ? ? ? ? 48 8D 0D ? ? ? ? E8 ? ? ? ? 90 48 8D 0D ? ? ? ? E8 ? ? ? ? 48 8D 05 ? ? ? ? 48 8D A5 ? ? ? ? 5F 5D C3 CC CC CC CC CC 48 89 4C 24 ? 55 57 48 81 EC ? ? ? ? 48 8D 6C 24 ? 48 8D 7C 24";
const SCHEDULE:&str="66 0F 2E 0D ? ? ? ? 7A ? 75 ? 66 0F 2E 15";
struct Owned(HANDLE);impl Drop for Owned{fn drop(&mut self){unsafe{let _=CloseHandle(self.0);}}}unsafe impl Send for Owned{}
fn wide(s:&[u16])->String{String::from_utf16_lossy(&s[..s.iter().position(|c|*c==0).unwrap_or(s.len())])}
#[derive(Clone,serde::Serialize)]#[serde(rename_all="camelCase")]
pub struct TimingSample{pub song_id:String,pub title:String,pub position:f64,pub duration:f64,pub playing:bool,pub captured_ms:i64,pub cover_url:Option<String>}
struct Reader{process:Owned,player:usize,schedule:usize,metadata_id:String,title:String,cover:Option<String>,next_metadata:Instant,remote:Option<mpsc::Receiver<(String,String,Option<String>)>>,next_remote:Instant}
impl Reader{
    fn attach()->Result<Self,String>{unsafe{
        let snapshot=Owned(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0).map_err(|e|e.to_string())?);
        let mut entry=PROCESSENTRY32W::default();entry.dwSize=std::mem::size_of_val(&entry) as u32;
        let mut valid=Process32FirstW(snapshot.0,&mut entry).is_ok();
        while valid{
            if wide(&entry.szExeFile).eq_ignore_ascii_case("cloudmusic.exe"){
                if let Ok(mods)=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,entry.th32ProcessID){
                    let mods=Owned(mods);let mut module=MODULEENTRY32W::default();module.dwSize=std::mem::size_of_val(&module) as u32;
                    let mut more=Module32FirstW(mods.0,&mut module).is_ok();
                    while more{
                        if wide(&module.szModule).eq_ignore_ascii_case("cloudmusic.dll"){
                            if let Ok(reader)=Self::from_module(entry.th32ProcessID,module.modBaseAddr as usize,wide(&module.szExePath)){return Ok(reader)}
                        }
                        more=Module32NextW(mods.0,&mut module).is_ok();
                    }
                }
            }
            valid=Process32NextW(snapshot.0,&mut entry).is_ok();
        }
        Err("No supported NetEase player process".into())
    }}
    fn from_module(pid:u32,base:usize,path:String)->Result<Self,String>{
        let process=Owned(unsafe{OpenProcess(PROCESS_VM_READ,false,pid)}.map_err(|e|e.to_string())?);
        let mut file=std::fs::File::open(&path).map_err(|e|e.to_string())?;use std::io::{Read,Seek,SeekFrom};
        let mut dos=[0u8;64];file.read_exact(&mut dos).map_err(|e|e.to_string())?;
        let pe=u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
        file.seek(SeekFrom::Start(pe)).map_err(|e|e.to_string())?;let mut coff=[0u8;24];file.read_exact(&mut coff).map_err(|e|e.to_string())?;
        if &coff[..4]!=b"PE\0\0"{return Err("invalid PE".into())}
        let sections=u16::from_le_bytes(coff[6..8].try_into().unwrap());let optional=u16::from_le_bytes(coff[20..22].try_into().unwrap());
        file.seek(SeekFrom::Start(pe+24+optional as u64)).map_err(|e|e.to_string())?;
        for _ in 0..sections.min(96){let mut section=[0u8;40];file.read_exact(&mut section).map_err(|e|e.to_string())?;
            if &section[..5]!=b".text"{continue}
            let value=|at|u32::from_le_bytes(section[at..at+4].try_into().unwrap());let rva=value(12);let length=value(16);let offset=value(20);
            let p=lyric_core::memory_scan::scan_rip(&path,offset as u64,length as u64,rva as i64,PLAYER,3).map_err(|e|e.to_string())?;
            let s=lyric_core::memory_scan::scan_rip(&path,offset as u64,length as u64,rva as i64,SCHEDULE,4).map_err(|e|e.to_string())?;
            return Ok(Self{process,player:(base as i64+p) as usize,schedule:(base as i64+s) as usize,metadata_id:String::new(),title:String::new(),cover:None,next_metadata:Instant::now(),remote:None,next_remote:Instant::now()})
        }
        Err("missing executable section".into())
    }
    fn bytes<const N:usize>(&self,address:usize)->Result<[u8;N],String>{
        if address==0{return Err("invalid player address".into())}let mut output=[0u8;N];let mut read=0;
        unsafe{ReadProcessMemory(self.process.0,address as _,output.as_mut_ptr() as _,N,Some(&mut read))}.map_err(|e|e.to_string())?;
        if read!=N{return Err("incomplete player field".into())}Ok(output)
    }
    fn id(&self)->Result<String,String>{
        let info=u64::from_le_bytes(self.bytes::<8>(self.player+0x50)?) as usize;if info==0{return Err("missing track".into())}
        let text=info+0x10;let length=u64::from_le_bytes(self.bytes::<8>(text+0x10)?) as usize;if !(1..=128).contains(&length){return Err("invalid identifier length".into())}
        let address=if length<=15{text}else{u64::from_le_bytes(self.bytes::<8>(text)?) as usize};
        let mut buffer=vec![0u8;length];let mut read=0;
        unsafe{ReadProcessMemory(self.process.0,address as _,buffer.as_mut_ptr() as _,length,Some(&mut read))}.map_err(|e|e.to_string())?;
        let id=String::from_utf8_lossy(&buffer).split('_').next().unwrap_or("").trim_end_matches('\0').to_string();
        if id.is_empty()||id.len()>20||!id.bytes().all(|b|b.is_ascii_digit()){return Err("invalid song ID".into())}Ok(id)
    }
    fn read(&mut self,local:&PathBuf,online:bool)->Result<Option<TimingSample>,String>{
        let state=i32::from_le_bytes(self.bytes::<4>(self.player+0x60)?);if state!=1&&state!=2{return Ok(None)}
        let id=self.id()?;let position=f64::from_le_bytes(self.bytes::<8>(self.schedule)?);let duration=f64::from_le_bytes(self.bytes::<8>(self.player+0xA8)?);
        if self.metadata_id!=id || self.title.is_empty()&&Instant::now()>=self.next_metadata{
            if self.metadata_id!=id{self.title.clear();self.cover=None;self.remote=None;self.next_remote=Instant::now()}
            self.metadata_id=id.clone();self.next_metadata=Instant::now()+Duration::from_secs(1);
            let (title,cover)=local_metadata(local,&id);if !title.is_empty(){self.title=title}if cover.is_some(){self.cover=cover}
        }
        if online&&(self.title.is_empty()||self.cover.is_none()){
            if let Some(r)=self.remote.as_ref(){if let Ok((remote_id,title,cover))=r.try_recv(){if remote_id==id{if !title.is_empty(){self.title=title}if cover.is_some(){self.cover=cover}}self.remote=None;}}
            if self.remote.is_none()&&Instant::now()>=self.next_remote{
                let (tx,rx)=mpsc::channel();self.remote=Some(rx);self.next_remote=Instant::now()+Duration::from_secs(10);let id=id.clone();
                std::thread::spawn(move||{let (title,cover)=remote_metadata(&id).unwrap_or_default();let _=tx.send((id,title,cover));});
            }
        }
        if self.title.is_empty()||self.id()?!=id||!valid(position,duration){return Ok(None)}
        Ok(Some(TimingSample{song_id:id,title:self.title.clone(),position,duration,playing:state==1,captured_ms:unix_ms(),cover_url:self.cover.clone()}))
    }
}
fn valid(p:f64,d:f64)->bool{p.is_finite()&&d.is_finite()&&p>=0.&&d>0.&&d<86400.&&p<=d+0.5}
fn local_metadata(local:&PathBuf,id:&str)->(String,Option<String>){
    let directory=local.join("NetEase/CloudMusic/WebData/file");
    for (file,key) in [("playingList","list"),("fmPlay","queue")]{
        let path=directory.join(file);if std::fs::metadata(&path).map(|m|m.len()>32*1024*1024).unwrap_or(true){continue}
        let Ok(input)=std::fs::File::open(path)else{continue};let Ok(value)=serde_json::from_reader::<_,serde_json::Value>(std::io::BufReader::new(input))else{continue};
        if let Some(list)=value[key].as_array(){for item in list{if text(item,"id")==id{let t=item.get("track").unwrap_or(item);let a=t.get("album").unwrap_or(&t["al"]);let cover=if text(a,"cover").is_empty(){text(a,"picUrl")}else{text(a,"cover")};return(text(t,"name"),if cover.is_empty(){None}else{Some(cover)})}}}
    }
    (String::new(),None)
}
fn remote_metadata(id:&str)->Result<(String,Option<String>),String>{
    let ctx=RequestContext::new(Duration::from_secs(4));let response=http::get(&format!("https://music.163.com/api/song/detail/?id={id}&ids=%5B{id}%5D"),&ctx)?;
    let song=&response["songs"][0];if text(song,"id")!=id{return Err("metadata ID mismatch".into())}let cover=text(&song["album"],"picUrl");Ok((text(song,"name"),if cover.is_empty(){None}else{Some(cover)}))
}
pub struct NeteaseTimeline{reader:Option<Reader>,pub latest:Option<TimingSample>,pub status:String,pub error:Option<String>,retry:Instant,local:PathBuf}
impl NeteaseTimeline{
    pub fn new(local:PathBuf)->Self{Self{reader:None,latest:None,status:"等待网易云".into(),error:None,retry:Instant::now(),local}}
    pub fn read(&mut self,online:bool){
        if Instant::now()<self.retry{return}let result=(||{if self.reader.is_none(){self.reader=Some(Reader::attach()?)}self.reader.as_mut().unwrap().read(&self.local,online)})();
        match result{Ok(sample)=>{self.latest=sample;self.error=None;self.status=if self.latest.is_some(){"网易云实时进度"}else{"等待网易云有效进度"}.into()},Err(error)=>{self.latest=None;self.reader=None;self.error=Some(error);self.status="网易云读取不可用，使用系统媒体会话".into();self.retry=Instant::now()+Duration::from_secs(5)}}
    }
    pub fn matching(&self,track:&lyric_core::models::TrackInfo)->Option<TimingSample>{
        let s=self.latest.as_ref()?;let age=unix_ms()-s.captured_ms;
        if track.source_app!="Netease"||!(0..800).contains(&age)||track.title.trim().to_lowercase()!=s.title.trim().to_lowercase()||track.song_id.as_ref().is_some_and(|id|id!=&s.song_id){return None}Some(s.clone())
    }
}
#[cfg(test)]mod tests{use super::*;#[test]fn invalid_timelines_are_rejected(){assert!(valid(0.,180.));assert!(!valid(f64::NAN,180.));assert!(!valid(182.,180.));assert!(!valid(0.,0.));}}


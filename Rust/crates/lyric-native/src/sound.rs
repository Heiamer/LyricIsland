//! Original short chimes and optional PCM WAV files. Audio runs on one bounded worker.
use std::{path::Path,sync::{OnceLock,mpsc},time::{Duration,Instant}};use serde_json::Value;
#[link(name="winmm")]unsafe extern "system"{fn PlaySoundW(sound:*const u16,module:isize,flags:u32)->i32;}
pub fn chime(kind:&str,volume:f64)->Vec<u8>{
 let bytes:&[u8]=match kind{
  "Connect"=>include_bytes!("../../../../Assets/Sounds/device-connect.wav"),
  "Glass"=>include_bytes!("../../../../Assets/Sounds/notification-glass.wav"),
  "Bell"=>include_bytes!("../../../../Assets/Sounds/todo-bell.wav"),
  _=>include_bytes!("../../../../Assets/Sounds/notification-soft.wav"),
 };custom(bytes,volume).expect("bundled sound must be valid PCM")
}
fn u32at(b:&[u8],i:usize)->Option<u32>{Some(u32::from_le_bytes(b.get(i..i+4)?.try_into().ok()?))}
pub fn custom(bytes:&[u8],volume:f64)->Result<Vec<u8>,String>{
 if bytes.len()>8*1024*1024||bytes.get(..4)!=Some(b"RIFF")||bytes.get(8..12)!=Some(b"WAVE"){return Err("请选择小于 8 MB 的 PCM WAV 音效".into())}let mut out=bytes.to_vec();let mut pos=12;let mut format=None;let mut data=None;
 while pos+8<=bytes.len(){let n=u32at(bytes,pos+4).ok_or("WAV 文件不完整")?as usize;let end=pos.checked_add(8).and_then(|p|p.checked_add(n)).filter(|e|*e<=bytes.len()).ok_or("WAV 文件不完整")?;if &bytes[pos..pos+4]==b"fmt "&&n>=16{format=Some(pos+8)}if &bytes[pos..pos+4]==b"data"{data=Some((pos+8,end))}pos=end+(n%2);}
 let f=format.ok_or("WAV 缺少格式信息")?;let read16=|i|u16::from_le_bytes([bytes[i],bytes[i+1]]);let channels=read16(f+2)as usize;let rate=u32at(bytes,f+4).unwrap_or(0)as usize;
 if read16(f)!=1||read16(f+14)!=16||!(1..=2).contains(&channels)||!(8000..=48000).contains(&rate){return Err("自定义音效需要 16 位 PCM WAV，单声道或立体声，8–48 kHz".into())}
 let (start,end)=data.ok_or("WAV 缺少音频数据")?;if end-start>rate*channels*2*10||(end-start)%(channels*2)!=0{return Err("音效请控制在 10 秒以内，并使用完整 PCM 数据".into())}let gain=(volume/100.).clamp(0.,1.);for chunk in out[start..end].chunks_exact_mut(2){let value=i16::from_le_bytes([chunk[0],chunk[1]]);chunk.copy_from_slice(&((value as f64*gain)as i16).to_le_bytes());}Ok(out)
}
pub fn validate_file(path:&str)->Result<(),String>{if path.is_empty(){return Ok(())}let p=Path::new(path);if !p.is_absolute()||!p.extension().is_some_and(|e|e.eq_ignore_ascii_case("wav")){return Err("请选择本地 WAV 文件".into())}let len=std::fs::metadata(p).map_err(|_|"找不到音效文件")?.len();if len>8*1024*1024{return Err("音效文件不能超过 8 MB".into())}custom(&std::fs::read(p).map_err(|e|e.to_string())?,100.).map(|_|())}
struct Cue{kind:String,path:String,volume:f64}
pub fn play(settings:&Value,todo:bool,preview:bool)->Result<(),String>{
 let prefix=if todo{"Todo"}else{"Notification"};if !preview&&!settings[format!("{prefix}SoundEnabled")].as_bool().unwrap_or(true){return Ok(())}
 let path=settings[format!("{prefix}SoundPath")].as_str().unwrap_or("").to_owned();if preview{validate_file(&path)?;}let kind=settings[format!("{prefix}Sound")].as_str().unwrap_or(if todo{"Bell"}else{"Soft"}).to_owned();let volume=settings[format!("{prefix}Volume")].as_f64().unwrap_or(35.);
 static WORKER:OnceLock<mpsc::SyncSender<Cue>>=OnceLock::new();let tx=WORKER.get_or_init(||{let(tx,rx)=mpsc::sync_channel::<Cue>(1);std::thread::spawn(move||{let mut last=Instant::now()-Duration::from_secs(5);while let Ok(c)=rx.recv(){if last.elapsed()<Duration::from_millis(180){continue}let bytes=if c.path.is_empty(){None}else{std::fs::read(&c.path).ok().and_then(|b|custom(&b,c.volume).ok())}.unwrap_or_else(||chime(&c.kind,c.volume));if c.volume>0.{unsafe{PlaySoundW(bytes.as_ptr().cast(),0,0x0004|0x0002|0x2000);}}last=Instant::now();}});tx});let _=tx.try_send(Cue{kind,path,volume});Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn chimes_are_valid_bounded_and_volume_zero_is_silent(){let b=chime("Bell",0.);assert!(b[44..].iter().all(|v|*v==0));assert!(custom(&chime("Glass",100.),50.).is_ok());assert!(custom(b"not audio",100.).is_err());}#[test]fn malformed_chunk_lengths_are_rejected(){let mut b=chime("Soft",50.);b[40..44].copy_from_slice(&u32::MAX.to_le_bytes());assert!(custom(&b,50.).is_err());}}

pub fn play_device(settings:&Value,preview:bool)->Result<(),String>{let mut copy=settings.clone();for suffix in ["SoundEnabled","Sound","SoundPath","Volume"]{if let Some(value)=settings.get(format!("Device{suffix}")){copy[format!("Notification{suffix}")]=value.clone();}}play(&copy,false,preview)}

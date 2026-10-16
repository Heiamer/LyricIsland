//! Optional current-user compatibility reader. Never writes to the Windows store or reads
//! deleted records. Notification content is transient and is never logged or uploaded.
use std::{collections::{HashSet,HashMap},ffi::{c_char,c_void,CStr,CString},path::{Path,PathBuf},time::{SystemTime,UNIX_EPOCH,Instant,Duration}};
use windows::{core::{PCSTR,PCWSTR},Win32::{Foundation::{FreeLibrary,HMODULE},System::LibraryLoader::{LoadLibraryW,GetProcAddress}}};
use crate::notifications::{Notice,Policy};
type Ptr=*mut c_void;
struct Api{library:HMODULE,open:unsafe extern "C" fn(*const c_char,*mut Ptr,i32,*const c_char)->i32,close:unsafe extern "C" fn(Ptr)->i32,prepare:unsafe extern "C" fn(Ptr,*const c_char,i32,*mut Ptr,*mut *const c_char)->i32,step:unsafe extern "C" fn(Ptr)->i32,finalize:unsafe extern "C" fn(Ptr)->i32,text:unsafe extern "C" fn(Ptr,i32)->*const u8,blob:unsafe extern "C" fn(Ptr,i32)->*const c_void,bytes:unsafe extern "C" fn(Ptr,i32)->i32,int:unsafe extern "C" fn(Ptr,i32)->i64,busy:unsafe extern "C" fn(Ptr,i32)->i32}
impl Drop for Api{fn drop(&mut self){unsafe{let _=FreeLibrary(self.library);}}}
impl Api{
    unsafe fn load()->Result<Self,String>{
        let path=std::env::current_exe().map_err(|_|"无法定位通知组件")?.parent().ok_or("无法定位通知组件")?.join("e_sqlite3.dll");
        let path=path.to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let library=LoadLibraryW(PCWSTR(path.as_ptr())).map_err(|_|"通知数据库组件缺失，请完整解压程序")?;
        macro_rules! symbol{($name:literal)=>{{match GetProcAddress(library,PCSTR(concat!($name,"\0").as_ptr())){Some(p)=>std::mem::transmute(p),None=>{let _=FreeLibrary(library);return Err("通知数据库组件版本不兼容".into())}}}}}
        Ok(Self{library,open:symbol!("sqlite3_open_v2"),close:symbol!("sqlite3_close"),prepare:symbol!("sqlite3_prepare_v2"),step:symbol!("sqlite3_step"),finalize:symbol!("sqlite3_finalize"),text:symbol!("sqlite3_column_text"),blob:symbol!("sqlite3_column_blob"),bytes:symbol!("sqlite3_column_bytes"),int:symbol!("sqlite3_column_int64"),busy:symbol!("sqlite3_busy_timeout")})
    }
}
struct Db<'a>(&'a Api,Ptr);impl Drop for Db<'_>{fn drop(&mut self){unsafe{(self.0.close)(self.1);}}}
struct Statement<'a>(&'a Api,Ptr);impl Drop for Statement<'_>{fn drop(&mut self){unsafe{(self.0.finalize)(self.1);}}}
pub fn filetime()->i64{(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()/100)as i64+116444736000000000}
fn path()->PathBuf{
    if std::env::var("LYRICISLAND_TEST_PIPE").is_ok_and(|s|s.starts_with("LyricIsland.Settings.test.")){
        if let Some(path)=std::env::var_os("LYRICISLAND_NOTIFICATION_DB"){return PathBuf::from(path)}
    }
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Microsoft/Windows/Notifications/wpndatabase.db")
}
type Stamp=(Option<(SystemTime,u64)>,Option<(SystemTime,u64)>);
fn stamp(path:&Path)->Stamp{let info=|p:&Path|std::fs::metadata(p).ok().and_then(|m|m.modified().ok().map(|t|(t,m.len())));(info(path),info(&PathBuf::from(format!("{}-wal",path.to_string_lossy()))))}
pub struct Store{icons:HashMap<String,Option<Vec<u8>>>,last_read:Instant,stamp:Option<Stamp>,since:i64,seen:HashSet<(i64,i64,i64)>,api:Option<Api>}
impl Store{
    pub fn new()->Self{Self{icons:HashMap::new(),last_read:Instant::now()-Duration::from_secs(2),stamp:None,since:filetime(),seen:HashSet::new(),api:None}}
    pub fn reset(&mut self){self.since=filetime();self.seen.clear();self.stamp=None;}
    pub fn poll(&mut self,policy:&Policy)->Result<Vec<Notice>,String>{unsafe{
        if self.api.is_none(){self.api=Some(Api::load()?);}
        let api=self.api.as_ref().unwrap();let path=path();
        let current=stamp(&path);if self.stamp.as_ref()==Some(&current)&&self.last_read.elapsed()<Duration::from_secs(2){return Ok(vec![])}
        let rows=read(api,&path,self.since,policy)?;self.stamp=Some(current);self.last_read=Instant::now();
        let mut notices=vec![];let mut latest=self.since;
        for (key,id,payload) in rows{
            latest=latest.max(key.2);
            if !self.seen.insert(key)||policy.paused{continue}
            if notices.len()<8{if let Some(mut notice)=parse(&payload,&id){if !self.icons.contains_key(&id){if self.icons.len()>=64{self.icons.clear();}self.icons.insert(id.clone(),crate::app_icon::shell_icon(&id));}notice.icon=self.icons.get(&id).cloned().flatten();notices.push(notice);}}
        }
        self.since=latest;self.seen.retain(|k|k.2>=latest);Ok(notices)
    }}
}
type Row=((i64,i64,i64),String,Vec<u8>);
unsafe fn read(api:&Api,path:&Path,since:i64,policy:&Policy)->Result<Vec<Row>,String>{
    let file=CString::new(path.to_string_lossy().as_bytes()).map_err(|_|"通知数据库路径无效")?;let mut raw=std::ptr::null_mut();
    // SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX. Never CREATE, READWRITE, backup or checkpoint.
    if (api.open)(file.as_ptr(),&mut raw,1|0x8000,std::ptr::null())!=0{if !raw.is_null(){(api.close)(raw);}return Err("兼容通知数据库暂不可读，请稍后重试或选择系统接口".into())}
    let db=Db(api,raw);(api.busy)(db.1,100);
    let sql=CString::new(format!("SELECT n.Id,n.HandlerId,n.ArrivalTime,h.PrimaryId,n.Payload FROM Notification n JOIN NotificationHandler h ON n.HandlerId=h.RecordId WHERE n.Type='toast' AND n.ArrivalTime>={since} AND n.ArrivalTime<={} ORDER BY n.ArrivalTime,n.[Order] LIMIT 256",filetime()+300000000)).unwrap();
    let mut raw=std::ptr::null_mut();if (api.prepare)(db.1,sql.as_ptr(),-1,&mut raw,std::ptr::null_mut())!=0{if !raw.is_null(){(api.finalize)(raw);}return Err("此 Windows 版本的通知格式尚不兼容，可选择系统接口".into())}
    let statement=Statement(api,raw);let mut rows=vec![];
    loop{match (api.step)(statement.1){
        100=>{
            let key=((api.int)(statement.1,0),(api.int)(statement.1,1),(api.int)(statement.1,2));
            let id=(api.text)(statement.1,3);if id.is_null(){continue}let id=CStr::from_ptr(id.cast()).to_string_lossy().into_owned();
            let name=app_name(&id);let mut payload=vec![];
            if !policy.paused&&policy.permits(&id,&name){let size=(api.bytes)(statement.1,4);if (1..=262144).contains(&size){let blob=(api.blob)(statement.1,4);if !blob.is_null(){payload=std::slice::from_raw_parts(blob.cast::<u8>(),size as usize).to_vec();}}}
            rows.push((key,id,payload));
        },
        101=>break,
        _=>return Err("通知数据库正忙或已更新，稍后自动重试".into())
    }}Ok(rows)
}
fn app_name(id:&str)->String{
    windows::ApplicationModel::AppInfo::GetFromAppUserModelId(&windows::core::HSTRING::from(id)).and_then(|a|a.DisplayInfo()?.DisplayName()).map(|n|n.to_string()).unwrap_or_else(|_|{
        let lower=id.to_ascii_lowercase();if lower.contains("qq")&&!lower.contains("qqmusic"){if lower.contains(".nt")||lower.contains("qqnt"){"QQNT".into()}else{"QQ".into()}}else{id.into()}
    })
}
fn decode(bytes:&[u8])->Option<String>{
    if bytes.starts_with(&[255,254]){Some(String::from_utf16_lossy(&bytes[2..].chunks_exact(2).map(|c|u16::from_le_bytes([c[0],c[1]])).collect::<Vec<_>>()))}
    else{std::str::from_utf8(bytes).ok().map(str::to_owned)}
}
fn parse(bytes:&[u8],id:&str)->Option<Notice>{
    if bytes.is_empty()||bytes.len()>262144{return None}
    let xml=decode(bytes)?;let document=roxmltree::Document::parse(xml.trim_matches(['\u{feff}','\0'])).ok()?;
    if !document.root_element().has_tag_name("toast"){return None}
    let binding=document.descendants().find(|n|n.has_tag_name("binding")&&n.attribute("template")==Some("ToastGeneric")).or_else(||document.descendants().find(|n|n.has_tag_name("binding")))?;
    let text=binding.descendants().filter(|n|n.has_tag_name("text")).filter_map(|n|n.text()).map(str::trim).filter(|s|!s.is_empty()).take(8).map(|s|s.chars().take(1024).collect::<String>()).collect::<Vec<_>>();
    let title=text.first()?.clone();Some(Notice{app_id:id.into(),app_name:app_name(id),title,body:text.into_iter().skip(1).collect::<Vec<_>>().join("\n"),icon:None})
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn parses_escaped_xml_and_multiple_text_lines(){let n=parse(br#"<toast><visual><binding template="ToastGeneric"><text>A &amp; B</text><text>second</text></binding></visual></toast>"#,"Example.App").unwrap();assert_eq!(n.title,"A & B");assert_eq!(n.body,"second");}
    #[test]fn rejects_malformed_oversize_and_non_toast_payloads(){assert!(parse(b"<broken>","a").is_none());assert!(parse(b"<tile />","a").is_none());assert!(parse(&vec![b'x';262145],"a").is_none());assert!(parse(br#"<!DOCTYPE toast [<!ENTITY x SYSTEM "file:///private">]><toast>&x;</toast>"#,"a").is_none());}
    #[test]fn new_store_starts_at_current_time_without_replaying_history(){let s=Store::new();assert!(s.since>116444736000000000);assert!(s.seen.is_empty());}
}

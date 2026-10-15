//! Compatible .NET cache/config persistence and native SQLite mapping lookup.
use std::{ffi::{c_char,c_void,CStr,CString},path::{Path,PathBuf},sync::Arc,time::Duration};
use serde::{Serialize,de::DeserializeOwned};
use serde_json::{Value,json};
use lyric_core::{json_store::JsonStore,cache::{LyricCacheStore,try_create_cache_key,ResolvedLyricCacheEntry},models::*,resolution::{LyricMappingResolver,LyricMapping,SongSearchMap,SongSearchMapResolver}};

pub fn duration_from_text(s:&str)->Option<Duration>{
    let parts=s.split(':').collect::<Vec<_>>();if parts.len()!=3{return None}
    let (days,hours)=parts[0].split_once('.').map(|(d,h)|Some((d.parse::<u64>().ok()?,h.parse::<u64>().ok()?))).unwrap_or_else(||Some((0,parts[0].parse::<u64>().ok()?)))?;
    let mins=parts[1].parse::<u64>().ok()?;let seconds=parts[2].parse::<f64>().ok()?;
    let total=(days*86400+hours*3600+mins*60) as f64+seconds;
    if !total.is_finite()||total<0.||total>u64::MAX as f64{return None}Some(Duration::from_secs_f64(total))
}
fn duration_text(v:&Value)->Option<String>{
    let sec=v.get("secs")?.as_u64()?;let nano=v.get("nanos")?.as_u64()?;
    let day=sec/86400;Some(format!("{}{:02}:{:02}:{:02}.{:07}",if day>0{format!("{day}.")}else{String::new()},sec%86400/3600,sec%3600/60,sec%60,nano/100))
}
fn is_time(key:&str)->bool{matches!(key,"startTime"|"endTime"|"duration"|"timestamp"|"relativeOffset")}
pub fn from_managed(v:Value)->Value{
    match v{
        Value::Array(a)=>Value::Array(a.into_iter().map(from_managed).collect()),
        Value::Object(m)=>Value::Object(m.into_iter().map(|(key,value)|{
            let mut chars=key.chars();let key=chars.next().map(|c|format!("{}{}",c.to_ascii_lowercase(),chars.as_str())).unwrap_or_default();
            let value=if key=="diagnostics"{value}else if is_time(&key)&&value.is_string(){duration_from_text(value.as_str().unwrap()).map(|d|json!({"secs":d.as_secs(),"nanos":d.subsec_nanos()})).unwrap_or(value)}else{from_managed(value)};
            (key,value)
        }).collect()),
        v=>v
    }
}
fn enum_number(key:&str,value:&str)->Option<usize>{
    let options:&[&str]=match key{
        "format"=>&["Unknown","Lrc","Qrc","Krc","Yrc","Ttml","PlainText"],
        "timingKind"=>&["Unsynced","LineTimed","WordTimed","CharacterTimed","Mixed"],
        "timingProvenance"=>&["Unknown","ProviderSupplied","Synthetic"],
        "acquisition"=>&["Unknown","Searching","MemoryCache","DiskCache","Remote","LocalFile","SongMapping","NotFound","UserBinding"],_=>return None};
    options.iter().position(|s|*s==value)
}
pub fn to_managed(v:Value,pascal:bool)->Value{
    match v{
        Value::Array(a)=>Value::Array(a.into_iter().map(|v|to_managed(v,pascal)).collect()),
        Value::Object(m)=>Value::Object(m.into_iter().map(|(key,value)|{
            let value=if key=="diagnostics"{value}else if is_time(&key)&&value.is_object(){duration_text(&value).map(Value::String).unwrap_or(value)}else if let Some(n)=value.as_str().and_then(|v|enum_number(&key,v)){json!(n)}else{to_managed(value,pascal)};
            let key=if pascal{let mut c=key.chars();c.next().map(|s|format!("{}{}",s.to_ascii_uppercase(),c.as_str())).unwrap_or_default()}else{key};(key,value)
        }).collect()),v=>v
    }
}
pub struct CompactStore<T>{pub store:Arc<JsonStore>,phantom:std::marker::PhantomData<T>}
impl<T> CompactStore<T>{pub fn new(store:Arc<JsonStore>)->Self{Self{store,phantom:std::marker::PhantomData}}}
impl<T:Serialize+DeserializeOwned+Send+Sync> LyricCacheStore<T> for CompactStore<T>{
    fn try_get(&self,key:&str)->Option<(T,LyricAcquisitionKind)>{let(raw,kind)=self.store.get(key)?;let v=from_managed(serde_json::from_str(&raw).ok()?);Some((serde_json::from_value(v).ok()?,if kind==2{LyricAcquisitionKind::DiskCache}else{LyricAcquisitionKind::MemoryCache}))}
    fn store(&self,key:&str,value:T){if let Ok(mut v)=serde_json::to_value(value){if v.get("fetchedAtUtc").is_some(){let t=unsafe{windows::Win32::System::SystemInformation::GetSystemTime()};v["fetchedAtUtc"]=json!(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute,t.wSecond,t.wMilliseconds));}let _=self.store.store(key,&to_managed(v,true).to_string());}}
    fn remove(&self,key:&str){self.store.remove(key);}
    fn clear(&self){self.store.clear();}
}
pub struct FinalCache(pub Arc<JsonStore>);
impl FinalCache{
    pub fn get(&self,t:&TrackInfo)->Option<ResolvedLyrics>{
        self.get_context(t,Some(MANUAL_CACHE_CONTEXT)).or_else(||self.get_context(t,None))
    }
    fn get_context(&self,t:&TrackInfo,context:Option<&str>)->Option<ResolvedLyrics>{
        let key=try_create_cache_key(&t.title,&t.artist,context)?;let(s,kind)=self.0.get(&key)?;
        let entry:ResolvedLyricCacheEntry=serde_json::from_value(from_managed(serde_json::from_str(&s).ok()?)).ok()?;
        if try_create_cache_key(entry.title.as_deref()?,entry.artist.as_deref()?,entry.selection_context.as_deref())?!=key{return None}
        let content=entry.content?;if lyric_core::cache::get_content_validation_failure(Some(&content)).is_some(){return None}
        Some(ResolvedLyrics{provider_id:LyricProviderId::try_new(entry.provider_id.as_deref()?).ok()?,candidate_id:entry.candidate_id?,acquisition:if kind==2{LyricAcquisitionKind::DiskCache}else{LyricAcquisitionKind::MemoryCache},content,diagnostics:entry.diagnostics.unwrap_or_default()})
    }
    pub fn put(&self,t:&TrackInfo,r:&ResolvedLyrics){
        let Some(key)=try_create_cache_key(&t.title,&t.artist,None)else{return};
        let entry=ResolvedLyricCacheEntry{track_id:Some(t.id.clone()),title:Some(t.title.clone()),artist:Some(t.artist.clone()),album:Some(t.album.clone()),source_app:Some(t.source_app.clone()),duration:t.duration,song_id:t.song_id.clone(),provider_id:Some(r.provider_id.to_string()),candidate_id:Some(r.candidate_id.clone()),acquisition:r.acquisition,diagnostics:Some(r.diagnostics.clone()),content:Some(r.content.clone()),selection_context:None};
        if let Ok(v)=serde_json::to_value(entry){let _=self.0.store(&key,&to_managed(v,false).to_string());}
    }
}
pub fn atomic_write(path:&Path,data:&[u8])->Result<(),String>{
    let parent=path.parent().ok_or("missing parent")?;std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    let temporary=path.with_extension(format!("{}.tmp",std::process::id()));
    let result=(||{use std::io::Write;let mut f=std::fs::File::create(&temporary)?;f.write_all(data)?;f.sync_all()?;drop(f);
        use windows::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH};
        use windows::core::PCWSTR;let s=temporary.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();let d=path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe{MoveFileExW(PCWSTR(s.as_ptr()),PCWSTR(d.as_ptr()),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH).map_err(|e|std::io::Error::other(e.to_string()))}
    })();if result.is_err(){let _=std::fs::remove_file(temporary);}result.map_err(|e:std::io::Error|e.to_string())
}

// Dynamically use the already distributed SQLite library; no managed EF runtime.
type Db=*mut c_void;
struct Sqlite { _library:windows::Win32::Foundation::HMODULE, open:unsafe extern "C" fn(*const c_char,*mut Db,i32,*const c_char)->i32,
    close:unsafe extern "C" fn(Db)->i32,prepare:unsafe extern "C" fn(Db,*const c_char,i32,*mut Db,*mut *const c_char)->i32,
    step:unsafe extern "C" fn(Db)->i32,text:unsafe extern "C" fn(Db,i32)->*const u8,int:unsafe extern "C" fn(Db,i32)->i32,finalize:unsafe extern "C" fn(Db)->i32 }
impl Sqlite{
    unsafe fn load()->Result<Self,String>{use windows::{Win32::System::LibraryLoader::{LoadLibraryW,GetProcAddress}};
        let path=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().join("e_sqlite3.dll");
        let wide=path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let lib=LoadLibraryW(windows::core::PCWSTR(wide.as_ptr())).map_err(|e|e.to_string())?;
        macro_rules! symbol{($name:literal)=>{{let name=windows::core::PCSTR(concat!($name,"\0").as_ptr());std::mem::transmute(GetProcAddress(lib,name).ok_or(concat!("missing ",$name))?)}}}
        Ok(Self{_library:lib,open:symbol!("sqlite3_open_v2"),close:symbol!("sqlite3_close"),prepare:symbol!("sqlite3_prepare_v2"),step:symbol!("sqlite3_step"),text:symbol!("sqlite3_column_text"),int:symbol!("sqlite3_column_int"),finalize:symbol!("sqlite3_finalize")})
    }
}
impl Drop for Sqlite{fn drop(&mut self){unsafe{let _=windows::Win32::Foundation::FreeLibrary(self._library);}}}
pub struct MappingDb(pub PathBuf);
impl LyricMappingResolver for MappingDb{
    fn resolve(&self,track:&TrackInfo)->LyricMapping{
        let rows=unsafe{self.rows()}.unwrap_or_default();SongSearchMapResolver::new(rows).resolve(track)
    }
}
impl MappingDb{
    unsafe fn rows(&self)->Result<Vec<SongSearchMap>,String>{
        if !self.0.exists(){return Ok(vec![])}let api=Sqlite::load()?;let mut db=null_mut();
        let path=CString::new(self.0.to_string_lossy().as_bytes()).map_err(|e|e.to_string())?;
        if (api.open)(path.as_ptr(),&mut db,1,std::ptr::null())!=0{if !db.is_null(){(api.close)(db);}return Err("mapping database open failed".into())}
        let sql=CString::new("SELECT OriginalTitle,OriginalArtist,OriginalAlbum,MappedTitle,MappedArtist,MappedAlbum,PreferredProvider,IsMarkedAsPureMusic FROM SongSearchMaps ORDER BY Id").unwrap();
        let mut statement=null_mut();let mut rows=vec![];
        if (api.prepare)(db,sql.as_ptr(),-1,&mut statement,null_mut())==0{
            while (api.step)(statement)==100{
                let col=|i|{let s=(api.text)(statement,i);if s.is_null(){String::new()}else{CStr::from_ptr(s as _).to_string_lossy().into_owned()}};
                let p=col(6);rows.push(SongSearchMap{original_title:col(0),original_artist:col(1),original_album:col(2),mapped_title:col(3),mapped_artist:col(4),mapped_album:col(5),preferred_provider:if p.is_empty(){None}else{Some(p)},is_marked_as_pure_music:(api.int)(statement,7)!=0});
            }
            (api.finalize)(statement);
        }
        (api.close)(db);Ok(rows)
    }
}
use std::ptr::null_mut;
#[cfg(test)]mod tests{use super::*;#[test]fn managed_time_and_enums_roundtrip(){let v=json!({"StartTime":"00:01:23.1234567","EndTime":null,"Format":4,"TimingKind":2,"Diagnostics":{"KeepCase":"v"}});let r=from_managed(v.clone());assert_eq!(r["startTime"]["secs"],83);assert_eq!(to_managed(r,true),v);assert_eq!(duration_from_text("1.02:03:04").unwrap().as_secs(),93784);}}



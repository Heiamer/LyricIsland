//! Desktop backend ABI. WPF only owns visuals, model conversion and GUI dispatch.
use std::{ffi::c_char,path::PathBuf,sync::{Arc,Mutex,OnceLock,atomic::{AtomicU64,Ordering}},collections::HashMap};
use serde_json::{Value,json};
use lyric_backend::{runtime::Backend,ipc::{Server,GuiCallback},io,shell};
use crate::{from_c_str,respond,error_json};
static NEXT:AtomicU64=AtomicU64::new(1);
static BACKENDS:OnceLock<Mutex<HashMap<u64,Arc<Backend>>>>=OnceLock::new();
static SERVERS:OnceLock<Mutex<HashMap<u64,Arc<Server>>>>=OnceLock::new();
fn backends()->&'static Mutex<HashMap<u64,Arc<Backend>>>{BACKENDS.get_or_init(||Mutex::new(HashMap::new()))}
fn servers()->&'static Mutex<HashMap<u64,Arc<Server>>>{SERVERS.get_or_init(||Mutex::new(HashMap::new()))}
fn get(handle:u64)->Option<Arc<Backend>>{backends().lock().ok()?.get(&handle).cloned()}
#[no_mangle]pub unsafe extern "C" fn li_backend_create(settings:*const c_char,data:*const c_char,local:*const c_char,logs:*const c_char)->u64{
    std::panic::catch_unwind(||{
        let Ok(settings)=serde_json::from_str(&from_c_str(settings))else{return 0};
        let backend=Backend::new(settings,PathBuf::from(from_c_str(data)),PathBuf::from(from_c_str(local)),PathBuf::from(from_c_str(logs)));let id=NEXT.fetch_add(1,Ordering::Relaxed);
        if let Ok(mut map)=backends().lock(){map.insert(id,backend);id}else{backend.close();0}
    }).unwrap_or(0)
}
#[no_mangle]pub extern "C" fn li_backend_destroy(handle:u64){if let Ok(mut map)=backends().lock(){if let Some(backend)=map.remove(&handle){backend.close();}}}
#[no_mangle]pub extern "C" fn li_backend_snapshot(handle:u64,last_cover:u64)->*mut c_char{respond(||get(handle).map(|b|b.snapshot(last_cover).to_string()).unwrap_or_else(||error_json("invalid backend handle")))}
#[no_mangle]pub unsafe extern "C" fn li_backend_apply(handle:u64,settings:*const c_char)->i32{std::panic::catch_unwind(||{if let (Some(b),Ok(settings))=(get(handle),serde_json::from_str(&from_c_str(settings))){b.apply(settings);1}else{0}}).unwrap_or(0)}
#[no_mangle]pub unsafe extern "C" fn li_backend_command(handle:u64,action:*const c_char,seconds:f64)->*mut c_char{respond(||{let Some(b)=get(handle)else{return error_json("invalid backend handle")};match b.command(&from_c_str(action),seconds){Ok(success)=>json!({"success":success}).to_string(),Err(e)=>error_json(e)}})}
#[no_mangle]pub unsafe extern "C" fn li_settings_server_create(name:*const c_char,callback:Option<GuiCallback>)->u64{
    std::panic::catch_unwind(||{let Some(callback)=callback else{return 0};let id=NEXT.fetch_add(1,Ordering::Relaxed);if let Ok(mut map)=servers().lock(){map.insert(id,Arc::new(Server::new(&from_c_str(name),callback)));id}else{0}}).unwrap_or(0)
}
#[no_mangle]pub extern "C" fn li_settings_server_close(handle:u64){if let Ok(map)=servers().lock(){if let Some(s)=map.get(&handle){s.close()}}}
#[no_mangle]pub extern "C" fn li_settings_server_finished(handle:u64)->i32{servers().lock().ok().and_then(|m|m.get(&handle).map(|s|s.finished())).map(|b|b as i32).unwrap_or(1)}
#[no_mangle]pub extern "C" fn li_settings_server_destroy(handle:u64){if let Ok(mut map)=servers().lock(){if let Some(s)=map.remove(&handle){s.close()}}}
#[no_mangle]pub unsafe extern "C" fn li_native_io(action:*const c_char,argument:*const c_char,payload:*const c_char)->*mut c_char{respond(||{
    let action=from_c_str(action);let argument=from_c_str(argument);let payload=from_c_str(payload);let value=serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null);
    let result:Result<Value,String>=(||{Ok(match action.as_str(){
        "stableIdentity"=>json!(lyric_core::sync::LyricSyncService::build_stable_track_identity(&lyric_core::models::TrackInfo{id:value["Id"].as_str().unwrap_or("").into(),title:value["Title"].as_str().unwrap_or("").into(),artist:value["Artist"].as_str().unwrap_or("").into(),album:value["Album"].as_str().unwrap_or("").into(),source_app:value["SourceApp"].as_str().unwrap_or("").into(),duration:std::time::Duration::ZERO,song_id:value["SongId"].as_str().map(str::to_string)})),
        "readSettings"=>io::read_settings(std::path::Path::new(&argument)),
        "saveSettings"=>{io::save_settings(std::path::Path::new(&argument),&value)?;json!({"success":true})},
        "log"=>{io::log(std::path::Path::new(&argument),value["category"].as_str().unwrap_or("GUI"),value["message"].as_str().unwrap_or(""));json!({"success":true})},
        "startupInfo"=>shell::startup_info(if argument.is_empty(){None}else{Some(&argument)}),
        "setStartup"=>{shell::set_startup(value["enabled"].as_bool().unwrap_or(false),value["executable"].as_str().ok_or("missing executable")?,std::path::Path::new(&argument),&value["settings"],value["keyPath"].as_str())?;json!({"success":true})},
        "ipc"=>io::pipe_request(&argument,&value)?,
        "favorite"=>shell::favorite(value["title"].as_str().unwrap_or(""),value["sourceApp"].as_str().unwrap_or(""),value["invoke"].as_bool().unwrap_or(false)),
        "validateSetting"=>validate_setting(value)?,
        "networkSmoke"=>{
            use lyric_core::{models::*,resolution::LyricSource};use lyric_backend::{http::{self,RequestContext},providers::OnlineSource};use std::time::Duration;
            let context=RequestContext::new(Duration::from_secs(5));let metadata=http::get("https://music.163.com/api/song/detail/?id=347230&ids=%5B347230%5D",&context)?;
            let cover=metadata["songs"][0]["album"]["picUrl"].as_str().ok_or("missing cover")?;
            let cover_ok=!http::request(cover,"GET",&[],"","",&RequestContext::new(Duration::from_secs(5)),4*1024*1024)?.is_empty();
            let source=OnlineSource::new("Netease",RequestContext::new(Duration::from_secs(5)));let candidate=SourceTrackCandidate::try_new(known_providers::netease(),"347230","海阔天空",vec!["Beyond".into()],"",Duration::ZERO,"direct-song-id",Default::default()).map_err(|e|e.to_string())?;
            let lyrics=source.fetch(&candidate).map_err(|e|format!("{e:?}"))?.is_some();json!({"success":cover_ok&&lyrics,"metadataDirect":true,"coverDirect":cover_ok,"lyricsDirect":lyrics,"httpBackend":"WinHTTP NO_PROXY"})
        },
        _=>return Err("unknown native IO operation".into())
    })})();match result{Ok(v)=>v.to_string(),Err(e)=>error_json(e)}
})}
fn validate_setting(v:Value)->Result<Value,String>{
    let key=v["key"].as_str().ok_or("missing key")?;let value=&v["value"];let mut settings=v["settings"].clone();if !settings.is_object(){return Err("invalid settings".into())}
    let range=match key{"CompactWidth"=>Some((140.,480.)),"FontSize"=>Some((11.,17.)),"OffsetMilliseconds"=>Some((-5000.,5000.)),"MotionGap"=>Some((0.,24.)),"MotionDuration"=>Some((250.,1200.)),"MotionCollapseDuration"=>Some((150.,700.)),"MotionRadius"=>Some((8.,48.)),"SpringStiffness"=>Some((100.,1000.)),"SpringDamping"=>Some((10.,80.)),"SpringMass"=>Some((0.3,2.)),_=>None};
    let value=if let Some((min,max))=range{let n=value.as_f64().ok_or("invalid number")?;if !n.is_finite()||n<min||n>max{return Err("number outside range".into())}if key=="OffsetMilliseconds"{json!(n.round() as i32)}else{json!(n)}}else if ["Floating","Translation","Animation","HoverExpand","ClickAnywhereExpand","WordHighlight","AlwaysOnTop","AutoStart","OnlineSongInfo","HideWhenPaused"].contains(&key){json!(value.as_bool().ok_or("invalid bool")?)}
    else if key=="CardMaterial"{if value.as_str()!=Some("Default"){return Err("invalid material".into())}value.clone()}
    else if key.ends_with("Color"){let s=value.as_str().ok_or("invalid color")?;if !["ExpandedTextColor","ExpandedComponentColor","LyricProgressColor"].contains(&key)||!s.starts_with('#')||s.len()!=7||!s[1..].bytes().all(|b|b.is_ascii_hexdigit()){return Err("invalid color".into())}json!(s.to_uppercase())}
    else if key=="AllowedSources"{let a=value.as_array().ok_or("invalid whitelist")?;if a.len()>32{return Err("whitelist too long".into())}let mut out=vec![];for s in a{let s=s.as_str().ok_or("invalid source")?.trim();if s.is_empty()||s.encode_utf16().count()>256{return Err("invalid source".into())}if !out.iter().any(|v:&String|v.eq_ignore_ascii_case(s)){out.push(s.to_string())}}json!(out)}else{return Err("unknown setting".into())};
    if let Some(name)=key.strip_prefix("Motion"){settings["Motion"][name]=value;}else if let Some(name)=key.strip_prefix("Spring"){settings["Motion"]["Spring"][name]=value;}else{settings[key]=value;}Ok(settings)
}


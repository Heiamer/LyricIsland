//! Native SMTC sessions. Every WinRT operation has an explicit bounded wait.
use std::{sync::{Arc,atomic::{AtomicU64,Ordering}},time::{Duration,Instant,SystemTime,UNIX_EPOCH}};
use windows::{core::Result,Foundation::TypedEventHandler,Media::Control::*,Storage::Streams::{DataReader,InputStreamOptions}};
use lyric_core::models::{TrackInfo,PlaybackSnapshot};

macro_rules! wait {
    ($operation:expr)=>{{
        let operation=$operation?;let deadline=Instant::now()+Duration::from_secs(2);
        while operation.Status()?.0==0{
            if Instant::now()>deadline{let _=operation.Cancel();return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x800705B4u32 as i32)))}
            std::thread::sleep(Duration::from_millis(5));
        }
        operation.GetResults()?
    }};
}
pub fn unix_ms()->i64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64}
/// Read-only diagnostics: report the capabilities advertised by every session.
/// Never issues a seek, play/pause or other media command.
pub fn control_diagnostics()->Result<serde_json::Value>{
    unsafe{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);}
    let manager=wait!(GlobalSystemMediaTransportControlsSessionManager::RequestAsync());
    let current=manager.GetCurrentSession().ok();let sessions=manager.GetSessions()?;let mut rows=vec![];
    for i in 0..sessions.Size()?{let s=sessions.GetAt(i)?;let info=s.GetPlaybackInfo()?;let timeline=s.GetTimelineProperties()?;
        rows.push(serde_json::json!({"sourceId":s.SourceAppUserModelId()?.to_string(),"isCurrent":current.as_ref()==Some(&s),"status":info.PlaybackStatus()?.0,"canSeek":info.Controls()?.IsPlaybackPositionEnabled()?,"positionSeconds":timeline.Position()?.Duration as f64/10000000.,"startSeconds":timeline.StartTime()?.Duration as f64/10000000.,"endSeconds":timeline.EndTime()?.Duration as f64/10000000.}));
    }Ok(serde_json::json!({"readOnly":true,"sessions":rows}))
}
pub fn normalize_source(source:&str)->String{
    let s=source.to_lowercase();
    for (needles,name) in [(&["qqmusic"][..],"QQMusic"),(&["cloudmusic","netease","163music","music.163","wyy"][..],"Netease"),(&["kugou"][..],"Kugou"),(&["kwmusic","kuwo"][..],"Kuwo"),(&["spotify"][..],"Spotify"),(&["sodamusic"][..],"SodaMusic"),(&["applemusic","apple music"][..],"AppleMusic"),(&["youtubemusic","youtube music"][..],"YouTubeMusic"),(&["potplayer"][..],"PotPlayer"),(&["saltplayer","salt player"][..],"SaltPlayer"),(&["lx-music","lxmusic"][..],"LXMusic"),(&["musicfree"][..],"MusicFree"),(&["yesplaymusic"][..],"YesPlayMusic"),(&["cider"][..],"Cider"),(&["foobar"][..],"Foobar2000"),(&["musicbee"][..],"MusicBee"),(&["aimp"][..],"AIMP"),(&["chrome","msedge","firefox","browser","opera","brave","vivaldi"][..],"Browser")]{
        if needles.iter().any(|n|s.contains(n)){return name.into()}
    }
    source.into()
}
/// Which media sessions may drive the island.
///
/// Expressed as exclusions, not as an allow-list: a session responds unless its source is named in
/// `ignored`. That is what lets a player nobody has heard of work without being added first — the
/// old allow-list silently ignored 汽水音乐, PotPlayer and every local player that was not one of the
/// six defaults. `only` is the diagnostic counterpart: the verification harnesses pin the island to
/// a single session with it, and it stays empty in normal operation.
#[derive(Debug,Clone,Default)]
pub struct SourceFilter{pub ignored:Vec<String>,pub only:Vec<String>}
impl SourceFilter{
    pub fn allows(&self,source:&str)->bool{
        if blocked(source){return false}
        if !self.only.is_empty()&&!self.only.iter().any(|rule|matches(rule,source)){return false}
        !self.ignored.iter().any(|rule|matches(rule,source))
    }
}
pub fn matches(rule:&str,source:&str)->bool{
    if rule.eq_ignore_ascii_case(source)||rule.eq_ignore_ascii_case(&normalize_source(source)){return true}
    let program=|s:&str|s.trim().trim_matches('"').rsplit(['\\','/']).next().unwrap_or(s).trim_end_matches(".exe").to_lowercase();
    let r=program(rule);r==program(source)||!r.contains('.')&&source.to_lowercase().split(['.','!','/','\\',':']).any(|p|p==r)
}
pub fn blocked(source:&str)->bool{let s=source.to_lowercase();["explorer","shellexperiencehost","searchhost","startmenuexperiencehost"].iter().any(|p|s.contains(p))}
pub fn netease_window_metadata(filter:&SourceFilter)->Option<(String,String)>{
    if !filter.allows("cloudmusic.exe"){return None}
    type Cache=(Instant,Option<(String,String)>);
    static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
    let mut cache=CACHE.get_or_init(||std::sync::Mutex::new((Instant::now()-Duration::from_secs(2),None))).lock().ok()?;
    if cache.0.elapsed()<Duration::from_secs(1){return cache.1.clone()}
    use windows::Win32::{UI::WindowsAndMessaging::{EnumWindows,IsWindowVisible,GetWindowTextW,GetWindowThreadProcessId},Foundation::{HWND,LPARAM,CloseHandle},System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION,QueryFullProcessImageNameW,PROCESS_NAME_WIN32}};
    unsafe extern "system" fn visit(window:HWND,param:LPARAM)->windows::core::BOOL{
        if !IsWindowVisible(window).as_bool(){return windows::core::BOOL(1)}let result=&mut *(param.0 as *mut Option<(String,String)>);
        let mut title=[0u16;1024];let count=GetWindowTextW(window,&mut title);if count<=0{return windows::core::BOOL(1)}
        let title=String::from_utf16_lossy(&title[..count as usize]);let Some((song,artist))=title.split_once(['-','|','—'])else{return windows::core::BOOL(1)};
        if song.trim().is_empty()||artist.trim().is_empty(){return windows::core::BOOL(1)}
        let mut pid=0;GetWindowThreadProcessId(window,Some(&mut pid));if let Ok(process)=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid){
            let mut name=[0u16;32768];let mut length=name.len() as u32;let ok=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,windows::core::PWSTR(name.as_mut_ptr()),&mut length).is_ok();let _=CloseHandle(process);
            if ok{let path=String::from_utf16_lossy(&name[..length as usize]);let name=path.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();if name=="cloudmusic.exe"{*result=Some((song.trim().into(),artist.trim().into()));return windows::core::BOOL(0)}}
        }windows::core::BOOL(1)
    }
    let mut result=None;unsafe{let _=EnumWindows(Some(visit),LPARAM(&mut result as *mut _ as isize));}*cache=(Instant::now(),result.clone());result
}
/// No session may be playing or current, but a paused player is still the one
/// the island must control; only compare against a priority that exists.
fn better(priority:Option<(bool,bool)>,candidate:(bool,bool))->bool{priority.is_none_or(|p|candidate<p)}
#[derive(Clone)]pub struct MediaSample { pub playback:PlaybackSnapshot,pub updated_ms:i64,pub raw:f64,pub status:String,pub rate:f64,pub cover_identity:String }
pub struct Media {
    manager:GlobalSystemMediaTransportControlsSessionManager,
    pub active:Option<GlobalSystemMediaTransportControlsSession>,
    pub events:Arc<AtomicU64>,manager_tokens:(i64,i64),session_tokens:Option<(GlobalSystemMediaTransportControlsSession,i64,i64,i64)>,
    cached_properties:Option<GlobalSystemMediaTransportControlsSessionMediaProperties>,properties_at:Instant,properties_events:u64,
    cached_cover:Option<Vec<u8>>,cover_key:String,cover_retry:Instant,
}
impl Media{
    pub fn new()->Result<Self>{
        unsafe{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);}
        let manager=wait!(GlobalSystemMediaTransportControlsSessionManager::RequestAsync());let events=Arc::new(AtomicU64::new(0));
        let count=events.clone();let a=manager.SessionsChanged(&TypedEventHandler::new(move|_,_|{count.fetch_add(1,Ordering::Relaxed);Ok(())}))?;
        let count=events.clone();let b=manager.CurrentSessionChanged(&TypedEventHandler::new(move|_,_|{count.fetch_add(1,Ordering::Relaxed);Ok(())}))?;
        Ok(Self{manager,active:None,events,manager_tokens:(a,b),session_tokens:None,cached_properties:None,properties_at:Instant::now(),properties_events:u64::MAX,cached_cover:None,cover_key:String::new(),cover_retry:Instant::now()})
    }
    pub fn read(&mut self,filter:&SourceFilter)->Result<MediaSample>{
        let sessions=self.manager.GetSessions()?;let current=self.manager.GetCurrentSession().ok();let mut best=None;let mut priority:Option<(bool,bool)>=None;
        for i in 0..sessions.Size()?{
            let s=sessions.GetAt(i)?;let source=s.SourceAppUserModelId()?.to_string();if !filter.allows(&source){continue}
            let playing=s.GetPlaybackInfo().ok().and_then(|p|p.PlaybackStatus().ok())==Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
            let candidate=(!playing,current.as_ref()!=Some(&s));if better(priority,candidate){priority=Some(candidate);best=Some(s)}
        }
        let Some(session)=best else{self.set_active(None);return Ok(MediaSample{playback:PlaybackSnapshot::idle(),updated_ms:0,raw:0.,status:"Closed".into(),rate:1.,cover_identity:String::new()})};
        self.set_active(Some(session.clone()));
        let source_id=session.SourceAppUserModelId()?.to_string();let source=normalize_source(&source_id);
        let events=self.events.load(Ordering::Relaxed);
        if self.cached_properties.is_none() || events!=self.properties_events || self.properties_at.elapsed()>=Duration::from_secs(5){
            self.cached_properties=Some(wait!(session.TryGetMediaPropertiesAsync()));self.properties_at=Instant::now();self.properties_events=events;
        }
        let props=self.cached_properties.as_ref().unwrap();
        let mut title=props.Title()?.to_string();let mut artist=props.Artist()?.to_string();
        if artist.trim().is_empty(){artist=props.AlbumArtist().map(|s|s.to_string()).unwrap_or_default()}
        if source=="Netease"&&(title.trim().is_empty()||title.eq_ignore_ascii_case("Unknown Title")){if let Some((name,singer))=netease_window_metadata(filter){title=name;artist=singer;}}
        let mut song_id=None;if let Ok(genres)=props.Genres(){for i in 0..genres.Size().unwrap_or(0){if let Ok(genre)=genres.GetAt(i){let genre=genre.to_string();let prefix=if source=="Netease"{"NCM-"}else if source=="QQMusic"{"QQ-"}else{""};if !prefix.is_empty(){if let Some(id)=genre.strip_prefix(prefix){song_id=Some(id.to_string());break}}}}}
        let album=props.AlbumTitle()?.to_string();let info=session.GetPlaybackInfo()?;let status=info.PlaybackStatus()?;
        let playing=status==GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
        let status_name=match status{GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing=>"Playing",GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused=>"Paused",GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped=>"Stopped",GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed=>"Closed",_=>"Changing"};
        let timeline=session.GetTimelineProperties()?;
        let raw=timeline.Position()?.Duration as f64/10000000.;let end=timeline.EndTime()?.Duration;let duration=(if end>0{end}else{timeline.MaxSeekTime()?.Duration} as f64/10000000.).max(0.);
        let updated_ms=timeline.LastUpdatedTime()?.UniversalTime/10000-11644473600000;
        let cover_key=format!("{source}|{title}|{artist}|{album}");
        if self.cover_key!=cover_key{self.cached_cover=None;self.cover_key=cover_key.clone();self.cover_retry=Instant::now()}
        if self.cached_cover.is_none()&&Instant::now()>=self.cover_retry{
            self.cover_retry=Instant::now()+Duration::from_secs(5);
            self.cached_cover=Self::cover(&props).ok().flatten();
        }
        let is_media=!title.trim().is_empty()&&status_name!="Closed"&&status_name!="Stopped";
        let track=if is_media{Some(TrackInfo{id:format!("{source}|{title}|{artist}"),title,artist,album,source_app:source,duration:Duration::from_secs_f64(duration.min(86400.)),song_id})}else{None};
        Ok(MediaSample{playback:PlaybackSnapshot{track,is_playing:playing,position:Duration::from_secs_f64(raw.clamp(0.,86400.)),raw_position:Some(Duration::from_secs_f64(raw.clamp(0.,86400.))),extrapolated_position:None,cover_image_bytes:self.cached_cover.clone(),is_cover_loading:self.cached_cover.is_none(),source_app_user_model_id:source_id,can_seek:info.Controls()?.IsPlaybackPositionEnabled()?},updated_ms,raw,status:status_name.into(),rate:info.PlaybackRate().ok().and_then(|r|r.Value().ok()).unwrap_or(1.),cover_identity:cover_key})
    }
    fn cover(props:&GlobalSystemMediaTransportControlsSessionMediaProperties)->Result<Option<Vec<u8>>>{
        let Ok(thumbnail)=props.Thumbnail()else{return Ok(None)};
        let stream=wait!(thumbnail.OpenReadAsync());let size=stream.Size()?;
        if size==0||size>4*1024*1024{return Ok(None)}
        let input=stream.GetInputStreamAt(0)?;let reader=DataReader::CreateDataReader(&input)?;
        reader.SetInputStreamOptions(InputStreamOptions::None)?;let count=wait!(reader.LoadAsync(size as u32));
        let mut bytes=vec![0u8;count as usize];reader.ReadBytes(&mut bytes)?;let _=reader.Close();Ok(Some(bytes))
    }
    fn set_active(&mut self,active:Option<GlobalSystemMediaTransportControlsSession>){
        if self.active==active{return}
        if let Some((s,p,m,t))=self.session_tokens.take(){let _=s.RemovePlaybackInfoChanged(p);let _=s.RemoveMediaPropertiesChanged(m);let _=s.RemoveTimelinePropertiesChanged(t);}
        self.cached_properties=None;self.cached_cover=None;self.cover_key.clear();self.cover_retry=Instant::now();
        self.active=active.clone();
        if let Some(s)=active{
            let c=self.events.clone();let p=s.PlaybackInfoChanged(&TypedEventHandler::new(move|_,_|{c.fetch_add(1,Ordering::Relaxed);Ok(())})).unwrap_or(0);
            let c=self.events.clone();let m=s.MediaPropertiesChanged(&TypedEventHandler::new(move|_,_|{c.fetch_add(1,Ordering::Relaxed);Ok(())})).unwrap_or(0);
            let c=self.events.clone();let t=s.TimelinePropertiesChanged(&TypedEventHandler::new(move|_,_|{c.fetch_add(1,Ordering::Relaxed);Ok(())})).unwrap_or(0);
            self.session_tokens=Some((s,p,m,t));
        }
    }
    pub fn command(session:&GlobalSystemMediaTransportControlsSession,action:&str,seconds:f64)->Result<bool>{
        Ok(match action{"toggle"=>wait!(session.TryTogglePlayPauseAsync()),"play"=>wait!(session.TryPlayAsync()),"pause"=>wait!(session.TryPauseAsync()),"next"=>wait!(session.TrySkipNextAsync()),"previous"=>wait!(session.TrySkipPreviousAsync()),"seek"=>{if !seconds.is_finite()||seconds<0.||!session.GetPlaybackInfo()?.Controls()?.IsPlaybackPositionEnabled()?{return Ok(false)}wait!(session.TryChangePlaybackPositionAsync((seconds*10000000.) as i64))},_=>false})
    }
}
impl Drop for Media{fn drop(&mut self){self.set_active(None);let _=self.manager.RemoveSessionsChanged(self.manager_tokens.0);let _=self.manager.RemoveCurrentSessionChanged(self.manager_tokens.1);}}
#[cfg(test)]mod tests{use super::*;#[test]fn rules_preserve_whitelist_boundaries(){assert!(matches("QQMusic","QQMusic.exe"));assert!(matches("Browser","chrome.exe"));assert!(!matches("Spotify","chrome.exe"));assert!(blocked("Microsoft.Windows.ShellExperienceHost"));}
/// The filter answers "does this respond?" for every player the app has never heard of, which is the
/// whole point of turning the old allow-list around. Only what is named here stays silent.
#[test]fn only_named_players_are_excluded(){
    let filter=SourceFilter{ignored:vec!["Browser".into()],only:vec![]};
    assert!(filter.allows("SodaMusic.exe"));
    assert!(filter.allows("PotPlayerMini64.exe"));
    assert!(filter.allows("QQMusic.exe"));
    assert!(!filter.allows("chrome.exe"));
    assert!(!filter.allows("Microsoft.Windows.ShellExperienceHost"));
}
/// A blocked built-in still reaches its aliases: 网易云 is one session id in SMTC and another in the
/// window-title fallback, and both have to go quiet together.
#[test]fn exclusion_follows_aliases_and_the_diagnostic_pin(){
    let netease=SourceFilter{ignored:vec!["Netease".into()],only:vec![]};
    assert!(!netease.allows("cloudmusic.exe"));
    assert!(!netease.allows("Netease"));
    assert!(netease.allows("QQMusic.exe"));
    let pinned=SourceFilter{ignored:vec![],only:vec!["LyricIsland.Verify13".into()]};
    assert!(pinned.allows("LyricIsland.Verify13"));
    assert!(!pinned.allows("QQMusic.exe"));
}
/// A paused player without SMTC focus is the normal resume case; it must not
/// lose to an empty priority and leave the island without a controllable session.
#[test]fn a_paused_non_current_session_can_still_be_controlled(){
    assert!(better(None,(true,true)));
    assert!(!better(Some((true,true)),(true,true)));
    assert!(better(Some((true,true)),(true,false)));
    assert!(better(Some((true,true)),(false,true)));
}}



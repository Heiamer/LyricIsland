//! Owns the non-GUI lifecycle: media, resolution jobs, clocks, artwork and cancellation.
use std::{path::PathBuf,sync::{Arc,Mutex,mpsc,atomic::{AtomicBool,AtomicU64,Ordering}},time::{Duration,Instant}};
use serde_json::{Value,json};
use base64::{Engine,engine::general_purpose::STANDARD};
use lyric_core::{models::*,cache::{LyricPipelineCache,RawLyricCacheEnvelope,ParsedLyricCacheEnvelope},json_store::JsonStore,resolution::{LyricResolutionCoordinator,LyricResolutionCoordinatorBuilder,LyricMappingResolver,select_best_scored,ScoredCandidate},sync::LyricSyncService,local::LocalLyricProvider};
use crate::{media::{Media,MediaSample,unix_ms},clock::PlaybackClock,netease::NeteaseTimeline,providers::OnlineSource,http::{self,RequestContext},storage::{CompactStore,FinalCache,MappingDb},io};

struct State{settings:Value,playback:PlaybackSnapshot,clock:PlaybackClock,sync:LyricSyncService,active:Option<windows::Media::Control::GlobalSystemMediaTransportControlsSession>,events:u64,cover:Option<Vec<u8>>,cover_version:u64,cover_key:String,artwork_identity:String,duration_source:String,native_status:String,native_error:Option<String>,track_key:String,pending_weak:Option<(Instant,Instant,String)>,last_strong_track:Option<TrackInfo>}
pub struct Backend{state:Mutex<State>,closed:AtomicBool,generation:AtomicU64,started:Instant,data:PathBuf,local:PathBuf,raw:Arc<JsonStore>,parsed:Arc<JsonStore>,final_cache:FinalCache,logs:PathBuf,search_cancel:Mutex<Option<Arc<AtomicBool>>>}
impl Backend{
    pub fn new(settings:Value,data:PathBuf,local:PathBuf,logs:PathBuf)->Arc<Self>{
        let backend=Arc::new(Self{raw:Arc::new(JsonStore::new(data.join("database/lyric-pipeline-raw-v1.json"))),parsed:Arc::new(JsonStore::new(data.join("database/lyric-pipeline-parsed-v1.json"))),final_cache:FinalCache(Arc::new(JsonStore::new_enveloped(data.join("database/resolved-lyrics-v1.json")))),data,local,logs,
            state:Mutex::new(State{settings,playback:PlaybackSnapshot::idle(),clock:PlaybackClock::default(),sync:LyricSyncService::new(),active:None,events:0,cover:None,cover_version:0,cover_key:String::new(),artwork_identity:String::new(),duration_source:"等待歌曲信息".into(),native_status:"等待网易云".into(),native_error:None,track_key:String::new(),pending_weak:None,last_strong_track:None}),closed:AtomicBool::new(false),generation:AtomicU64::new(0),started:Instant::now(),search_cancel:Mutex::new(None)});
        io::log(&backend.logs,"BUILD","Version=1.2; Backend=Rust; GUI=Direct2D; Settings=GPUI; HTTP=WinHTTP NO_PROXY");
        let worker=backend.clone();std::thread::spawn(move||worker.run());backend
    }
    pub fn close(&self){self.closed.store(true,Ordering::Release);self.generation.fetch_add(1,Ordering::AcqRel);if let Ok(mut c)=self.search_cancel.lock(){if let Some(c)=c.take(){c.store(true,Ordering::Release)}}}
    pub fn apply(&self,settings:Value){if let Ok(mut s)=self.state.lock(){s.settings=settings;}}
    pub fn snapshot(&self,last_cover:u64)->Value{
        let Ok(mut s)=self.state.lock()else{return json!({"error":"backend lock failed"})};
        let mut playback=s.playback.clone();let position=s.clock.position(self.started.elapsed().as_secs_f64());let duration=playback.track.as_ref().map(|t|t.duration.as_secs_f64()).unwrap_or(0.);
        playback.position=Duration::from_secs_f64(if duration>0.{position.min(duration)}else{position}.clamp(0.,86400.));
        let offset=s.settings["OffsetMilliseconds"].as_f64().unwrap_or(0.)/1000.;let mut lyric_playback=playback.clone();lyric_playback.position=Duration::from_secs_f64((playback.position.as_secs_f64()+offset).max(0.));
        let frame=s.sync.get_display_frame(&lyric_playback);
        let track=playback.track.as_ref().map(|t|json!({"id":t.id,"title":t.title,"artist":t.artist,"album":t.album,"sourceApp":t.source_app,"durationSeconds":t.duration.as_secs_f64(),"songId":t.song_id}));
        let cover=if s.cover_version!=last_cover{s.cover.as_ref().map(|b|STANDARD.encode(b))}else{None};
        json!({"version":"1.2.2","backend":"Rust","track":track,"isPlaying":playback.is_playing,"positionSeconds":playback.position.as_secs_f64(),"rawSeconds":playback.raw_position.map(|p|p.as_secs_f64()),"sourceId":playback.source_app_user_model_id,"canSeek":playback.can_seek,"isCoverLoading":playback.is_cover_loading,"coverVersion":s.cover_version,"cover":cover,"artworkIdentity":s.artwork_identity,"durationSource":s.duration_source,"nativeStatus":s.native_status,"nativeError":s.native_error,"events":s.events,"localClock":s.clock.local,"clockStatus":s.clock.status,"lyricSource":s.sync.current_lyric_source_app(),"frame":{"currentLine":frame.current_line,"nextLine":frame.next_line,"title":frame.title,"lineProgress":frame.line_progress,"currentLineIndex":frame.current_line_index,"isPureMusic":frame.is_pure_music,"wordScanProgress":frame.word_scan_progress,"currentTranslation":frame.current_translation,"nextTranslation":frame.next_translation,"hasTrackTranslation":frame.has_track_translation}})
    }
    pub fn command(&self,action:&str,seconds:f64)->Result<bool,String>{
        if action=="synchronize"{if let Ok(mut s)=self.state.lock(){s.clock.synchronize(seconds,self.started.elapsed().as_secs_f64());return Ok(true)}return Ok(false)}
        unsafe{let _=windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);}
        let (session,before,key,duration)={let s=self.state.lock().map_err(|_|"backend lock")?;(s.active.clone(),s.playback.is_playing,s.track_key.clone(),s.playback.track.as_ref().map(|t|t.duration.as_secs_f64()).unwrap_or(0.))};
        let Some(session)=session else{return Ok(false)};
        let seconds=if action=="seek"{seconds.clamp(0.,(duration-0.5).max(0.))}else{seconds};
        if !Media::command(&session,action,seconds).map_err(|e|e.to_string())?{return Ok(false)}
        if matches!(action,"toggle"|"play"|"pause"|"seek"){
            let until=Instant::now()+Duration::from_millis(1200);
            while Instant::now()<until{
                if self.closed.load(Ordering::Acquire){return Ok(false)}
                if self.state.lock().map_err(|_|"backend lock")?.track_key!=key{return Ok(false)}
                let playing=session.GetPlaybackInfo().and_then(|p|p.PlaybackStatus()).map_err(|e|e.to_string())?==windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
                let ack=match action{"toggle"=>playing!=before,"play"=>playing,"pause"=>!playing,"seek"=>session.GetTimelineProperties().and_then(|t|t.Position()).is_ok_and(|p|(p.Duration as f64/10000000.-seconds).abs()<2.),_=>true};
                if ack{return Ok(true)}std::thread::sleep(Duration::from_millis(40));
            }
            return Ok(false)
        }
        Ok(true)
    }
    fn run(self:Arc<Self>){
        let mut media:Option<Media>=None;let mut retry=Instant::now();let mut netease=NeteaseTimeline::new(self.local.clone());let mut last_cover=String::new();let mut cover_retry=Instant::now();let mut durations=crate::duration::DurationResolver::new(self.data.join("database/track-durations-v1.json"));
        while !self.closed.load(Ordering::Acquire){
            let (allowed,online)=match self.state.lock(){Ok(s)=>(s.settings["AllowedSources"].as_array().map(|a|a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()).unwrap_or_else(||["Netease","QQMusic","Kugou","Kuwo","Spotify","TwilightEcho"].map(str::to_string).to_vec()),s.settings["OnlineSongInfo"].as_bool().unwrap_or(false)),Err(_)=>break};
            if media.is_none()&&Instant::now()>=retry{match Media::new(){Ok(m)=>media=Some(m),Err(e)=>{io::log(&self.logs,"SMTC",&e.to_string());retry=Instant::now()+Duration::from_secs(5)}}}
            let result=media.as_mut().map(|m|m.read(&allowed));
            match result{
                Some(Ok(mut sample))=>{
                    if allowed.iter().any(|s|crate::media::matches(s,"cloudmusic.exe")){netease.read(online)}
                    if sample.playback.track.is_none(){
                        if let Some((title,artist))=crate::media::netease_window_metadata(&allowed){
                            sample.playback.track=Some(TrackInfo::new(format!("Netease|{title}|{artist}"),title,artist,"","Netease",Duration::ZERO));sample.playback.source_app_user_model_id="cloudmusic.exe".into();
                        }
                    }
                    if sample.playback.track.is_none(){if let Some(n)=netease.latest.as_ref().filter(|n|unix_ms()-n.captured_ms<800){
                        let mut track=TrackInfo::new(format!("Netease|{}",n.song_id),&n.title,&n.artist,&n.album,"Netease",Duration::from_secs_f64(n.duration));track.song_id=Some(n.song_id.clone());sample.playback.track=Some(track);sample.playback.source_app_user_model_id="cloudmusic.exe".into();
                    }}
                    let mut native=false;let mut cover_url=None;
                    if let Some(t)=sample.playback.track.as_mut(){if let Some(n)=netease.matching(t){
                        cover_url=n.cover_url.clone();if t.title.trim().is_empty()||t.title.eq_ignore_ascii_case("Unknown Title"){t.title=n.title.clone();}if t.artist.trim().is_empty(){t.artist=n.artist.clone();}if t.album.trim().is_empty(){t.album=n.album.clone();}
                        if sample.updated_ms<=0||sample.raw<=0.||sample.updated_ms>unix_ms()+5000{
                            let age=((unix_ms()-n.captured_ms) as f64/1000.).max(0.);t.song_id=Some(n.song_id);t.duration=Duration::from_secs_f64(n.duration);sample.playback.is_playing=n.playing;sample.playback.raw_position=Some(Duration::from_secs_f64(n.position));sample.playback.position=Duration::from_secs_f64((n.position+if n.playing{age}else{0.}).min(n.duration));sample.updated_ms=n.captured_ms;sample.raw=n.position;native=true;
                        }else{t.song_id=Some(n.song_id);}
                    }}
                    if !native&&sample.updated_ms>0&&sample.updated_ms<=unix_ms()+5000&&sample.playback.is_playing{
                        let age=((unix_ms()-sample.updated_ms) as f64/1000.).max(0.);let p=sample.raw+age*sample.rate.max(0.);sample.playback.position=Duration::from_secs_f64(p.clamp(0.,86400.));
                    }
                    durations.enrich(&mut sample.playback);
                    let job=self.accept(&sample,native,media.as_ref().and_then(|m|m.active.clone()),media.as_ref().map(|m|m.events.load(Ordering::Relaxed)).unwrap_or(0),&netease);
                    if let Ok(mut state)=self.state.lock(){if !native{state.duration_source=durations.source.clone();}}
                    if let Some(track)=job{self.start_search(track);}
                    let key=sample.playback.track.as_ref().map(|t|LyricSyncService::build_stable_track_identity(t)).unwrap_or_default();
                    if key!=last_cover{last_cover=key.clone();cover_retry=Instant::now();}
                    if let Some(url)=cover_url{let needed=self.state.lock().is_ok_and(|s|s.artwork_identity.is_empty());if needed&&Instant::now()>=cover_retry{cover_retry=Instant::now()+Duration::from_secs(15);self.start_artwork(key,url);}}
                }
                Some(Err(e))=>{io::log(&self.logs,"SMTC",&e.to_string());media=None;retry=Instant::now()+Duration::from_secs(1);}
                None=>{}
            }
            std::thread::sleep(Duration::from_millis(160));
        }
    }
    fn accept(&self,sample:&MediaSample,native:bool,active:Option<windows::Media::Control::GlobalSystemMediaTransportControlsSession>,events:u64,netease:&NeteaseTimeline)->Option<TrackInfo>{
        let mut s=self.state.lock().ok()?;let mut p=sample.playback.clone();
        if let (Some(old),Some(new))=(&s.last_strong_track,&p.track){
            let same_source=old.source_app==new.source_app;let same_title=old.title.trim()==new.title.trim();let weak=same_source&&same_title&&!old.artist.trim().is_empty()&&new.artist.trim().is_empty();
            if weak{
                let now=Instant::now();let id=LyricSyncService::build_stable_track_identity(new);
                if s.pending_weak.as_ref().is_none_or(|(_,_,key)|*key!=id){s.pending_weak=Some((now+Duration::from_millis(750),now+Duration::from_millis(1500),id))}
                if s.pending_weak.as_ref().is_some_and(|(quiet,max,_)|now<*quiet&&now<*max){p.track=s.last_strong_track.clone();}
            }else{s.pending_weak=None;s.last_strong_track=p.track.clone();}
        }else{s.pending_weak=None;s.last_strong_track=p.track.clone();}
        let key=p.track.as_ref().map(LyricSyncService::build_stable_track_identity).unwrap_or_default();
        let changed=key!=s.track_key;
        if changed{s.track_key=key.clone();s.cover_key=sample.cover_identity.clone();s.cover=p.cover_image_bytes.clone();s.cover_version+=1;s.artwork_identity.clear();}
        else if s.artwork_identity.is_empty()&&p.cover_image_bytes!=s.cover{s.cover=p.cover_image_bytes.clone();s.cover_version+=1;}
        p.cover_image_bytes=None;
        let position=s.clock.observe(&p,sample.updated_ms,unix_ms(),self.started.elapsed().as_secs_f64(),native);p.position=Duration::from_secs_f64(position.clamp(0.,86400.));
        s.duration_source=if native{"网易云实时读取"}else if p.track.as_ref().is_some_and(|t|t.duration>Duration::ZERO){"播放器"}else{"正在补取歌曲时长"}.into();
        s.active=active;s.events=events;s.native_status=netease.status.clone();s.native_error=netease.error.clone();s.playback=p.clone();
        if let Some(track)=p.track{
            if s.sync.begin_track(&track){return Some(track)}
        }else if changed{
            s.sync.get_display_frame(&PlaybackSnapshot::idle());self.generation.fetch_add(1,Ordering::AcqRel);if let Ok(mut c)=self.search_cancel.lock(){if let Some(c)=c.take(){c.store(true,Ordering::Release)}}
        }
        None
    }
    fn start_search(self:&Arc<Self>,track:TrackInfo){
        let generation=self.generation.fetch_add(1,Ordering::AcqRel)+1;let cancel=Arc::new(AtomicBool::new(false));
        if let Ok(mut previous)=self.search_cancel.lock(){if let Some(old)=previous.replace(cancel.clone()){old.store(true,Ordering::Release)}}
        let backend=self.clone();std::thread::spawn(move||{
            if !wait_delay(&cancel,Duration::from_millis(750)){return}
            let mut result=LyricResolutionResult::unavailable();
            for (attempt,delay) in [0,2,5].into_iter().enumerate(){
                if attempt>0&&!wait_delay(&cancel,Duration::from_secs(delay)){return}
                if cancel.load(Ordering::Acquire){return}result=backend.resolve(&track,cancel.clone());
                if result.status!=LyricResolutionStatus::TransientFailure{break}
            }
            if backend.closed.load(Ordering::Acquire)||cancel.load(Ordering::Acquire)||backend.generation.load(Ordering::Acquire)!=generation{return}
            if let Ok(mut s)=backend.state.lock(){s.sync.complete_search(&track,result.lyrics.as_ref());}
        });
    }
    fn resolve(&self,track:&TrackInfo,cancel:Arc<AtomicBool>)->LyricResolutionResult{
        if let Some(cached)=self.final_cache.get(track){return LyricResolutionResult::from_lyrics(Some(cached))}
        let settings=self.state.lock().map(|s|s.settings.clone()).unwrap_or_default();
        let mapping=MappingDb(self.data.join("database/song_maps.db")).resolve(track);
        if mapping.is_pure_music{return LyricResolutionResult::from_lyrics(Some(ResolvedLyrics{content:ParsedLyrics::pure_music_placeholder(LyricPayloadFormat::PlainText),provider_id:known_providers::local(),candidate_id:format!("mapping:{}",track.id),acquisition:LyricAcquisitionKind::SongMapping,diagnostics:Default::default()}))}
        let mut mapped=track.clone();mapped.title=mapping.title;mapped.artist=mapping.artist;if let Some(a)=mapping.album{mapped.album=a}
        let local=settings["LocalFolders"].as_array().map(|a|a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
        if !local.is_empty(){let provider=LocalLyricProvider::new(local);if let Some((content,acquisition))=provider.get_parsed_lyrics(&mapped){return LyricResolutionResult::from_lyrics(Some(ResolvedLyrics{provider_id:known_providers::local(),candidate_id:track.id.clone(),content,acquisition,diagnostics:Default::default()}))}}
        let order=known_providers::online_trust_order();let providers=if let Some(p)=mapping.preferred_provider{order.iter().filter(|i|i.eq_ignore_ascii_case(&p)).cloned().collect::<Vec<_>>()}else{order.clone()};
        let (tx,rx)=mpsc::channel();let primary_cancel=Arc::new(AtomicBool::new(false));
        for provider in &providers{
            let tx=tx.clone();let provider=provider.clone();let mapped=mapped.clone();let raw=self.raw.clone();let parsed=self.parsed.clone();let cancel=cancel.clone();let secondary=primary_cancel.clone();let logs=self.logs.clone();
            std::thread::spawn(move||{
                if cancel.load(Ordering::Acquire){return}let context=RequestContext{cancelled:secondary,deadline:Instant::now()+Duration::from_secs(5)};
                // One coordinator per provider, so its trust policy has to name exactly that provider.
                // Leaving it to the default asks for the whole online order and fails with
                // MissingProvider, which used to be swallowed by unwrap_or_else and turned every
                // online resolution into "unavailable" (i.e. no lyrics unless it was already cached).
                let trust_policy=lyric_core::policy::LyricProviderTrustPolicy::new(vec![provider.clone()],vec![provider.clone()]).ok();
                let coordinator=LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder{sources:vec![Box::new(OnlineSource::new(provider.value(),context))],trust_policy,pipeline_cache:Some(LyricPipelineCache::new(Box::new(CompactStore::<RawLyricCacheEnvelope>::new(raw)),Box::new(CompactStore::<ParsedLyricCacheEnvelope>::new(parsed)))),..Default::default()});
                let result=match coordinator{Ok(coordinator)=>coordinator.resolve(&mapped),Err(error)=>{io::log(&logs,"LYRIC_ERROR",&format!("coordinator for {} could not be built: {error}",provider.value()));LyricResolutionResult::unavailable()}};
                if !cancel.load(Ordering::Acquire){let _=tx.send((provider,result));}
            });
        }
        drop(tx);let mut results=vec![];let until=Instant::now()+Duration::from_secs(6);
        while results.len()<providers.len()&&Instant::now()<until{
            if cancel.load(Ordering::Acquire)||self.closed.load(Ordering::Acquire){primary_cancel.store(true,Ordering::Release);return LyricResolutionResult::unavailable()}
            if let Ok((provider,result))=rx.recv_timeout(Duration::from_millis(40)){
                if providers.first()==Some(&provider)&&result.lyrics.as_ref().and_then(ResolvedLyrics::identity_score).is_some_and(|s|s>=95){primary_cancel.store(true,Ordering::Release);if let Some(r)=&result.lyrics{self.final_cache.put(track,r)}return result}
                results.push((provider,result));
            }
        }
        primary_cancel.store(true,Ordering::Release);
        let scored=results.iter().filter_map(|(provider,r)|r.lyrics.as_ref().map(|l|ScoredCandidate{provider_id:provider.clone(),identity_score:l.identity_score().unwrap_or(0),candidate_id:l.candidate_id.clone()})).collect::<Vec<_>>();
        if let Some(selected)=select_best_scored(&scored,&order){if let Some((_,r))=results.iter().find(|(p,r)|p==&selected.provider_id&&r.lyrics.as_ref().is_some_and(|l|l.candidate_id==selected.candidate_id)){if let Some(l)=&r.lyrics{self.final_cache.put(track,l)}return r.clone()}}
        // Nothing usable came back. Record exactly what every provider did so a "找不到歌词"
        // report can be diagnosed from the log instead of guessed at.
        let detail=results.iter().map(|(p,r)|format!("{}={{status:{:?} lyrics:{} candidate:{:?} score:{:?}}}",p.value(),r.status,r.lyrics.is_some(),r.lyrics.as_ref().map(|l|l.candidate_id.clone()),r.lyrics.as_ref().and_then(ResolvedLyrics::identity_score))).collect::<Vec<_>>().join(" ");
        let answered=results.iter().map(|(p,_)|p.value().to_string()).collect::<Vec<_>>();
        let silent=providers.iter().filter(|p|!answered.contains(&p.value().to_string())).map(|p|p.value().to_string()).collect::<Vec<_>>();
        io::log(&self.logs,"LYRIC_MISS",&format!("title={:?} artist={:?} songId={:?} sourceApp={:?} asked=[{}] silent=[{}] directSongIdForNetease={} detail: {}",
            track.title,track.artist,track.song_id,track.source_app,
            providers.iter().map(|p|p.value()).collect::<Vec<_>>().join(","),silent.join(","),
            lyric_core::policy::ProviderSongIdPolicy::can_use_direct_song_id(&TrackIdentity::from_track_info(track),&known_providers::netease()),detail));
        if results.iter().any(|(_,r)|r.status==LyricResolutionStatus::TransientFailure){LyricResolutionResult{lyrics:None,status:LyricResolutionStatus::TransientFailure}}else{LyricResolutionResult::from_lyrics(None)}
    }
    fn start_artwork(self:&Arc<Self>,key:String,url:String){
        let Some(rest)=url.strip_prefix("https://").or_else(||url.strip_prefix("http://"))else{return};let host=rest.split('/').next().unwrap_or("").to_ascii_lowercase();if host!="music.126.net"&&!host.ends_with(".music.126.net"){return}
        let url=format!("https://{rest}{}",if rest.contains('?'){""}else{"?param=300y300"});let backend=self.clone();
        std::thread::spawn(move||{let ctx=RequestContext::new(Duration::from_secs(5));match http::request(&url,"GET",&[],"","",&ctx,4*1024*1024){Ok(bytes)=>{if let Ok(mut s)=backend.state.lock(){if s.track_key==key&&!backend.closed.load(Ordering::Acquire){s.cover=Some(bytes);s.cover_version+=1;s.artwork_identity=format!("Netease|{}|{}",s.playback.track.as_ref().and_then(|t|t.song_id.clone()).unwrap_or_default(),url);s.playback.is_cover_loading=false;}}},Err(e)=>io::log(&backend.logs,"ARTWORK",&e)}});
    }
}
fn wait_delay(cancel:&AtomicBool,delay:Duration)->bool{let until=Instant::now()+delay;while Instant::now()<until{if cancel.load(Ordering::Acquire){return false}std::thread::sleep(Duration::from_millis(30));}true}
#[cfg(test)]mod tests{
use super::*;
use lyric_core::resolution::LyricSource;
/// `resolve` builds one coordinator per provider. With the default trust policy that asks for the
/// whole online order and fails for a single-source coordinator, which used to be swallowed and
/// made every online resolution report "unavailable" — i.e. no lyrics unless already cached.
#[test]fn one_source_coordinator_requires_a_matching_trust_policy(){
    let source=||Box::new(OnlineSource::new("Netease",RequestContext::new(Duration::from_secs(1)))) as Box<dyn LyricSource>;
    let default_policy=LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder{sources:vec![source()],..Default::default()});
    assert!(default_policy.is_err(),"the default policy must keep failing for a one-source coordinator");
    let provider=known_providers::netease();
    let policy=lyric_core::policy::LyricProviderTrustPolicy::new(vec![provider.clone()],vec![provider.clone()]).expect("single-provider policy");
    let matched=LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder{sources:vec![source()],trust_policy:Some(policy),..Default::default()});
    assert!(matched.is_ok(),"a matching trust policy must let the per-provider coordinator be built");
}
}


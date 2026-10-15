//! Native bounded duration enrichment, preserving the managed v1 cache format.
use std::{path::PathBuf,collections::VecDeque,sync::mpsc,time::{Duration,Instant}};
use serde_json::{Value,json};
use lyric_core::{models::*,policy::LyricIdentityEvaluator,resolution::LyricSource,sync::LyricSyncService};
use crate::{io,storage::{duration_from_text,to_managed},providers::OnlineSource,http::RequestContext};
#[derive(Clone)]struct Entry{duration:Duration,provider:String,id:String}
pub struct DurationResolver{path:PathBuf,cache:VecDeque<(String,Entry)>,identity:String,pending:Option<mpsc::Receiver<Option<Entry>>>,retry:Instant,pub source:String}
impl DurationResolver{
    pub fn new(path:PathBuf)->Self{
        let mut cache=VecDeque::new();let value=io::read_settings(&path);
        if let Some(entries)=value.as_object(){for(key,value)in entries{if let Some(duration)=value["Duration"].as_str().and_then(duration_from_text).filter(|d|valid(*d)){cache.push_back((key.clone(),Entry{duration,provider:value["Provider"].as_str().unwrap_or("").into(),id:value["CandidateId"].as_str().unwrap_or("").into()}));while cache.len()>256{cache.pop_front();}}}}
        Self{path,cache,identity:String::new(),pending:None,retry:Instant::now(),source:"等待歌曲信息".into()}
    }
    pub fn enrich(&mut self,playback:&mut PlaybackSnapshot){
        let key=playback.track.as_ref().map(LyricSyncService::build_stable_track_identity).unwrap_or_default();
        if key!=self.identity{self.identity=key.clone();self.pending=None;self.retry=Instant::now()}
        let Some(track)=playback.track.as_mut()else{self.source="等待歌曲信息".into();return};
        if valid(track.duration){self.source="播放器".into();return}
        if let Some(receiver)=&self.pending{match receiver.try_recv(){Ok(result)=>{self.pending=None;self.retry=Instant::now()+Duration::from_secs(30);if let Some(entry)=result{self.cache.retain(|(id,_)|*id!=key);self.cache.push_back((key.clone(),entry));while self.cache.len()>256{self.cache.pop_front();}self.save();}},Err(mpsc::TryRecvError::Disconnected)=>{self.pending=None;self.retry=Instant::now()+Duration::from_secs(30)},Err(_)=>{}}}
        if let Some((_,entry))=self.cache.iter().find(|(id,_)|*id==key){track.duration=entry.duration;self.source=format!("{}歌曲信息",entry.provider);return}
        self.source="正在补取歌曲时长".into();
        if self.pending.is_none()&&Instant::now()>=self.retry{let(tx,rx)=mpsc::channel();self.pending=Some(rx);let track=track.clone();std::thread::spawn(move||{let _=tx.send(lookup(&track));});}
    }
    fn save(&self){let map=self.cache.iter().map(|(key,e)|(key.clone(),json!({"Duration":to_managed(json!({"duration":{"secs":e.duration.as_secs(),"nanos":e.duration.subsec_nanos()}}),true)["Duration"],"Provider":e.provider,"CandidateId":e.id}))).collect::<serde_json::Map<_,_>>();let _=io::save_settings(&self.path,&Value::Object(map));}
}
fn valid(duration:Duration)->bool{duration>Duration::ZERO&&duration<=Duration::from_secs(86400)}
fn select(identity:&TrackIdentity,candidates:Vec<SourceTrackCandidate>)->Option<SourceTrackCandidate>{
    candidates.into_iter().filter(|c|valid(c.duration)).map(|c|{let e=LyricIdentityEvaluator::evaluate(identity,&c);(c,e)}).filter(|(_,e)|e.is_admitted&&e.score>=95).max_by_key(|(_,e)|e.score).map(|(c,_)|c)
}
fn lookup(track:&TrackInfo)->Option<Entry>{
    if track.title.trim().is_empty(){return None}let mut identity=TrackIdentity::from_track_info(track);identity.song_id=None;identity.duration=Duration::ZERO;
    let plan=LyricSearchPlan{original_track:identity.clone(),variants:vec![SearchQueryVariant{id:"exact".into(),title:identity.title.clone(),artists:identity.artists.clone(),album:identity.album.clone(),duration:Duration::ZERO,relaxation_reasons:vec![]}]};
    let providers=if track.source_app=="Netease"{["Netease","QQMusic","LRCLIB"]}else{["QQMusic","Netease","LRCLIB"]};
    for provider in providers{let source=OnlineSource::new(provider,RequestContext::new(Duration::from_secs(5)));if let Ok(candidates)=source.search(&plan){if let Some(c)=select(&identity,candidates){return Some(Entry{duration:c.duration,provider:provider.into(),id:c.candidate_id})}}}None
}
#[cfg(test)]mod tests{use super::*;#[test]fn duration_never_accepts_live_variant_or_unknown_identity(){let t=TrackInfo::new("t","歌曲","歌手","","Spotify",Duration::ZERO);let identity=TrackIdentity::from_track_info(&t);let candidate=|title:&str,artist:&str|SourceTrackCandidate::try_new(known_providers::netease(),"1",title,vec![artist.into()],"",Duration::from_secs(180),"exact",Default::default()).unwrap();assert!(select(&identity,vec![candidate("歌曲 (Live)","歌手")]).is_none());assert!(select(&identity,vec![candidate("歌曲","另一位")]).is_none());assert!(select(&identity,vec![candidate("歌曲","歌手")]).is_some());}}

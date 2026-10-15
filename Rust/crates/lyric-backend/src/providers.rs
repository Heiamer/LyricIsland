//! Native ports of the service protocols used by Lyricify Lyrics Helper (Apache-2.0).
//! Reimplemented in Rust; request construction, transport and decryption are native.
use std::{collections::{BTreeMap,HashSet},time::{Duration,SystemTime,UNIX_EPOCH}};
use aes::{Aes128,cipher::{BlockEncrypt,KeyInit,generic_array::GenericArray}};
use md5::{Md5,Digest};
use base64::{Engine,engine::general_purpose::STANDARD};
use serde_json::{Value,json};
use lyric_core::{models::*, resolution::LyricSource, policy::{SourceFailure,ProviderSongIdPolicy},crypto};
use crate::http::{self,RequestContext};

pub struct OnlineSource { pub id:LyricProviderId, pub context:RequestContext }
pub fn text(v:&Value,key:&str)->String{match &v[key]{Value::String(s)=>s.clone(),Value::Number(n)=>n.to_string(),_=>String::new()}}
fn artists(v:&Value)->Vec<String>{v.as_array().map(|a|a.iter().map(|i|text(i,"name")).filter(|s|!s.is_empty()).collect()).unwrap_or_default()}
fn items<'a>(v:&'a Value,pointer:&str)->&'a [Value]{v.pointer(pointer).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])}
fn failure(e:String)->SourceFailure{if e=="cancelled"{SourceFailure::Other(e.clone())}else if e=="timeout"{SourceFailure::Timeout}else{SourceFailure::Connection}}

pub fn eapi_body(path:&str,data:Value)->String{
    let body=data.to_string();let digest=Md5::digest(format!("nobody{path}use{body}md5forencrypt"));
    let digest=digest.iter().map(|b|format!("{b:02x}")).collect::<String>();
    let mut message=format!("{path}-36cd479b6b5-{body}-36cd479b6b5-{digest}").into_bytes();
    let pad=16-message.len()%16;message.extend(std::iter::repeat_n(pad as u8,pad));
    let cipher=Aes128::new(GenericArray::from_slice(b"e82ckenh8dichen8"));
    for block in message.chunks_exact_mut(16){cipher.encrypt_block(GenericArray::from_mut_slice(block));}
    format!("params={}",message.iter().map(|b|format!("{b:02X}")).collect::<String>())
}
fn eapi(path:&str,mut data:Value,ctx:&RequestContext)->Result<Value,String>{
    let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    data["header"]=Value::String(json!({"__csrf":"","appver":"8.0.0","buildver":now.as_secs().to_string(),"channel":"","deviceId":"","mobilename":"","resolution":"1920x1080","os":"android","osver":"","requestId":format!("{}_0001",now.as_millis()),"versioncode":"140","MUSIC_U":""}).to_string());
    http::post(&format!("https://interface.music.163.com/e{}",&path[1..]),&eapi_body(path,data),"application/x-www-form-urlencoded","https://music.163.com/",ctx)
}
impl OnlineSource{
    pub fn new(id:&str,context:RequestContext)->Self{Self{id:LyricProviderId::try_new(id).unwrap(),context}}
    fn candidate(&self,id:String,title:String,artists:Vec<String>,album:String,duration:f64,variant:&str,metadata:BTreeMap<String,String>)->SourceTrackCandidate{
        SourceTrackCandidate{provider_id:self.id.clone(),candidate_id:id,title,artists,album,duration:Duration::from_secs_f64(duration.max(0.).min(86400.)),query_variant_id:variant.into(),fetch_metadata:metadata}
    }
    fn variant(&self,v:&SearchQueryVariant)->Result<Vec<SourceTrackCandidate>,String>{
        self.context.check()?;let query=std::iter::once(v.title.as_str()).chain(v.artists.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
        let mut out=vec![];
        match self.id.value(){
            "Netease"=>{
                let response=eapi("/api/cloudsearch/pc",json!({"s":query,"type":"1","limit":"30","offset":"0","total":"true"}),&self.context)?;
                for s in items(&response,"/result/songs"){
                    out.push(self.candidate(text(s,"id"),text(s,"name"),artists(s.get("ar").unwrap_or(&s["artists"])),text(s.get("al").unwrap_or(&s["album"]),"name"),s.get("dt").unwrap_or(&s["duration"]).as_f64().unwrap_or(0.)/1000.,&v.id,BTreeMap::new()));
                }
            }
            "QQMusic"=>{
                let data=json!({"req_1":{"method":"DoSearchForQQMusicDesktop","module":"music.search.SearchCgiService","param":{"num_per_page":"20","page_num":"1","query":query,"search_type":0}}});
                let response=http::post("https://u.y.qq.com/cgi-bin/musicu.fcg",&data.to_string(),"application/json","https://y.qq.com/",&self.context)?;
                for s in items(&response,"/req_1/data/body/song/list"){
                    let title=if text(s,"title").is_empty(){text(s,"name")}else{text(s,"title")};
                    out.push(self.candidate(text(s,"id"),title,artists(&s["singer"]),text(&s["album"],"name"),s["interval"].as_f64().unwrap_or(0.),&v.id,BTreeMap::from([("mid".into(),text(s,"mid"))])));
                }
            }
            "LRCLIB"=>{
                let response=http::get(&format!("https://lrclib.net/api/search?track_name={}&artist_name={}&album_name={}",http::encode(&v.title),http::encode(&v.artists.join(", ")),http::encode(&v.album)),&self.context)?;
                for s in response.as_array().map(Vec::as_slice).unwrap_or(&[]){out.push(self.candidate(text(s,"id"),text(s,"trackName"),vec![text(s,"artistName")],text(s,"albumName"),s["duration"].as_f64().unwrap_or(0.),&v.id,BTreeMap::new()));}
            }
            "Kugou"=>{
                let response=http::get(&format!("https://mobilecdn.kugou.com/api/v3/search/song?format=json&keyword={}&page=1&pagesize=20&showtype=1",http::encode(&query)),&self.context)?;
                for s in items(&response,"/data/info"){
                    out.extend(self.kugou_candidates(&text(s,"hash"),&text(s,"songname"),&text(s,"singername"),&text(s,"album_name"),&v.id)?);
                }
            }
            _=>{}
        }
        out.retain(|c|!c.candidate_id.is_empty());Ok(out)
    }
    fn kugou_candidates(&self,hash:&str,title:&str,artist:&str,album:&str,variant:&str)->Result<Vec<SourceTrackCandidate>,String>{
        let url=format!("https://lyrics.kugou.com/search?ver=1&man=yes&client=pc&hash={}",http::encode(hash));
        let mut response=http::get(&url,&self.context)?;
        if items(&response,"/candidates").is_empty(){std::thread::sleep(Duration::from_millis(150));response=http::get(&url,&self.context)?;}
        Ok(items(&response,"/candidates").iter().filter(|s|!text(s,"id").is_empty()&&!text(s,"accesskey").is_empty()).map(|s|self.candidate(text(s,"id"),if text(s,"song").is_empty(){title.into()}else{text(s,"song")},if text(s,"singer").is_empty(){artist}else{s["singer"].as_str().unwrap_or(artist)}.split(['&','/','、','，',',']).map(|a|a.trim().to_string()).filter(|a|!a.is_empty()).collect(),album.into(),s["duration"].as_f64().unwrap_or(0.)/1000.,variant,BTreeMap::from([("access-key".into(),text(s,"accesskey")),("song-hash".into(),hash.into())]))).collect())
    }
    fn payload(&self,c:&SourceTrackCandidate)->Result<Option<RawLyricPayload>,String>{
        let (format,original,translation,pure)=match self.id.value(){
            "Netease"=>{
                let response=eapi("/api/song/lyric/v1",json!({"id":c.candidate_id,"cp":"false","lv":"0","kv":"0","tv":"0","rv":"0","yv":"0","ytv":"0","yrv":"0","csrf_token":""}),&self.context)?;
                if response["nolyric"].as_bool()==Some(true)||response["uncollected"].as_bool()==Some(true){return Ok(None)}
                if !text(&response["yrc"],"lyric").is_empty(){(LyricPayloadFormat::Yrc,text(&response["yrc"],"lyric"),if !text(&response["ytlrc"],"lyric").is_empty(){text(&response["ytlrc"],"lyric")}else{text(&response["tlyric"],"lyric")},false)}
                else{(LyricPayloadFormat::Lrc,text(&response["lrc"],"lyric"),text(&response["tlyric"],"lyric"),false)}
            }
            "LRCLIB"=>{
                if c.candidate_id.parse::<u64>().is_err(){return Ok(None)}
                let response=http::get(&format!("https://lrclib.net/api/get/{}",c.candidate_id),&self.context)?;
                if !text(&response,"syncedLyrics").is_empty(){(LyricPayloadFormat::Lrc,text(&response,"syncedLyrics"),String::new(),response["instrumental"].as_bool().unwrap_or(false))}
                else{(LyricPayloadFormat::PlainText,text(&response,"plainLyrics"),String::new(),response["instrumental"].as_bool().unwrap_or(false))}
            }
            "Kugou"=>{
                let key=c.fetch_metadata.get("access-key").ok_or("missing access-key")?;
                let response=http::get(&format!("https://lyrics.kugou.com/download?ver=1&client=pc&id={}&accesskey={}&fmt=krc&charset=utf8",http::encode(&c.candidate_id),http::encode(key)),&self.context)?;
                (LyricPayloadFormat::Krc,crypto::decrypt_krc(&text(&response,"content")).map_err(|e|e.to_string())?,String::new(),false)
            }
            "QQMusic"=>{
                let body=format!("version=15&miniversion=82&lrctype=4&musicid={}",http::encode(&c.candidate_id));
                let xml=String::from_utf8(http::request("https://c.y.qq.com/qqmusic/fcgi-bin/lyric_download.fcg","POST",body.as_bytes(),"application/x-www-form-urlencoded","https://y.qq.com/",&self.context,8*1024*1024)?).map_err(|e|e.to_string())?;
                let xml=xml.replace("<!--","").replace("-->","");
                let extract=|tag:&str|->String{
                    let re=regex::Regex::new(&format!(r"(?s)<{tag}[^>]*>(.*?)</{tag}>" )).unwrap();
                    let raw=re.captures(&xml).map(|c|c[1].trim().trim_start_matches("<![CDATA[").trim_end_matches("]]>").to_string()).unwrap_or_default();
                    let decoded=crypto::decrypt_qrc(&raw).unwrap_or(raw);
                    let attribute=regex::Regex::new(r#"(?s)<Lyric_1\b[^>]*\bLyricContent="(.*?)""#).unwrap();
                    attribute.captures(&decoded).map(|c|c[1].replace("&#13;","\r").replace("&#10;","\n").replace("&quot;","\"").replace("&apos;","'").replace("&lt;","<").replace("&gt;",">").replace("&amp;","&")).unwrap_or(decoded)
                };
                let qrc=extract("content");let trans=extract("contentts");
                if !qrc.is_empty(){(LyricPayloadFormat::Qrc,qrc,trans,false)}else{
                    let Some(mid)=c.fetch_metadata.get("mid")else{return Ok(None)};
                    let body=format!("songmid={}&g_tk=5381&loginUin=0&hostUin=0&format=json&inCharset=utf8&outCharset=utf8&platform=yqq&needNewCode=0",http::encode(mid));
                    let response=http::post("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg",&body,"application/x-www-form-urlencoded","https://y.qq.com/",&self.context)?;
                    let decode=|s:String|String::from_utf8(STANDARD.decode(&s).unwrap_or_default()).unwrap_or_default();
                    (LyricPayloadFormat::Lrc,decode(text(&response,"lyric")),decode(text(&response,"trans")),false)
                }
            }
            _=>return Ok(None)
        };
        if original.trim().is_empty()&&!pure{return Ok(None)}
        Ok(Some(RawLyricPayload{provider_id:self.id.clone(),candidate_id:c.candidate_id.clone(),format,original_lyrics:Some(original),translation_lyrics:if translation.is_empty(){None}else{Some(translation)},is_encrypted:false,is_pure_music:pure,diagnostics:BTreeMap::new(),has_stable_identity:true}))
    }
}
impl LyricSource for OnlineSource{
    fn provider_id(&self)->&LyricProviderId{&self.id}
    fn search(&self,plan:&LyricSearchPlan)->Result<Vec<SourceTrackCandidate>,SourceFailure>{
        if ProviderSongIdPolicy::can_use_direct_song_id(&plan.original_track,&self.id){
            let t=&plan.original_track;let id=t.song_id.clone().unwrap_or_default();
            if self.id.value()=="Kugou"{return self.kugou_candidates(&id,&t.title,&t.artists.join(", "),&t.album,"direct-song-id").map_err(failure)}
            return Ok(vec![self.candidate(id,t.title.clone(),t.artists.clone(),t.album.clone(),t.duration.as_secs_f64(),"direct-song-id",BTreeMap::from([("direct".into(),"true".into())]))]);
        }
        let mut all=vec![];let mut seen=HashSet::new();
        for variant in &plan.variants {
            let found=self.variant(variant).map_err(failure)?;
            for candidate in found{if seen.insert(candidate.candidate_id.clone()){all.push(candidate)}}
            if !all.is_empty(){break}
        }
        Ok(all)
    }
    fn fetch(&self,c:&SourceTrackCandidate)->Result<Option<RawLyricPayload>,SourceFailure>{self.payload(c).map_err(failure)}
}
#[cfg(test)]mod tests{use super::*;#[test]fn eapi_cipher_is_reproducible_and_form_safe(){let s=eapi_body("/api/song/lyric/v1",json!({"id":"1"}));assert!(s.starts_with("params="));assert!(s[7..].bytes().all(|b|b.is_ascii_hexdigit()));assert_eq!((s.len()-7)%32,0);assert_eq!(s,eapi_body("/api/song/lyric/v1",json!({"id":"1"})));}}



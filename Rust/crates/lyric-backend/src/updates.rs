//! GitHub Releases is the update source; installation is performed in place.
use serde_json::{json,Value};
use std::time::Duration;
pub const RELEASES:&str="https://github.com/Heiamer/LyricIsland/releases/latest";
fn version(input:&str)->Option<[u32;4]>{
    let input=input.strip_prefix('v').unwrap_or(input);let mini=input.ends_with("mini");let input=if mini{input.strip_suffix("mini").unwrap().trim_end_matches('-')}else{input};
    let parts=input.split('.').collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len()){return None}
    let mut out=[0;4];out[3]=if mini{1}else{0};for (i,p) in parts.iter().enumerate(){if p.is_empty()||!p.bytes().all(|c|c.is_ascii_digit()){return None}out[i]=p.parse().ok()?;}Some(out)
}
pub fn describe(release:&Value,current:&str)->Result<Value,String>{
    if release["draft"]==true||release["prerelease"]==true{return Err("未找到正式更新".into())}
    let tag=release["tag_name"].as_str().ok_or("更新响应缺少版本号")?;
    let latest=version(tag).ok_or("更新版本号格式无效")?;
    let newer=latest>version(current).ok_or("当前版本号无效")?;
    Ok(json!({"success":true,"message":if newer{format!("发现新版本 {tag}，点击“下载并更新”自动安装，保留现有设置。")}else{format!("当前已是最新版本（{current}）")},"newer":newer,"version":tag,"url":RELEASES}))
}
fn sources()->Value{let path=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("Assets/UpdateSources.json")));path.and_then(|p|std::fs::read(p).ok()).and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_else(||serde_json::from_str(include_str!("../../../../Assets/UpdateSources.json")).unwrap_or_else(|_|json!({})))}
fn repository(input:&str,host:&str)->Result<String,String>{let prefix=format!("https://{host}/");let slug=input.trim().strip_prefix(&prefix).ok_or("更新源必须使用官方 HTTPS 仓库地址")?.trim_end_matches('/').trim_end_matches(".git");let parts=slug.split('/').collect::<Vec<_>>();if parts.len()!=2||parts.iter().any(|p|p.is_empty()||!p.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.".contains(&b))||*p=="."||*p==".."){return Err("更新仓库地址无效".into())}Ok(slug.to_owned())}
fn gitee_asset_url(slug:&str,release_id:u64,assets:&Value,name:&str)->Result<String,String>{let item=assets.as_array().and_then(|a|a.iter().find(|a|a["name"].as_str().or_else(||a["file_name"].as_str())==Some(name))).ok_or_else(||format!("国内发行版缺少 {name}"))?;let id=item["id"].as_u64().filter(|n|*n>0).ok_or("国内附件标识无效")?;Ok(format!("https://gitee.com/api/v5/repos/{slug}/releases/{release_id}/attach_files/{id}/download"))}
fn check_gitee(repo:&str)->Result<Value,String>{let slug=repository(repo,"gitee.com")?;let api=format!("https://gitee.com/api/v5/repos/{slug}");let ctx=crate::http::RequestContext::new(Duration::from_secs(10));let release=crate::http::get_system(&format!("{api}/releases/latest"),&ctx)?;let mut result=describe(&release,env!("CARGO_PKG_VERSION"))?;let tag=release["tag_name"].as_str().ok_or("国内版本号缺失")?;result["source"]=json!("Gitee 国内源");result["url"]=json!(format!("https://gitee.com/{slug}/releases/tag/{tag}"));if result["newer"]==true{let id=release["id"].as_u64().filter(|n|*n>0).ok_or("国内发行版标识无效")?;let attachments=crate::http::get_system(&format!("{api}/releases/{id}/attach_files?per_page=100"),&ctx)?;let v=tag.trim_start_matches('v');result["manifestUrl"]=json!(gitee_asset_url(&slug,id,&attachments,&format!("LyricIsland-{v}-SHA256.txt"))?);result["zipUrl"]=json!(gitee_asset_url(&slug,id,&attachments,&format!("LyricIsland-{v}-Portable.zip"))?);}Ok(result)}
fn check_github()->Result<Value,String>{let ctx=crate::http::RequestContext::new(Duration::from_secs(10));let release=match crate::http::get_system("https://api.github.com/repos/Heiamer/LyricIsland/releases/latest",&ctx){Ok(value)=>value,Err(api_error)=>{let ctx=crate::http::RequestContext::new(Duration::from_secs(10));let bytes=crate::http::request_system(RELEASES,&ctx,4*1024*1024).map_err(|e|format!("API：{api_error}；页面：{e}"))?;page_release(&String::from_utf8_lossy(&bytes))?}};let mut result=describe(&release,env!("CARGO_PKG_VERSION"))?;let v=release["tag_name"].as_str().ok_or("版本号缺失")?.trim_start_matches('v');let base=format!("https://github.com/Heiamer/LyricIsland/releases/download/v{v}/LyricIsland-{v}");result["source"]=json!("GitHub");result["manifestUrl"]=json!(format!("{base}-SHA256.txt"));result["zipUrl"]=json!(format!("{base}-Portable.zip"));Ok(result)}
/// Request both official sources concurrently. A reachable source with an update may win
/// immediately; a no-update response waits for the other source before declaring up to date.
pub fn check()->Result<Value,String>{let config=sources();let repo=config["giteeRepository"].as_str().unwrap_or("").to_owned();let (tx,rx)=std::sync::mpsc::channel();let mut count=1;if !repo.is_empty(){let tx=tx.clone();count+=1;std::thread::spawn(move||{let _=tx.send(("Gitee 国内源",check_gitee(&repo)));});}let github_tx=tx.clone();std::thread::spawn(move||{let _=github_tx.send(("GitHub",check_github()));});drop(tx);let mut available=None;let mut errors=vec![];for _ in 0..count{match rx.recv_timeout(Duration::from_secs(22)){Ok((source,Ok(mut result)))=>{let message=result["message"].as_str().unwrap_or("").to_owned();result["message"]=json!(format!("{message} · {source}"));if result["newer"]==true{return Ok(result)}available=Some(result);},Ok((source,Err(e)))=>errors.push(format!("{source}：{e}")),Err(_)=>{errors.push("其他更新源请求超时".into());break}}}available.ok_or_else(||format!("检查更新失败。{}。请检查网络或系统代理。",errors.join("；")))}
fn page_release(html:&str)->Result<Value,String>{
    let pattern=regex::Regex::new(r#"<meta\s+property="og:url"\s+content="(?:https://github\.com)?/Heiamer/LyricIsland/releases/tag/(v?[0-9]+\.[0-9]+(?:\.[0-9]+)?(?:-?mini)?)""#).unwrap();
    let tag=pattern.captures(html).and_then(|c|c.get(1)).ok_or("GitHub 页面未提供正式版本号，请稍后重试。")?.as_str();
    Ok(json!({"tag_name":tag}))
}
/// The helper runs from a unique temporary directory so it can replace its installed copy.
pub fn install()->Result<Value,String>{
    let release=check()?;if release["newer"]!=true{return Ok(release)}
    let version=release["version"].as_str().ok_or("missing version")?.trim_start_matches('v');
    let base=std::env::current_exe().map_err(|e|e.to_string())?.parent().ok_or("missing directory")?.to_path_buf();
    let data=std::env::var_os("LYRICISLAND_NATIVE_DATA").map(std::path::PathBuf::from).unwrap_or_else(||base.join("Data"));
    let pipe=std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_|"LyricIsland.Native.Settings.v1".into());
    let state=crate::io::pipe_request(&pipe,&json!({"version":1,"command":"state"}))?;
    let pid=state["processId"].as_u64().ok_or("无法确认正在运行的应用")?;
    let temp=std::env::temp_dir().join(format!("LyricIsland.UpdateHelper.{}.{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    std::fs::create_dir(&temp).map_err(|e|e.to_string())?;let helper=temp.join("LyricIsland.Updater.exe");
    std::fs::copy(base.join("LyricIsland.Updater.exe"),&helper).map_err(|e|e.to_string())?;
    std::process::Command::new(helper).arg(version).arg(base).arg(data).arg(pid.to_string()).arg(std::process::id().to_string()).arg(pipe).arg(release["manifestUrl"].as_str().unwrap_or("")).arg(release["zipUrl"].as_str().unwrap_or("")).spawn().map_err(|e|e.to_string())?;
    Ok(json!({"success":true,"message":"正在下载并校验更新，完成后会自动重启；下载失败时保留当前版本。"}))
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn repositories_require_expected_hosts_and_two_safe_segments(){assert_eq!(repository("https://gitee.com/team/dotisle.git","gitee.com").unwrap(),"team/dotisle");for url in ["http://gitee.com/a/b","https://evil.test/a/b","https://gitee.com/a/b/extra","https://gitee.com/a/..","https://gitee.com/a/b?q=x"]{assert!(repository(url,"gitee.com").is_err());}}
    #[test]fn domestic_urls_are_constructed_from_validated_attachment_ids(){let assets=json!([{"name":"LyricIsland-1.3.1-Portable.zip","id":42,"browser_download_url":"https://evil.test"}]);assert_eq!(gitee_asset_url("team/dotisle",7,&assets,"LyricIsland-1.3.1-Portable.zip").unwrap(),"https://gitee.com/api/v5/repos/team/dotisle/releases/7/attach_files/42/download");assert!(gitee_asset_url("team/dotisle",7,&assets,"missing").is_err());}
    #[test]fn compares_numeric_versions_and_never_downgrades(){
        assert_eq!(describe(&json!({"tag_name":"v1.10"}),"1.2.0").unwrap()["newer"],true);
        assert_eq!(describe(&json!({"tag_name":"v1.1"}),"1.2.0").unwrap()["newer"],false);
        assert_eq!(describe(&json!({"tag_name":"v1.2"}),"1.2.0").unwrap()["newer"],false);
    }
    #[test]fn rejects_untrusted_tags_and_preview_releases(){for tag in ["latest","v1.3-beta","v1.3/evil"]{assert!(describe(&json!({"tag_name":tag}),"1.2.0").is_err());}assert!(describe(&json!({"tag_name":"v2.0","prerelease":true}),"1.2.0").is_err());}
    #[test]fn fallback_reads_only_canonical_repository_metadata(){assert_eq!(page_release(r#"<meta property="og:url" content="/Heiamer/LyricIsland/releases/tag/v1.2" />"#).unwrap()["tag_name"],"v1.2");assert!(page_release(r#"<meta property="og:url" content="/other/project/releases/tag/v9.0" />"#).is_err());}
}

/// Launch the exact checked candidate without a second latest-version race.
pub fn launch_candidate(release:&Value,base:&std::path::Path,data:&std::path::Path,pipe:&str,job:&str,nonce:&str)->Result<(),String>{
 let version=release["version"].as_str().ok_or("缺少更新版本")?.trim_start_matches('v');
 let pid=std::process::id();
 let temp=std::env::temp_dir().join(format!("DotIsle.UpdateHelper.{}",job.replace(['{','}','-'],"")));std::fs::create_dir_all(&temp).map_err(|e|e.to_string())?;
 let helper=temp.join("LyricIsland.Updater.exe");std::fs::copy(base.join("LyricIsland.Updater.exe"),&helper).map_err(|e|e.to_string())?;
 std::process::Command::new(helper).arg(version).arg(base).arg(data).arg(pid.to_string()).arg("0").arg(pipe).arg(release["manifestUrl"].as_str().unwrap_or("")).arg(release["zipUrl"].as_str().unwrap_or("")).arg(job).arg(nonce).spawn().map_err(|e|e.to_string())?;Ok(())
}

#[cfg(test)]mod mini_version_tests{use super::*;
#[test]fn mini_version_orders_between_base_and_next_patch(){assert!(version("2.1.5mini")>version("2.1.5"));assert_eq!(version("v2.1.5mini"),version("2.1.5-mini"));assert!(version("2.1.6")>version("2.1.5mini"));assert!(version("2.1.5mini/evil").is_none());assert!(version("2.1.5-beta").is_none());}
}

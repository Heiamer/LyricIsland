//! GitHub Releases is the update source. Downloads are opened for the user to install.
use serde_json::{json,Value};
use std::time::Duration;
pub const RELEASES:&str="https://github.com/Heiamer/LyricIsland/releases/latest";
fn version(input:&str)->Option<[u32;3]>{
    let parts=input.strip_prefix('v').unwrap_or(input).split('.').collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len()){return None}
    let mut out=[0;3];for (i,p) in parts.iter().enumerate(){if p.is_empty()||!p.bytes().all(|c|c.is_ascii_digit()){return None}out[i]=p.parse().ok()?;}Some(out)
}
pub fn describe(release:&Value,current:&str)->Result<Value,String>{
    if release["draft"]==true||release["prerelease"]==true{return Err("未找到正式更新".into())}
    let tag=release["tag_name"].as_str().ok_or("更新响应缺少版本号")?;
    let latest=version(tag).ok_or("更新版本号格式无效")?;
    let newer=latest>version(current).ok_or("当前版本号无效")?;
    Ok(json!({"success":true,"message":if newer{format!("发现新版本 {tag}，点击“前往下载”在 GitHub 下载并安装。升级前请退出旧版。")}else{format!("当前已是最新版本（{current}）")},"newer":newer,"version":tag,"url":RELEASES}))
}
pub fn check()->Result<Value,String>{
    let ctx=crate::http::RequestContext::new(Duration::from_secs(10));
    let release=match crate::http::get("https://api.github.com/repos/Heiamer/LyricIsland/releases/latest",&ctx){
        Ok(value)=>value,
        Err(api_error)=>{
            // Public API requests can hit a shared IP rate limit. GitHub's latest page also
            // resolves only to a stable release; read its own canonical metadata as fallback.
            let ctx=crate::http::RequestContext::new(Duration::from_secs(10));
            let bytes=crate::http::request(RELEASES,"GET",&[],"text/html","",&ctx,4*1024*1024)
                .map_err(|_|format!("检查更新失败：{api_error}。可点击“前往下载”访问 GitHub。"))?;
            page_release(&String::from_utf8_lossy(&bytes))?
        }
    };
    describe(&release,env!("CARGO_PKG_VERSION"))
}
fn page_release(html:&str)->Result<Value,String>{
    let pattern=regex::Regex::new(r#"<meta\s+property="og:url"\s+content="(?:https://github\.com)?/Heiamer/LyricIsland/releases/tag/(v?[0-9]+\.[0-9]+(?:\.[0-9]+)?)""#).unwrap();
    let tag=pattern.captures(html).and_then(|c|c.get(1)).ok_or("GitHub 页面未提供正式版本号，请点击“前往下载”查看。")?.as_str();
    Ok(json!({"tag_name":tag}))
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn compares_numeric_versions_and_never_downgrades(){
        assert_eq!(describe(&json!({"tag_name":"v1.10"}),"1.2.0").unwrap()["newer"],true);
        assert_eq!(describe(&json!({"tag_name":"v1.1"}),"1.2.0").unwrap()["newer"],false);
        assert_eq!(describe(&json!({"tag_name":"v1.2"}),"1.2.0").unwrap()["newer"],false);
    }
    #[test]fn rejects_untrusted_tags_and_preview_releases(){for tag in ["latest","v1.3-beta","v1.3/evil"]{assert!(describe(&json!({"tag_name":tag}),"1.2.0").is_err());}assert!(describe(&json!({"tag_name":"v2.0","prerelease":true}),"1.2.0").is_err());}
    #[test]fn fallback_reads_only_canonical_repository_metadata(){assert_eq!(page_release(r#"<meta property="og:url" content="/Heiamer/LyricIsland/releases/tag/v1.2" />"#).unwrap()["tag_name"],"v1.2");assert!(page_release(r#"<meta property="og:url" content="/other/project/releases/tag/v9.0" />"#).is_err());}
}

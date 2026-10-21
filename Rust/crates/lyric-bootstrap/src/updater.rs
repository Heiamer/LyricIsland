use crate::error::{Error, Result};
use crate::http;
use crate::paths;
use crate::payload;
use crate::ui::Overlay;
use crate::win;
use lyric_archive::ZipWriter;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CANCELLED_MESSAGE: &str = "更新已取消，原程序保持不变。";
const PINNED_MISMATCH: &str = "同一版本的更新源摘要不一致，已停止更新。";

#[derive(Clone, Default)]
pub struct OverlayState {
    pub phase: String,
    pub received: u64,
    pub length: Option<u64>,
    pub message: String,
}

pub fn shared_state() -> Arc<Mutex<OverlayState>> { Arc::new(Mutex::new(OverlayState::default())) }

pub struct Reporter {
    authenticated: bool,
    job: String,
    nonce: String,
    pipe: String,
    seq: u64,
    last: Option<Instant>,
    directive: String,
    state: Arc<Mutex<OverlayState>>,
}

impl Reporter {
    pub fn new(authenticated: bool, job: String, nonce: String, pipe: String, state: Arc<Mutex<OverlayState>>) -> Self {
        Reporter { authenticated, job, nonce, pipe, seq: 0, last: None, directive: String::new(), state }
    }

    pub fn state(&self) -> Arc<Mutex<OverlayState>> { self.state.clone() }

    pub fn set_message(&self, phase: &str, message: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.phase = phase.to_owned();
            state.message = message.to_owned();
        }
    }

    pub fn report(&mut self, phase: &str, received: u64, length: Option<u64>, message: &str, force: bool) -> Result<String> {
        if let Ok(mut state) = self.state.lock() {
            state.phase = phase.to_owned();
            state.received = received;
            state.length = length;
            state.message = message.to_owned();
        }
        if !self.authenticated { return Ok(String::new()); }
        if !force && self.last.is_some_and(|last| last.elapsed() < Duration::from_millis(250)) { return Ok(self.directive.clone()); }
        self.last = Some(Instant::now());
        self.seq += 1;
        let request = json!({
            "version": 1,
            "command": "updateProgress",
            "value": {
                "jobId": self.job,
                "nonce": self.nonce,
                "seq": self.seq,
                "phase": phase,
                "receivedBytes": received,
                "totalBytes": length,
                "message": message,
            }
        });
        match win::pipe_request(&self.pipe, &request, Duration::from_secs(2)) {
            Ok(reply) => {
                if let Some(action) = reply.get("action").and_then(Value::as_str) {
                    self.directive = action.to_owned();
                }
                if self.directive == "cancel" && phase != "applying" && phase != "restarting" && phase != "cancelled" {
                    return Err(Error::cancelled(CANCELLED_MESSAGE));
                }
                Ok(self.directive.clone())
            }
            Err(_) if phase == "ready" => Err(Error("主程序已退出；未确认安装，已保留原版本。".into())),
            Err(_) => Ok(self.directive.clone()),
        }
    }
}

pub fn official_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') { return false; }
    let host = authority.split(':').next().unwrap_or("");
    host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("gitee.com")
}

pub fn verify(zip: &Path, expected: &str) -> Result<()> {
    let digest = Regex::new(r"^[0-9a-f]{64}$").unwrap();
    if !digest.is_match(expected) || lyric_archive::sha256_file(zip)? != expected {
        return Err(Error("下载校验失败，原程序保持不变".into()));
    }
    Ok(())
}

pub fn extract(zip: &Path, stage: &Path, version: &str) -> Result<PathBuf> {
    payload::extract_zip_file(zip, stage, &format!("LyricIsland-{version}"), payload::required_runtime(), 512 * 1024 * 1024)
}

pub fn manifest_digest(manifest: &str, version: &str) -> Option<String> {
    let name = format!("LyricIsland-{version}-Portable.zip");
    for line in manifest.lines() {
        let mut parts = line.split_whitespace();
        let digest = parts.next().unwrap_or("");
        let file = parts.next().unwrap_or("");
        if file == name && digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(digest.to_lowercase());
        }
    }
    None
}

pub fn domestic_pair(target: &Path, version: &str) -> Result<(String, String)> {
    let config = target.join("Assets/UpdateSources.json");
    if !config.is_file() { return Err(Error("未配置国内更新源".into())); }
    let settings: Value = paths::read_json(&config)?;
    let repository = settings.get("giteeRepository").and_then(Value::as_str).unwrap_or("").trim_end_matches('/');
    let pattern = Regex::new(r"^https://gitee\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)$").unwrap();
    let captures = pattern.captures(repository).ok_or_else(|| Error("国内更新源地址无效".into()))?;
    let api = format!("https://gitee.com/api/v5/repos/{}/{}", &captures[1], &captures[2]);
    let release: Value = serde_json::from_slice(&http::get_bytes(&format!("{api}/releases/tags/v{version}"), 2 * 1024 * 1024)?)?;
    let id = release["id"].as_u64().filter(|value| *value > 0).ok_or_else(|| Error("国内发行标识无效".into()))?;
    let attachments: Value = serde_json::from_slice(&http::get_bytes(&format!("{api}/releases/{id}/attach_files?per_page=100"), 4 * 1024 * 1024)?)?;
    let mut manifest = None;
    let mut zip = None;
    for item in attachments.as_array().into_iter().flatten() {
        let Some(file_id) = item["id"].as_u64().filter(|value| *value > 0) else { continue };
        let name = item["name"].as_str().or_else(|| item["file_name"].as_str()).unwrap_or("");
        let url = format!("{api}/releases/{id}/attach_files/{file_id}/download");
        if name == format!("LyricIsland-{version}-SHA256.txt") { manifest = Some(url.clone()); }
        if name == format!("LyricIsland-{version}-Portable.zip") { zip = Some(url); }
    }
    match (manifest, zip) {
        (Some(manifest), Some(zip)) => Ok((manifest, zip)),
        _ => Err(Error("国内源缺少更新附件".into())),
    }
}

pub fn download_bundle(target: &Path, version: &str, scratch: &Path, preferred: Option<(String, String)>, reporter: &mut Reporter) -> Result<()> {
    let name = format!("LyricIsland-{version}");
    let github = format!("https://github.com/Heiamer/LyricIsland/releases/download/v{version}/");
    let mut errors = Vec::new();
    let mut tried = HashSet::new();
    let mut pinned: Option<String> = None;
    for step in 0..3 {
        let pair = match step {
            0 => preferred.clone(),
            1 => match domestic_pair(target, version) {
                Ok(pair) => Some(pair),
                Err(error) => { errors.push(format!("Gitee：{error}")); continue; }
            },
            _ => Some((format!("{github}{name}-SHA256.txt"), format!("{github}{name}-Portable.zip"))),
        };
        let Some((manifest_url, zip_url)) = pair else { continue };
        if !tried.insert(manifest_url.clone()) { continue; }
        reporter.report(if step == 0 { "downloading" } else { "switching" }, 0, None, if step == 0 { "开始下载" } else { "切换到备用更新源" }, true)?;
        let attempt = (|| -> Result<()> {
            if !official_url(&manifest_url) || !official_url(&zip_url) { return Err(Error("更新下载地址无效".into())); }
            let zip_path = scratch.join("update.zip");
            let manifest = http::get_bytes(&manifest_url, 64 * 1024)?;
            let expected = manifest_digest(&String::from_utf8_lossy(&manifest), version).ok_or_else(|| Error("校验清单缺少更新包".into()))?;
            if pinned.as_ref().is_some_and(|pinned| pinned != &expected) { return Err(Error(PINNED_MISMATCH.into())); }
            pinned = Some(expected.clone());
            let (received, length) = http::download(&zip_url, &zip_path, 256 * 1024 * 1024, |received, length| {
                reporter.report("downloading", received, length, &format!("正在下载已选版本 {version}"), false).map(|_| ())
            })?;
            reporter.report("verifying", received, length, "正在校验下载内容", true)?;
            verify(&zip_path, &expected)?;
            Ok(())
        })();
        match attempt {
            Ok(()) => return Ok(()),
            Err(error) if error.is_cancelled() => return Err(error),
            Err(error) if error.0 == PINNED_MISMATCH => return Err(error),
            Err(error) => {
                errors.push(format!("更新源：{error}"));
                let _ = std::fs::write(scratch.join("download-errors.txt"), errors.join("\r\n"));
            }
        }
    }
    Err(Error(format!("国内源与 GitHub 均未能完成更新下载：{}", errors.join("；"))))
}

pub fn apply(stage: &Path, target: &Path, backup: &Path, fail_after: usize, after: Option<&mut dyn FnMut() -> Result<()>>) -> Result<()> {
    let mut changed: Vec<String> = Vec::new();
    let mut existed: HashSet<String> = HashSet::new();
    let mut after = after;
    let outcome = (|| -> Result<()> {
        let files = payload::relative_files(stage)?;
        for relative in files {
            let input = stage.join(relative.replace('/', "\\"));
            let destination = target.join(relative.replace('/', "\\"));
            let old = backup.join(relative.replace('/', "\\"));
            if destination.is_file() {
                if let Some(parent) = old.parent() { std::fs::create_dir_all(parent)?; }
                std::fs::copy(&destination, &old)?;
                existed.insert(relative.clone());
            }
            changed.push(relative.clone());
            if let Some(parent) = destination.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::copy(&input, &destination)?;
            if fail_after > 0 && changed.len() >= fail_after { return Err(Error("simulated write failure".into())); }
        }
        if let Some(after) = after.as_mut() { after()?; }
        Ok(())
    })();
    if let Err(error) = outcome {
        let mut rollback_errors = Vec::new();
        for relative in changed.iter().rev() {
            let destination = target.join(relative.replace('/', "\\"));
            let old = backup.join(relative.replace('/', "\\"));
            let outcome: Result<()> = if existed.contains(relative) {
                std::fs::copy(&old, &destination).map(|_| ()).map_err(Error::from)
            } else {
                paths::remove_file_if_present(&destination)
            };
            if let Err(rollback) = outcome { rollback_errors.push(rollback.to_string()); }
        }
        if !rollback_errors.is_empty() {
            return Err(Error(format!("更新回滚未完成，备份保留在：{}；{}", backup.display(), rollback_errors.join("；"))));
        }
        return Err(error);
    }
    Ok(())
}

pub fn stop_app(pipe: &str) -> Result<()> {
    let reply = win::pipe_request(pipe, &json!({"version": 1, "command": "prepareUpdateExit"}), Duration::from_secs(5))?;
    if reply.get("success").and_then(Value::as_bool) != Some(true) { return Err(Error("应用未确认保存设置，已取消更新".into())); }
    Ok(())
}

pub fn wait(pid: i32) -> Result<()> {
    if pid <= 0 { return Ok(()); }
    if !win::wait_process(pid as u32, Duration::from_secs(15)) { return Err(Error("应用尚未退出，更新已取消".into())); }
    Ok(())
}

pub fn start(target: &Path, data: &Path) -> Result<()> {
    let executable = target.join("LyricIsland.exe");
    let mut command = std::process::Command::new(&executable);
    if !data.as_os_str().is_empty() { command.arg("--data-dir").arg(data); }
    command.current_dir(target);
    command.spawn().map_err(|error| Error(format!("无法启动新版：{error}")))?;
    Ok(())
}

pub fn merge_marker(stage: &Path, target: &Path, version: &str) -> Result<()> {
    let marker_path = target.join(payload::INSTALLED_MARKER);
    if !marker_path.is_file() { return Ok(()); }
    let mut marker = payload::read_marker(target)?;
    let mut files: HashSet<String> = payload::marker_files(&marker).into_iter().collect();
    for file in payload::relative_files(stage)? { files.insert(file); }
    let mut files: Vec<String> = files.into_iter().collect();
    files.sort();
    marker["version"] = json!(version);
    marker["files"] = json!(files);
    paths::write_json(&stage.join(payload::INSTALLED_MARKER), &marker)
}

pub fn executable_hash(target: &Path) -> Result<String> { Ok(lyric_archive::sha256_file(&target.join("LyricIsland.exe"))?) }

pub fn self_test(report: &Path) -> Result<()> {
    let root = paths::temp_unique("LyricIsland.UpdateTest");
    std::fs::create_dir_all(&root)?;
    let outcome = (|| -> Result<()> {
        let target = root.join("target");
        let stage = root.join("stage");
        std::fs::create_dir_all(&target)?;
        std::fs::create_dir_all(&stage)?;
        std::fs::create_dir_all(target.join("Data"))?;
        std::fs::write(target.join("Data/settings.json"), "keep")?;
        std::fs::write(target.join("a"), "old")?;
        std::fs::write(stage.join("a"), "new")?;
        std::fs::write(stage.join("b"), "added")?;
        let failed = apply(&stage, &target, &root.join("backup"), 1, None).is_err();
        if !failed || std::fs::read_to_string(target.join("a"))? != "old" { return Err(Error("rollback failed".into())); }
        apply(&stage, &target, &root.join("backup2"), 0, None)?;
        if std::fs::read_to_string(target.join("a"))? != "new" || std::fs::read_to_string(target.join("Data/settings.json"))? != "keep" {
            return Err(Error("apply failed".into()));
        }
        let evil = root.join("evil.zip");
        let mut writer = ZipWriter::create(&evil)?;
        writer.add_bytes("LyricIsland-9.9/../escape", b"bad")?;
        writer.finish()?;
        if extract(&evil, &root.join("extract"), "9.9").is_ok() { return Err(Error("unsafe ZIP accepted".into())); }
        paths::write_json(report, &json!({"success": true, "rollback": true, "preservesData": true, "rejectsTraversal": true}))
    })();
    let _ = std::fs::remove_dir_all(&root);
    outcome
}

fn confirm_started(pipe: &str, version: &str, old_pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok(state) = win::pipe_request(pipe, &json!({"version": 1, "command": "state"}), Duration::from_secs(1)) {
            if state["version"].as_str() == Some(version) && state["processId"].as_u64() != Some(old_pid as u64) { return true; }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

pub fn run(args: Vec<String>) -> i32 {
    if args.len() == 2 && args[0] == "--self-test" {
        return match self_test(Path::new(&args[1])) { Ok(()) => 0, Err(error) => { eprintln!("{error}"); 1 } };
    }
    let mut overlaid: Option<Overlay> = None;
    let mut target: Option<PathBuf> = None;
    let mut data: Option<PathBuf> = None;
    let mut stopped = false;
    let mut scratch: Option<PathBuf> = None;
    let result = (|| -> Result<()> {
        if !matches!(args.len(), 6 | 8 | 10) { return Err(Error("更新参数无效".into())); }
        let version = args[0].clone();
        if !Regex::new(r"^\d+\.\d+(\.\d+)?(?:-?mini)?$").unwrap().is_match(&version) { return Err(Error("版本号无效".into())); }
        let target_path = paths::full_path(&args[1])?;
        let data_path = paths::full_path(&args[2])?;
        let pid: i32 = args[3].parse().map_err(|_| Error("更新参数无效".into()))?;
        let settings: i32 = args[4].parse().map_err(|_| Error("更新参数无效".into()))?;
        let pipe = args[5].clone();
        target = Some(target_path.clone());
        data = Some(data_path.clone());
        let (authenticated, job, nonce) = if args.len() == 10 { (true, args[8].clone(), args[9].clone()) } else { (false, String::new(), String::new()) };
        if !target_path.join("LyricIsland.exe").is_file() { return Err(Error("找不到原应用".into())); }
        let mutex_name = format!("Local.LyricIsland.OTA.{}", executable_hash(&target_path)?);
        let _gate = win::NamedMutex::acquire(&mutex_name)?.ok_or_else(|| Error("已有更新正在进行".into()))?;
        let work = paths::temp_unique("LyricIsland.Update");
        std::fs::create_dir_all(&work)?;
        scratch = Some(work.clone());
        let preferred = if args.len() >= 8 && !args[6].is_empty() && !args[7].is_empty() { Some((args[6].clone(), args[7].clone())) } else { None };
        let state = shared_state();
        let mut reporter = Reporter::new(authenticated, job, nonce, pipe.clone(), state.clone());
        download_bundle(&target_path, &version, &work, preferred, &mut reporter)?;
        let zip = work.join("update.zip");
        let stage = work.join("stage");
        extract(&zip, &stage, &version)?;
        merge_marker(&stage, &target_path, &version)?;

        if authenticated {
            reporter.report("ready", 0, None, "校验完成，等待你点击重启并安装", true)?;
            loop {
                if reporter.report("ready", 0, None, "校验完成，等待你点击重启并安装", true)? == "apply" { break; }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        reporter.report("applying", 0, None, "正在保存数据并安装", true)?;
        overlaid = Some(Overlay::show(state.clone())?);
        stop_app(&pipe)?;
        stopped = true;
        wait(pid)?;
        wait(settings)?;
        let confirmation_pipe = pipe.clone();
        let confirmation_version = version.clone();
        let mut after = || -> Result<()> {
            reporter.set_message("restarting", "正在启动并确认新版…");
            start(&target_path, &data_path)?;
            if !confirm_started(&confirmation_pipe, &confirmation_version, pid as u32) {
                let _ = stop_app(&confirmation_pipe);
                std::thread::sleep(Duration::from_millis(600));
                return Err(Error("新版未确认启动，准备回滚".into()));
            }
            Ok(())
        };
        apply(&stage, &target_path, &work.join("backup"), 0, Some(&mut after))?;
        stopped = false;
        reporter.set_message("completed", "新版已启动，设置已保留。");
        std::thread::sleep(Duration::from_millis(1200));
        if let Some(mut overlay) = overlaid.take() { overlay.close(); }
        std::fs::write(work.join("success.txt"), &version)?;
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) if error.is_cancelled() => {
            if let Some(mut overlay) = overlaid.take() { overlay.close(); }
            2
        }
        Err(error) => {
            if let Some(mut overlay) = overlaid.take() { overlay.close(); }
            let _ = report_failure(&args, &error);
            if stopped { if let (Some(target), Some(data)) = (target.as_ref(), data.as_ref()) { let _ = start(target, data); } }
            let details = scratch.map(|path| format!("\n诊断和备份：{}", path.display())).unwrap_or_default();
            win::message_box(&format!("在线更新未完成：{error}{details}"), "点屿在线更新", true);
            1
        }
    }
}

fn report_failure(args: &[String], error: &Error) -> Result<()> {
    if args.len() != 10 { return Ok(()); }
    let state = shared_state();
    let mut reporter = Reporter::new(true, args[8].clone(), args[9].clone(), args[5].clone(), state.clone());
    let _ = state.lock().map(|mut value| value.phase = "failed".into());
    reporter.report("failed", 0, None, &error.to_string(), true)?;
    Ok(())
}

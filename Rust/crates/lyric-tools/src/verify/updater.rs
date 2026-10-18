//! OTA helper integration: checksum, staged extraction, in-place apply and restart.

use crate::common::{check, strip_verbatim, unique, write_json, Result, ToolError};
use crate::process;
use lyric_bootstrap::updater;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn state(pipe: &str) -> Result<Value> { process::request(pipe, "state", None, Duration::from_secs(2)) }

fn wait_state(pipe: &str) -> Result<Value> {
    for _ in 0..50 {
        if let Ok(value) = state(pipe) { return Ok(value); }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(ToolError("应用管道不可用".into()))
}

fn wait_restarted(pipe: &str, old_pid: u32) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(value) = state(pipe) {
            if value["processId"].as_u64() != Some(old_pid as u64) { return Ok(value); }
        }
        if Instant::now() >= deadline { return Err(ToolError("新版未重新启动".into())); }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn kill(child: &mut Option<Child>) {
    if let Some(child) = child.as_mut() {
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let zip = strip_verbatim(PathBuf::from(args.first().ok_or_else(|| ToolError("缺少更新包".into()))?).canonicalize()?);
    let expected = args.get(1).cloned().ok_or_else(|| ToolError("缺少摘要".into()))?;
    let root = PathBuf::from(args.get(2).ok_or_else(|| ToolError("缺少工作目录".into()))?);
    std::fs::create_dir_all(&root)?;
    let root = strip_verbatim(root.canonicalize()?);
    let report_path = PathBuf::from(args.get(3).ok_or_else(|| ToolError("缺少报告路径".into()))?);
    let version = args.get(4).cloned().unwrap_or_else(|| "1.2.1".into());

    let target = root.join("app");
    let stage = root.join("stage");
    let data = target.join("Data");
    let backup = root.join("backup");
    let pipe = format!("LyricIsland.Settings.test.ota.{}", unique());
    let mut launched: Option<Child> = None;

    let outcome = (|| -> Result<()> {
        updater::verify(&zip, &expected)?;
        check(updater::verify(&zip, &"0".repeat(64)).is_err(), "错误摘要不应被接受")?;
        updater::extract(&zip, &target, &version)?;
        updater::extract(&zip, &stage, &version)?;
        std::fs::create_dir_all(&data)?;
        std::fs::write(data.join("settings.json"), "{\"CompactWidth\":317,\"MusicClickOpenApp\":true,\"NotificationsEnabled\":false,\"AllowedSources\":[\"LyricIsland.OTATest\"],\"OnlineSongInfo\":false}")?;

        let mut command = Command::new(target.join("LyricIsland.exe"));
        command.arg("--data-dir").arg(&data).current_dir(&target).env("LYRICISLAND_TEST_PIPE", &pipe);
        launched = Some(command.spawn()?);
        let initial = wait_state(&pipe)?;
        check(initial["settings"]["CompactWidth"].as_f64() == Some(317.0), format!("初始设置缺失：{initial}"))?;
        let old_pid = launched.as_ref().map(|child| child.id()).unwrap_or(0);

        updater::stop_app(&pipe)?;
        updater::wait(old_pid as i32)?;
        std::env::set_var("LYRICISLAND_TEST_PIPE", &pipe);
        let mut after = || updater::start(&target, &data);
        updater::apply(&stage, &target, &backup, 0, Some(&mut after))?;
        let current = wait_restarted(&pipe, old_pid)?;
        check(current["settings"]["CompactWidth"].as_f64() == Some(317.0), format!("重启后设置丢失：{current}"))?;
        check(current["settings"]["MusicClickOpenApp"] == true, format!("重启后设置损坏：{current}"))?;
        check(current["processId"].as_u64() != Some(old_pid as u64), "进程未重启")?;

        process::request(&pipe, "exit", None, Duration::from_secs(2))?;
        let report = json!({
            "success": true,
            "checksumFailureRejected": true,
            "originalDirectoryUpdated": true,
            "restarted": true,
            "preservedSettings": true
        });
        write_json(&report_path, &report)?;
        println!("{report}");
        Ok(())
    })();
    let _ = process::request(&pipe, "exit", None, Duration::from_secs(1));
    kill(&mut launched);
    let _ = std::env::remove_var("LYRICISLAND_TEST_PIPE");
    outcome
}

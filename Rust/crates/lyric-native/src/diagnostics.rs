//! One-click diagnostics: the last error, a filtered error summary and the tails of every
//! current log, written as a single text file so a report can be attached without a zip tool.
use std::path::{Path, PathBuf};

const FILES: usize = 8;
const TAIL: usize = 400;
const SUMMARY: usize = 200;

fn local_now() -> (String, String) {
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (
        format!("{:04}-{:02}-{:02}", time.wYear, time.wMonth, time.wDay),
        format!("{:02}:{:02}:{:02}", time.wHour, time.wMinute, time.wSecond),
    )
}

fn log_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("app_debug-") || name.starts_with("app_error-")) || !name.contains(".log") { return None }
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(FILES).map(|(_, path)| path).collect()
}

fn category(line: &str) -> &str {
    line.split("] [").nth(1).and_then(|rest| rest.split(']').next()).unwrap_or("")
}

/// Log lines carry a category, not a level, so an error is recognised either by its category
/// (`LYRIC_ERROR`, `NATIVE_RENDER`, ...) or by message wording that only failures produce.
fn looks_like_error(line: &str) -> bool {
    let category = category(line).to_ascii_uppercase();
    if category.contains("ERROR") || category.contains("FAIL") || category.contains("PANIC") { return true }
    let message = line.to_ascii_lowercase();
    ["error", "failed", "failure", "panic", "exception", "timed out", "timeout", "unavailable", "could not", "cannot ", "无法", "失败", "错误", "异常", "超时"]
        .iter()
        .any(|marker| message.contains(marker))
}

fn read_log(path: &Path) -> Option<String> {
    use std::io::{Read,Seek,SeekFrom};
    let mut file=std::fs::File::open(path).ok()?;let len=file.metadata().ok()?.len();let start=len.saturating_sub(128*1024);file.seek(SeekFrom::Start(start)).ok()?;let mut bytes=Vec::new();file.take(128*1024).read_to_end(&mut bytes).ok()?;
    let text=String::from_utf8_lossy(&bytes);let text=if start>0{text.split_once('\n').map(|(_,tail)|tail).unwrap_or("")}else{text.trim_start_matches('\u{feff}')};
    Some(text.lines().map(lyric_backend::io::redact).collect::<Vec<_>>().join("\n"))
}

fn write_bundle(directories: &[PathBuf], last_error: Option<&str>, version: &str) -> Result<PathBuf, String> {
    let logs = directories.first().ok_or("没有可用的日志目录")?;
    std::fs::create_dir_all(logs).map_err(|e| e.to_string())?;
    let (date, clock) = local_now();
    let stamp = format!("{}-{}", date.replace('-', ""), clock.replace(':', ""));
    let path = (0..100)
        .map(|i| logs.join(format!("diagnostics-{stamp}{}.txt", if i == 0 { String::new() } else { format!(".{i}") })))
        .find(|path| !path.exists())
        .ok_or("无法创建诊断文件")?;

    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for directory in directories {
        for file in log_files(directory) {
            if seen.insert(file.clone()) { files.push(file); }
        }
    }
    let mut out = String::new();
    out.push_str(&format!("DotIsle 诊断日志\n版本: {version}\n生成时间: {date} {clock}\n"));
    out.push_str(&format!("程序: {}\n", std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()));
    out.push_str(&format!("数据目录: {}\n", directories.first().and_then(|p| p.parent()).map(|p| p.display().to_string()).unwrap_or_default()));
    out.push_str(&format!("日志文件: {}\n", files.len()));
    out.push_str(&format!("系统: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("最近错误: {}\n\n", lyric_backend::io::redact(last_error.filter(|e| !e.is_empty()).unwrap_or("(无)"))));
    out.push_str("诊断策略: 每文件读取末128KiB；最近8份；错误摘要200行；连续相同事件15秒合并；每进程日志约18MiB上限。\n");
    if let Some(data)=logs.parent(){let settings=lyric_backend::io::read_settings(&data.join("settings.json"));let mut safe=serde_json::Map::new();for key in ["MusicExpandedLayout","Animation","AutoCompactWidth","AutoCheckUpdatesEnabled","OnlineSongInfo","SystemStateHintsEnabled","DeviceRemindersEnabled","CompactWidth","FontSize"]{safe.insert(key.into(),settings[key].clone());}out.push_str(&format!("安全配置摘要: {}\n",serde_json::Value::Object(safe)));}

    out.push_str("===== 错误摘要 =====\n");
    let mut hits = 0;
    for file in &files {
        let Some(text) = read_log(file) else { continue };
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        for line in text.lines().filter(|line| looks_like_error(line)) {
            if hits < SUMMARY { out.push_str(&format!("{name}: {line}\n")); }
            hits += 1;
        }
    }
    if hits == 0 { out.push_str("(当前日志里没有错误行)\n"); }
    else if hits > SUMMARY { out.push_str(&format!("……共 {hits} 行，仅保留前 {SUMMARY} 行\n")); }

    out.push_str("\n===== 日志尾部 =====\n");
    for file in &files {
        let Some(text) = read_log(file) else { continue };
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(TAIL);
        out.push_str(&format!("\n--- {}（{} 行，显示末 {} 行）---\n", file.file_name().unwrap_or_default().to_string_lossy(), lines.len(), lines.len() - start));
        for line in &lines[start..] { out.push_str(line); out.push('\n'); }
    }

    // A BOM keeps Notepad on UTF-8 when the bundle is opened by double-click.
    let mut old=std::fs::read_dir(logs).into_iter().flatten().flatten().filter(|e|e.file_name().to_string_lossy().starts_with("diagnostics-")&&e.file_name().to_string_lossy().ends_with(".txt")).collect::<Vec<_>>();
    old.sort_by_key(|e|std::cmp::Reverse(e.metadata().and_then(|m|m.modified()).ok()));
    for entry in old.into_iter().skip(3){let _=std::fs::remove_file(entry.path());}
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn collect(data: &Path, last_error: Option<&str>, version: &str) -> Result<PathBuf, String> {
    let mut directories = vec![data.join("Logs")];
    // The settings window logs to the roaming data directory, which differs from the
    // portable data directory that hosts the island.
    let roaming = lyric_backend::io::data_directory().join("Logs");
    if roaming != directories[0] { directories.push(roaming); }
    write_bundle(&directories, last_error, version)
}

pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_summarises_errors_and_keeps_the_log_tail() {
        let dir = std::env::temp_dir().join(format!("dotisle-diagnostics-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Logs")).unwrap();
        let mut body = String::from("\u{feff}");
        for i in 0..600 { body.push_str(&format!("[2026-10-17 10:00:{:02}.000] [SMTC] line {i}\n", i % 60)); }
        body.push_str("[2026-10-17 10:01:00.000] [LYRIC_ERROR] request failed: timeout\n");
        std::fs::write(dir.join("Logs").join("app_debug-2026-10-17.log"), body).unwrap();

        let path = write_bundle(&[dir.join("Logs")], Some("render failed"), "test-version").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("test-version"));
        assert!(text.contains("render failed"));
        assert!(text.contains("[LYRIC_ERROR]"));
        assert!(text.contains("line 599"), "the tail must keep the newest lines");
        assert!(!text.contains("line 0\n"), "the tail must not carry the whole file");
        assert!(text.contains("日志尾部"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

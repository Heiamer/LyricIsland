use crate::error::{Error, Result};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::System::SystemInformation::GetLocalTime;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn full_path(input: impl AsRef<Path>) -> Result<PathBuf> {
    let input = input.as_ref();
    let absolute = if input.is_absolute() { input.to_path_buf() } else { std::env::current_dir()?.join(input) };
    let mut output = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(Path::new("\\")),
            Component::CurDir => {}
            Component::ParentDir => { output.pop(); }
            Component::Normal(part) => output.push(part),
        }
    }
    Ok(output)
}

pub fn normalized(path: impl AsRef<Path>) -> Result<String> {
    let text = full_path(path)?.to_string_lossy().replace('/', "\\");
    Ok(text.trim_end_matches('\\').to_lowercase())
}

pub fn same_path(left: impl AsRef<Path>, right: impl AsRef<Path>) -> bool {
    match (normalized(left), normalized(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub fn starts_with_path(path: impl AsRef<Path>, root: impl AsRef<Path>) -> bool {
    match (normalized(path), normalized(root)) {
        (Ok(path), Ok(root)) => path == root || path.starts_with(&(root + "\\")),
        _ => false,
    }
}

pub fn quote(value: &str) -> String { format!("\"{}\"", value.replace('"', "")) }

pub fn unique_suffix() -> String {
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}.{}.{}", std::process::id(), millis, count)
}

pub fn temp_unique(prefix: &str) -> PathBuf { std::env::temp_dir().join(format!("{prefix}.{}", unique_suffix())) }

pub fn user_profile() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Users\\Public"))
}

pub fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| user_profile().join("AppData/Local"))
}

pub fn app_data() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| user_profile().join("AppData/Roaming"))
}

pub fn windows_dir() -> PathBuf {
    std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

pub fn program_files() -> PathBuf {
    std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Program Files"))
}

pub fn programs_menu() -> PathBuf { app_data().join("Microsoft/Windows/Start Menu/Programs") }

pub fn protected_folders() -> Vec<PathBuf> {
    vec![
        user_profile(),
        app_data(),
        local_app_data(),
        user_profile().join("Documents"),
        user_profile().join("Desktop"),
        windows_dir(),
        program_files(),
    ]
}

pub fn owned_data_default() -> PathBuf { app_data().join("DotIsle") }

pub fn local_stamp(compact: bool) -> String {
    let time = unsafe { GetLocalTime() };
    if compact {
        format!("{:04}{:02}{:02}", time.wYear, time.wMonth, time.wDay)
    } else {
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond)
    }
}

pub fn read_json(path: &Path) -> Result<Value> { Ok(serde_json::from_slice(&std::fs::read(path)?)?) }

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}

/// Rejects rooted paths, alternate data streams and the mutable data folders. The caller
/// still checks containment after joining, because `..` can climb out of any prefix.
pub fn safe_child(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains(':') { return Err(Error("安装清单路径无效".into())); }
    let candidate_text = relative.replace('/', "\\");
    let candidate = Path::new(&candidate_text);
    if candidate.is_absolute() { return Err(Error("安装清单路径无效".into())); }
    let first = relative.split(['/', '\\']).next().unwrap_or("");
    if first.eq_ignore_ascii_case("Data") || first.eq_ignore_ascii_case("Logs") { return Err(Error("安装清单路径无效".into())); }
    let joined = full_path(root.join(candidate))?;
    let normal_root = full_path(root)?;
    if !starts_with_path(&joined, &normal_root) { return Err(Error("安装路径越界".into())); }
    Ok(joined)
}

pub fn has_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(path).map(|meta| meta.file_attributes() & 0x400 != 0).unwrap_or(false)
}

pub fn normalize_target(input: &str) -> Result<PathBuf> {
    let text = input.trim();
    let target = full_path(text)?;
    let text = target.to_string_lossy().trim_end_matches('\\').to_owned();
    if text.len() <= 3 { return Err(Error("请选择独立的应用文件夹".into())); }
    if starts_with_path(&target, windows_dir()) { return Err(Error("请选择独立的应用文件夹".into())); }
    if target.exists() {
        let empty = std::fs::read_dir(&target)?.next().is_none();
        if !empty && !target.join("Installed.json").is_file() {
            return Err(Error("所选文件夹不为空，请选择空文件夹或点屿原安装目录。".into()));
        }
    }
    Ok(PathBuf::from(text))
}

pub fn ensure_dir(path: &Path) -> Result<()> { std::fs::create_dir_all(path)?; Ok(()) }

pub fn remove_file_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn os_string(path: &Path) -> OsString { path.as_os_str().to_owned() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_rejects_traversal_and_mutable_folders() {
        let root = std::env::temp_dir().join("dotisle-safe-child");
        assert!(safe_child(&root, "../outside.txt").is_err());
        assert!(safe_child(&root, "Data/settings.json").is_err());
        assert!(safe_child(&root, "Logs/a.txt").is_err());
        assert!(safe_child(&root, "C:\\Windows\\evil").is_err());
        assert!(safe_child(&root, "Assets/icon.bin").is_ok());
        assert!(safe_child(&root, "a/../b").is_ok());
    }

    #[test]
    fn path_comparisons_ignore_case_and_separators() {
        assert!(same_path("C:/Temp/A", "c:\\temp\\a"));
        assert!(starts_with_path("C:\\Temp\\A\\b", "c:/temp/a"));
        assert!(!starts_with_path("C:\\Temp\\AB", "C:/Temp/A"));
        assert_eq!(quote("a\" b"), "\"a b\"");
    }
}

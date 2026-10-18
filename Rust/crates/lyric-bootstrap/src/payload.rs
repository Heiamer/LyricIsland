use crate::error::{Error, Result};
use crate::paths::{self, safe_child};
use crate::win;
use lyric_archive::{sha256_bytes, ZipReader};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PAYLOAD_RESOURCE: &str = "DotIsle.InstallPayload.zip";
pub const PAYLOAD_DIGEST_RESOURCE: &str = "DotIsle.InstallPayload.sha256";
pub const LAUNCHER_PAYLOAD_RESOURCE: &str = "LyricIsland.Payload.zip";
pub const INSTALLED_MARKER: &str = "Installed.json";

pub fn required_runtime() -> &'static [&'static str] {
    &["LyricIsland.exe", "LyricIsland.Settings.exe", "LyricIsland.Updater.exe", "Uninstall.exe", "e_sqlite3.dll", "Assets/Fonts/NotoSansSC.ttf", "Assets/Fonts/NotoSansJP.ttf"]
}

pub fn read_marker(target: &Path) -> Result<Value> { paths::read_json(&target.join(INSTALLED_MARKER)) }

pub fn marker_product(marker: &Value) -> &str { marker["product"].as_str().unwrap_or("") }

pub fn marker_version(marker: &Value) -> &str { marker["version"].as_str().unwrap_or("") }

pub fn marker_data(marker: &Value) -> Option<PathBuf> { marker["dataDirectory"].as_str().filter(|text| !text.is_empty()).map(PathBuf::from) }

pub fn marker_files(marker: &Value) -> Vec<String> {
    marker["files"].as_array().map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

pub fn write_marker(stage: &Path, data: &Path, files: &[String]) -> Result<()> {
    paths::write_json(&stage.join(INSTALLED_MARKER), &json!({
        "product": "DotIsle",
        "version": VERSION,
        "dataDirectory": data.to_string_lossy(),
        "files": files,
    }))
}

pub fn relative_files(root: &Path) -> Result<Vec<String>> {
    let mut output = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() { stack.push(path); }
            else if entry.file_type()?.is_file() {
                let relative = path.strip_prefix(root).map_err(|_| Error("安装文件路径无效".into()))?;
                output.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    output.sort();
    Ok(output)
}

pub fn extract_payload(payload: &[u8], directory: &Path) -> Result<PathBuf> {
    let scratch = paths::temp_unique("DotIsle.Payload");
    std::fs::create_dir_all(&scratch)?;
    let zip_path = scratch.join("payload.zip");
    std::fs::write(&zip_path, payload)?;
    let result = extract_zip_file(&zip_path, directory, &format!("LyricIsland-{VERSION}"), required_runtime(), 512 * 1024 * 1024);
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

pub fn extract_zip_file(zip_path: &Path, stage: &Path, folder: &str, required: &[&str], limit: u64) -> Result<PathBuf> {
    let mut reader = ZipReader::open(zip_path)?;
    extract_reader(&mut reader, stage, folder, required, limit)?;
    Ok(stage.to_path_buf())
}

pub fn extract_reader(reader: &mut ZipReader, stage: &Path, folder: &str, required: &[&str], limit: u64) -> Result<()> {
    let prefix = format!("{folder}/");
    std::fs::create_dir_all(stage)?;
    let mut total = 0u64;
    let mut seen = HashSet::new();
    for index in 0..reader.entries().len() {
        let entry = reader.entries()[index].clone();
        if !entry.name.starts_with(&prefix) { return Err(Error("更新包目录无效".into())); }
        let relative = &entry.name[prefix.len()..];
        if relative.is_empty() { continue; }
        let path = safe_child(stage, relative)?;
        let key = path.to_string_lossy().to_lowercase();
        if !seen.insert(key) { return Err(Error("更新包包含重复路径".into())); }
        if entry.is_dir { std::fs::create_dir_all(&path)?; continue; }
        total = total.saturating_add(entry.size);
        if total > limit { return Err(Error("更新包超过大小限制".into())); }
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let data = reader.read(index)?;
        std::fs::write(&path, data)?;
    }
    for name in required {
        if !stage.join(name).is_file() { return Err(Error(format!("更新包缺少必要文件：{name}"))); }
    }
    Ok(())
}

pub fn launcher_payload() -> Result<Vec<u8>> {
    win::resource(LAUNCHER_PAYLOAD_RESOURCE).map_err(|_| Error("启动组件缺失。".into()))
}

pub fn install_payload() -> Result<Vec<u8>> {
    win::resource(PAYLOAD_RESOURCE).map_err(|_| Error("安装组件缺失".into()))
}

pub fn verified_install_payload() -> Result<Vec<u8>> {
    let bytes = install_payload()?;
    let digest = win::resource(PAYLOAD_DIGEST_RESOURCE).map_err(|_| Error("安装组件校验失败".into()))?;
    let expected = String::from_utf8_lossy(&digest).trim().to_ascii_lowercase();
    if expected.is_empty() || sha256_bytes(&bytes) != expected { return Err(Error("安装组件校验失败".into())); }
    Ok(bytes)
}

pub fn write_verification(directory: &Path, executable: &Path, full: bool) -> Result<()> {
    let value = if full {
        json!({
            "success": true,
            "version": VERSION,
            "executable": executable.to_string_lossy(),
            "gui": "Direct2D",
            "settingsUi": "GPUI",
            "backend": "Rust",
        })
    } else {
        json!({"success": true, "version": VERSION, "installer": true})
    };
    paths::write_json(&directory.join("verification.json"), &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyric_archive::ZipWriter;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("dotisle-payload-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn extraction_rejects_traversal_and_folder_escapes() {
        let root = scratch("traversal");
        let zip = root.join("evil.zip");
        let mut writer = ZipWriter::create(&zip).unwrap();
        writer.add_bytes("LyricIsland-9.9/../escape", b"bad").unwrap();
        writer.finish().unwrap();
        let outcome = extract_zip_file(&zip, &root.join("stage"), "LyricIsland-9.9", &[], 1024);
        assert!(outcome.is_err());
        assert!(!root.join("escape").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extraction_requires_declared_runtime_files() {
        let root = scratch("required");
        let zip = root.join("bundle.zip");
        let mut writer = ZipWriter::create(&zip).unwrap();
        writer.add_bytes(&format!("LyricIsland-{VERSION}/LyricIsland.exe"), b"x").unwrap();
        writer.finish().unwrap();
        let outcome = extract_zip_file(&zip, &root.join("stage"), &format!("LyricIsland-{VERSION}"), required_runtime(), 4096);
        assert!(outcome.unwrap_err().to_string().contains("缺少必要文件"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

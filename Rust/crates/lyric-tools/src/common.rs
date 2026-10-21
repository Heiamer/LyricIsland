//! Shared helpers for the release pipeline and its integration checks.

use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ToolError(pub String);

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for ToolError {}
impl From<std::io::Error> for ToolError {
    fn from(value: std::io::Error) -> Self { ToolError(value.to_string()) }
}
impl From<serde_json::Error> for ToolError {
    fn from(value: serde_json::Error) -> Self { ToolError(value.to_string()) }
}
impl From<lyric_archive::Error> for ToolError {
    fn from(value: lyric_archive::Error) -> Self { ToolError(value.0) }
}
impl From<lyric_bootstrap::error::Error> for ToolError {
    fn from(value: lyric_bootstrap::error::Error) -> Self { ToolError(value.0) }
}
#[allow(dead_code)]
impl From<windows::core::Error> for ToolError {
    fn from(value: windows::core::Error) -> Self { ToolError(value.message().to_string()) }
}

pub type Result<T> = std::result::Result<T, ToolError>;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::common::ToolError(format!($($arg)*))) };
}

pub fn check(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition { Ok(()) } else { Err(ToolError(message.into())) }
}

/// Windows canonical paths carry a `\\?\` prefix that breaks tools mixing `/`
/// and `\` separators, so strip it before handing paths to cargo or FXC.
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") { return PathBuf::from(format!(r"\\{rest}")); }
    if let Some(rest) = text.strip_prefix(r"\\?\") { return PathBuf::from(rest); }
    path
}

pub fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().map(strip_verbatim).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub fn default_version() -> &'static str { "2.1.6mini" }

pub fn unique() -> String { lyric_bootstrap::paths::unique_suffix() }

pub fn read_json(path: &Path) -> Result<Value> { Ok(serde_json::from_slice(&std::fs::read(path)?)?) }

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

pub fn sha256(path: &Path) -> Result<String> { Ok(lyric_archive::sha256_file(path)?) }

pub fn sha256_bytes(bytes: &[u8]) -> String { lyric_archive::sha256_bytes(bytes) }

pub fn copy_file(source: &Path, target: &Path) -> Result<()> {
    if !source.is_file() { bail!("缺少文件：{}", source.display()); }
    if let Some(parent) = target.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::copy(source, target)?;
    Ok(())
}

pub fn copy_tree_files(source: &Path, target_root: &Path) -> Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    let mut stack = vec![source.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if entry.file_type()?.is_file() {
                let relative = path.strip_prefix(source).map_err(|_| ToolError("路径无效".into()))?;
                copy_file(&path, &target_root.join(relative))?;
                output.push(relative.to_path_buf());
            }
        }
    }
    output.sort();
    Ok(output)
}

pub fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned()
}

pub fn default_sqlite() -> PathBuf {
    lyric_bootstrap::paths::user_profile().join(".nuget/packages/sqlitepclraw.lib.e_sqlite3/2.1.6/runtimes/win-x64/native/e_sqlite3.dll")
}

pub fn temp_dir(prefix: &str) -> PathBuf { lyric_bootstrap::paths::temp_unique(prefix) }

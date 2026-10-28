//! Single-file launcher: unpacks the embedded runtime, keeps it fresh and starts the app.

use crate::error::{Error, Result};
use crate::paths;
use crate::payload;
use crate::win;
use std::path::{Path, PathBuf};
use std::time::Duration;

const COMPLETE_FILES: &[&str] = &[
    "LyricIsland.exe",
    "LyricIsland.Settings.exe",
    "e_sqlite3.dll",
    "Assets/Fonts/GoogleSansFlex-Regular.ttf",
    "Assets/Fonts/GoogleSansFlex-Medium.ttf",
    "Assets/Fonts/GoogleSansFlex-SemiBold.ttf",
    "Assets/Fonts/GoogleSansFlex-Bold.ttf",
    "Assets/Fonts/NotoSansSC.ttf",
    "Assets/Fonts/NotoSansJP.ttf",
];

const PREVIOUS_VERSIONS: &[&str] = &["1.2.1", "1.2", "1.1", "1.0"];

fn folder() -> String { format!("LyricIsland-{}", payload::VERSION) }

fn runtime_root() -> PathBuf { paths::local_app_data().join("LyricIsland/Runtime").join(payload::VERSION) }

fn complete(root: &Path) -> bool {
    let folder = folder();
    COMPLETE_FILES.iter().all(|name| root.join(&folder).join(name).is_file())
}

fn extract(root: &Path, bytes: &[u8]) -> Result<()> {
    let folder = folder();
    let scratch = paths::temp_unique("DotIsle.Launcher");
    std::fs::create_dir_all(&scratch)?;
    let zip = scratch.join("payload.zip");
    std::fs::write(&zip, bytes)?;
    let outcome = payload::extract_zip_file(&zip, &root.join(&folder), &folder, COMPLETE_FILES, 512 * 1024 * 1024);
    let _ = std::fs::remove_dir_all(&scratch);
    outcome?;
    Ok(())
}

fn migrate_previous_data(root: &Path) -> Result<()> {
    let destination = root.join(folder()).join("Data");
    if destination.join("settings.json").is_file() { return Ok(()); }
    let runtime = paths::local_app_data().join("LyricIsland/Runtime");
    let previous = PREVIOUS_VERSIONS.iter().map(|version| runtime.join(version).join(format!("LyricIsland-{version}")).join("Data"))
        .find(|candidate| candidate.join("settings.json").is_file());
    let Some(previous) = previous else { return Ok(()); };
    std::fs::create_dir_all(&destination)?;
    std::fs::copy(previous.join("settings.json"), destination.join("settings.json"))?;
    let maps = previous.join("database");
    if maps.is_dir() {
        let target = destination.join("database");
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(&maps)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() { continue; }
            let output = target.join(entry.file_name());
            if !output.exists() { std::fs::copy(entry.path(), &output)?; }
        }
    }
    Ok(())
}

/// Cheap launcher identity used instead of hashing the whole embedded payload.
///
/// The runtime root is already versioned, so "same version, same launcher file,
/// complete runtime" is enough to detect a changed build; `complete()` still
/// verifies the extracted files exist. The old check read the ~70 MB embedded
/// payload and SHA-256'd it on every single launch, which was pure cold-start
/// cost on the login critical path.
fn launcher_identity() -> Result<String> {
    let exe = std::env::current_exe()?;
    let metadata = std::fs::metadata(&exe)?;
    let modified = metadata.modified().map(|t| format!("{t:?}")).unwrap_or_default();
    Ok(format!("{}:{}:{}", payload::VERSION, metadata.len(), modified))
}

fn launch(args: &[String]) -> Result<i32> {
    let verify = args.len() == 2 && args[0] == "--verify-package";
    let root = if verify { paths::full_path(&args[1])? } else { runtime_root() };
    let _gate = win::NamedMutex::acquire_timeout(&format!("Local.LyricIsland.Bundle.{}", payload::VERSION), Duration::from_secs(60))?
        .ok_or_else(|| Error("另一份程序正在准备组件，请稍后重试。".into()))?;
    let folder = folder();
    let executable = root.join(&folder).join("LyricIsland.exe");
    let marker = root.join("ready.hash");
    let identity = launcher_identity()?;
    let fresh = marker.is_file() && std::fs::read_to_string(&marker).map(|text| text.trim().to_owned()).ok().as_deref() == Some(identity.as_str());
    if !fresh || !complete(&root) {
        let bytes = payload::launcher_payload()?;
        std::fs::create_dir_all(&root)?;
        extract(&root, &bytes)?;
        if !complete(&root) { return Err(Error("组件释放不完整，请重新下载后运行。".into())); }
        std::fs::write(&marker, &identity)?;
    }
    if verify {
        payload::write_verification(&root, &executable, true)?;
        return Ok(0);
    }
    migrate_previous_data(&root)?;
    // Login startup registers this launcher, not the extracted runtime, whose
    // path is versioned and replaced on the next update.
    let launcher = std::env::current_exe()?;
    let _ = std::fs::write(root.join(&folder).join("launcher.txt"), launcher.to_string_lossy().as_bytes());
    let mut command = std::process::Command::new(&executable);
    if let Some(parent) = executable.parent() { command.current_dir(parent); }
    command.spawn().map_err(|error| Error(format!("无法启动点屿：{error}")))?;
    Ok(0)
}

pub fn run(args: Vec<String>) -> i32 {
    match launch(&args) {
        Ok(code) => code,
        Err(error) => {
            win::message_box(&format!("无法启动点屿：{error}"), "DotIsle", true);
            1
        }
    }
}

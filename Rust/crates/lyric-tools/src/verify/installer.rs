//! Installer lifecycle against a throwaway registry key and unmanaged files.

use crate::common::{check, unique, write_json, Result, ToolError};
use lyric_bootstrap::{installer, registry};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn fixture(root: &Path, name: &str) -> Result<(PathBuf, PathBuf)> {
    let app = root.join(name).join("App");
    let data = root.join(name).join("UserData");
    std::fs::create_dir_all(app.join("Assets"))?;
    std::fs::create_dir_all(data.join("Logs"))?;
    std::fs::write(app.join("LyricIsland.exe"), "binary")?;
    std::fs::write(app.join("Uninstall.exe"), "uninstaller")?;
    std::fs::write(app.join("Assets/icon.bin"), "asset")?;
    std::fs::write(data.join("settings.json"), "keep setting")?;
    std::fs::write(data.join("Logs/log.txt"), "log")?;
    std::fs::write(data.join(".dotisle-data.json"), serde_json::to_vec(&json!({"product": "DotIsle", "installation": app.to_string_lossy()}))?)?;
    std::fs::write(app.join("Installed.json"), serde_json::to_vec(&json!({
        "product": "DotIsle",
        "version": "2.0.2",
        "dataDirectory": data.to_string_lossy(),
        "files": ["LyricIsland.exe", "Uninstall.exe", "Assets/icon.bin", "Installed.json"]
    }))?)?;
    Ok((app, data))
}

fn registry_values(key: &registry::Key, app: &Path) -> Result<()> {
    check(key.string("DisplayName").as_deref() == Some("点屿 DotIsle"), "缺少应用显示名")?;
    check(key.u32("SystemComponent") == Some(0) && key.u32("NoRemove") == Some(0), "应用登记不应隐藏或不可卸载")?;
    let uninstall = key.string("UninstallString").ok_or_else(|| ToolError("缺少卸载命令".into()))?;
    check(uninstall.contains(&format!("--install-dir \"{}\"", app.display())), format!("卸载目标未加引号：{uninstall}"))?;
    let quiet = key.string("QuietUninstallString").ok_or_else(|| ToolError("缺少静默卸载命令".into()))?;
    check(quiet.contains("--keep-data"), format!("静默卸载应保留数据：{quiet}"))?;
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let root = PathBuf::from(args.first().ok_or_else(|| ToolError("缺少工作目录".into()))?);
    let report_path = PathBuf::from(args.get(1).ok_or_else(|| ToolError("缺少报告路径".into()))?);
    std::fs::create_dir_all(&root)?;
    let path = format!(r"Software\DotIsle\InstallerTests\{}", unique());
    let outcome = (|| -> Result<Value> {
        let (app, data) = fixture(&root, "Keep")?;
        let key = registry::Key::create(&path)?;
        installer::write_registration(&key, &app)?;
        registry_values(&key, &app)?;

        installer::remove_owned_files(&app)?;
        check(!app.exists() && data.join("settings.json").is_file(), "保留数据的卸载不应删除用户数据")?;

        let (app, data) = fixture(&root, "Remove")?;
        std::fs::write(app.join("user-note.txt"), "unmanaged")?;
        installer::delete_owned_data(&data, &app)?;
        installer::remove_owned_files(&app)?;
        check(!data.exists() && app.join("user-note.txt").is_file(), "完整清理影响了未托管文件")?;

        let (app, _) = fixture(&root, "Traversal")?;
        let sentinel = root.join("Traversal/outside.txt");
        std::fs::write(&sentinel, "outside")?;
        std::fs::write(app.join("Installed.json"), serde_json::to_vec(&json!({
            "product": "DotIsle",
            "version": "2.0.2",
            "files": ["LyricIsland.exe", "../outside.txt", "Installed.json"]
        }))?)?;
        check(installer::remove_owned_files(&app).is_err() && sentinel.is_file() && app.join("LyricIsland.exe").is_file(), "越界清单删除前未被拒绝")?;

        let foreign = root.join("Foreign");
        std::fs::create_dir_all(&foreign)?;
        std::fs::write(foreign.join("keep.txt"), "personal")?;
        check(installer::delete_owned_data(&foreign, &app).is_err() && foreign.join("keep.txt").is_file(), "非点屿数据目录被删除")?;

        check(installer::defaults_keep_user_data(), "卸载向导默认应保留用户数据")?;
        Ok(json!({
            "success": true,
            "registryValuesVerified": true,
            "uninstallDirectoryQuoted": true,
            "keepData": true,
            "removeData": true,
            "unmanagedFilesPreserved": true,
            "unsafeManifestRejectedBeforeDeletion": true,
            "foreignDataProtected": true,
            "retainDataCheckboxDefault": true,
            "realInstallationRegistrationChanged": false
        }))
    })();
    let _ = registry::delete_tree(&path);
    let report = outcome?;
    write_json(&report_path, &report)?;
    println!("{report}");
    Ok(())
}

//! Reproduces the release layout: stage, portable ZIP, setup payload, OTA bundle and source ZIP.

use crate::bail;
use crate::common::{check, copy_file, default_sqlite, default_version, sha256, sha256_bytes, source_root, strip_verbatim, Result};
use crate::icon::{embed_icon, replace_resource};
use lyric_archive::ZipWriter;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FONT_SHA256: &str = "a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da";
const JAPANESE_SHA256: &str = "c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f";

fn zip_entry(relative: &Path) -> String { relative.to_string_lossy().replace('\\', "/") }

fn write_zip(zip_path: &Path, files: &BTreeSet<String>, stage: &Path, prefix: &str, extras: &[(&str, Vec<u8>)]) -> Result<()> {
    let _ = std::fs::remove_file(zip_path);
    let mut writer = ZipWriter::create(zip_path)?;
    for relative in files {
        writer.add_file(&format!("{prefix}{relative}"), &stage.join(relative.replace('/', "\\")))?;
    }
    for (name, bytes) in extras { writer.add_bytes(&format!("{prefix}{name}"), bytes)?; }
    writer.finish()?;
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let root = source_root();
    let version = crate::common::arg_value(args, "--version").unwrap_or_else(|| default_version().to_owned());
    let binary_dir = crate::common::arg_value(args, "--binary-dir").map(PathBuf::from).unwrap_or_else(|| root.join("Rust/target-native/release"));
    let output = crate::common::arg_value(args, "--output").map(PathBuf::from).unwrap_or_else(|| root.join("publish/outputs"));
    let sqlite = crate::common::arg_value(args, "--sqlite").map(PathBuf::from).unwrap_or_else(default_sqlite);
    let name = format!("DotIsle-{version}");
    let bundle = format!("LyricIsland-{version}");
    let out = output.canonicalize().map(strip_verbatim).unwrap_or_else(|_| output.clone());
    std::fs::create_dir_all(&out)?;
    let stage = root.join("publish").join(&name);
    if stage.exists() { std::fs::remove_dir_all(&stage)?; }
    std::fs::create_dir_all(&stage)?;
    let mut files = BTreeSet::new();

    copy_file(&binary_dir.join("LyricIsland.exe"), &stage.join("LyricIsland.exe"))?;
    files.insert("LyricIsland.exe".into());
    copy_file(&binary_dir.join("lyric-island-settings.exe"), &stage.join("LyricIsland.Settings.exe"))?;
    files.insert("LyricIsland.Settings.exe".into());
    let icon = root.join("Assets/LyricIsland-v2.ico");
    embed_icon(&stage.join("LyricIsland.Settings.exe"), &icon)?;
    embed_icon(&stage.join("LyricIsland.exe"), &icon)?;

    copy_file(&binary_dir.join("lyric-updater.exe"), &stage.join("LyricIsland.Updater.exe"))?;
    files.insert("LyricIsland.Updater.exe".into());
    copy_file(&sqlite, &stage.join("e_sqlite3.dll"))?;
    files.insert("e_sqlite3.dll".into());

    let font = root.join("Assets/Fonts/NotoSansSC.ttf");
    check(sha256(&font)? == FONT_SHA256, "完整字体与已验证来源不一致")?;
    copy_file(&font, &stage.join("Assets/Fonts/NotoSansSC.ttf"))?;
    let japanese = root.join("Assets/Fonts/NotoSansJP.ttf");
    check(sha256(&japanese)? == JAPANESE_SHA256, "日文字体与已验证来源不一致")?;
    copy_file(&japanese, &stage.join("Assets/Fonts/NotoSansJP.ttf"))?;
    files.insert("Assets/Fonts/NotoSansSC.ttf".into());
    files.insert("Assets/Fonts/NotoSansJP.ttf".into());

    for asset in ["LyricIsland-v2.ico", "LyricIsland-v2.png", "LyricIsland-v3.svg", "UpdateSources.json"] {
        copy_file(&root.join("Assets").join(asset), &stage.join("Assets").join(asset))?;
        files.insert(format!("Assets/{asset}"));
    }
    for doc in ["NATIVE-README.md", "CHANGELOG.md", "LICENSE", "FONT-NOTICE.md", "THIRD-PARTY-NOTICES.md"] {
        copy_file(&root.join(doc), &stage.join(doc))?;
        files.insert(doc.to_owned());
    }
    copy_file(&root.join("NATIVE-README.md"), &stage.join("README.md"))?;
    files.insert("README.md".into());
    let licenses = root.join("licenses");
    if licenses.is_dir() {
        for relative in crate::common::copy_tree_files(&licenses, &stage.join("licenses"))? {
            files.insert(format!("licenses/{}", zip_entry(&relative)));
        }
    }
    let device_icons = root.join("Assets/DeviceIcons");
    if device_icons.is_dir() {
        for entry in std::fs::read_dir(&device_icons)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() { continue; }
            let relative = format!("Assets/DeviceIcons/{}", entry.file_name().to_string_lossy());
            copy_file(&entry.path(), &stage.join(&relative))?;
            files.insert(relative);
        }
    }

    let portable = out.join(format!("{name}-Portable.zip"));
    let portable_note = "点屿便携版\r\n完整解压到可写目录，运行 LyricIsland.exe。数据保存在旁边的 Data。\r\n保留 Portable.json，移动时携带整个目录。不会跳转安装版或弹出安装迁移向导。\r\n一次运行一份点屿；如旧版已运行，请从托盘退出后再运行此版本。\r\n";
    write_zip(&portable, &files, &stage, &format!("{name}/"), &[
        ("Portable.json", serde_json::to_vec(&json!({"portable": true, "version": version}))?),
        ("便携版说明.txt", portable_note.as_bytes().to_vec()),
    ])?;

    copy_file(&binary_dir.join("lyric-uninstall.exe"), &stage.join("Uninstall.exe"))?;
    files.insert("Uninstall.exe".into());
    let payload_path = root.join("publish/InstallPayload.zip");
    write_zip(&payload_path, &files, &stage, &format!("{bundle}/"), &[])?;
    let payload_bytes = std::fs::read(&payload_path)?;
    let payload_digest = sha256_bytes(&payload_bytes);

    let share = root.join("publish/DotIsle.Engine.exe");
    copy_file(&binary_dir.join("lyric-setup.exe"), &share)?;
    embed_icon(&share, &icon)?;
    replace_resource(&share, "DotIsle.InstallPayload.zip", &payload_bytes)?;
    replace_resource(&share, "DotIsle.InstallPayload.sha256", payload_digest.as_bytes())?;
    let engine=share;
    let share=out.join(format!("{name}-Setup.exe"));
    let iscc=crate::common::arg_value(args,"--iscc").or_else(||std::env::var("INNO_SETUP_COMPILER").ok()).ok_or_else(||crate::common::ToolError("请用 --iscc 指定 Inno Setup 6.7+ 的 ISCC.exe".into()))?;
    check(Path::new(&iscc).is_file(),"Inno Setup 编译器不存在")?;
    let status=std::process::Command::new(&iscc).arg(format!("/DAppVersion={version}")).arg(format!("/DStageDir={}",stage.display())).arg(format!("/DOutputDir={}",out.display())).arg(format!("/DEngineFile={}",engine.display())).arg(root.join("build/installer.iss")).status()?;
    check(status.success(),"Inno Setup 标准安装器构建失败")?;
    copy_file(&share, &stage.join("DotIsle.Setup.exe"))?;
    files.insert("DotIsle.Setup.exe".into());

    std::fs::write(stage.join("OtaInstall.json"), "{\"offerInstaller\":true}")?;
    files.insert("OtaInstall.json".into());

    let legacy = out.join("OTA-Compatibility");
    std::fs::create_dir_all(&legacy)?;
    let ota = legacy.join(format!("{bundle}-Portable.zip"));
    let _ = std::fs::remove_file(&ota);
    write_zip(&ota, &files, &stage, &format!("{bundle}/"), &[])?;
    std::fs::write(legacy.join(format!("{bundle}-SHA256.txt")), format!("{}  {}\n", sha256(&ota)?, ota.file_name().unwrap_or_default().to_string_lossy()))?;

    let source = out.join(format!("{name}-Source.zip"));
    let _ = std::fs::remove_file(&source);
    let excluded = ["obj", "target", "target-native", "artifacts", "Logs", "Data", "TestResults", ".git", "__pycache__", "publish"];
    let mut writer = ZipWriter::create(&source)?;
    let mut stack = vec![root.clone()];
    let mut paths = Vec::new();
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(&root).map_err(|_| crate::common::ToolError("路径无效".into()))?;
            let excluded_here = relative.components().any(|component| excluded.iter().any(|name| component.as_os_str().to_string_lossy().eq_ignore_ascii_case(name)));
            if excluded_here { continue; }
            if entry.file_type()?.is_dir() { stack.push(path); }
            else if entry.file_type()?.is_file() { paths.push(relative.to_path_buf()); }
        }
    }
    paths.sort();
    for relative in paths { writer.add_file(&format!("{name}-Source/{}", zip_entry(&relative)), &root.join(&relative))?; }
    writer.finish()?;

    let checks = [
        (sha256(&share)?, share.file_name().unwrap_or_default().to_string_lossy().to_string()),
        (sha256(&portable)?, portable.file_name().unwrap_or_default().to_string_lossy().to_string()),
        (sha256(&source)?, source.file_name().unwrap_or_default().to_string_lossy().to_string()),
    ];
    std::fs::write(out.join(format!("{name}-SHA256.txt")), checks.iter().map(|(hash, name)| format!("{hash}  {name}")).collect::<Vec<_>>().join("\n") + "\n")?;

    println!("{}", json!({
        "nativeBytes": std::fs::metadata(stage.join("LyricIsland.exe"))?.len(),
        "portableBytes": std::fs::metadata(&portable)?.len(),
        "shareBytes": std::fs::metadata(&share)?.len(),
        "sourceBytes": std::fs::metadata(&source)?.len(),
        "fontSha256": FONT_SHA256,
        "payloadSha256": payload_digest,
        "version": version,
    }));
    if !stage.join("DotIsle.Setup.exe").is_file() { bail!("安装程序未生成"); }
    Ok(())
}

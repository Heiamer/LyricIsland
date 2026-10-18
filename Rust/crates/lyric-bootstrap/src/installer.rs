use crate::error::{Error, Result};
use crate::paths::{self, safe_child};
use crate::payload;
use crate::registry::{self, Key, RUN_KEY, RUN_VALUE, UNINSTALL_KEY};
use crate::updater;
use crate::win;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::core::{GUID, Interface, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::IShellLinkW;

const DEFAULT_PIPE: &str = "LyricIsland.Native.Settings.v1";
const DATA_OWNER: &str = ".dotisle-data.json";

pub struct Running {
    pub exe: PathBuf,
    pub data: PathBuf,
}

pub fn pipe_name() -> String {
    std::env::var("LYRICISLAND_TEST_PIPE").ok().filter(|value| !value.is_empty()).unwrap_or_else(|| DEFAULT_PIPE.into())
}

pub fn default_install_folder() -> PathBuf {
    Key::open(&(UNINSTALL_KEY.to_owned()+"_is1")).ok().flatten().or_else(||Key::open(UNINSTALL_KEY).ok().flatten())
        .and_then(|key| key.string("InstallLocation"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::local_app_data().join("Programs/DotIsle"))
}

pub fn stop_running(only_target: Option<&Path>) -> Option<Running> {
    let pipe = pipe_name();
    let state = win::pipe_request(&pipe, &json!({"version": 1, "command": "state"}), Duration::from_millis(600)).ok()?;
    let pid = state["processId"].as_u64()? as u32;
    let exe = win::process_image(pid)?;
    if let Some(only) = only_target {
        if !paths::starts_with_path(&exe, only) { return None; }
    }
    let mut data = state["dataDirectory"].as_str().filter(|text| !text.is_empty()).map(PathBuf::from);
    if data.is_none() {
        if let Some(directory) = exe.parent() {
            let candidate = directory.join("Data");
            if candidate.join("settings.json").is_file() { data = Some(candidate); }
        }
    }
    updater::stop_app(&pipe).ok()?;
    updater::wait(pid as i32).ok()?;
    Some(Running { exe, data: data.unwrap_or_default() })
}

pub fn reject_links(root: &Path) -> Result<()> {
    let full = paths::full_path(root)?;
    let mut current = Some(full.as_path());
    while let Some(path) = current {
        if path.exists() && paths::has_reparse_point(path) { return Err(Error(format!("目录包含链接，已停止删除：{}", path.display()))); }
        current = path.parent();
    }
    if !full.exists() { return Ok(()); }
    let mut queue = std::collections::VecDeque::from([full]);
    while let Some(directory) = queue.pop_front() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if paths::has_reparse_point(&path) { return Err(Error(format!("数据含链接，已保留数据：{}", path.display()))); }
            if entry.file_type()?.is_dir() { queue.push_back(path); }
        }
    }
    Ok(())
}

pub fn mark_data_owner(data: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(data)?;
    paths::write_json(&data.join(DATA_OWNER), &json!({"product": "DotIsle", "installation": target.to_string_lossy()}))
}

pub fn delete_owned_data(data: &Path, target: &Path) -> Result<()> {
    if data.as_os_str().is_empty() || !data.exists() { return Ok(()); }
    let data = paths::full_path(data)?;
    for protected in paths::protected_folders() {
        if paths::same_path(&data, &protected) || paths::starts_with_path(&protected, &data) {
            return Err(Error(format!("数据路径不是独立应用目录，已保留：{}", data.display())));
        }
    }
    if data.to_string_lossy().trim_end_matches('\\').len() <= 3 { return Err(Error("不能删除磁盘根目录".into())); }
    let mut owned = paths::same_path(&data, paths::owned_data_default()) || paths::same_path(&data, target.join("Data"));
    let owner = data.join(DATA_OWNER);
    if owner.is_file() {
        if let Ok(record) = paths::read_json(&owner) {
            owned |= record["product"].as_str() == Some("DotIsle")
                && record.get("installation").and_then(Value::as_str).is_some_and(|installation| paths::same_path(installation, target));
        }
    }
    if !owned { return Err(Error(format!("无法确认此数据目录属于点屿，已保留：{}", data.display()))); }
    reject_links(&data)?;
    std::fs::remove_dir_all(&data)?;
    Ok(())
}

pub fn remove_owned_files(target: &Path) -> Result<()> {
    let marker = payload::read_marker(target)?;
    if payload::marker_product(&marker) != "DotIsle" { return Err(Error("安装标识无效".into())); }
    reject_links(target)?;
    let mut paths_to_remove = Vec::new();
    for relative in payload::marker_files(&marker) {
        if relative.eq_ignore_ascii_case(payload::INSTALLED_MARKER) { continue; }
        paths_to_remove.push(safe_child(target, &relative)?);
    }
    for path in paths_to_remove {
        let mut attempt = 0;
        while path.exists() {
            match std::fs::remove_file(&path) {
                Ok(()) => break,
                Err(_) if attempt >= 20 => return Err(Error(format!("组件仍被占用，尚未卸载完成：{}", path.file_name().unwrap_or_default().to_string_lossy()))),
                Err(_) => { attempt += 1; std::thread::sleep(Duration::from_millis(100)); }
            }
        }
    }
    paths::remove_file_if_present(&target.join(payload::INSTALLED_MARKER))?;
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut queue = std::collections::VecDeque::from([target.to_path_buf()]);
    while let Some(directory) = queue.pop_front() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() { directories.push(entry.path()); queue.push_back(entry.path()); }
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.as_os_str().len()));
    for directory in directories {
        if directory.exists() && std::fs::read_dir(&directory)?.next().is_none() { let _ = std::fs::remove_dir(&directory); }
    }
    if target.exists() && std::fs::read_dir(target)?.next().is_none() { let _ = std::fs::remove_dir(target); }
    Ok(())
}

pub fn copy_data(source: &Path, target: &Path) -> Result<()> {
    if source.as_os_str().is_empty() || !source.exists() || paths::same_path(source, target) { return Ok(()); }
    std::fs::create_dir_all(target)?;
    let mut queue = std::collections::VecDeque::from([source.to_path_buf()]);
    let mut total = 0u64;
    while let Some(directory) = queue.pop_front() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if paths::has_reparse_point(&path) {
                return Err(Error(if entry.file_type()?.is_dir() { "迁移数据目录包含链接，原数据已保留。".into() } else { "迁移数据包含链接".into() }));
            }
            if entry.file_type()?.is_dir() {
                if entry.file_name().to_string_lossy().eq_ignore_ascii_case("Logs") { continue; }
                queue.push_back(path);
                continue;
            }
            let size = entry.metadata()?.len();
            total = total.saturating_add(size);
            if total > 1024 * 1024 * 1024 { return Err(Error("迁移数据超过1GB，请先整理缓存；原数据未删除。".into())); }
            let relative = path.strip_prefix(source).map_err(|_| Error("迁移数据路径无效".into()))?;
            let output = target.join(relative);
            if let Some(parent) = output.parent() { std::fs::create_dir_all(parent)?; }
            if !output.exists() { std::fs::copy(&path, &output)?; }
        }
    }
    Ok(())
}

pub fn stop_owned_programs(target: &Path) -> Result<()> {
    let _ = stop_running(Some(target));
    let current = win::current_pid();
    let names = ["LyricIsland.Settings", "lyric-island-settings", "LyricIsland", "LyricIsland.Updater"];
    for entry in win::processes() {
        if entry.pid == current { continue; }
        let matches = names.iter().any(|name| entry.name.eq_ignore_ascii_case(&format!("{name}.exe")) || entry.name.eq_ignore_ascii_case(name));
        if !matches { continue; }
        let Some(image) = win::process_image(entry.pid) else { continue; };
        if !paths::starts_with_path(&image, target) { continue; }
        win::close_main_windows(entry.pid);
        if !win::wait_process(entry.pid, Duration::from_secs(3)) {
            if !win::terminate_process(entry.pid) { return Err(Error(format!("点屿组件尚未退出：{}", entry.name))); }
        }
    }
    Ok(())
}

fn quote_target(target: &Path) -> String { paths::quote(&target.to_string_lossy()) }

pub fn write_registration(key: &Key, target: &Path) -> Result<()> {
    let marker = payload::read_marker(target)?;
    let version = payload::marker_version(&marker);
    let target_text = target.to_string_lossy().to_string();
    let uninstall = format!("{} --uninstall --install-dir {}", quote_target(&target.join("Uninstall.exe")), quote_target(target));
    key.set_string("DisplayName", "点屿 DotIsle")?;
    key.set_string("DisplayVersion", version)?;
    key.set_string("Publisher", "DotIsle")?;
    key.set_string("InstallLocation", &target_text)?;
    key.set_string("DisplayIcon", &format!("{},0", quote_target(&target.join("LyricIsland.exe"))))?;
    key.set_string("UninstallString", &uninstall)?;
    key.set_string("QuietUninstallString", &format!("{uninstall} --quiet --keep-data"))?;
    key.set_string("InstallDate", &paths::local_stamp(true))?;
    let mut bytes = 0u64;
    for entry in payload::marker_files(&marker) {
        if let Ok(path) = safe_child(target, &entry) {
            if let Ok(meta) = std::fs::metadata(&path) { bytes = bytes.saturating_add(meta.len()); }
        }
    }
    key.set_u32("EstimatedSize", (bytes.saturating_add(1023) / 1024).min(i32::MAX as u64) as u32)?;
    for name in ["NoModify", "NoRepair"] { key.set_u32(name, 1)?; }
    for name in ["NoRemove", "SystemComponent", "WindowsInstaller"] { key.set_u32(name, 0)?; }
    if key.string("DisplayName").as_deref() != Some("点屿 DotIsle")
        || !key.string("InstallLocation").is_some_and(|value| paths::same_path(&value, target))
        || key.string("UninstallString").as_deref() != Some(uninstall.as_str())
        || !target.join("Uninstall.exe").is_file()
    {
        return Err(Error("Windows卸载登记校验失败，安装尚未完成。".into()));
    }
    Ok(())
}

fn create_shortcut(link_path: &Path, target: &Path) -> Result<()> {
    unsafe {
        let class: GUID = GUID::from_u128(0x0002_1401_0000_0000_c000_0000_0000_0046);
        let link: IShellLinkW = CoCreateInstance(&class, None, CLSCTX_INPROC_SERVER)?;
        let target_wide = win::wide(&target.to_string_lossy());
        link.SetPath(PCWSTR(target_wide.as_ptr()))?;
        let directory = win::wide(&target.parent().unwrap_or(Path::new("")).to_string_lossy());
        link.SetWorkingDirectory(PCWSTR(directory.as_ptr()))?;
        let persist: IPersistFile = link.cast()?;
        let link_wide = win::wide(&link_path.to_string_lossy());
        persist.Save(PCWSTR(link_wide.as_ptr()), true)?;
    }
    Ok(())
}

fn register_shortcuts(target: &Path) -> Result<()> {
    let programs = paths::programs_menu().join("DotIsle");
    std::fs::create_dir_all(&programs)?;
    create_shortcut(&programs.join("点屿.lnk"), &target.join("LyricIsland.exe"))?;
    create_shortcut(&programs.join("卸载点屿.lnk"), &target.join("Uninstall.exe"))?;
    Ok(())
}

pub fn register(target: &Path) -> Result<()> {
    let key = Key::create(UNINSTALL_KEY)?;
    write_registration(&key, target)?;
    let pointer = paths::local_app_data().join("DotIsle/install.json");
    paths::write_json(&pointer, &json!({"target": target.to_string_lossy()}))?;
    let target = target.to_path_buf();
    let shortcut_target = target.clone();
    let worker = std::thread::spawn(move || -> Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let outcome = register_shortcuts(&shortcut_target);
            CoUninitialize();
            outcome
        }
    });
    let shortcut_error = match worker.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some(Error("快捷方式创建线程失败".into())),
    };
    if let Some(error) = shortcut_error { let _ = std::fs::write(target.join("shortcut-warning.txt"), error.to_string()); }
    if let Some(run) = Key::open(RUN_KEY)? {
        if run.string(RUN_VALUE).is_some() { run.set_string(RUN_VALUE, &quote_target(&target.join("LyricIsland.exe")))?; }
    }
    Ok(())
}

fn registration_location() -> Option<PathBuf> {
    Key::open(UNINSTALL_KEY).ok().flatten().and_then(|key| key.string("InstallLocation")).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn other_install_uses_data(target: &Path, data: &Path) -> bool {
    let Some(other) = registration_location() else { return false; };
    if paths::same_path(&other, target) { return false; }
    let marker = other.join(payload::INSTALLED_MARKER);
    if !marker.is_file() { return false; }
    payload::read_marker(&other).ok().and_then(|marker| payload::marker_data(&marker)).is_some_and(|other_data| paths::same_path(other_data, data))
}

pub fn repair_if_needed(target: &Path) -> Result<()> {
    let marker = payload::read_marker(target)?;
    if payload::marker_product(&marker) != "DotIsle" { return Err(Error("安装标识无效".into())); }
    if marker["installer"]=="Inno"{return crate::inno::repair(target,&marker)}
    let current = Key::open(UNINSTALL_KEY)?.is_some_and(|key| {
        key.string("InstallLocation").is_some_and(|value| paths::same_path(value, target))
            && key.string("DisplayVersion").as_deref() == Some(payload::marker_version(&marker))
            && key.string("DisplayName").as_deref() == Some("点屿 DotIsle")
            && key.u32("SystemComponent") == Some(0)
            && key.string("UninstallString").is_some_and(|value| value.contains("--install-dir"))
    });
    if !current { register(target)?; }
    Ok(())
}

pub fn repair_registration(target: &Path) -> Result<()> {
    if payload::marker_product(&payload::read_marker(target)?) != "DotIsle" { return Err(Error("安装标识无效".into())); }
    register(target)
}

pub fn uninstall(target: &Path, quiet: bool, remove_data: bool, parent: u32) -> Result<()> {
    if parent > 0 && parent != win::current_pid() && !win::wait_process(parent, Duration::from_secs(15)) {
        return Err(Error("卸载入口尚未退出，请稍后重试。".into()));
    }
    let target = paths::full_path(target)?;
    let marker = payload::read_marker(&target)?;
    if payload::marker_product(&marker) != "DotIsle" { return Err(Error("安装标识无效".into())); }
    let data = payload::marker_data(&marker).unwrap_or_default();
    let registered_here = registration_location().is_some_and(|location| paths::same_path(location, &target));
    if remove_data && other_install_uses_data(&target, &data) {
        return Err(Error("其他安装正在使用这个数据目录，请保留数据后卸载此旧副本。".into()));
    }
    let mut keep = !remove_data;
    if !quiet {
        match crate::ui::confirm_uninstall(&target, &data) {
            None => return Ok(()),
            Some(choice) => keep = choice,
        }
    }
    if !keep {
        if other_install_uses_data(&target, &data) {
            return Err(Error("其他安装正在使用这个数据目录，请保留数据后卸载此旧副本。".into()));
        }
        if !data.as_os_str().is_empty() { reject_links(&data)?; }
    }
    stop_owned_programs(&target)?;
    if !keep { delete_owned_data(&data, &target)?; }
    remove_owned_files(&target)?;
    if registration_location().is_some_and(|location| paths::same_path(location, &target)) { registry::delete_tree(UNINSTALL_KEY)?; }
    let pointer = paths::local_app_data().join("DotIsle/install.json");
    if pointer.is_file() {
        if let Ok(value) = paths::read_json(&pointer) {
            if value["target"].as_str().is_some_and(|value| paths::same_path(value, &target)) { let _ = paths::remove_file_if_present(&pointer); }
        }
    }
    if registered_here {
        let programs = paths::programs_menu().join("DotIsle");
        for name in ["点屿.lnk", "卸载点屿.lnk"] { let _ = paths::remove_file_if_present(&programs.join(name)); }
        if programs.exists() && std::fs::read_dir(&programs)?.next().is_none() { let _ = std::fs::remove_dir(&programs); }
    }
    if let Some(run) = Key::open(RUN_KEY)? {
        if run.string(RUN_VALUE).is_some_and(|value| paths::starts_with_path(&value, &target)) { run.delete_value(RUN_VALUE)?; }
    }
    if !quiet { win::message_box(if keep { "点屿已卸载，用户数据已保留。" } else { "点屿及用户数据已删除。" }, "点屿", false); }
    Ok(())
}

pub fn uninstall_self(target: &Path, quiet: bool, remove_data: bool) -> Result<()> {
    let source = std::env::current_exe()?;
    let copy = paths::temp_unique("DotIsle.Uninstall").with_extension("exe");
    std::fs::copy(&source, &copy)?;
    let mut command = std::process::Command::new(&copy);
    command.arg("--uninstall-worker").arg("--install-dir").arg(target).arg("--parent").arg(win::current_pid().to_string());
    if quiet { command.arg("--quiet"); }
    if remove_data { command.arg("--remove-data"); }
    command.spawn().map_err(|error| Error(format!("无法启动卸载程序：{error}")))?;
    Ok(())
}

pub fn install(requested: &str, launch: bool, register_target: bool, migration: Option<&Path>) -> Result<()> {
    let target = paths::normalize_target(requested)?;
    let scratch = paths::temp_unique("DotIsle.Setup");
    std::fs::create_dir_all(&scratch)?;
    let stage = scratch.join("stage");
    payload::extract_payload(&payload::verified_install_payload()?, &stage)?;
    let mut data = paths::owned_data_default();
    if target.join(payload::INSTALLED_MARKER).is_file() {
        if let Ok(marker) = payload::read_marker(&target) {
            if let Some(existing) = payload::marker_data(&marker) { data = paths::full_path(existing)?; }
        }
    }
    if !register_target { data = scratch.join("test-data"); }
    let mut files = payload::relative_files(&stage)?;
    files.push(payload::INSTALLED_MARKER.into());
    files.sort();
    payload::write_marker(&stage, &data, &files)?;
    std::fs::create_dir_all(&target)?;
    let probe = target.join(format!(".dotisle-write-{}", paths::unique_suffix()));
    std::fs::write(&probe, [])?;
    std::fs::remove_file(&probe)?;
    let previous = stop_running(None);
    let migration = migration.map(Path::to_path_buf).or_else(|| previous.as_ref().map(|running| running.data.clone()));
    let outcome = (|| -> Result<()> {
        if let Some(source) = migration.as_ref().filter(|source| source.exists()) {
            if data.join("settings.json").is_file() && !paths::same_path(source, &data) {
                copy_data(source, &data.join("MigrationBackups").join(paths::local_stamp(false)))?;
            } else {
                copy_data(source, &data)?;
            }
        }
        if register_target {
            stop_owned_programs(&target)?;
            mark_data_owner(&data, &target)?;
        }
        let target_for_register = target.clone();
        let mut after = || -> Result<()> {
            if register_target { register(&target_for_register)?; }
            Ok(())
        };
        updater::apply(&stage, &target, &scratch.join("backup"), 0, Some(&mut after))?;
        if launch { updater::start(&target, &data)?; }
        Ok(())
    })();
    if outcome.is_err() {
        if let Some(previous) = previous.as_ref().filter(|running| running.exe.is_file()) {
            let mut command = std::process::Command::new(&previous.exe);
            if !previous.data.as_os_str().is_empty() { command.arg("--data-dir").arg(&previous.data); }
            if let Some(directory) = previous.exe.parent() { command.current_dir(directory); }
            let _ = command.spawn();
        }
    }
    if outcome.is_err() { let _ = std::fs::remove_dir_all(&scratch); }
    outcome
}

pub struct InstallReport {
    pub installed: bool,
}

pub fn test_install(directory: &Path, migration: Option<&Path>) -> Result<InstallReport> {
    std::env::set_var("LYRICISLAND_TEST_PIPE", format!("LyricIsland.Settings.test.installer.{}", paths::unique_suffix()));
    install(&directory.to_string_lossy(), false, false, migration)?;
    Ok(InstallReport { installed: directory.join(payload::INSTALLED_MARKER).is_file() })
}

pub fn defaults_keep_user_data() -> bool { true }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("dotisle-installer-{name}-{}", paths::unique_suffix()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn fixture(root: &Path, name: &str) -> (PathBuf, PathBuf) {
        let app = root.join(name).join("App");
        let data = root.join(name).join("UserData");
        std::fs::create_dir_all(app.join("Assets")).unwrap();
        std::fs::create_dir_all(data.join("Logs")).unwrap();
        std::fs::write(app.join("LyricIsland.exe"), "binary").unwrap();
        std::fs::write(app.join("Uninstall.exe"), "uninstaller").unwrap();
        std::fs::write(app.join("Assets/icon.bin"), "asset").unwrap();
        std::fs::write(data.join("settings.json"), "keep setting").unwrap();
        std::fs::write(data.join("Logs/log.txt"), "log").unwrap();
        std::fs::write(data.join(DATA_OWNER), json!({"product": "DotIsle", "installation": app}).to_string()).unwrap();
        std::fs::write(app.join(payload::INSTALLED_MARKER), json!({
            "product": "DotIsle", "version": payload::VERSION, "dataDirectory": data,
            "files": ["LyricIsland.exe", "Uninstall.exe", "Assets/icon.bin", "Installed.json"]
        }).to_string()).unwrap();
        (app, data)
    }

    fn test_key(name: &str) -> (Key, String) {
        let path = format!(r"Software\DotIsle\InstallerTests\{name}-{}", paths::unique_suffix());
        (Key::create(&path).unwrap(), path)
    }

    #[test]
    fn registration_keeps_the_directory_quoted_and_the_entry_removable() {
        let root = scratch("registration");
        let (app, _data) = fixture(&root, "Keep");
        let (key, path) = test_key("reg");
        write_registration(&key, &app).unwrap();
        assert_eq!(key.string("DisplayName").as_deref(), Some("点屿 DotIsle"));
        assert_eq!(key.u32("SystemComponent"), Some(0));
        assert_eq!(key.u32("NoRemove"), Some(0));
        assert!(key.string("UninstallString").unwrap().contains(&format!("--install-dir \"{}\"", app.to_string_lossy())));
        assert!(key.string("QuietUninstallString").unwrap().contains("--keep-data"));
        drop(key);
        registry::delete_tree(&path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keep_data_uninstall_leaves_user_files_and_removes_owned_files() {
        let root = scratch("keep");
        let (app, data) = fixture(&root, "Keep");
        remove_owned_files(&app).unwrap();
        assert!(!app.exists());
        assert!(data.join("settings.json").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn full_cleanup_preserves_unmanaged_files_and_foreign_data_is_protected() {
        let root = scratch("remove");
        let (app, data) = fixture(&root, "Remove");
        std::fs::write(app.join("user-note.txt"), "unmanaged").unwrap();
        delete_owned_data(&data, &app).unwrap();
        remove_owned_files(&app).unwrap();
        assert!(!data.exists());
        assert!(app.join("user-note.txt").is_file());
        let foreign = root.join("Foreign");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("keep.txt"), "personal").unwrap();
        assert!(delete_owned_data(&foreign, &app).is_err());
        assert!(foreign.join("keep.txt").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_manifest_is_rejected_before_any_file_is_deleted() {
        let root = scratch("traversal");
        let (app, data) = fixture(&root, "Traversal");
        let sentinel = root.join("Traversal").join("outside.txt");
        std::fs::write(&sentinel, "outside").unwrap();
        std::fs::write(app.join(payload::INSTALLED_MARKER), json!({
            "product": "DotIsle", "version": payload::VERSION, "dataDirectory": data,
            "files": ["LyricIsland.exe", "../outside.txt", "Installed.json"]
        }).to_string()).unwrap();
        assert!(remove_owned_files(&app).is_err());
        assert!(sentinel.is_file());
        assert!(app.join("LyricIsland.exe").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uninstall_dialog_defaults_to_keeping_data() {
        assert!(defaults_keep_user_data());
    }
}

//! Command line entry points shared by DotIsle.Setup.exe and Uninstall.exe.

use crate::error::{Error, Result};
use crate::paths;
use crate::payload;
use crate::ui;
use crate::win;
use serde_json::json;
use std::path::Path;

fn console_mode(args: &[String]) -> bool {
    if args.first().is_some_and(|arg|arg.starts_with("--inno-")){return true}
    args.first().is_some_and(|arg| matches!(arg.as_str(), "--verify-package" | "--test-install" | "--repair-if-needed"))
}

fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned()
}

fn has(args: &[String], name: &str) -> bool { args.iter().any(|arg| arg == name) }

pub fn run(args: Vec<String>) -> i32 {
    match dispatch(&args) {
        Ok(code) => code,
        Err(error) => {
            if console_mode(&args) {
                eprintln!("{error}");
            } else {
                win::message_box(&error.to_string(), "点屿安装", true);
            }
            1
        }
    }
}

fn dispatch(args: &[String]) -> Result<i32> {
    if let Some(mode)=args.first().filter(|arg|arg.starts_with("--inno-")){
        let target=value_after(args,"--install-dir").ok_or_else(||Error("缺少安装目录".into()))?;let target=paths::full_path(target)?;
        let record=value_after(args,"--record").map(std::path::PathBuf::from).unwrap_or_default();
        match mode.as_str(){
         "--inno-prepare"=>crate::inno::prepare(&target,&record,value_after(args,"--migrate-data").as_deref().map(Path::new))?,
         "--inno-finalize"=>crate::inno::finalize(&target,&record,value_after(args,"--data-dir").as_deref().filter(|s|!s.is_empty()).map(Path::new),&value_after(args,"--registry-key").ok_or_else(||Error("缺少卸载登记路径".into()))?)?,
         "--inno-cleanup"=>crate::inno::cleanup(&target,has(args,"--remove-data"))?,
         _=>return Err(Error("未知安装维护命令".into())),
        }return Ok(0)
    }
    if args.len() == 2 && args[0] == "--verify-package" {
        let root = paths::full_path(&args[1])?;
        payload::extract_payload(&payload::verified_install_payload()?, &root)?;
        payload::write_verification(&root, &root.join("LyricIsland.exe"), false)?;
        return Ok(0);
    }
    if (args.len() == 3 || args.len() == 5) && args[0] == "--test-install" {
        let migration = if args.len() == 5 && args[3] == "--migrate-data" { Some(paths::full_path(&args[4])?) } else { None };
        let report = crate::installer::test_install(Path::new(&args[1]), migration.as_deref())?;
        paths::write_json(Path::new(&args[2]), &json!({
            "success": true,
            "installed": report.installed,
            "registered": false,
            "launched": false,
        }))?;
        return Ok(0);
    }
    if args.len() == 2 && args[0] == "--repair-if-needed" {
        crate::installer::repair_if_needed(Path::new(&args[1]))?;
        return Ok(0);
    }
    if args.len() == 2 && args[0] == "--repair-registration" {
        crate::installer::repair_registration(Path::new(&args[1]))?;
        return Ok(0);
    }
    if has(args, "--uninstall-worker") {
        let target = value_after(args, "--install-dir").ok_or_else(|| Error("缺少卸载目录".into()))?;
        let parent = value_after(args, "--parent").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
        crate::installer::uninstall(Path::new(&target), has(args, "--quiet"), has(args, "--remove-data"), parent)?;
        return Ok(0);
    }
    let self_exe = std::env::current_exe()?;
    let uninstall_entry = has(args, "--uninstall")
        || (args.is_empty() && self_exe.file_name().is_some_and(|name| name.eq_ignore_ascii_case("Uninstall.exe")));
    if uninstall_entry {
        let target = match value_after(args, "--install-dir") {
            Some(value) => paths::full_path(value)?,
            None => self_exe.parent().unwrap_or(Path::new(".")).to_path_buf(),
        };
        if target.join("unins000.exe").is_file(){let mut command=std::process::Command::new(target.join("unins000.exe"));if has(args,"--quiet"){command.arg("/VERYSILENT").arg("/SUPPRESSMSGBOXES");}if has(args,"--remove-data"){command.arg("/REMOVEDATA=1");}command.spawn()?;return Ok(0)}
        crate::installer::uninstall_self(&target, has(args, "--quiet"), has(args, "--remove-data"))?;
        return Ok(0);
    }
    let migration = value_after(args, "--migrate-data").map(|value| paths::full_path(value)).transpose()?;
    let _gate = win::NamedMutex::acquire("Local.DotIsle.Setup")?;
    if _gate.is_none() { return Ok(0); }
    ui::install_wizard(migration)?;
    Ok(0)
}

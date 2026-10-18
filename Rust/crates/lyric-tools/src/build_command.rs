//! Configure the GNU toolchain, then run the workspace checks and the release build.

use crate::bail;
use crate::common::{arg_value, check, source_root, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|directory| directory.join(name)).find(|path| path.is_file())
}

fn existing(value: Option<String>, environment: &str, name: &str, flag: &str) -> Result<PathBuf> {
    let path = value.or_else(|| std::env::var(environment).ok());
    let path = match path {
        Some(path) => Some(PathBuf::from(path)),
        None => find_in_path(name),
    };
    let path = path.ok_or_else(|| crate::common::ToolError(format!("请用 {flag} 提供 {name} 的绝对路径。")))?;
    check(path.is_file(), format!("找不到 {name}：{}", path.display()))?;
    Ok(path)
}

fn cargo(arguments: &[String], environment: &[(OsString, OsString)]) -> Result<()> {
    let mut command = Command::new("cargo.exe");
    command.args(arguments);
    for (key, value) in environment { command.env(key, value); }
    let status = command.status()?;
    if !status.success() { bail!("cargo 失败：{}", arguments.join(" ")); }
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let root = source_root();
    let toolchain = std::env::var_os("LYRICISLAND_TOOLCHAIN").map(PathBuf::from)
        .unwrap_or_else(|| lyric_bootstrap::paths::user_profile().join(".rustup/toolchains/stable-x86_64-pc-windows-gnu"));
    let dlltool = existing(arg_value(args, "--dlltool"), "LLVM_DLLTOOL", "llvm-dlltool.exe", "--dlltool")?;
    let fxc = existing(arg_value(args, "--fxc"), "GPUI_FXC_PATH", "fxc.exe", "--fxc")?;
    let zig = existing(arg_value(args, "--zig-cc"), "LYRICISLAND_ZIG_CC", "zig.exe", "--zig-cc")?;
    let target = arg_value(args, "--target-dir").map(PathBuf::from).unwrap_or_else(|| root.join("Rust/target-native"));

    let toolchain_bin = toolchain.join("bin");
    let self_contained = toolchain.join("lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained");
    let mut paths = vec![toolchain_bin.clone(), self_contained.clone()];
    if let Some(existing) = std::env::var_os("PATH") { paths.extend(std::env::split_paths(&existing)); }
    let path = std::env::join_paths(paths).map_err(|error| crate::common::ToolError(format!("PATH 无效：{error}")))?;

    let ar = toolchain.join("lib/rustlib/x86_64-pc-windows-gnu/bin/llvm-ar.exe");
    let linker = toolchain.join("lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe");
    check(ar.is_file(), format!("缺少 llvm-ar：{}", ar.display()))?;
    check(linker.is_file(), format!("缺少 rust-lld：{}", linker.display()))?;

    let library_root = std::env::var_os("LYRICISLAND_NATIVE_LIB_ROOT").map(PathBuf::from).unwrap_or_else(|| root.join("build"));
    let shlwapi = library_root.join("shlwapi");
    let ktmw32 = library_root.join("ktmw32");
    let flags = [
        "-C".to_owned(), "target-feature=+crt-static".to_owned(),
        "-C".to_owned(), format!("dlltool={}", dlltool.display()),
        "-C".to_owned(), format!("linker={}", linker.display()),
        "-C".to_owned(), "linker-flavor=ld.lld".to_owned(),
        "-L".to_owned(), format!("native={}", shlwapi.display()),
        "-L".to_owned(), format!("native={}", ktmw32.display()),
    ].join("\u{1f}");

    let environment: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), path),
        ("CARGO_TARGET_DIR".into(), target.clone().into_os_string()),
        ("GPUI_FXC_PATH".into(), fxc.into_os_string()),
        ("CC".into(), zig.into_os_string()),
        ("AR".into(), ar.into_os_string()),
        ("CARGO_ENCODED_RUSTFLAGS".into(), flags.into()),
    ];

    let mut common = Vec::new();
    if args.iter().any(|arg| arg == "--offline") { common.push("--offline".to_owned()); }

    std::env::set_current_dir(root.join("Rust"))?;
    let mut test = vec!["test".to_owned()];
    test.extend(common.clone());
    test.extend(["--workspace".to_owned()]);
    cargo(&test, &environment)?;

    let mut build = vec!["build".to_owned()];
    build.extend(common);
    build.extend([
        "--release".to_owned(),
        "--package".to_owned(), "lyric-native".to_owned(),
        "--package".to_owned(), "lyric-settings".to_owned(),
        "--package".to_owned(), "lyric-bootstrap".to_owned(),
        "--package".to_owned(), "lyric-tools".to_owned(),
    ]);
    cargo(&build, &environment)?;

    println!("已生成可执行文件：{}", target.join("release").display());
    Ok(())
}

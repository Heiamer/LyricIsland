//! Release and verification tooling for DotIsle, replacing the former Python/PowerShell scripts.

mod build_command;
mod common;
mod icon;
mod package;
mod process;
mod sqlite;
mod verify;

use crate::common::Result;

fn embed(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        crate::bail!("用法：lyric-tools embed-icon <exe> <ico>");
    }
    icon::embed_icon(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]))
}

fn usage() {
    eprintln!("点屿构建与验证工具");
    eprintln!("  lyric-tools build [--offline] [--dlltool <path>] [--fxc <path>] [--zig-cc <path>] [--target-dir <dir>]");
    eprintln!("  lyric-tools package [--version <v>] [--binary-dir <dir>] [--output <dir>] [--sqlite <dll>]");
    eprintln!("  lyric-tools embed-icon <exe> <ico>");
    eprintln!("  lyric-tools verify e2e <runtime> [--report <json>] [--artifacts <dir>] [--version <v>]");
    eprintln!("  lyric-tools verify redesign <runtime> <output>");
    eprintln!("  lyric-tools verify notifications --runtime <dir> --work <dir>");
    eprintln!("  lyric-tools verify progress <pipe> <work> <report>");
    eprintln!("  lyric-tools verify updater <zip> <sha256> <work> <report> [version]");
    eprintln!("  lyric-tools verify installer <work> <report>");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("package") => package::run(&args[1..]),
        Some("build") => build_command::run(&args[1..]),
        Some("embed-icon") => embed(&args[1..]),
        Some("verify") => verify::run(&args[1..]),
        _ => {
            usage();
            std::process::exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

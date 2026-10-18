#![windows_subsystem = "windows"]

fn main() {
    std::process::exit(lyric_bootstrap::updater::run(std::env::args().skip(1).collect()));
}

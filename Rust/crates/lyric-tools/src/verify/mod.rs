//! End-to-end contracts that used to live in the Python and C# test harnesses.

pub mod e2e;
pub mod installer;
pub mod notifications;
pub mod progress;
pub mod redesign;
pub mod updater;

use crate::common::Result;
use serde_json::Value;

pub fn same_number(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => (left - right).abs() < 1e-9,
        _ => left == right,
    }
}

pub fn same_value(left: &Value, right: &Value) -> bool {
    if right.is_number() { same_number(left, right) } else { left == right }
}

pub fn paths_equal(left: &std::path::Path, right: &std::path::Path) -> bool {
    lyric_bootstrap::paths::same_path(left, right)
}

pub fn run(args: &[String]) -> Result<()> {
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    match mode {
        "e2e" => e2e::run(rest),
        "redesign" => redesign::run(rest),
        "notifications" => notifications::run(rest),
        "progress" => progress::run(rest),
        "updater" => updater::run(rest),
        "installer" => installer::run(rest),
        other => crate::bail!("未知验证模式：{other}（可用 e2e、redesign、notifications、progress、updater、installer）"),
    }
}

//! `Utilities.Log.cs` — a dependency-free logger.
//!
//! The C# original writes to a rolling file under `%APPDATA%\LyricIsland\logs` and also
//! mirrors `Log.Diagnostic(category, message)` calls into the debugger. The Rust port keeps
//! the same level semantics and the `Diagnostic` category convention, writing to stderr and
//! optionally to a file, without pulling a logging framework in.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

static LEVEL: OnceLock<Mutex<LogLevel>> = OnceLock::new();
static FILE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn level_cell() -> &'static Mutex<LogLevel> {
    LEVEL.get_or_init(|| Mutex::new(LogLevel::Warn))
}

fn file_cell() -> &'static Mutex<Option<PathBuf>> {
    FILE.get_or_init(|| Mutex::new(None))
}

/// Sets the minimum level that is emitted.
pub fn set_level(level: LogLevel) {
    if let Ok(mut guard) = level_cell().lock() {
        *guard = level;
    }
}

pub fn level() -> LogLevel {
    level_cell().lock().map(|guard| *guard).unwrap_or(LogLevel::Off)
}

/// Mirrors log output into `path` (append, created on demand).
pub fn set_log_file(path: Option<PathBuf>) {
    if let Ok(mut guard) = file_cell().lock() {
        *guard = path;
    }
}

pub fn log(level: LogLevel, message: &str) {
    if level > self::level() {
        return;
    }

    let line = format!("[{:?}] {message}\n", level);
    eprint!("{line}");

    if let Ok(guard) = file_cell().lock() {
        if let Some(path) = guard.as_ref() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }
}

/// `Log.Diagnostic(category, message)`.
pub fn diagnostic(category: &str, message: &str) {
    log(LogLevel::Debug, &format!("{category} {message}"));
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::log($crate::log::LogLevel::Error, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::log($crate::log::LogLevel::Warn, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::log($crate::log::LogLevel::Info, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_diagnostic {
    ($category:expr, $($arg:tt)*) => {{
        if $crate::log::level() >= $crate::log::LogLevel::Debug {
            $crate::log::diagnostic($category, &format!($($arg)*));
        }
    }};
}

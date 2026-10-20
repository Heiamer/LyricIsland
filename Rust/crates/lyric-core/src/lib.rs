//! # lyric-core — Rust port of `TaskbarLyrics.Core`
//!
//! This crate is a **re-implementation** of the lyric pipeline that ships with
//! LyricIsland 0.7.8 / TaskbarLyrics 1.7.4, released as LyricIsland **1.0**.
//!
//! Ported (see `PORTING-NOTES.md` for the full mapping):
//! models, LRC/QRC/KRC/YRC/TTML parsing, QRC/KRC decryption, the Lyricify payload
//! decoder/parser, the identity matcher (Jaro-Winkler), the search planner and stage
//! executor, every policy (trust order, source routing, admission, transient failures),
//! the two-stage pipeline cache, the resolved-lyric cache, the local file provider, the
//! resolution coordinator and the display-frame sync service.
//!
//! Not ported: the WPF/WinUI shell, DWM materials, tray/monitor placement, SMTC and the
//! NetEase read-only progress adapter, and the online lyric providers.
//!
//! ```no_run
//! use lyric_core::models::TrackInfo;
//! use lyric_core::resolution::{LyricResolutionCoordinator, LyricResolutionCoordinatorBuilder};
//!
//! let track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify",
//!     std::time::Duration::from_secs(200));
//! let coordinator =
//!     LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder::default()).unwrap();
//! let result = coordinator.resolve(&track);
//! assert!(result.lyrics.is_none());
//! ```

pub mod cache;
pub mod hotkey;
pub mod json_store;
pub mod memory_scan;
pub mod crypto;
pub mod formats;
pub mod lines;
pub mod local;
pub mod log;
pub mod matching;
pub mod music_layout;
pub mod models;
pub mod payload;
pub mod planner;
pub mod policy;
pub mod projector;
pub mod resolution;
pub mod sync;
pub mod text;

use std::path::PathBuf;

/// Release name of this port (the `Cargo.toml` version is the semver-compatible
/// `1.0.0`).
pub const VERSION: &str = "1.0";

/// Base name of the original application's data directory.
pub const DATA_DIRECTORY_NAME: &str = "LyricIsland";

/// `%APPDATA%\LyricIsland` on Windows, `$XDG_DATA_HOME/LyricIsland` elsewhere.
pub fn default_data_directory() -> PathBuf {
    if let Ok(app_data) = std::env::var("APPDATA") {
        return PathBuf::from(app_data).join(DATA_DIRECTORY_NAME);
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(DATA_DIRECTORY_NAME);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share").join(DATA_DIRECTORY_NAME);
    }
    PathBuf::from(DATA_DIRECTORY_NAME)
}

/// The three cache files the pipeline uses, mirroring the original layout.
pub struct DatabasePaths {
    pub raw: PathBuf,
    pub parsed: PathBuf,
    pub resolved: PathBuf,
}

impl DatabasePaths {
    /// `%APPDATA%\LyricIsland\database\...`, matching `LyricPipelineCache` and
    /// `JsonResolvedLyricCache`'s default locations.
    pub fn default_paths() -> Self {
        let directory = default_data_directory().join("database");
        Self {
            raw: directory.join("lyric-pipeline-raw-v1.json"),
            parsed: directory.join("lyric-pipeline-parsed-v1.json"),
            resolved: directory.join(cache::JsonResolvedLyricCache::DEFAULT_FILE_NAME),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_match_the_original_file_names() {
        let paths = DatabasePaths::default_paths();
        assert!(paths.raw.ends_with("lyric-pipeline-raw-v1.json"));
        assert!(paths.parsed.ends_with("lyric-pipeline-parsed-v1.json"));
        assert!(paths.resolved.ends_with("resolved-lyrics-v1.json"));
    }

    #[test]
    fn version_is_the_requested_release_name() {
        assert_eq!(VERSION, "1.0");
    }
}

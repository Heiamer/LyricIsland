//! Installer, updater, launcher and uninstaller for DotIsle.
//!
//! Everything here talks to Windows directly so the shipped bootstrap binaries
//! have no managed runtime dependency.

pub mod error;
pub mod http;
pub mod installer;
pub mod inno;
pub mod launcher;
pub mod paths;
pub mod payload;
pub mod registry;
pub mod setup;
pub mod ui;
pub mod vello_overlay;
pub mod updater;
pub mod win;

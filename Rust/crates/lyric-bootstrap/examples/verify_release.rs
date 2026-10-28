//! Exercise the shipped OTA archive in an isolated installation directory.
use std::path::PathBuf;
use lyric_bootstrap::{updater,payload};
const VERSION: &str = env!("CARGO_PKG_VERSION");
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args=std::env::args().skip(1).collect::<Vec<_>>();assert_eq!(args.len(),3);let zip=PathBuf::from(&args[0]);let manifest=std::fs::read_to_string(&args[1])?;let out=PathBuf::from(&args[2]);assert!(!out.exists(),"use a new evidence directory");std::fs::create_dir_all(&out)?;
 let digest=updater::manifest_digest(&manifest,VERSION).ok_or("manifest digest missing")?;updater::verify(&zip,&digest)?;let stage=updater::extract(&zip,&out.join("stage"),VERSION)?;let target=out.join("target");std::fs::create_dir_all(target.join("Data"))?;std::fs::write(target.join("Data/settings.json"),b"preserved-user-data")?;std::fs::write(target.join("LyricIsland.exe"),b"old-isolated-executable")?;
 assert!(updater::apply(&stage,&target,&out.join("rollback"),3,None).is_err());assert_eq!(std::fs::read(target.join("LyricIsland.exe"))?,b"old-isolated-executable");assert_eq!(std::fs::read(target.join("Data/settings.json"))?,b"preserved-user-data");
 updater::apply(&stage,&target,&out.join("backup"),0,None)?;for relative in payload::relative_files(&stage)?{assert_eq!(lyric_archive::sha256_file(&stage.join(&relative))?,lyric_archive::sha256_file(&target.join(&relative))?,"{relative}");}assert_eq!(std::fs::read(target.join("Data/settings.json"))?,b"preserved-user-data");
 std::fs::write(out.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"success":true,"archive":format!("LyricIsland-{VERSION}-Portable.zip"),"manifest":true,"productionArchiveExtraction":true,"rollback":true,"appliedFileHashes":true,"preservesData":true,"scope":"isolated files only; does not launch, stop or update the user's application"}))?)?;println!("PASS: production OTA extraction, rollback, hashes and data preservation");Ok(())
}

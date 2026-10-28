#![windows_subsystem="windows"]
mod tutorial;mod tutorial_checks;
mod expressive_checks;mod polish_checks;
mod hybrid_checks;mod preferences_ui;mod appearance;mod island_settings;mod remote;mod draw;mod widgets;mod software_update;
mod timer_surface;mod tool_preferences;
mod timeline;
mod views;mod model;mod gpu;mod platform;mod motion;mod gestures;mod verify;mod shell;mod workflow;mod picker;
use std::path::PathBuf;
pub static STARTED:std::sync::OnceLock<std::time::Instant>=std::sync::OnceLock::new();
fn main(){STARTED.get_or_init(std::time::Instant::now);let args=std::env::args().collect::<Vec<_>>();if let Some(i)=args.iter().position(|a|a=="--verify"){let path=args.get(i+1).map(PathBuf::from).unwrap_or_else(||PathBuf::from("verification"));if let Err(e)=verify::run(&path){let _=std::fs::create_dir_all(&path);let _=std::fs::write(path.join("failure.txt"),e);std::process::exit(1);}return}
 let data=args.iter().position(|a|a=="--data-dir").and_then(|i|args.get(i+1)).map(PathBuf::from).or_else(||std::env::var_os("LYRICISLAND_NATIVE_DATA").map(PathBuf::from)).unwrap_or_else(default_data_dir);let demo=args.iter().any(|a|a=="--preview");let mut model=model::Model::new(data,demo);if let Some(i)=args.iter().position(|a|a=="--page"){if args.get(i+1).is_some_and(|s|s=="island"){model.page=model::Page::Island;}}if let Some(i)=args.iter().position(|a|a=="--island-tab"){model.island_tab=args.get(i+1).and_then(|s|s.parse().ok()).unwrap_or(0).min(4);}if args.iter().any(|a|a=="--tutorial"){if demo{model.dispatch(model::Action::TutorialStart);}else{model.tutorial_requested=true;}}shell::run(model);}

fn default_data_dir()->PathBuf{if let Some(base)=std::env::current_exe().ok().and_then(|p|p.parent().map(std::path::Path::to_path_buf)){if base.join("Portable.json").is_file(){return base.join("Data")}if let Ok(bytes)=std::fs::read(base.join("Installed.json")){if let Ok(value)=serde_json::from_slice::<serde_json::Value>(&bytes){if let Some(path)=value["dataDirectory"].as_str(){return PathBuf::from(path)}}}}dotisle_domain::preferences::data_dir()}

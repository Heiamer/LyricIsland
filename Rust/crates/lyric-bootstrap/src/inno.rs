//! Data lifecycle for the standard Inno Setup shell. No UI or package copying lives here.
use crate::{error::{Error,Result},installer,paths,payload,registry::{self,Key,RUN_KEY,RUN_VALUE,UNINSTALL_KEY}};
use serde_json::json;
use std::path::Path;

pub fn prepare(target:&Path,record:&Path,migration:Option<&Path>)->Result<()>{
 paths::normalize_target(&target.to_string_lossy())?;
 installer::reject_links(target)?;
 let previous=installer::stop_running(None);
 let source=migration.map(|p|p.to_path_buf()).or_else(||previous.map(|p|p.data));
 paths::write_json(record,&json!({"migration":source}))
}
pub fn finalize(target:&Path,record:&Path,data_override:Option<&Path>,key:&str)->Result<()>{
 if !key.starts_with(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DotIsle")||!key.ends_with("_is1"){return Err(Error("卸载登记路径无效".into()))}
 let isolated=key.contains(r"\DotIsle.QA");
 if isolated&&data_override.is_none(){return Err(Error("隔离安装必须指定独立数据目录".into()))}
 let old=payload::read_marker(target).ok();
 let data=data_override.map(paths::full_path).transpose()?.or_else(||old.as_ref().and_then(payload::marker_data)).unwrap_or_else(paths::owned_data_default);
 installer::reject_links(&data)?;
 if let Ok(value)=paths::read_json(record){if let Some(source)=value["migration"].as_str().filter(|s|!s.is_empty()).map(Path::new){
  if source.exists()&&!paths::same_path(source,&data){if data.join("settings.json").exists(){installer::copy_data(source,&data.join("MigrationBackups").join(paths::local_stamp(false)))?;}else{installer::copy_data(source,&data)?;}}
 }}
 installer::mark_data_owner(&data,target)?;
 let files=payload::relative_files(target)?.into_iter().filter(|f|!f.to_lowercase().starts_with("unins")&&!f.starts_with("Data/")).collect::<Vec<_>>();
 paths::write_json(&target.join(payload::INSTALLED_MARKER),&json!({"product":"DotIsle","version":payload::VERSION,"dataDirectory":data,"files":files,"installer":"Inno","uninstallKey":key}))?;
 if isolated{return Ok(())}
 paths::write_json(&paths::local_app_data().join("DotIsle/install.json"),&json!({"target":target}))?;
 if let Some(old)=Key::open(UNINSTALL_KEY)?{if old.string("InstallLocation").is_some_and(|p|paths::same_path(p,target)){drop(old);registry::delete_tree(UNINSTALL_KEY)?;}}
 Ok(())
}
pub fn cleanup(target:&Path,remove:bool)->Result<()>{
 let marker=payload::read_marker(target)?;
 if payload::marker_product(&marker)!="DotIsle"{return Err(Error("安装标识无效".into()))}
 let data=payload::marker_data(&marker).unwrap_or_default();
 if remove&&data.exists(){
  let owner=paths::read_json(&data.join(".dotisle-data.json"))?;
  if owner["installation"].as_str().is_none_or(|p|!paths::same_path(p,target)){return Err(Error("其他安装正在使用数据，请保留数据。".into()))}
 }
 installer::stop_owned_programs(target)?;
 if remove{installer::delete_owned_data(&data,target)?;}
 if marker["uninstallKey"].as_str().is_some_and(|k|k.contains(r"\DotIsle.QA")){return Ok(())}
 let pointer=paths::local_app_data().join("DotIsle/install.json");
 if let Ok(value)=paths::read_json(&pointer){if value["target"].as_str().is_some_and(|p|paths::same_path(p,target)){paths::remove_file_if_present(&pointer)?;}}
 if let Some(run)=Key::open(RUN_KEY)?{if run.string(RUN_VALUE).is_some_and(|p|p.contains(&target.to_string_lossy().to_string())){Key::create(RUN_KEY)?.delete_value(RUN_VALUE)?;}}
 Ok(())
}
pub fn repair(target:&Path,marker:&serde_json::Value)->Result<()>{
 let key=marker["uninstallKey"].as_str().ok_or_else(||Error("缺少标准卸载登记路径".into()))?;
 if !key.starts_with(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DotIsle")||!key.ends_with("_is1"){return Err(Error("卸载登记路径无效".into()))}
 let uninstall=target.join("unins000.exe");if !uninstall.is_file(){return Err(Error("标准卸载器缺失，请重新安装修复。".into()))}
 if Key::open(key)?.is_some(){return Ok(())}
 let key=Key::create(key)?;key.set_string("DisplayName","点屿 DotIsle")?;key.set_string("DisplayVersion",payload::marker_version(marker))?;
 key.set_string("InstallLocation",&target.to_string_lossy())?;key.set_string("DisplayIcon",&target.join("LyricIsland.exe").to_string_lossy())?;
 let command=format!("\"{}\"",uninstall.display());key.set_string("UninstallString",&command)?;key.set_string("QuietUninstallString",&(command+" /VERYSILENT /SUPPRESSMSGBOXES"))?;
 key.set_u32("NoModify",1)?;key.set_u32("NoRepair",1)?;Ok(())
}

//! Official Material Symbols Rounded, pinned in Assets/MaterialSymbols/REVISION.txt.
//! Apache-2.0 SVG outlines are compiled into the Rust binary; no font or runtime loader.
use std::{collections::HashMap,sync::OnceLock};
use vello::{Scene,kurbo::{Affine,BezPath},peniko::Fill};
use crate::draw::{color,Icon};
fn paths()->&'static HashMap<&'static str,Vec<BezPath>>{
 static PATHS:OnceLock<HashMap<&'static str,Vec<BezPath>>>=OnceLock::new();PATHS.get_or_init(||{
 let sources:&[(&str,&str)]=&[
  ("home",include_str!("../../../../Assets/MaterialSymbols/home.svg")),
  ("music_note",include_str!("../../../../Assets/MaterialSymbols/music_note.svg")),
  ("timer",include_str!("../../../../Assets/MaterialSymbols/timer.svg")),
  ("checklist",include_str!("../../../../Assets/MaterialSymbols/checklist.svg")),
  ("settings",include_str!("../../../../Assets/MaterialSymbols/settings.svg")),
  ("play_arrow",include_str!("../../../../Assets/MaterialSymbols/play_arrow.svg")),
  ("pause",include_str!("../../../../Assets/MaterialSymbols/pause.svg")),
  ("skip_next",include_str!("../../../../Assets/MaterialSymbols/skip_next.svg")),
  ("skip_previous",include_str!("../../../../Assets/MaterialSymbols/skip_previous.svg")),
  ("add",include_str!("../../../../Assets/MaterialSymbols/add.svg")),
  ("check",include_str!("../../../../Assets/MaterialSymbols/check.svg")),
  ("close",include_str!("../../../../Assets/MaterialSymbols/close.svg")),
  ("arrow_forward",include_str!("../../../../Assets/MaterialSymbols/arrow_forward.svg")),
  ("light_mode",include_str!("../../../../Assets/MaterialSymbols/light_mode.svg")),
  ("dark_mode",include_str!("../../../../Assets/MaterialSymbols/dark_mode.svg")),
  ("arrow_back",include_str!("../../../../Assets/MaterialSymbols/arrow_back.svg")),
  ("delete",include_str!("../../../../Assets/MaterialSymbols/delete.svg")),
  ("restart_alt",include_str!("../../../../Assets/MaterialSymbols/restart_alt.svg")),
  ("search",include_str!("../../../../Assets/MaterialSymbols/search.svg")),
  ("volume_up",include_str!("../../../../Assets/MaterialSymbols/volume_up.svg")),
  ("folder",include_str!("../../../../Assets/MaterialSymbols/folder.svg")),
  ("download",include_str!("../../../../Assets/MaterialSymbols/download.svg")),
  ("eco",include_str!("../../../../Assets/MaterialSymbols/eco.svg")),
  ("stop",include_str!("../../../../Assets/MaterialSymbols/stop.svg")),
  ("minimize",include_str!("../../../../Assets/MaterialSymbols/minimize.svg")),
  ("crop_square",include_str!("../../../../Assets/MaterialSymbols/crop_square.svg")),
  ("filter_none",include_str!("../../../../Assets/MaterialSymbols/filter_none.svg")),
  ("translate",include_str!("../../../../Assets/MaterialSymbols/translate.svg")),
  ("favorite",include_str!("../../../../Assets/MaterialSymbols/favorite.svg")),
  ("open_with",include_str!("../../../../Assets/MaterialSymbols/open_with.svg")),
  ("mouse",include_str!("../../../../Assets/MaterialSymbols/mouse.svg")),
  ("keyboard",include_str!("../../../../Assets/MaterialSymbols/keyboard.svg")),
  ("headphones",include_str!("../../../../Assets/MaterialSymbols/headphones.svg")),
  ("speaker",include_str!("../../../../Assets/MaterialSymbols/speaker.svg")),
  ("smartphone",include_str!("../../../../Assets/MaterialSymbols/smartphone.svg")),
  ("sports_esports",include_str!("../../../../Assets/MaterialSymbols/sports_esports.svg")),
  ("hard_drive",include_str!("../../../../Assets/MaterialSymbols/hard_drive.svg")),
  ("usb",include_str!("../../../../Assets/MaterialSymbols/usb.svg")),
  ("touchpad_mouse",include_str!("../../../../Assets/MaterialSymbols/touchpad_mouse.svg")),
  ("keyboard_capslock",include_str!("../../../../Assets/MaterialSymbols/keyboard_capslock.svg")),
  ("dialpad",include_str!("../../../../Assets/MaterialSymbols/dialpad.svg")),
  ("mic",include_str!("../../../../Assets/MaterialSymbols/mic.svg")),
  ("mic_off",include_str!("../../../../Assets/MaterialSymbols/mic_off.svg")),
 ];sources.iter().map(|(name,svg)|{let mut paths=vec![];let mut rest=*svg;while let Some(i)=rest.find(" d=\""){rest=&rest[i+4..];let Some(end)=rest.find('"')else{break};paths.push(BezPath::from_svg(&rest[..end]).expect("official symbol path"));rest=&rest[end+1..];}(*name,paths)}).collect()
 })
}
pub fn paint(scene:&mut Scene,name:&str,x:f64,y:f64,size:f64,rgb:u32)->bool{let Some(paths)=paths().get(name)else{return false};let tr=Affine::translate((x,y+size))*Affine::scale(size/960.);for path in paths{scene.fill(Fill::NonZero,tr,color(rgb),None,path);}true}
pub fn icon(scene:&mut Scene,icon:Icon,x:f64,y:f64,size:f64,rgb:u32)->bool{let name=match icon{
 Icon::Home=>"home",
 Icon::Music=>"music_note",
 Icon::Focus=>"timer",
 Icon::Todo=>"checklist",
 Icon::Settings=>"settings",
 Icon::Play=>"play_arrow",
 Icon::Pause=>"pause",
 Icon::Next=>"skip_next",
 Icon::Prev=>"skip_previous",
 Icon::Plus=>"add",
 Icon::Check=>"check",
 Icon::Close=>"close",
 Icon::Arrow=>"arrow_forward",
 Icon::Sun=>"light_mode",
 Icon::Moon=>"dark_mode",
 Icon::Back=>"arrow_back",
 Icon::Trash=>"delete",
 Icon::Reset=>"restart_alt",
 Icon::Search=>"search",
 Icon::Volume=>"volume_up",
 Icon::Folder=>"folder",
 Icon::Import=>"download",
 Icon::Leaf=>"eco",
 Icon::Stop=>"stop",
 Icon::Minimize=>"minimize",
 Icon::Maximize=>"crop_square",
 Icon::Restore=>"filter_none",
 Icon::Translate=>"translate",
 Icon::Heart=>"favorite",
 Icon::Move=>"open_with",
 Icon::Island=>return false,
 };paint(scene,name,x,y,size,rgb)
}

#[cfg(test)]mod tests{
 use super::*;use vello::kurbo::Shape;
 #[test]fn every_bundled_material_symbol_has_valid_visible_paths(){assert_eq!(paths().len(),43);for(name,paths)in paths(){assert!(!paths.is_empty(),"{name}");for path in paths{let bounds=path.bounding_box();assert!(bounds.width()>0.&&bounds.height()>0.,"{name}");assert!(bounds.x0>=-1.&&bounds.x1<=961.&&bounds.y0>=-961.&&bounds.y1<=1.,"{name}: {bounds:?}");}}}
}

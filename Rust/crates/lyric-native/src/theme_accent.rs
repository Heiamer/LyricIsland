//! Functional card controls follow the settings appearance; music keeps its Monet palette.
use serde_json::Value;
use std::{cell::RefCell,time::{Duration,Instant}};
#[derive(Clone,Copy)]pub struct Accent{pub fill:u32,pub hover:u32,pub pressed:u32,pub ink:u32}
thread_local!{static SYSTEM:RefCell<Option<(Instant,bool)>>=const{RefCell::new(None)};}
pub fn system_dark()->bool{SYSTEM.with(|cache|{
    let mut cache=cache.borrow_mut();if let Some((at,dark))=*cache{if at.elapsed()<Duration::from_secs(1){return dark}}
    let dark=windows::UI::ViewManagement::UISettings::new().and_then(|s|s.GetColorValue(windows::UI::ViewManagement::UIColorType::Background)).map(|c|(c.R as u32+c.G as u32+c.B as u32)<384).unwrap_or(false);
    *cache=Some((Instant::now(),dark));dark
})}
pub fn resolve(settings:&Value)->Accent{
    let dark=match settings["SettingsTheme"].as_str().unwrap_or("System"){"Dark"=>true,"Light"=>false,_=>system_dark()};
    let key=if dark{"SettingsDarkAccentColor"}else{"SettingsLightAccentColor"};
    let fill=settings[key].as_str().filter(|s|s.len()==7&&s.starts_with('#')).and_then(|s|u32::from_str_radix(&s[1..],16).ok()).unwrap_or(if dark{0x6ea8ff}else{0x2563eb});
    let contrast=crate::cover_color::contrast;
    let ink=if contrast(fill,0xffffff)>=contrast(fill,0){0xffffffff}else{0xff000000};
    let variant=|toward,t|{let c=crate::cover_color::mix(fill,toward,t);0xff000000|if contrast(c,ink)>=4.5{c}else{fill}};
    Accent{fill:0xff000000|fill,hover:variant(0xffffff,0.08),pressed:variant(0,0.16),ink}
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn independent_theme_colours_and_readable_button_states(){for dark in [false,true]{for hex in ["#000000","#FFFFFF","#FFFF00","#777777","#2563EB"]{
        let s=serde_json::json!({"SettingsTheme":if dark{"Dark"}else{"Light"},"SettingsLightAccentColor":hex,"SettingsDarkAccentColor":hex});let a=resolve(&s);
        for c in [a.fill,a.hover,a.pressed]{assert!(crate::cover_color::contrast(c,a.ink)>=4.5);}
    }}let s=serde_json::json!({"SettingsTheme":"Light","SettingsLightAccentColor":"#2563EB","SettingsDarkAccentColor":"#6EA8FF"});assert_eq!(resolve(&s).fill,0xff2563eb);}
}

thread_local!{static SYSTEM_ACCENT:RefCell<Option<(Instant,Option<u32>)>>=const{RefCell::new(None)};}
pub fn system_accent()->Option<u32>{SYSTEM_ACCENT.with(|cache|{let mut cache=cache.borrow_mut();if let Some((at,c))=*cache{if at.elapsed()<Duration::from_secs(1){return c}}let c=windows::UI::ViewManagement::UISettings::new().and_then(|s|s.GetColorValue(windows::UI::ViewManagement::UIColorType::Accent)).ok().map(|c|(c.R as u32)<<16|(c.G as u32)<<8|c.B as u32);*cache=Some((Instant::now(),c));c})}

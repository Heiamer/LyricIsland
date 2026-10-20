use serde_json::{Value,json};
pub fn defaults()->Value {let mut defaults=json!({"AutoCheckUpdatesEnabled":true,"LyricColorFromCover":true,"DeviceRemindersEnabled":true,"DeviceSoundEnabled":true,"DeviceSound":"Connect","DeviceSoundPath":"","DeviceVolume":55.,"HeadphoneConnectionRemindersEnabled":true,"SpeakerConnectionRemindersEnabled":true,"GamepadConnectionRemindersEnabled":true,"MouseConnectionRemindersEnabled":false,"KeyboardConnectionRemindersEnabled":false,"DeviceDisconnectRemindersEnabled":true,"SystemStateHintsEnabled":true,"SystemStateHintPlacement":"Island","CapsLockHintEnabled":true,"NumLockHintEnabled":true,"MicrophoneMuteHintEnabled":true,"TouchpadHintEnabled":true,"SwipeActivities":true,"AutoCompactMaxWidth":420.,"TodoDefaultColor":"#F3D79B","TodoRemindersEnabled":true,"TodoIslandEnabled":true,"AutoCompactWidth":true,"CompactTranslation":true,"ChineseTwoLine":true,"NotificationSoundEnabled":true,"TodoSoundEnabled":true,"NotificationSound":"Soft","TodoSound":"Bell","NotificationSoundPath":"","TodoSoundPath":"","QuickReminderHotkey":"","FocusCountUp":false,"FocusPauseOnLock":false,"AppEntryRemindersEnabled":false,"AppUsageRemindersEnabled":false,"NotificationVolume":35.,"TodoVolume":45.,"CompactWidth":260.,"FontSize":12.5,"Floating":false,"Translation":true,"Animation":true,"IslandFluidMotion":true,"MotionPreferenceVersion":1,"HoverExpand":false,"ClickAnywhereExpand":true,"NotificationsEnabled":true,"PauseNotifications":false,"NotificationClickOpenApp":true,"MusicClickOpenApp":false,"MusicExpandedLayout":"SideBySide","NotificationWhitelistEnabled":false,"NotificationBlacklistEnabled":false,"NotificationBlockedApps":[],"NotificationAllowedApps":["QQNT","QQ"],"NotificationHistoryEnabled":true,"NotificationHistoryDays":7,"WordHighlight":true,"AlwaysOnTop":true,"AutoStart":true,"OnlineSongInfo":true,"CoverBackground":true,"HideWhenPaused":true,"IgnoredSources":["Browser"],"PlayerFilterVersion":2,"DefaultsOnboardingVersion":0,"ExpandedTextColor":"#F4F6F8","ExpandedComponentColor":"#B1E8D1","LyricProgressColor":"#A9E6CD","SettingsTheme":"System","ControlCenterStartPage":"overview","WorkflowModules":[{"id":"builtin-thinking","name":"整理思路","minutes":5,"kind":0},{"id":"builtin-focus","name":"专注","minutes":25,"kind":1},{"id":"builtin-rest","name":"休息","minutes":5,"kind":2},{"id":"builtin-review","name":"复盘","minutes":10,"kind":3},{"id":"builtin-move","name":"活动一下","minutes":3,"kind":4},{"id":"builtin-custom","name":"自定义","minutes":10,"kind":5}],"CenterX":0.5,"TopFraction":0.08,"Display":"","OffsetMilliseconds":0,"LocalFolders":[],"Motion":crate::motion::Options::default()});defaults["SettingsLightAccentColor"]=json!("#2563EB");defaults["SettingsDarkAccentColor"]=json!("#6EA8FF");defaults["IslandMaterial"]=json!("Acrylic");defaults["SwipeUpHide"]=json!(true);defaults["GestureTutorialSeen"]=json!(false);defaults}
pub fn load(path:&std::path::Path)->Value {
    let stored=lyric_backend::io::read_settings(path);let mut s=defaults();
    let legacy_material=stored.get("IslandGlassMaterial").and_then(Value::as_bool);
    let has_material=stored.get("IslandMaterial").and_then(Value::as_str).is_some();
    if let Some(fields)=stored.as_object(){for (k,v) in fields{if k=="Motion"{if let Ok(o)=serde_json::from_value::<crate::motion::Options>(v.clone()){s[k]=serde_json::to_value(o).unwrap();}}else{s[k]=v.clone();}}}
    if let Some(fields)=s.as_object_mut(){fields.remove("FollowWindowsDnd");fields.remove("NotificationCompatibility");fields.remove("CardMaterial");fields.remove("IslandGlassMaterial");}
    // 2.0.x stored a boolean for the one optional frosted material. Carry that choice into the
    // three-value setting once; a file that already has IslandMaterial wins over the legacy key.
    if !has_material{s["IslandMaterial"]=json!(match legacy_material{Some(true)=>"Acrylic",Some(false)=>"Classic",None=>"Acrylic"});}
    // 1.5 ships three previously opt-in features as defaults. An upgrade persists every key, so a
    // stored `false` is indistinguishable from a deliberate opt-out; migrate once, then respect it.
    if s["DefaultsOnboardingVersion"].as_u64().unwrap_or(0)<1{for k in ["SwipeActivities","NotificationsEnabled","DeviceRemindersEnabled"]{s[k]=json!(true);}s["DefaultsOnboardingVersion"]=json!(1);}
    // 2.0.0 shipped mouse and keyboard reminders inside the device bundle, so every install that
    // ran it persisted `true` without anyone choosing it. A mouse that sleeps and wakes, or a
    // keyboard behind a KVM, reconnects many times a day — far past the point where a banner is
    // still information. 2.0.1 defaults them off and carries existing installs over once; a user
    // who wants them turns them back on and the marker keeps that choice.
    if s["DeviceDefaultsVersion"].as_u64().unwrap_or(0)<1{
        for k in ["MouseConnectionRemindersEnabled","KeyboardConnectionRemindersEnabled"]{s[k]=json!(false);}
        s["DeviceDefaultsVersion"]=json!(1);
    }
    // 2.0.2 and earlier asked "which players may respond?" and shipped six of ten switched on, so a
    // player nobody had heard of — 汽水音乐, PotPlayer, 洛雪 — stayed silent until the user found the
    // list. The filter is now expressed the other way round: everything responds, and the entries
    // here are the ones that do not.
    migrate_player_filter(&mut s);
    // Four legacy per-kind flags are aliases of the single system-hint preference.
    let enabled=s["SystemStateHintsEnabled"].as_bool().unwrap_or(true);for key in ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"]{s[key]=json!(enabled);}
    // Upgrade only the old factory timings; keep deliberate custom values and startup opt-outs.
    if s["ThrowMotionVersion"].as_u64().unwrap_or(0)<1{
        if s["Motion"]["Duration"]==json!(360.){s["Motion"]["Duration"]=json!(480.);}
        if s["Motion"]["CollapseDuration"]==json!(300.){s["Motion"]["CollapseDuration"]=json!(440.);}
        s["ThrowMotionVersion"]=json!(1);
    }
    if !["Classic","Acrylic","LiquidGlass"].contains(&s["IslandMaterial"].as_str().unwrap_or("")){s["IslandMaterial"]=json!("Acrylic");}
    for (key,fallback) in [("SettingsLightAccentColor","#2563EB"),("SettingsDarkAccentColor","#6EA8FF")]{
        let valid=s[key].as_str().is_some_and(|v|v.len()==7&&v.starts_with('#')&&v[1..].bytes().all(|b|b.is_ascii_hexdigit()));
        if !valid{s[key]=json!(fallback);}
    }
    // Validate persisted numbers before they enter geometry or a spring denominator.
    for (k,min,max,fallback) in [("AutoCompactMaxWidth",140.,480.,420.),("CompactWidth",140.,480.,260.),("FontSize",11.,17.,12.5),("CenterX",0.,1.,0.5),("TopFraction",0.,1.,0.08)]{s[k]=json!(s[k].as_f64().filter(|v|v.is_finite()).unwrap_or(fallback).clamp(min,max));}
    for (k,min,max,fallback) in [("Gap",0.,24.,8.),("Duration",250.,1200.,480.),("CollapseDuration",150.,700.,440.),("Radius",8.,48.,30.)]{s["Motion"][k]=json!(s["Motion"][k].as_f64().unwrap_or(fallback).clamp(min,max));}
    for (k,min,max,fallback) in [("Stiffness",100.,1000.,420.),("Damping",10.,80.,30.),("Mass",0.3,2.,0.8)]{s["Motion"]["Spring"][k]=json!(s["Motion"]["Spring"][k].as_f64().unwrap_or(fallback).clamp(min,max));}
    if !["Separated","SideBySide","LyricFocus"].contains(&s["MusicExpandedLayout"].as_str().unwrap_or("")){s["MusicExpandedLayout"]=json!("SideBySide");}
    if !["Island","Bottom"].contains(&s["SystemStateHintPlacement"].as_str().unwrap_or("")){s["SystemStateHintPlacement"]=json!("Island");}
    s
}
/// Convert a 2.0.2 allow-list into the exclusion list that replaced it.
///
/// Only the ten players that list ever offered are considered, so a player introduced since (汽水,
/// PotPlayer, 洛雪…) starts responding instead of being silently excluded by an old file. Anything
/// the user had switched off stays off, which is the one part of the old choice worth carrying over.
fn migrate_player_filter(s: &mut Value) {
    let Some(allowed) = s.get("AllowedSources").and_then(Value::as_array).cloned() else { return };
    let legacy = ["Netease","QQMusic","Kugou","Kuwo","Spotify","Foobar2000","MusicBee","AIMP","Browser"];
    let ignored = legacy.iter().copied()
        .filter(|id| !allowed.iter().any(|value| value.as_str().is_some_and(|value| value.eq_ignore_ascii_case(id))))
        .collect::<Vec<_>>();
    s["IgnoredSources"] = json!(ignored);
    s["PlayerFilterVersion"] = json!(2);
    if let Some(fields) = s.as_object_mut() { fields.remove("AllowedSources"); }
}

pub fn validate_setting(v:Value)->Result<Value,String>{
    let key=v["key"].as_str().ok_or("missing key")?;let value=&v["value"];let mut settings=v["settings"].clone();if !settings.is_object(){return Err("invalid settings".into())}
    let range=match key{"DeviceVolume"|"NotificationVolume"|"TodoVolume"=>Some((0.,100.)),"AutoCompactMaxWidth"|"CompactWidth"=>Some((140.,480.)),"FontSize"=>Some((11.,17.)),"OffsetMilliseconds"=>Some((-5000.,5000.)),"NotificationHistoryDays"=>Some((1.,30.)),"MotionGap"=>Some((0.,24.)),"MotionDuration"=>Some((250.,1200.)),"MotionCollapseDuration"=>Some((150.,700.)),"MotionRadius"=>Some((8.,48.)),"SpringStiffness"=>Some((100.,1000.)),"SpringDamping"=>Some((10.,80.)),"SpringMass"=>Some((0.3,2.)),_=>None};
    let value=if let Some((min,max))=range{let n=value.as_f64().ok_or("invalid number")?;if !n.is_finite()||n<min||n>max{return Err("number outside range".into())}if key=="OffsetMilliseconds"{json!(n.round() as i32)}else if key=="NotificationHistoryDays"{json!(n.round() as u32)}else{json!(n)}}else if key=="IslandMaterial"{let s=value.as_str().ok_or("invalid material")?;if !["Classic","Acrylic","LiquidGlass"].contains(&s){return Err("invalid material".into())}json!(s)}
    else if ["LyricColorFromCover","AutoCheckUpdatesEnabled","DeviceRemindersEnabled","DeviceSoundEnabled","SwipeActivities","SwipeUpHide","GestureTutorialSeen","TodoRemindersEnabled","TodoIslandEnabled","AutoCompactWidth","CompactTranslation","ChineseTwoLine","NotificationSoundEnabled","TodoSoundEnabled","Floating","Translation","Animation","IslandFluidMotion","HoverExpand","ClickAnywhereExpand","WordHighlight","AlwaysOnTop","AutoStart","OnlineSongInfo","HideWhenPaused","NotificationsEnabled","PauseNotifications","NotificationClickOpenApp","MusicClickOpenApp","NotificationBlacklistEnabled","NotificationWhitelistEnabled","NotificationHistoryEnabled","CoverBackground","FocusCountUp","FocusPauseOnLock","AppEntryRemindersEnabled","AppUsageRemindersEnabled","HeadphoneConnectionRemindersEnabled","SpeakerConnectionRemindersEnabled","GamepadConnectionRemindersEnabled","MouseConnectionRemindersEnabled","KeyboardConnectionRemindersEnabled","DeviceDisconnectRemindersEnabled","SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"].contains(&key){json!(value.as_bool().ok_or("invalid bool")?)}
    else if key=="DeviceSound"||key=="NotificationSound"||key=="TodoSound"{let name=value.as_str().ok_or("invalid sound")?;if !["Soft","Glass","Bell","Connect"].contains(&name){return Err("invalid sound".into())}json!(name)}
    else if key=="DeviceSoundPath"||key=="NotificationSoundPath"||key=="TodoSoundPath"{let path=value.as_str().ok_or("invalid path")?;crate::sound::validate_file(path)?;json!(path)}
    // The settings window's own appearance. The capsule is dark by design, so this never reaches
    // the renderer — it is persisted here purely so the choice survives a restart.
    else if key=="SettingsTheme"{let s=value.as_str().ok_or("invalid theme")?;if !["System","Light","Dark"].contains(&s){return Err("invalid theme".into())}value.clone()}
    // Any modifier+key combination RegisterHotKey can take, not just the three presets: the
    // recorder in 设置 · 常规 writes whatever the user pressed. Stored in canonical spelling so
    // two runs of the same binding cannot produce two different strings.
    else if key=="QuickReminderHotkey"{let s=value.as_str().ok_or("invalid hotkey")?;if s.is_empty(){json!("")}else{match lyric_core::hotkey::normalize(s){Some(canonical)=>json!(canonical),None=>return Err("这个组合不能用作全局快捷键：需要至少一个修饰键（Ctrl / Alt / Shift / Win）加一个字母、数字或 F1–F24".into())}}}
    // The 流程 module library is user data, not a preset list. Every entry carries a stable id so
    // renames and duplicates cannot collide, and an explicit kind so a renamed module keeps its icon.
    else if key=="WorkflowModules"{
        let list=value.as_array().ok_or("invalid modules")?;
        if list.len()>24{return Err("模块库最多 24 个，请先删除不用的模块".into())}
        let mut out=Vec::with_capacity(list.len());let mut ids=std::collections::HashSet::new();
        for item in list{
            let id=item["id"].as_str().ok_or("invalid module id")?;
            if id.is_empty()||id.len()>64||!id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'||c=='_'){return Err("模块标识无效".into())}
            if !ids.insert(id.to_owned()){return Err("模块标识重复".into())}
            let name=item["name"].as_str().map(str::trim).ok_or("invalid module name")?;
            let count=name.chars().count();if count==0||count>40||name.chars().any(char::is_control){return Err("模块名称需要 1–40 个字".into())}
            let minutes=item["minutes"].as_u64().ok_or("invalid module minutes")?;
            if !(1..=240).contains(&minutes){return Err("模块时长需要 1–240 分钟".into())}
            let kind=item["kind"].as_u64().ok_or("invalid module kind")?;
            if kind>5{return Err("模块外观类型无效".into())}
            out.push(json!({"id":id,"name":name,"minutes":minutes,"kind":kind}));
        }
        json!(out)
    }
    else if key=="ControlCenterStartPage"{let s=value.as_str().ok_or("invalid start page")?;if !["overview","notifications","todos","timers","island","settings","about"].contains(&s){return Err("invalid start page".into())}json!(s)}
    // Layout is a layout type, not a pair of booleans: exactly two accepted strings.
    else if key=="SystemStateHintPlacement"{let s=value.as_str().ok_or("invalid placement")?;if !["Island","Bottom"].contains(&s){return Err("invalid placement".into())}json!(s)}
    else if key=="MusicExpandedLayout"{let s=value.as_str().ok_or("invalid layout")?;if !["Separated","SideBySide","LyricFocus"].contains(&s){return Err("invalid layout".into())}json!(s)}
    else if key.ends_with("Color"){let s=value.as_str().ok_or("invalid color")?;if !["TodoDefaultColor","ExpandedTextColor","ExpandedComponentColor","LyricProgressColor","SettingsLightAccentColor","SettingsDarkAccentColor"].contains(&key)||!s.starts_with('#')||s.len()!=7||!s[1..].bytes().all(|b|b.is_ascii_hexdigit()){return Err("invalid color".into())}json!(s.to_uppercase())}
    else if key=="IgnoredSources"||key=="AllowedSources"||key=="NotificationAllowedApps"||key=="NotificationBlockedApps"{let a=value.as_array().ok_or("invalid source list")?;if a.len()>32{return Err("source list too long".into())}let mut out=vec![];for s in a{let s=s.as_str().ok_or("invalid source")?.trim();if s.is_empty()||s.encode_utf16().count()>256{return Err("invalid source".into())}if !out.iter().any(|v:&String|v.eq_ignore_ascii_case(s)){out.push(s.to_string())}}json!(out)}else{return Err("unknown setting".into())};
    if ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"].contains(&key){for alias in ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"]{settings[alias]=value.clone();}return Ok(settings)}
    if let Some(name)=key.strip_prefix("Motion"){settings["Motion"][name]=value;}else if let Some(name)=key.strip_prefix("Spring"){settings["Motion"]["Spring"][name]=value;}else{settings[key]=value;}Ok(settings)
}



#[cfg(test)]mod unified_osd_tests{use super::*;
#[test]fn one_osd_switch_controls_all_legacy_aliases(){let s=validate_setting(json!({"key":"SystemStateHintsEnabled","value":false,"settings":defaults()})).unwrap();for key in ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"]{assert_eq!(s[key],false);}let s=validate_setting(json!({"key":"CapsLockHintEnabled","value":true,"settings":s})).unwrap();assert_eq!(s["SystemStateHintsEnabled"],true);assert_eq!(s["TouchpadHintEnabled"],true);assert!(validate_setting(json!({"key":"SystemStateHintsEnabled","value":"true","settings":s})).is_err());}
}

#[cfg(test)]mod workflow_module_tests{use super::*;
fn accept(value:Value)->Result<Value,String>{validate_setting(json!({"key":"WorkflowModules","value":value,"settings":defaults()}))}
#[test]fn the_library_ships_the_six_built_in_modules(){let modules=defaults()["WorkflowModules"].clone();let list=modules.as_array().unwrap();assert_eq!(list.len(),6);assert_eq!(list[1]["name"],"专注");assert_eq!(list[1]["minutes"],25);let ids=list.iter().map(|m|m["id"].as_str().unwrap()).collect::<std::collections::HashSet<_>>();assert_eq!(ids.len(),6);}
#[test]fn modules_are_normalized_to_id_name_minutes_and_kind(){let s=accept(json!([{"id":"user-1","name":" 专注 ","minutes":25,"kind":1,"extra":"dropped"}])).unwrap();assert_eq!(s["WorkflowModules"],json!([{"id":"user-1","name":"专注","minutes":25,"kind":1}]));}
#[test]fn an_empty_library_is_valid_because_every_module_can_be_deleted(){assert_eq!(accept(json!([])).unwrap()["WorkflowModules"],json!([]));}
#[test]fn duplicate_ids_are_rejected_even_when_names_differ(){assert!(accept(json!([{"id":"same","name":"一","minutes":5,"kind":0},{"id":"same","name":"二","minutes":5,"kind":0}])).is_err());}
#[test]fn names_minutes_and_kinds_are_bounded(){for module in [json!({"id":"a","name":"","minutes":5,"kind":0}),json!({"id":"a","name":"   ","minutes":5,"kind":0}),json!({"id":"a","name":"名","minutes":0,"kind":0}),json!({"id":"a","name":"名","minutes":241,"kind":0}),json!({"id":"a","name":"名","minutes":5,"kind":6}),json!({"id":"bad id","name":"名","minutes":5,"kind":0}),json!({"id":"a","name":"名".repeat(41),"minutes":5,"kind":0})]{assert!(accept(json!([module])).is_err());}}
#[test]fn the_capacity_limit_is_twenty_four(){let modules=(0..25).map(|i|json!({"id":format!("m-{i}"),"name":format!("模块{i}"),"minutes":10,"kind":5})).collect::<Vec<_>>();assert!(accept(json!(modules)).is_err());}
}

#[cfg(test)]mod player_filter_tests{
use super::*;
/// A default 2.0.2 install allowed six of ten. Upgrading keeps exactly that behaviour: the four it
/// did not allow stay silent, and the six it did keep responding.
#[test]fn an_upgraded_install_keeps_what_it_had_switched_off(){
    let mut s=defaults();
    s["AllowedSources"]=json!(["Netease","QQMusic","Kugou","Kuwo","Spotify","TwilightEcho"]);
    migrate_player_filter(&mut s);
    assert_eq!(s["IgnoredSources"],json!(["Foobar2000","MusicBee","AIMP","Browser"]));
    assert!(s.get("AllowedSources").is_none(),"the old key must not survive to fight the new one");
    assert_eq!(s["PlayerFilterVersion"],2);
    // TwilightEcho is no longer a source. An old file that mentions it must not put a name the app
    // cannot recognise back into the list.
    assert!(!s["IgnoredSources"].as_array().unwrap().iter().any(|id|id=="TwilightEcho"));
}
/// The whole point of the change: a player that list never mentioned responds without being added.
#[test]fn a_fresh_install_only_silences_the_browser(){
    let s=defaults();
    assert_eq!(s["IgnoredSources"],json!(["Browser"]));
    assert!(s.get("AllowedSources").is_none());
}
/// The verification harnesses pin the island to a fake session by writing the old key; the migration
/// has to silence every real player for that pin to still mean what it did.
#[test]fn a_pinned_verification_install_silences_every_built_in_player(){
    let mut s=defaults();
    s["AllowedSources"]=json!(["LyricIsland.Verify13"]);
    migrate_player_filter(&mut s);
    assert_eq!(s["IgnoredSources"].as_array().map(Vec::len),Some(9));
    assert!(!s["IgnoredSources"].as_array().unwrap().iter().any(|id|id=="LyricIsland.Verify13"));
}
/// Idempotent: the conversion runs on every load until a save drops the old key, and a second pass
/// must not reinterpret its own output.
#[test]fn running_the_conversion_twice_changes_nothing(){
    let mut s=defaults();
    s["AllowedSources"]=json!(["Netease"]);
    migrate_player_filter(&mut s);
    let once=s["IgnoredSources"].clone();
    migrate_player_filter(&mut s);
    assert_eq!(s["IgnoredSources"],once);
}
}

#[cfg(test)] mod material_tests {
    use super::*;
    #[test] fn material_defaults_to_acrylic_and_accepts_three_values() {
        let s=defaults();assert_eq!(s["AutoStart"],true);assert_eq!(s["IslandMaterial"],"Acrylic");
        for value in ["Classic","Acrylic","LiquidGlass"]{
            let updated=validate_setting(json!({"key":"IslandMaterial","value":value,"settings":s.clone()})).unwrap();
            assert_eq!(updated["IslandMaterial"],value);
        }
        assert!(validate_setting(json!({"key":"IslandMaterial","value":"acrylic","settings":s})).is_err());
    }
    #[test] fn legacy_glass_flag_migrates_once() {
        let p=std::env::temp_dir().join(format!("dotisle-material-upgrade-{}.json",std::process::id()));
        std::fs::write(&p,r#"{"AutoStart":false,"IslandGlassMaterial":true,"Motion":{"Duration":600,"CollapseDuration":550}}"#).unwrap();
        let s=load(&p);assert_eq!(s["IslandMaterial"],"Acrylic");assert_eq!(s["AutoStart"],false);assert_eq!(s["Motion"]["Duration"],600.);
        std::fs::write(&p,r#"{"IslandGlassMaterial":false}"#).unwrap();
        let s=load(&p);assert_eq!(s["IslandMaterial"],"Classic");
        std::fs::write(&p,r#"{"IslandMaterial":"LiquidGlass","IslandGlassMaterial":false}"#).unwrap();
        let s=load(&p);assert_eq!(s["IslandMaterial"],"LiquidGlass");
        let _=std::fs::remove_file(p);
    }
}

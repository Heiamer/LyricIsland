use serde_json::{Value,json};
pub fn defaults()->Value {let mut defaults=json!({"AutoCheckUpdatesEnabled":true,"LyricColorFromCover":true,"DeviceRemindersEnabled":true,"DeviceSoundEnabled":true,"DeviceSound":"Connect","DeviceSoundPath":"","DeviceVolume":55.,"HeadphoneConnectionRemindersEnabled":true,"SpeakerConnectionRemindersEnabled":true,"GamepadConnectionRemindersEnabled":true,"MouseConnectionRemindersEnabled":false,"KeyboardConnectionRemindersEnabled":false,"DeviceDisconnectRemindersEnabled":true,"SystemStateHintsEnabled":true,"SystemStateHintPlacement":"Island","CapsLockHintEnabled":true,"NumLockHintEnabled":true,"MicrophoneMuteHintEnabled":true,"TouchpadHintEnabled":true,"SwipeActivities":true,"AutoCompactMaxWidth":420.,"TodoDefaultColor":"#F3D79B","TodoRemindersEnabled":true,"TodoIslandEnabled":true,"AutoCompactWidth":true,"CompactTranslation":true,"ChineseTwoLine":true,"NotificationSoundEnabled":true,"TodoSoundEnabled":true,"NotificationSound":"Soft","TodoSound":"Bell","NotificationSoundPath":"","TodoSoundPath":"","QuickReminderHotkey":"","FocusCountUp":false,"FocusPauseOnLock":false,"AppEntryRemindersEnabled":false,"AppUsageRemindersEnabled":false,"NotificationVolume":35.,"TodoVolume":45.,"CompactWidth":260.,"FontSize":14.,"Floating":false,"Translation":true,"TranslationButton":true,"Animation":true,"HoverExpand":false,"ClickAnywhereExpand":true,"NotificationsEnabled":true,"PauseNotifications":false,"NotificationClickOpenApp":true,"MusicClickOpenApp":false,"MusicExpandedLayout":"SideBySide","NotificationWhitelistEnabled":false,"NotificationBlacklistEnabled":false,"NotificationBlockedApps":[],"NotificationAllowedApps":["QQNT","QQ"],"NotificationHistoryEnabled":true,"NotificationHistoryDays":7,"WordHighlight":true,"AlwaysOnTop":true,"AutoStart":true,"OnlineSongInfo":true,"CoverBackground":false,"IslandMonetPreferenceVersion":1,"HideWhenPaused":true,"IgnoredSources":["Browser"],"PlayerFilterVersion":2,"DefaultsOnboardingVersion":0,"ExpandedTextColor":"#F4F6F8","ExpandedComponentColor":"#B1E8D1","LyricProgressColor":"#A9E6CD","SettingsTheme":"System","ControlCenterStartPage":"overview","WorkflowModules":[{"id":"builtin-thinking","name":"整理思路","minutes":5,"kind":0},{"id":"builtin-focus","name":"专注","minutes":25,"kind":1},{"id":"builtin-rest","name":"休息","minutes":5,"kind":2},{"id":"builtin-review","name":"复盘","minutes":10,"kind":3},{"id":"builtin-move","name":"活动一下","minutes":3,"kind":4},{"id":"builtin-custom","name":"自定义","minutes":10,"kind":5}],"CenterX":0.5,"TopFraction":0.08,"Display":"","OffsetMilliseconds":0,"NeteaseCdpPort":"9223","LocalFolders":[],"Motion":crate::motion::Options::default()});defaults["ControlCenterColorSource"]=json!("Content");defaults["IslandColorSource"]=json!("Content");defaults["IslandDynamicSeedColor"]=json!("#B1E8D1");defaults["DynamicColorHighContrast"]=json!(false);defaults["ControlCenterMonetEnabled"]=json!(true);defaults["IslandFontFamily"]=json!("Noto Sans SC");defaults["SettingsLightAccentColor"]=json!("#2563EB");defaults["SettingsDarkAccentColor"]=json!("#6EA8FF");defaults["IslandMaterial"]=json!("Acrylic");defaults["IslandSizeMode"]=json!("Classic");defaults["ClassicCapsuleMaterial"]=json!("Acrylic");defaults["MiniMaterial"]=json!("Acrylic");defaults["MiniExpandedMaterial"]=json!("Acrylic");defaults["MiniAutoContrast"]=json!(true);
// Acrylic keeps the artwork as a frosted layer under the card, tuned per surface. The strip
// starts at zero so an existing installation looks exactly as it did before the sliders existed.
defaults["AcrylicCardFrost"]=json!(40.);defaults["AcrylicCardOpacity"]=json!(60.);
defaults["AcrylicCapsuleFrost"]=json!(40.);defaults["AcrylicCapsuleOpacity"]=json!(0.);
defaults["ExpandedMonetEnabled"]=json!(true);defaults["ExpandedCoverOverlay"]=json!(true);
defaults["AcrylicAdjustTarget"]=json!("Card");
defaults["SoundLibraryVersion"]=json!(2);for p in ["Notification","Todo","Device"]{defaults[format!("{p}SoundSource")]=json!("Builtin");}defaults["CapsuleCornerRadius"]=Value::Null;defaults["SwipeUpHide"]=json!(true);defaults["GestureTutorialSeen"]=json!(false);defaults}
pub fn load(path:&std::path::Path)->Value {
    let stored=lyric_backend::io::read_settings(path);let mut s=defaults();
    let legacy_material=stored.get("IslandGlassMaterial").and_then(Value::as_bool);
    let has_material=stored.get("IslandMaterial").and_then(Value::as_str).is_some();
    if let Some(fields)=stored.as_object(){for (k,v) in fields{if k=="Motion"{if let Ok(o)=serde_json::from_value::<crate::motion::Options>(v.clone()){s[k]=serde_json::to_value(o).unwrap();}}else{s[k]=v.clone();}}}
    if let Some(fields)=s.as_object_mut(){fields.remove("IslandFluidMotion");fields.remove("MotionPreferenceVersion");fields.remove("FollowWindowsDnd");fields.remove("NotificationCompatibility");fields.remove("CardMaterial");fields.remove("IslandGlassMaterial");fields.remove("MusicControlTheme");fields.remove("AlcoveThemeUnlocked");}
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
    s["IslandFontFamily"]=json!("Noto Sans SC");
    migrate_player_filter(&mut s);
    // Four legacy per-kind flags are aliases of the single system-hint preference.
    let enabled=s["SystemStateHintsEnabled"].as_bool().unwrap_or(true);for key in ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"]{s[key]=json!(enabled);}
    // Upgrade only the old factory timings; keep deliberate custom values and startup opt-outs.
    if s["ThrowMotionVersion"].as_u64().unwrap_or(0)<1{
        if s["Motion"]["Duration"]==json!(360.){s["Motion"]["Duration"]=json!(480.);}
        if s["Motion"]["CollapseDuration"]==json!(300.){s["Motion"]["CollapseDuration"]=json!(440.);}
        s["ThrowMotionVersion"]=json!(1);
    }
    // Retire the implicit cover tint once; subsequent explicit opt-ins survive restarts.
    if stored["IslandMonetPreferenceVersion"].as_u64().unwrap_or(0)<1{s["CoverBackground"]=json!(false);}
    if !s["CoverBackground"].is_boolean(){s["CoverBackground"]=json!(false);}
    s["IslandMonetPreferenceVersion"]=json!(1);
    for key in ["ExpandedMonetEnabled","ExpandedCoverOverlay","TranslationButton"]{if !s[key].is_boolean(){s[key]=json!(true);}}
    for (key,fallback) in [("AcrylicCardFrost",40.),("AcrylicCardOpacity",60.),("AcrylicCapsuleFrost",40.),("AcrylicCapsuleOpacity",0.)]{s[key]=json!(s[key].as_f64().filter(|v|v.is_finite()).unwrap_or(fallback).clamp(0.,100.));}
    for prefix in ["Notification","Todo","Device"]{if stored[format!("{prefix}SoundSource")].is_null(){s[format!("{prefix}SoundSource")]=json!(if stored[format!("{prefix}SoundPath")].as_str().is_some_and(|p|!p.is_empty()){"Custom"}else{"Builtin"});}}
    if stored["SoundLibraryVersion"].as_u64().unwrap_or(0)<2{s=crate::sound::reset_builtin(&s);}
    migrate_materials(&stored,&mut s);
    if !["Classic","Acrylic","Glass"].contains(&s["ClassicCapsuleMaterial"].as_str().unwrap_or("")){s["ClassicCapsuleMaterial"]=json!("Acrylic");}
    if !["Classic","Acrylic","Glass"].contains(&s["MiniMaterial"].as_str().unwrap_or("")){s["MiniMaterial"]=json!("Acrylic");}
    if !["Classic","Acrylic","Glass"].contains(&s["MiniExpandedMaterial"].as_str().unwrap_or("")){s["MiniExpandedMaterial"]=json!("Acrylic");}
    if !["Classic","Mini"].contains(&s["IslandSizeMode"].as_str().unwrap_or("")){s["IslandSizeMode"]=json!("Classic");}
    if !["Classic","Acrylic","Glass"].contains(&s["IslandMaterial"].as_str().unwrap_or("")){s["IslandMaterial"]=json!("Acrylic");}
    for (key,fallback) in [("SettingsLightAccentColor","#2563EB"),("SettingsDarkAccentColor","#6EA8FF")]{
        let valid=s[key].as_str().is_some_and(|v|v.len()==7&&v.starts_with('#')&&v[1..].bytes().all(|b|b.is_ascii_hexdigit()));
        if !valid{s[key]=json!(fallback);}
    }
    // Validate persisted numbers before they enter geometry or a spring denominator.
    for (k,min,max,fallback) in [("AutoCompactMaxWidth",140.,480.,420.),("CompactWidth",140.,480.,260.),("FontSize",11.,20.,14.),("CenterX",0.,1.,0.5),("TopFraction",0.,1.,0.08)]{s[k]=json!(s[k].as_f64().filter(|v|v.is_finite()).unwrap_or(fallback).clamp(min,max));}
    for (k,min,max,fallback) in [("Gap",0.,24.,8.),("Duration",250.,1200.,480.),("CollapseDuration",150.,700.,440.),("Radius",8.,48.,30.)]{s["Motion"][k]=json!(s["Motion"][k].as_f64().unwrap_or(fallback).clamp(min,max));}
    for (k,min,max,fallback) in [("Stiffness",100.,1000.,420.),("Damping",10.,80.,30.),("Mass",0.3,2.,0.8)]{s["Motion"]["Spring"][k]=json!(s["Motion"]["Spring"][k].as_f64().unwrap_or(fallback).clamp(min,max));}
    if !["Separated","SideBySide","LyricFocus","LyricRight"].contains(&s["MusicExpandedLayout"].as_str().unwrap_or("")){s["MusicExpandedLayout"]=json!("SideBySide");}
    if !["Island","Bottom"].contains(&s["SystemStateHintPlacement"].as_str().unwrap_or("")){s["SystemStateHintPlacement"]=json!("Island");}
    s
}
/// Upgrade stored preferences once; the removed renderer is never selected at runtime.
fn migrate_materials(stored:&Value,s:&mut Value){
    for key in ["IslandMaterial","MiniMaterial","MiniExpandedMaterial","ClassicCapsuleMaterial"]{
        if s[key].as_str()==Some("LiquidGlass"){s[key]=json!("Glass");}
    }
    // Previously the Mini strip and its card shared one preference. Preserve that card choice
    // for existing installations; fresh installations use acrylic for both expanded sizes.
    if stored.get("MiniExpandedMaterial").is_none(){
        if let Some(material)=stored.get("MiniMaterial").and_then(Value::as_str){
            s["MiniExpandedMaterial"]=json!(if material=="LiquidGlass"{"Glass"}else{material});
        }
    }
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

pub fn is_acrylic_value(key:&str)->bool{["AcrylicCardFrost","AcrylicCardOpacity","AcrylicCapsuleFrost","AcrylicCapsuleOpacity"].contains(&key)}
pub fn is_paint_setting(key:&str)->bool{is_acrylic_value(key)||["CoverBackground","ExpandedMonetEnabled","ExpandedCoverOverlay","LyricColorFromCover","ExpandedTextColor","ExpandedComponentColor","LyricProgressColor","AcrylicAdjustTarget"].contains(&key)}
pub fn validate_setting(v:Value)->Result<Value,String>{
    let key=v["key"].as_str().ok_or("missing key")?;let value=&v["value"];let mut settings=v["settings"].clone();if !settings.is_object(){return Err("invalid settings".into())}
    let range=match key{"CapsuleCornerRadius"=>Some((0.,48.)),"DeviceVolume"|"NotificationVolume"|"TodoVolume"=>Some((0.,100.)),"AutoCompactMaxWidth"|"CompactWidth"=>Some((140.,480.)),"FontSize"=>Some((11.,20.)),"OffsetMilliseconds"=>Some((-5000.,5000.)),"NotificationHistoryDays"=>Some((1.,30.)),"MotionGap"=>Some((0.,24.)),"MotionDuration"=>Some((250.,1200.)),"MotionCollapseDuration"=>Some((150.,700.)),"MotionRadius"=>Some((8.,48.)),"SpringStiffness"=>Some((100.,1000.)),"SpringDamping"=>Some((10.,80.)),"SpringMass"=>Some((0.3,2.)),"AcrylicCardFrost"|"AcrylicCardOpacity"|"AcrylicCapsuleFrost"|"AcrylicCapsuleOpacity"=>Some((0.,100.)),_=>None};
    let value=if key=="IslandDynamicSeedColor"{let c=value.as_str().ok_or("invalid colour")?;if c.len()!=7||!c.starts_with('#')||!c[1..].bytes().all(|b|b.is_ascii_hexdigit()){return Err("invalid colour".into())}json!(c.to_uppercase())}else if key=="ControlCenterColorSource"||key=="IslandColorSource"{let source=value.as_str().ok_or("invalid colour source")?;if !["Content","System","Custom"].contains(&source){return Err("invalid colour source".into())}if key=="ControlCenterColorSource"{settings["ControlCenterMonetEnabled"]=json!(true);}json!(source)}else if key=="IslandFontFamily"{let name=value.as_str().ok_or("invalid font")?;if !["Noto Sans SC","Microsoft YaHei UI","Noto Serif SC"].contains(&name){return Err("unsupported font".into())}json!("Noto Sans SC")}else if key=="MotionCurve"{if value.is_null(){Value::Null}else{let c:[f64;4]=serde_json::from_value(value.clone()).map_err(|_|"invalid curve")?;if !lyric_core::motion_curve::valid(c){return Err("curve control points outside range".into())}json!(c)}}else if let Some((min,max))=range{let n=value.as_f64().ok_or("invalid number")?;if !n.is_finite()||n<min||n>max{return Err("number outside range".into())}if key=="OffsetMilliseconds"{json!(n.round() as i32)}else if key=="NotificationHistoryDays"{json!(n.round() as u32)}else{json!(n)}}else if key=="IslandSizeMode"{let s=value.as_str().ok_or("invalid size mode")?;if !["Classic","Mini"].contains(&s){return Err("invalid size mode".into())}json!(s)}else if key=="MiniMaterial"{let s=value.as_str().ok_or("invalid mini material")?;if !["Classic","Acrylic","Glass"].contains(&s){return Err("invalid mini material".into())}json!(s)}else if key=="ClassicCapsuleMaterial"{let s=value.as_str().ok_or("invalid capsule material")?;if !["Classic","Acrylic","Glass"].contains(&s){return Err("invalid capsule material".into())}json!(s)}else if key=="IslandMaterial"||key=="MiniExpandedMaterial"{let s=value.as_str().ok_or("invalid material")?;if !["Classic","Acrylic","Glass"].contains(&s){return Err("invalid material".into())}json!(s)}
    // Which surface the acrylic sliders edit; the two value pairs are stored independently.
    else if key=="AcrylicAdjustTarget"{let s=value.as_str().ok_or("invalid acrylic target")?;if !["Card","Capsule"].contains(&s){return Err("invalid acrylic target".into())}json!(s)}
    else if ["DynamicColorHighContrast","ControlCenterMonetEnabled","MiniAutoContrast","LyricColorFromCover","AutoCheckUpdatesEnabled","DeviceRemindersEnabled","DeviceSoundEnabled","SwipeActivities","SwipeUpHide","GestureTutorialSeen","TodoRemindersEnabled","TodoIslandEnabled","AutoCompactWidth","CompactTranslation","ChineseTwoLine","NotificationSoundEnabled","TodoSoundEnabled","Floating","Translation","TranslationButton","Animation","HoverExpand","ClickAnywhereExpand","WordHighlight","AlwaysOnTop","AutoStart","OnlineSongInfo","HideWhenPaused","NotificationsEnabled","PauseNotifications","NotificationClickOpenApp","MusicClickOpenApp","NotificationBlacklistEnabled","NotificationWhitelistEnabled","NotificationHistoryEnabled","CoverBackground","ExpandedMonetEnabled","ExpandedCoverOverlay","FocusCountUp","FocusPauseOnLock","AppEntryRemindersEnabled","AppUsageRemindersEnabled","HeadphoneConnectionRemindersEnabled","SpeakerConnectionRemindersEnabled","GamepadConnectionRemindersEnabled","MouseConnectionRemindersEnabled","KeyboardConnectionRemindersEnabled","DeviceDisconnectRemindersEnabled","SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"].contains(&key){json!(value.as_bool().ok_or("invalid bool")?)}
    else if key=="DeviceSound"||key=="NotificationSound"||key=="TodoSound"{let name=value.as_str().ok_or("invalid sound")?;if dotisle_domain::sound_catalog::find(name).is_none(){return Err("invalid sound".into())}json!(name)}
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
    // Layout values stay typed and are validated before they enter live geometry.
    else if key=="SystemStateHintPlacement"{let s=value.as_str().ok_or("invalid placement")?;if !["Island","Bottom"].contains(&s){return Err("invalid placement".into())}json!(s)}
    else if key=="MusicExpandedLayout"{let s=value.as_str().ok_or("invalid layout")?;if !["Separated","SideBySide","LyricFocus","LyricRight"].contains(&s){return Err("invalid layout".into())}json!(s)}
    else if key.ends_with("Color"){let s=value.as_str().ok_or("invalid color")?;if !["TodoDefaultColor","ExpandedTextColor","ExpandedComponentColor","LyricProgressColor","SettingsLightAccentColor","SettingsDarkAccentColor"].contains(&key)||!s.starts_with('#')||s.len()!=7||!s[1..].bytes().all(|b|b.is_ascii_hexdigit()){return Err("invalid color".into())}json!(s.to_uppercase())}
    else if key=="NeteaseCdpPort"{let raw=value.as_str().ok_or("端口需要 1024–65535 的数字")?;let port=raw.trim().parse::<u16>().ok().filter(|p|(1024..=65535).contains(p)).ok_or("端口需要 1024–65535 的数字")?;json!(port.to_string())}
    else if key=="IgnoredSources"||key=="AllowedSources"||key=="NotificationAllowedApps"||key=="NotificationBlockedApps"{let a=value.as_array().ok_or("invalid source list")?;if a.len()>32{return Err("source list too long".into())}let mut out=vec![];for s in a{let s=s.as_str().ok_or("invalid source")?.trim();if s.is_empty()||s.encode_utf16().count()>256{return Err("invalid source".into())}if !out.iter().any(|v:&String|v.eq_ignore_ascii_case(s)){out.push(s.to_string())}}json!(out)}else{return Err("unknown setting".into())};
    if ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"].contains(&key){for alias in ["SystemStateHintsEnabled","CapsLockHintEnabled","NumLockHintEnabled","MicrophoneMuteHintEnabled","TouchpadHintEnabled"]{settings[alias]=value.clone();}return Ok(settings)}
    if ["IslandMaterial","MiniExpandedMaterial"].contains(&key){settings["IslandMaterial"]=value.clone();settings["MiniExpandedMaterial"]=value;return Ok(settings)}
    if ["DeviceSoundPath","NotificationSoundPath","TodoSoundPath"].contains(&key){let prefix=key.trim_end_matches("SoundPath");settings[format!("{prefix}SoundSource")]=json!(if value.as_str().is_none_or(str::is_empty){"Builtin"}else{"Custom"});}
    if let Some(name)=key.strip_prefix("Motion"){settings["Motion"][name]=value;}else if let Some(name)=key.strip_prefix("Spring"){settings["Motion"]["Spring"][name]=value;}else{settings[key]=value;}Ok(settings)
}



#[cfg(test)]mod unified_osd_tests{use super::*;
#[test]fn mini_material_has_an_independent_clear_default_and_three_choices(){let d=defaults();assert_eq!(d["IslandSizeMode"],"Classic");assert_eq!(d["IslandMaterial"],"Acrylic");assert_eq!(d["MiniMaterial"],"Acrylic");for value in ["Classic","Acrylic","Glass"]{let s=validate_setting(json!({"key":"MiniMaterial","value":value,"settings":d})).unwrap();assert_eq!(s["MiniMaterial"],value);assert_eq!(s["IslandMaterial"],"Acrylic");}for value in [json!(true),json!("glass"),json!("LiquidGlass")]{assert!(validate_setting(json!({"key":"MiniMaterial","value":value,"settings":d})).is_err());}}
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
        for value in ["Classic","Acrylic","Glass"]{
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
        std::fs::write(&p,r#"{"IslandMaterial":"Glass","IslandGlassMaterial":false}"#).unwrap();
        let s=load(&p);assert_eq!(s["IslandMaterial"],"Glass");
        let _=std::fs::remove_file(p);
    }
}

#[cfg(test)]mod classic_capsule_tests{use super::*;#[test]fn capsule_material_is_independent_and_validated(){let d=defaults();assert_eq!(d["ClassicCapsuleMaterial"],"Acrylic");for material in ["Classic","Acrylic","Glass"]{let s=validate_setting(json!({"key":"ClassicCapsuleMaterial","value":material,"settings":d})).unwrap();assert_eq!(s["ClassicCapsuleMaterial"],material);assert_eq!(s["MiniMaterial"],"Acrylic");assert_eq!(s["IslandMaterial"],"Acrylic");}for value in [json!(true),json!("Black"),json!("glass"),json!("LiquidGlass")]{assert!(validate_setting(json!({"key":"ClassicCapsuleMaterial","value":value,"settings":d})).is_err());}}}

#[cfg(test)] mod expanded_material_tests{
    use super::*;
    use crate::renderer::Material;
    #[test] fn fresh_install_starts_in_classic_and_hides_paused_music(){
        let path=std::env::temp_dir().join(format!("dotisle-fresh-material-defaults-{}.json",std::process::id()));
        let fresh=load(&path);assert_eq!(fresh["HideWhenPaused"],true);assert_eq!(fresh["IslandSizeMode"],"Classic");
        assert_eq!(fresh["Motion"]["Radius"],30.);assert_eq!(fresh["CoverBackground"],false);assert_eq!(fresh["SystemStateHintsEnabled"],true);assert_eq!(fresh["NotificationsEnabled"],true);
        std::fs::write(&path,r#"{"HideWhenPaused":false,"IslandSizeMode":"Mini"}"#).unwrap();
        let existing=load(&path);assert_eq!(existing["HideWhenPaused"],false);assert_eq!(existing["IslandSizeMode"],"Mini");
        let _=std::fs::remove_file(path);
    }
    #[test] fn expanded_material_aliases_share_one_choice_and_survive_reload(){
        let path=std::env::temp_dir().join(format!("dotisle-mini-card-{}.json",std::process::id()));
        let mut settings=defaults();
        assert_eq!(settings["MiniExpandedMaterial"],"Acrylic");
        for classic in ["Classic","Acrylic","Glass"]{for mini in ["Classic","Acrylic","Glass"]{
            settings=validate_setting(json!({"key":"IslandMaterial","value":classic,"settings":settings})).unwrap();
            settings=validate_setting(json!({"key":"MiniExpandedMaterial","value":mini,"settings":settings})).unwrap();
            std::fs::write(&path,serde_json::to_vec(&settings).unwrap()).unwrap();settings=load(&path);
            assert_eq!(settings["IslandMaterial"],mini);assert_eq!(settings["MiniExpandedMaterial"],mini);
            assert_eq!(settings["MiniMaterial"],"Acrylic");assert_eq!(settings["ClassicCapsuleMaterial"],"Acrylic");
            assert_eq!(Material::expanded(&settings,false),Material::from_setting(mini));
            assert_eq!(Material::expanded(&settings,true),Material::from_setting(mini));
        }}
        for invalid in [json!(true),json!("glass"),json!("LiquidGlass"),json!("")]{
            assert!(validate_setting(json!({"key":"MiniExpandedMaterial","value":invalid,"settings":settings})).is_err());
        }
        let _=std::fs::remove_file(path);
    }
    #[test] fn retired_material_migrates_and_does_not_override_new_card_preference(){
        let path=std::env::temp_dir().join(format!("dotisle-retired-material-{}.json",std::process::id()));
        for (stored,mini) in [
            (json!({"IslandMaterial":"LiquidGlass","MiniMaterial":"LiquidGlass","ClassicCapsuleMaterial":"LiquidGlass"}),"Glass"),
            (json!({"MiniMaterial":"Classic"}),"Classic"),
            (json!({"MiniMaterial":"LiquidGlass","MiniExpandedMaterial":"Acrylic"}),"Acrylic"),
            (json!({"MiniMaterial":"Glass","MiniExpandedMaterial":"invalid"}),"Acrylic"),
            (json!({}),"Acrylic")
        ]{
            std::fs::write(&path,serde_json::to_vec(&stored).unwrap()).unwrap();let updated=load(&path);
            assert_eq!(updated["MiniExpandedMaterial"],mini);
            for key in ["IslandMaterial","MiniMaterial","ClassicCapsuleMaterial"]{
                if stored[key]=="LiquidGlass"{assert_eq!(updated[key],"Glass");}
            }
            std::fs::write(&path,serde_json::to_vec(&updated).unwrap()).unwrap();assert_eq!(load(&path),updated);
        }
        let _=std::fs::remove_file(path);
    }
    #[test] fn both_capsules_accept_three_materials(){
        let mut settings=defaults();
        assert_eq!(settings["ClassicCapsuleMaterial"],"Acrylic");
        assert_eq!(settings["MiniMaterial"],"Acrylic");
        for key in ["ClassicCapsuleMaterial","MiniMaterial"]{
            for value in ["Classic","Acrylic","Glass"]{
                settings=validate_setting(json!({"key":key,"value":value,"settings":settings})).unwrap();
                assert_eq!(settings[key],value);
            }
            for invalid in [json!(true),json!("glass"),json!("LiquidGlass"),json!("")]{
                assert!(validate_setting(json!({"key":key,"value":invalid,"settings":settings})).is_err());
            }
        }
        assert_eq!(settings["IslandMaterial"],"Acrylic");
    }
}

#[cfg(test)]mod monet_preference_tests{
    use super::*;
    #[test]fn all_material_defaults_are_acrylic_without_monet(){let s=defaults();for key in ["IslandMaterial","MiniExpandedMaterial","ClassicCapsuleMaterial","MiniMaterial"]{assert_eq!(s[key],"Acrylic");}assert_eq!(s["CoverBackground"],false);assert!(s.get("IslandFluidMotion").is_none());assert!(validate_setting(json!({"key":"IslandFluidMotion","value":false,"settings":s})).is_err());}
    #[test]fn legacy_implicit_tint_is_disabled_once_and_explicit_opt_in_persists(){
        let path=std::env::temp_dir().join(format!("dotisle-monet-migration-{}.json",std::process::id()));
        std::fs::write(&path,r#"{"CoverBackground":true,"IslandFluidMotion":false,"IslandMaterial":"Glass","MiniMaterial":"Classic"}"#).unwrap();
        let migrated=load(&path);assert_eq!(migrated["CoverBackground"],false);assert_eq!(migrated["IslandMaterial"],"Glass");assert_eq!(migrated["MiniMaterial"],"Classic");assert!(migrated.get("IslandFluidMotion").is_none());
        let enabled=validate_setting(json!({"key":"CoverBackground","value":true,"settings":migrated})).unwrap();std::fs::write(&path,serde_json::to_vec(&enabled).unwrap()).unwrap();assert_eq!(load(&path)["CoverBackground"],true);
        let disabled=validate_setting(json!({"key":"CoverBackground","value":false,"settings":enabled})).unwrap();std::fs::write(&path,serde_json::to_vec(&disabled).unwrap()).unwrap();assert_eq!(load(&path)["CoverBackground"],false);
        assert!(validate_setting(json!({"key":"CoverBackground","value":"true","settings":disabled})).is_err());let _=std::fs::remove_file(path);
    }
}
#[cfg(test)]mod acrylic_slider_tests{
    use super::*;
    #[test]fn the_card_starts_frosted_and_the_strip_starts_untouched(){
        let d=defaults();
        assert_eq!((d["AcrylicCardFrost"].as_f64(),d["AcrylicCardOpacity"].as_f64()),(Some(40.),Some(60.)));
        assert_eq!((d["AcrylicCapsuleFrost"].as_f64(),d["AcrylicCapsuleOpacity"].as_f64()),(Some(40.),Some(0.)));
        assert_eq!(d["AcrylicAdjustTarget"],"Card");
    }
    #[test]fn both_sliders_are_clamped_to_a_percentage_and_the_target_is_a_closed_set(){
        let d=defaults();
        for key in ["AcrylicCardFrost","AcrylicCardOpacity","AcrylicCapsuleFrost","AcrylicCapsuleOpacity"]{
            for value in [json!(0.),json!(55.),json!(100.)]{let s=validate_setting(json!({"key":key,"value":value,"settings":d})).unwrap();assert_eq!(s[key],value);}
            for value in [json!(-1.),json!(101.),json!("50")]{assert!(validate_setting(json!({"key":key,"value":value,"settings":d})).is_err());}
        }
        let s=validate_setting(json!({"key":"AcrylicAdjustTarget","value":"Capsule","settings":d})).unwrap();
        assert_eq!(s["AcrylicAdjustTarget"],"Capsule");
        assert!(validate_setting(json!({"key":"AcrylicAdjustTarget","value":"Body","settings":d})).is_err());
    }
}

#[cfg(test)]mod expanded_appearance_tests{
    use super::*;
    #[test]fn new_defaults_and_old_preferences_round_trip_independently(){
        let file=std::env::temp_dir().join(format!("dotisle-expanded-preferences-{}.json",std::process::id()));
        std::fs::write(&file,r#"{"IslandMonetPreferenceVersion":1,"CoverBackground":false,"AcrylicCardOpacity":75}"#).unwrap();
        let mut s=load(&file);assert_eq!(s["CoverBackground"],false);assert_eq!(s["ExpandedMonetEnabled"],true);assert_eq!(s["ExpandedCoverOverlay"],true);assert_eq!(s["AcrylicCardOpacity"],75.);
        for key in ["ExpandedMonetEnabled","ExpandedCoverOverlay"]{s=validate_setting(json!({"key":key,"value":false,"settings":s})).unwrap();}
        std::fs::write(&file,serde_json::to_vec(&s).unwrap()).unwrap();let restored=load(&file);
        for key in ["CoverBackground","ExpandedMonetEnabled","ExpandedCoverOverlay"]{assert_eq!(restored[key],false);assert!(validate_setting(json!({"key":key,"value":"false","settings":restored})).is_err());}
        let _=std::fs::remove_file(file);
    }
}

#[cfg(test)]mod retired_theme_tests{
    use super::*;
    #[test]fn the_hidden_theme_keys_are_dropped_and_the_classic_layout_survives(){
        let file=std::env::temp_dir().join(format!("dotisle-retired-theme-{}.json",std::process::id()));
        std::fs::write(&file,r#"{"MusicControlTheme":"Alcove","AlcoveThemeUnlocked":true,"MusicExpandedLayout":"LyricRight"}"#).unwrap();
        let loaded=load(&file);
        assert!(loaded.get("MusicControlTheme").is_none());assert!(loaded.get("AlcoveThemeUnlocked").is_none());
        assert_eq!(loaded["MusicExpandedLayout"],"LyricRight");
        assert!(validate_setting(json!({"key":"MusicControlTheme","value":"Alcove","settings":loaded.clone()})).is_err());
        assert!(validate_setting(json!({"key":"AlcoveThemeUnlocked","value":true,"settings":loaded.clone()})).is_err());
        let _=std::fs::remove_file(file);
    }
}

#[cfg(test)]mod translation_button_tests{
    use super::*;
    /// One switch hides the card's 译 button; the translation text preference is a different
    /// decision and neither may silently rewrite the other.
    #[test]fn the_translation_button_switch_defaults_on_and_round_trips(){
        let d=defaults();assert_eq!(d["TranslationButton"],true);assert_eq!(d["Translation"],true);
        let missing=load(std::path::Path::new("no-such-settings-for-translation-button.json"));
        assert_eq!(missing["TranslationButton"],true);
        let mut s=validate_setting(json!({"key":"TranslationButton","value":false,"settings":d})).unwrap();
        assert_eq!(s["TranslationButton"],false);assert_eq!(s["Translation"],true);
        s=validate_setting(json!({"key":"Translation","value":false,"settings":s})).unwrap();
        assert_eq!(s["Translation"],false);assert_eq!(s["TranslationButton"],false);
        for invalid in [json!("false"),json!(0),json!(null)]{assert!(validate_setting(json!({"key":"TranslationButton","value":invalid,"settings":s.clone()})).is_err());}
        let file=std::env::temp_dir().join(format!("dotisle-translation-button-{}.json",std::process::id()));
        std::fs::write(&file,serde_json::to_vec(&s).unwrap()).unwrap();let restored=load(&file);
        assert_eq!(restored["TranslationButton"],false);assert_eq!(restored["Translation"],false);
        std::fs::write(&file,r#"{"Translation":true,"TranslationButton":"yes"}"#).unwrap();
        assert_eq!(load(&file)["TranslationButton"],true);
        let _=std::fs::remove_file(file);
    }
}

#[cfg(test)]mod expressive_preferences{use super::*;
 #[test]fn centre_monet_and_lyric_colour_are_independent_and_persist_false(){let d=defaults();let s=validate_setting(json!({"key":"ControlCenterMonetEnabled","value":false,"settings":d})).unwrap();assert_eq!(s["ExpandedMonetEnabled"],true);assert_eq!(s["LyricColorFromCover"],true);let s=validate_setting(json!({"key":"LyricColorFromCover","value":false,"settings":s})).unwrap();assert_eq!(s["ControlCenterMonetEnabled"],false);assert_eq!(s["ExpandedMonetEnabled"],true);}
 #[test]fn right_layout_and_shared_expanded_material_are_persisted(){
 let path=std::env::temp_dir().join(format!("dotisle-immersive-{}.json",std::process::id()));
 let s=validate_setting(json!({"key":"MusicExpandedLayout","value":"LyricRight","settings":defaults()})).unwrap();let s=validate_setting(json!({"key":"MiniExpandedMaterial","value":"Glass","settings":s})).unwrap();assert_eq!(s["IslandMaterial"],"Glass");assert_eq!(s["MiniExpandedMaterial"],"Glass");std::fs::write(&path,serde_json::to_vec(&s).unwrap()).unwrap();let loaded=load(&path);let _=std::fs::remove_file(path);assert_eq!(loaded["MusicExpandedLayout"],"LyricRight");assert_eq!(loaded["IslandMaterial"],"Glass");
}
#[test]fn only_verified_outline_font_choices_are_accepted(){for name in ["Noto Sans SC","Noto Serif SC","Microsoft YaHei UI"]{let s=validate_setting(json!({"key":"IslandFontFamily","value":name,"settings":defaults()})).unwrap();assert_eq!(s["IslandFontFamily"],"Noto Sans SC");}assert!(validate_setting(json!({"key":"IslandFontFamily","value":"SimSun","settings":defaults()})).is_err());}
}

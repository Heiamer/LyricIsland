//! Settings IPC, ported unchanged from the Slint frontend.
//!
//! The native capsule hosts the pipe server; this process is only a client. The command names,
//! payload shapes and validation rules are part of that contract and must not drift.
use lyric_backend::io;
use serde_json::{json, Value};

/// Every player the 不响应的播放器 page offers, grouped the way a person thinks about them rather
/// than the way the session ids are spelled. The page draws one grid per group; nothing here is a
/// gate — a player that is not on this list still responds, it simply cannot be excluded by name.
pub const PLAYER_GROUPS: [(&str, &[(&str, &str)]); 4] = [
    ("国内平台", &[
        ("Netease", "网易云音乐"),
        ("QQMusic", "QQ 音乐"),
        ("Kugou", "酷狗音乐"),
        ("Kuwo", "酷我音乐"),
        ("SodaMusic", "汽水音乐"),
    ]),
    ("国外平台", &[
        ("Spotify", "Spotify"),
        ("AppleMusic", "Apple Music"),
        ("YouTubeMusic", "YouTube Music"),
    ]),
    ("本地播放器", &[
        ("PotPlayer", "PotPlayer"),
        ("Foobar2000", "Foobar2000"),
        ("MusicBee", "MusicBee"),
        ("AIMP", "AIMP"),
        ("SaltPlayer", "Salt Player"),
    ]),
    ("其它", &[
        ("Browser", "浏览器媒体"),
        ("LXMusic", "洛雪音乐"),
        ("MusicFree", "MusicFree"),
        ("Cider", "Cider"),
        ("YesPlayMusic", "YesPlayMusic"),
    ]),
];

/// The flat id list the validators need, in the same order the page draws them.
pub fn player_ids() -> impl Iterator<Item = &'static str> {
    PLAYER_GROUPS.iter().flat_map(|(_, players)| players.iter().map(|(id, _)| *id))
}

/// Notification commands that take no value.
const NOTIFICATION_COMMANDS: [&str; 1] = ["notificationPreview"];

fn pipe_name() -> String {
    std::env::var("LYRICISLAND_TEST_PIPE").unwrap_or_else(|_| "LyricIsland.Native.Settings.v1".into())
}

pub fn request(command: &str, value: Value) -> Result<Value, String> {
    io::pipe_request(&pipe_name(), &json!({"version": 1, "command": command, "value": value}))
}

/// Compose edits from fresh native state and preserve whitelist updates while
/// GPUI executes transport work on its background executor.
pub fn edit(key:&str,value:&str)->Result<(Value,Value),String>{
    static EDIT:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _guard=EDIT.lock().map_err(|_|"设置连接异常".to_owned())?;
    let snapshot=request("state",Value::Null)?;
    let reply=change(key,value,&snapshot)?;
    Ok((reply,request("state",Value::Null)?))
}

/// Todo and sound commands carry a JSON object rather than a scalar string, so
/// they bypass `change` and go straight to the pipe with a fresh state reply.
pub fn command(name:&str,value:Value)->Result<(Value,Value),String>{
    static EDIT:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _guard=EDIT.lock().map_err(|_|"设置连接异常".to_owned())?;
    let reply=request(name,value)?;
    Ok((reply,request("state",Value::Null)?))
}

/// Audition a chime. Never touches real notifications.
pub fn sound_preview(todo:bool)->Result<Value,String>{request("soundPreview",json!({"todo":todo}))}

/// Acknowledge a "jump to this page" request so the native side stops repeating it.
pub fn page_seen()->Result<Value,String>{request("pageSeen",Value::Null)}

/// Translate one UI edit into a pipe request. Returns a user-facing error message instead when
/// the input is rejected locally.
pub fn change(key: &str, text: &str, state: &Value) -> Result<Value, String> {
    if NOTIFICATION_COMMANDS.contains(&key) {
        return request(key, Value::Null);
    }
    if key=="notificationBlockedApps"||key.starts_with("notification-blocked:"){
        let mut blocked=state["settings"]["NotificationBlockedApps"].as_array().cloned().unwrap_or_default();
        if let Some(id)=key.strip_prefix("notification-blocked:"){blocked.retain(|v|!v.as_str().is_some_and(|s|s.eq_ignore_ascii_case(id)));if text=="true"{blocked.push(json!(id));}}
        else{for id in text.split([';','；',',','，','\n','\r']).map(str::trim).filter(|s|!s.is_empty()){if id.len()>256||id.chars().any(char::is_control){return Err("应用标识无效".into())}if !blocked.iter().any(|v|v.as_str().is_some_and(|s|s.eq_ignore_ascii_case(id))){blocked.push(json!(id));}}}
        if blocked.len()>32{return Err("最多添加32个应用".into())}return request("update",json!({"key":"NotificationBlockedApps","value":blocked}));
    }
    if let Some(id) = key.strip_prefix("notification-app:") {
        let mut allowed = state["settings"]["NotificationAllowedApps"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![json!("QQNT"), json!("QQ")]);
        allowed.retain(|s| s.as_str() != Some(id));
        if text == "true" {
            allowed.push(json!(id));
        }
        return request("update", json!({"key": "NotificationAllowedApps", "value": allowed}));
    }
    if key == "notificationApps" {
        let mut allowed = state["settings"]["NotificationAllowedApps"].as_array().cloned().unwrap_or_default();
        allowed.retain(|v| v.as_str().is_some_and(|s| ["QQNT", "QQ"].contains(&s)));
        for id in text.split([';', '；', ',', '，', '\n', '\r']).map(str::trim).filter(|s| !s.is_empty()) {
            if id.len() > 256 || id.chars().any(char::is_control) {
                return Err("应用标识无效".into());
            }
            if !allowed.iter().any(|v| v.as_str() == Some(id)) {
                allowed.push(json!(id));
            }
        }
        return request("update", json!({"key": "NotificationAllowedApps", "value": allowed}));
    }
    if ["resetAppearance", "resetMotion", "resetSettingsColors"].contains(&key) {
        return request(key, Value::Null);
    }
    // A plain string setting: the generic branch below parses JSON, and `System` is not JSON.
    if key == "SettingsTheme" {
        if !["System", "Light", "Dark"].contains(&text) {
            return Err("外观选项无效。".into());
        }
        return request("update", json!({"key": key, "value": text}));
    }
    // Segmented controls pass raw labels, not JSON string literals.
    if ["NotificationSound", "TodoSound", "DeviceSound"].contains(&key) {
        if !["Soft", "Glass", "Bell", "Connect"].contains(&text) {
            return Err("音色选项无效。".into());
        }
        return request("update", json!({"key": key, "value": text}));
    }
    if key == "ControlCenterStartPage" {
        if !["overview", "notifications", "todos", "timers", "island", "settings", "about"].contains(&text) {
            return Err("起始页选项无效。".into());
        }
        return request("update", json!({"key": key, "value": text}));
    }
    // A layout type, never a raw boolean pair.
    if key=="SystemStateHintPlacement"{if !["Island","Bottom"].contains(&text){return Err("提示位置无效".into())}return request("update",json!({"key":key,"value":text}));}
    if key == "MusicExpandedLayout" {
        if !["Separated", "SideBySide", "LyricFocus", "LyricRight"].contains(&text) {
            return Err("布局选项无效。".into());
        }
        return request("update", json!({"key": key, "value": text}));
    }
    // The material picker is a segmented control: it sends a bare label, not a JSON literal.
    if key=="IslandSizeMode"{let mode=match text{"Classic"=>"Classic","Mini"=>"Mini",_=>if text.parse::<f64>().map_err(|_|"尺寸选项无效")?>=0.5{"Mini"}else{"Classic"}};return request("update",json!({"key":key,"value":mode}));}
    // The capsule picker sends a bare label; the numeric branch keeps older builds working.
    if key=="ClassicCapsuleMaterial"{
        let mode=match text{
            "Classic"|"Acrylic"|"Glass"=>text,
            _=>{let n=text.parse::<f64>().map_err(|_|"材质选项无效")?.round();if n<=0.{"Classic"}else if n==1.{"Acrylic"}else{"Glass"}}
        };
        return request("update",json!({"key":key,"value":mode}));
    }
    if key == "MiniMaterial" {
        if !["Classic", "Acrylic", "Glass"].contains(&text) {return Err("Mini 材质选项无效。".into());}
        return request("update",json!({"key":key,"value":text}));
    }
    if key == "IslandMaterial" || key == "MiniExpandedMaterial" {
        if !["Classic", "Acrylic", "Glass"].contains(&text) {
            return Err("材质选项无效。".into());
        }
        return request("update", json!({"key": key, "value": text}));
    }
    if key == "synchronize" {
        let seconds = text.parse::<f64>().map_err(|_| "请输入有效秒数")?;
        if !seconds.is_finite() || !(0.0..=86400.0).contains(&seconds) {
            return Err("请输入 0–86400 范围的秒数。".into());
        }
        return request(key, json!(seconds));
    }
    let (key, value) = if let Some(id) = key.strip_prefix("source:") {
        // The tile is a switch for "this player stays silent", so `true` adds it to the exclusions
        // rather than granting it. Everything not named here keeps responding.
        let mut ignored = state["settings"]["IgnoredSources"].as_array().cloned().unwrap_or_else(|| vec![json!("Browser")]);
        ignored.retain(|s| !s.as_str().is_some_and(|s| s.eq_ignore_ascii_case(id)));
        if text == "true" {
            ignored.push(json!(id));
        }
        ("IgnoredSources", Value::Array(ignored))
    } else if key == "customSources" {
        let custom = text.split(['\r', '\n', ',', '，', ';', '；']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>();
        if custom.iter().any(|s| s.len() > 256 || s.chars().any(char::is_control) || s.contains(['*', '?', '"'])) {
            return Err("请输入有效程序名，不支持通配符和启动参数。".into());
        }
        let mut ignored = state["settings"]["IgnoredSources"].as_array().cloned().unwrap_or_else(|| vec![json!("Browser")]);
        ignored.retain(|s| s.as_str().is_some_and(|s| player_ids().any(|id| id.eq_ignore_ascii_case(s))));
        for s in custom {
            if !ignored.iter().any(|v| v.as_str().is_some_and(|v| v.eq_ignore_ascii_case(s))) {
                ignored.push(json!(s));
            }
        }
        if ignored.len() > 32 {
            return Err("最多 32 个程序。".into());
        }
        ("IgnoredSources", Value::Array(ignored))
    } else if key.ends_with("Color") {
        let value = if text == "default" {
            match key {
                "SettingsLightAccentColor" => "#2563EB",
                "SettingsDarkAccentColor" => "#6EA8FF",
                "ExpandedComponentColor" => "#B1E8D1",
                "LyricProgressColor" => "#A9E6CD",
                _ => "#F4F6F8",
            }
        } else {
            text
        };
        if !is_hex_color(value) {
            return Err("请输入 #RRGGBB 格式。".into());
        }
        (key, json!(value.to_uppercase()))
    } else {
        let value = serde_json::from_str::<Value>(text).map_err(|_| "设置值无效")?;
        (key, value)
    };
    request("update", json!({"key": key, "value": value}))
}

/// Accept only `#RRGGBB`, matching the native validator.
pub fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|c| c.is_ascii_hexdigit())
}


/// Exercise the same conversion as the visible sound buttons, on isolated test data only.
pub fn sound_selection_smoke() -> Result<Value, String> {
    if !pipe_name().starts_with("LyricIsland.Settings.test.") {
        return Err("音色自检只能使用独立测试管道".into());
    }
    let initial = request("state", Value::Null)?;
    let keys = ["NotificationSound", "TodoSound", "DeviceSound"];
    let outcome = (|| -> Result<Value, String> {
        let mut combinations = 0;
        let mut rejected = 0;
        for key in keys {
            for kind in ["Soft", "Glass", "Bell", "Connect"] {
                let reply = change(key, kind, &initial)?;
                if reply["success"] != true {
                    return Err(format!("{key}/{kind}: {}", reply["message"]));
                }
                let state = request("state", Value::Null)?;
                if state["settings"][key] != kind {
                    return Err(format!("{key}/{kind} 未保存"));
                }
                combinations += 1;
            }
            for invalid in ["", "soft", "Chime", "\"Soft\"", "true", "null"] {
                if change(key, invalid, &initial).is_ok() {
                    return Err(format!("{key} 接受了无效音色"));
                }
                rejected += 1;
            }
            if request("state", Value::Null)?["settings"][key] != "Connect" {
                return Err(format!("{key} 被无效输入覆盖"));
            }
        }
        Ok(json!({"success": true, "combinations": combinations, "invalidRejected": rejected}))
    })();
    // Restore even when one check failed. Never leave the test fixture in its final selection.
    let mut restoration_error = None;
    for key in keys {
        match request("update", json!({"key": key, "value": initial["settings"][key]})) {
            Ok(reply) if reply["success"] == true => {},
            other => restoration_error = Some(format!("恢复 {key} 失败：{other:?}")),
        }
    }
    if let Some(error) = restoration_error { return Err(error); }
    let final_state = request("state", Value::Null)?;
    if keys.iter().any(|key| final_state["settings"][*key] != initial["settings"][*key]) {
        return Err("音色自检未恢复原配置".into());
    }
    outcome
}

/// Exercise the same conversion as the visible material segments, on isolated test data only.
pub fn material_selection_smoke() -> Result<Value, String> {
    if !pipe_name().starts_with("LyricIsland.Settings.test.") {
        return Err("材质自检只能使用独立测试管道".into());
    }
    let initial = request("state", Value::Null)?;
    let outcome = (|| -> Result<Value, String> {
        for key in ["IslandMaterial","MiniExpandedMaterial"]{
            let original=initial["settings"][key].as_str().unwrap_or("Acrylic");
            let run=(|| -> Result<(),String>{
                for kind in ["Classic","Acrylic","Glass"]{
                    let reply=change(key,kind,&initial)?;
                    if reply["success"]!=true||request("state",Value::Null)?["settings"][key]!=kind{return Err(format!("{key}: {kind} 未保存"));}
                }
                for invalid in ["","acrylic","glass","LiquidGlass","\"Acrylic\"","true"]{
                    if change(key,invalid,&initial).is_ok(){return Err(format!("{key} 接受了无效取值：{invalid}"));}
                }
                if request("state",Value::Null)?["settings"][key]!="Glass"{return Err("无效输入覆盖了材质".into());}
                Ok(())
            })();
            match request("update",json!({"key":key,"value":original})){
                Ok(reply)if reply["success"]==true=>{},other=>return Err(format!("恢复 {key} 失败：{other:?}")),
            }
            run?;
        }
        Ok(json!({"success":true}))
    })();
    outcome
}

//! Settings IPC, ported unchanged from the Slint frontend.
//!
//! The native capsule hosts the pipe server; this process is only a client. The command names,
//! payload shapes and validation rules are part of that contract and must not drift.
use lyric_backend::io;
use serde_json::{json, Value};

/// Known players offered as toggles on the whitelist page.
pub const PLAYERS: [(&str, &str); 10] = [
    ("Netease", "网易云音乐"),
    ("QQMusic", "QQ 音乐"),
    ("Kugou", "酷狗音乐"),
    ("Kuwo", "酷我音乐"),
    ("Spotify", "Spotify"),
    ("TwilightEcho", "TwilightEcho"),
    ("Foobar2000", "Foobar2000"),
    ("MusicBee", "MusicBee"),
    ("AIMP", "AIMP"),
    ("Browser", "浏览器媒体"),
];

/// Notification commands that take no value.
const NOTIFICATION_COMMANDS: [&str; 4] = [
    "notificationSupport",
    "notificationAccess",
    "notificationPreview",
    "notificationSettings",
];

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

/// Translate one UI edit into a pipe request. Returns a user-facing error message instead when
/// the input is rejected locally.
pub fn change(key: &str, text: &str, state: &Value) -> Result<Value, String> {
    if NOTIFICATION_COMMANDS.contains(&key) {
        return request(key, Value::Null);
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
    if key == "resetAppearance" {
        return request(key, Value::Null);
    }
    // A plain string setting: the generic branch below parses JSON, and `System` is not JSON.
    if key == "SettingsTheme" {
        if !["System", "Light", "Dark"].contains(&text) {
            return Err("外观选项无效。".into());
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
        let mut allowed = state["settings"]["AllowedSources"].as_array().cloned().unwrap_or_default();
        allowed.retain(|s| !s.as_str().is_some_and(|s| s.eq_ignore_ascii_case(id)));
        if text == "true" {
            allowed.push(json!(id));
        }
        ("AllowedSources", Value::Array(allowed))
    } else if key == "customSources" {
        let custom = text.split(['\r', '\n', ',', '，', ';', '；']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>();
        if custom.iter().any(|s| s.len() > 256 || s.chars().any(char::is_control) || s.contains(['*', '?', '"'])) {
            return Err("请输入有效程序名，不支持通配符和启动参数。".into());
        }
        let mut allowed = state["settings"]["AllowedSources"].as_array().cloned().unwrap_or_default();
        allowed.retain(|s| s.as_str().is_some_and(|s| PLAYERS.iter().any(|(id, _)| id.eq_ignore_ascii_case(s))));
        for s in custom {
            if !allowed.iter().any(|v| v.as_str().is_some_and(|v| v.eq_ignore_ascii_case(s))) {
                allowed.push(json!(s));
            }
        }
        if allowed.len() > 32 {
            return Err("最多允许 32 个播放器。".into());
        }
        ("AllowedSources", Value::Array(allowed))
    } else if key.ends_with("Color") {
        let value = if text == "default" {
            match key {
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

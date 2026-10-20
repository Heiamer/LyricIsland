//! The 1.3/2.x end-to-end contract: defaults, todos, settings, tools, persistence, packaging.

use crate::bail;
use crate::common::{arg_value, check, default_version, read_json, sha256, strip_verbatim, temp_dir, write_json, Result, ToolError};
use crate::process::{self, Runtime};
use crate::verify::same_value;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn sleep(seconds: f64) { std::thread::sleep(Duration::from_secs_f64(seconds)); }

fn state(runtime: &Runtime) -> Result<Value> { process::call(runtime.pipe(), "state", None) }

fn settings(runtime: &Runtime) -> Result<Value> {
    let state = state(runtime)?;
    Ok(state.get("settings").cloned().unwrap_or(Value::Null))
}

fn update(runtime: &Runtime, key: &str, value: Value) -> Result<()> {
    process::call(runtime.pipe(), "update", Some(json!({"key": key, "value": value}))).map(|_| ())
}

fn rejected(runtime: &Runtime, command: &str, value: Option<Value>) -> Result<bool> {
    let reply = process::request(runtime.pipe(), command, value, Duration::from_secs(15))?;
    Ok(reply.get("success").and_then(Value::as_bool) != Some(true))
}

fn check_defaults(runtime: &Runtime) -> Result<Value> {
    let settings = settings(runtime)?;
    let expected = json!({
        "AutoCompactWidth": true,
        "AutoCompactMaxWidth": 420.0,
        "DeviceVolume": 55.0,
        "TodoIslandEnabled": true,
        "CompactTranslation": true,
        "ChineseTwoLine": true,
        "NotificationSoundEnabled": true,
        "TodoSoundEnabled": true,
        "NotificationSound": "Soft",
        "TodoSound": "Bell",
        "NotificationVolume": 35.0,
        "TodoVolume": 45.0,
        "NotificationSoundPath": "",
        "TodoSoundPath": "",
        "SwipeActivities": true,
        "DeviceRemindersEnabled": true,
        "NotificationsEnabled": true,
        "DefaultsOnboardingVersion": 1,
        "CoverBackground": true,
        "IslandMaterial": "Acrylic",
        "AutoStart": true,
        "NotificationClickOpenApp": true,
        "MusicClickOpenApp": false,
        "PauseNotifications": false,
        "NotificationWhitelistEnabled": false,
        "ClickAnywhereExpand": true,
        "OnlineSongInfo": false,
        "Translation": true,
        "Animation": true,
        "CompactWidth": 260.0,
        "FontSize": 12.5,
        "Floating": false
    });
    for (key, value) in expected.as_object().into_iter().flatten() {
        let actual = &settings[key];
        if !same_value(actual, value) { bail!("默认值不一致：{key} 实际 {actual} 期望 {value}"); }
    }
    Ok(expected)
}

fn check_todos(runtime: &Runtime, data: &Path) -> Result<()> {
    let today = state(runtime)?["today"].as_str().unwrap_or("").to_owned();
    let saved = process::call(runtime.pipe(), "todoSave", Some(json!({
        "title": "喝水", "content": "休息一下", "date": today, "time": "09:00", "daily": true
    })))?;
    let todo_id = saved["id"].as_str().ok_or_else(|| ToolError("待办未返回编号".into()))?.to_owned();

    let listed = process::call(runtime.pipe(), "todos", None)?;
    let todos = listed["todos"].as_array().cloned().unwrap_or_default();
    check(todos.len() == 1, format!("待办数量异常：{listed}"))?;
    check(todos[0]["title"] == "喝水" && todos[0]["daily"] == true, format!("待办内容异常：{listed}"))?;
    check(todos[0]["completed"] == false, format!("新待办不应已完成：{listed}"))?;

    for (payload, why) in [
        (json!({"title": "x", "date": "2026-02-29", "time": "09:00"}), "非闰年日期"),
        (json!({"title": "x", "date": today, "time": "24:00"}), "越界时间"),
        (json!({"title": "  ", "date": today, "time": "09:00"}), "空标题"),
        (json!({"title": "x", "date": today, "time": "9:00"}), "未补零时间"),
    ] {
        check(rejected(runtime, "todoSave", Some(payload))?, format!("未拒绝{why}"))?;
    }

    process::call(runtime.pipe(), "todoComplete", Some(json!({"id": todo_id, "completed": true})))?;
    check(process::call(runtime.pipe(), "todos", None)?["todos"][0]["completed"] == true, "完成状态未保存")?;
    process::call(runtime.pipe(), "todoComplete", Some(json!({"id": todo_id, "completed": false})))?;
    check(process::call(runtime.pipe(), "todos", None)?["todos"][0]["completed"] == false, "取消完成未保存")?;

    check(rejected(runtime, "todoComplete", Some(json!({"id": "missing", "completed": true})))?, "未知待办不应能完成")?;
    check(rejected(runtime, "todoDelete", Some(json!({"id": "missing"})))?, "未知待办不应能删除")?;

    let stored: Value = serde_json::from_slice(&std::fs::read(data.join("todos.json"))?)?;
    check(stored.as_array().is_some_and(|items| items[0]["id"] == todo_id.as_str() && items[0]["daily"] == true), format!("待办文件异常：{stored}"))?;

    process::call(runtime.pipe(), "todoDelete", Some(json!({"id": todo_id})))?;
    check(process::call(runtime.pipe(), "todos", None)?["todos"] == json!([]), "待办未清空")?;
    Ok(())
}

fn check_settings_updates(runtime: &Runtime, runtime_directory: &Path) -> Result<()> {
    let original=settings(runtime)?;
    for (key,color) in [("SettingsLightAccentColor","#9B4BC7"),("SettingsDarkAccentColor","#E5B86A")]{
        update(runtime,key,json!(color))?;check(settings(runtime)?[key]==color,"主题色未保存")?;
        check(rejected(runtime,"update",Some(json!({"key":key,"value":"#bad"})))?,"主题色应拒绝非法色号")?;
    }
    process::call(runtime.pipe(),"resetSettingsColors",None)?;
    let reset=settings(runtime)?;
    check(reset["SettingsLightAccentColor"]=="#2563EB"&&reset["SettingsDarkAccentColor"]=="#6EA8FF"&&reset["Motion"]==original["Motion"],"配色还原影响动效")?;
    update(runtime,"SettingsLightAccentColor",json!("#9B4BC7"))?;
    update(runtime,"SettingsDarkAccentColor",json!("#E5B86A"))?;
    update(runtime,"MotionDuration",json!(900.))?;update(runtime,"SpringMass",json!(1.7))?;
    update(runtime,"Animation",json!(false))?;update(runtime,"IslandFluidMotion",json!(false))?;
    process::call(runtime.pipe(),"resetMotion",None)?;
    let reset=settings(runtime)?;
    check(reset["Motion"]==original["Motion"]&&reset["Animation"]==true&&reset["IslandFluidMotion"]==true&&reset["SettingsLightAccentColor"]=="#9B4BC7"&&reset["SettingsDarkAccentColor"]=="#E5B86A","动效还原不完整或改变主题色")?;
    for value in ["Classic","Acrylic","LiquidGlass","Acrylic"]{update(runtime,"IslandMaterial",json!(value))?;check(settings(runtime)?["IslandMaterial"]==value,"展开卡片材质切换未保存")?;}
    check(rejected(runtime,"update",Some(json!({"key":"IslandMaterial","value":"acrylic"})))?,"材质应拒绝未知取值")?;
    process::call(runtime.pipe(), "update", Some(json!({"key": "AutoCompactWidth", "value": true})))?;
    check(settings(runtime)?["AutoCompactWidth"] == true, "自适应宽度未生效")?;

    for (key, value) in [
        ("AutoCompactWidth", json!("yes")),
        ("NotificationSound", json!("Chime")),
        ("TodoVolume", json!(120.0)),
        ("ChineseTwoLine", json!(1)),
    ] {
        check(rejected(runtime, "update", Some(json!({"key": key, "value": value})))?, format!("未拒绝无效设置 {key}={value}"))?;
    }

    check(rejected(runtime, "update", Some(json!({"key": "NotificationSoundPath", "value": runtime_directory.join("CHANGELOG.md")})))?, "非 WAV 提示音不应被接受")?;

    process::call(runtime.pipe(), "soundPreview", Some(json!({"todo": false})))?;
    process::call(runtime.pipe(), "soundPreview", Some(json!({"todo": true})))?;
    update(runtime, "AutoCompactWidth", json!(false))?;
    Ok(())
}

fn check_adaptive_width(runtime: &Runtime) -> Result<Value> {
    let stored = settings(runtime)?["CompactWidth"].as_f64().unwrap_or(0.0);

    let settle = |runtime: &Runtime| -> Result<Value> { sleep(1.2); state(runtime) };

    update(runtime, "AutoCompactWidth", json!(false))?;
    let current = settle(runtime)?;
    check(current["compactWidth"].as_f64().is_some_and(|width| (width - stored).abs() < 1e-9), format!("关闭自适应后宽度应为 {stored}：{current}"))?;
    check(current["compactHeight"].as_f64().is_some_and(|height| (height - 40.0).abs() < 1e-9), format!("折叠高度异常：{current}"))?;
    check(current["compactSecondary"] == "", format!("占位行不应有第二行：{current}"))?;

    update(runtime, "AutoCompactWidth", json!(true))?;
    let current = settle(runtime)?;
    let adaptive = current["compactWidth"].as_f64().unwrap_or(0.0);
    check((140.0..stored).contains(&adaptive), format!("自适应宽度应收缩到 [{}, {stored})：{adaptive}", 140.0))?;

    update(runtime, "AutoCompactWidth", json!(false))?;
    let current = settle(runtime)?;
    check(current["compactWidth"].as_f64().is_some_and(|width| (width - stored).abs() < 1e-9), "关闭自适应后未恢复保存宽度")?;
    Ok(json!({"stored": stored, "adaptive": true}))
}

fn check_tools(runtime: &mut Runtime) -> Result<Value> {
    for (key, value) in [("NotificationVolume", 0), ("TodoVolume", 0), ("DeviceVolume", 0)] {
        update(runtime, key, json!(value))?;
    }
    update(runtime, "TodoRemindersEnabled", json!(false))?;
    update(runtime, "AutoCompactMaxWidth", json!(180))?;
    check(rejected(runtime, "update", Some(json!({"key": "AutoCompactMaxWidth", "value": 90})))?, "宽度上限 90 应被拒绝")?;
    update(runtime, "AutoCompactWidth", json!(true))?;
    sleep(1.0);
    let width = state(runtime)?["compactWidth"].as_f64().unwrap_or(1e9);
    check(width <= 180.1, format!("自适应宽度未遵守上限：{width}"))?;
    update(runtime, "AutoCompactWidth", json!(false))?;

    let mut ids = Vec::new();
    for item in [
        json!({"mode": "countdown", "duration": 60}),
        json!({"mode": "stopwatch"}),
        json!({"mode": "pomodoro", "focus": 60, "rest": 5, "rounds": 2}),
    ] {
        ids.push(process::call(runtime.pipe(), "timerCreate", Some(item))?["id"].as_str().unwrap_or("").to_owned());
    }
    sleep(0.2);
    let current = state(runtime)?;
    check(current["timers"].as_array().is_some_and(|timers| timers.len() == 3), format!("计时器数量异常：{current}"))?;
    check(current["activeComponent"] == format!("timer:{}", ids[2]), format!("新计时器未选中：{current}"))?;

    process::call(runtime.pipe(), "timerAction", Some(json!({"id": ids[0], "action": "toggle"})))?;
    let paused = state(runtime)?["timers"].as_array().and_then(|timers| timers.iter().find(|timer| timer["id"] == ids[0].as_str())).map(|timer| timer["elapsedMs"].clone()).unwrap_or(Value::Null);
    sleep(0.3);
    let after = state(runtime)?["timers"].as_array().and_then(|timers| timers.iter().find(|timer| timer["id"] == ids[0].as_str())).map(|timer| timer["elapsedMs"].clone()).unwrap_or(Value::Null);
    check(paused == after, format!("暂停后计时仍变化：{paused} -> {after}"))?;

    process::call(runtime.pipe(), "timerAction", Some(json!({"id": ids[0], "action": "reset"})))?;
    let clock = state(runtime)?["timers"].as_array().and_then(|timers| timers.iter().find(|timer| timer["id"] == ids[0].as_str())).map(|timer| timer["clock"].clone()).unwrap_or(Value::Null);
    check(clock == "01:00", format!("重置后应为 01:00：{clock}"))?;
    process::call(runtime.pipe(), "timerAction", Some(json!({"id": ids[0], "action": "toggle"})))?;

    process::call(runtime.pipe(), "islandSelect", Some(json!(format!("timer:{}", ids[1]))))?;
    process::call(runtime.pipe(), "islandCycle", Some(json!(1)))?;
    check(state(runtime)?["activeComponent"] == format!("timer:{}", ids[2]), "轮换未落到下一个计时器")?;

    process::call(runtime.pipe(), "devicePreview", None)?;
    let generation = state(runtime)?["rendererGeneration"].clone();
    for volume in (0..50).step_by(5) { update(runtime, "DeviceVolume", json!(volume))?; }
    check(state(runtime)?["rendererGeneration"] == generation, "音量更新不应重建渲染器")?;
    update(runtime, "DeviceVolume", json!(0))?;
    check(state(runtime)?["notificationVisible"] == true, "设备提醒不应因音量为零而消失")?;

    for invalid in [json!({"mode": "bad"}), json!({"duration": 0}), json!({"mode": "pomodoro", "rounds": 17})] {
        check(rejected(runtime, "timerCreate", Some(invalid))?, "无效计时器不应被接受")?;
    }
    for timer_id in &ids { process::call(runtime.pipe(), "timerAction", Some(json!({"id": timer_id, "action": "delete"})))?; }
    check(state(runtime)?["timers"] == json!([]), "计时器未清空")?;

    let todo = process::call(runtime.pipe(), "todoSave", Some(json!({"title": "配色测试", "date": "2099-01-01", "time": "09:00", "color": "#B7DBCB"})))?;
    let todo_id = todo["id"].as_str().unwrap_or("").to_owned();
    let color = state(runtime)?["todos"].as_array().and_then(|items| items.iter().find(|item| item["id"] == todo_id.as_str())).map(|item| item["color"].clone()).unwrap_or(Value::Null);
    check(color == "#B7DBCB", format!("待办颜色未保存：{color}"))?;
    check(rejected(runtime, "todoSave", Some(json!({"title": "无效颜色", "date": "2099-01-01", "time": "09:00", "color": "white"})))?, "无效颜色不应被接受")?;
    process::call(runtime.pipe(), "todoDelete", Some(json!({"id": todo_id})))?;

    let timer_id = process::call(runtime.pipe(), "timerCreate", Some(json!({"mode": "stopwatch"})))?["id"].as_str().unwrap_or("").to_owned();
    let before = state(runtime)?["timers"][0]["elapsedMs"].as_i64().unwrap_or(0);
    let started = Instant::now();
    sleep(5.0);
    let after = state(runtime)?["timers"][0]["elapsedMs"].as_i64().unwrap_or(0);
    let wall = started.elapsed().as_millis() as i64;
    check((after - before - wall).abs() < 100, format!("计时漂移：{}ms 对 {}ms", after - before, wall))?;

    runtime.stop();
    runtime.start()?;
    let restored = state(runtime)?["timers"].as_array().and_then(|timers| timers.iter().find(|timer| timer["id"] == timer_id.as_str())).cloned().unwrap_or(Value::Null);
    check(restored["running"] == false && restored["elapsedMs"].as_i64().unwrap_or(0) >= 200, format!("恢复的计时器异常：{restored}"))?;
    process::call(runtime.pipe(), "timerAction", Some(json!({"id": timer_id, "action": "delete"})))?;

    Ok(json!({
        "concurrentTimers": true,
        "pauseResetResume": true,
        "cycleWithSwipeDisabled": true,
        "adaptiveMaximum": true,
        "pastelColor": true,
        "pausedSessionRestore": true,
        "devicePreview": true,
        "realHardwareTested": false
    }))
}

fn check_diagnostics(runtime: &Runtime) -> Result<Value> {
    let reply = process::call(runtime.pipe(), "collectLogs", None)?;
    let path = PathBuf::from(reply["path"].as_str().ok_or_else(|| ToolError("日志包未返回路径".into()))?);
    check(path.is_file(), format!("日志包不存在：{}", path.display()))?;
    let logs = runtime.data().join("Logs");
    check(crate::verify::paths_equal(path.parent().unwrap_or(Path::new("")), &logs), format!("日志包位置异常：{}", path.display()))?;
    let text = std::fs::read_to_string(&path)?.trim_start_matches('\u{feff}').to_owned();
    for marker in ["DotIsle 诊断日志", "错误摘要", "日志尾部"] {
        check(text.contains(marker), format!("日志包缺少“{marker}”"))?;
    }
    Ok(json!({"success": true, "file": path.file_name().unwrap_or_default().to_string_lossy(), "bytes": std::fs::metadata(&path)?.len()}))
}

fn run_child(mut command: Command, timeout: Duration) -> Result<bool> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? { return Ok(status.success()); }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn check_settings_selfcheck(runtime: &Runtime, workspace: &Path) -> Result<Value> {
    let directory = workspace.join("settings-smoke");
    std::fs::create_dir_all(&directory)?;
    let mut command = Command::new(runtime.directory().join("LyricIsland.Settings.exe"));
    command.arg("--smoke-ui").arg(&directory)
        .env("LYRICISLAND_TEST_PIPE", runtime.pipe())
        .env("LYRICISLAND_NATIVE_DATA", runtime.data())
        .env("LYRICISLAND_SMOKE_REGRESSIONS", "1");
    let passed = run_child(command, Duration::from_secs(120))?;
    let report = read_json(&directory.join("report.json"))?;
    check(passed && report["success"] == true, format!("设置窗口自检失败：{report}"))?;
    check(report["pages"] == 7, format!("设置页数量异常：{report}"))?;
    check(report["todoRegression"] == true, format!("待办回归失败：{report}"))?;
    check(report["whitelistExpandCollapse"] == true, format!("白名单展开回归失败：{report}"))?;
    check(report["settingsTabs"] == true, format!("设置分页回归失败：{report}"))?;
    check(report["sharedSettingsNav"] == true, format!("共享导航回归失败：{report}"))?;
    check(report["soundSelection"] == json!({"success": true, "combinations": 12, "invalidRejected": 18}), format!("音效选择回归失败：{report}"))?;
    Ok(report)
}

fn check_sound_persistence(runtime: &mut Runtime) -> Result<Value> {
    let keys = ["NotificationSound", "TodoSound", "DeviceSound"];
    let tones = ["Soft", "Glass", "Bell", "Connect"];
    for key in ["NotificationVolume", "TodoVolume", "DeviceVolume"] { update(runtime, key, json!(0))?; }
    for round in 0..4 {
        let mut expected = Map::new();
        for (index, key) in keys.iter().enumerate() {
            expected.insert((*key).to_owned(), json!(tones[(round + index) % 4]));
        }
        for (key, value) in &expected { update(runtime, key, value.clone())?; }
        let stored = settings(runtime)?;
        for (key, value) in &expected {
            check(stored[key.as_str()] == *value, format!("音效未写入设置：{key} {stored}"))?;
        }
        runtime.stop();
        runtime.start()?;
        let stored = settings(runtime)?;
        for (key, value) in &expected {
            check(stored[key.as_str()] == *value, format!("重启后音效丢失：{key} {stored}"))?;
        }
        for (command, value) in [("soundPreview", Some(json!({"todo": false}))), ("soundPreview", Some(json!({"todo": true}))), ("deviceSoundPreview", None)] {
            process::call(runtime.pipe(), command, value)?;
        }
    }
    Ok(json!({"combinations": 12, "restarts": 4, "previewCommandsAcceptedAtZeroVolume": 12}))
}

fn check_persistence(runtime: &mut Runtime, data: &Path) -> Result<Runtime> {
    let today = state(runtime)?["today"].as_str().unwrap_or("").to_owned();
    process::call(runtime.pipe(), "todoSave", Some(json!({"title": "交稿", "content": "", "date": today, "time": "20:00", "daily": false})))?;
    let directory = runtime.directory().to_path_buf();
    let run_ms = runtime.run_ms();
    runtime.stop();
    let mut restarted = Runtime::new(directory, data.to_path_buf(), run_ms);
    restarted.start()?;
    let persisted=settings(&restarted)?;
    check(persisted["SettingsLightAccentColor"]=="#9B4BC7"&&persisted["SettingsDarkAccentColor"]=="#E5B86A","两套主题色重启后未保留")?;
    let listed = process::call(restarted.pipe(), "todos", None)?;
    check(listed["todos"].as_array().is_some_and(|todos| todos.len() == 1 && todos[0]["title"] == "交稿"), format!("待办未持久化：{listed}"))?;
    Ok(restarted)
}

fn check_bootstrap(share: &Path, workspace: &Path, version: &str) -> Result<Value> {
    let directory = workspace.join("bootstrap");
    std::fs::create_dir_all(&directory)?;
    let mut command = Command::new(share);
    command.arg(format!("/VERIFYDIR={}",directory.display())).arg("/VERYSILENT").arg("/SUPPRESSMSGBOXES");
    // Verify mode intentionally cancels the setup wizard after checking the embedded Rust payload.
    let _=run_child(command,Duration::from_secs(60))?;
    check(directory.join("verification.json").is_file(),"标准安装器的嵌入载荷自检未通过")?;
    let text = std::fs::read_to_string(directory.join("verification.json"))?;
    let report: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    check(report["version"].as_str() == Some(version), format!("安装包版本异常：{report}"))?;
    Ok(report)
}

fn digests(directory: &Path, version: &str) -> Result<Map<String, Value>> {
    let names = ["DotIsle-1.3-Setup.exe", "DotIsle-1.3-Source.zip", "DotIsle-1.3-SHA256.txt"];
    let mut rows = Map::new();
    for original in names {
        let name = original.replace("1.3", version);
        let path = directory.join(&name);
        if path.is_file() { rows.insert(name, json!(sha256(&path)?)); }
    }
    Ok(rows)
}

pub fn run(args: &[String]) -> Result<()> {
    let runtime_directory = args.first().map(PathBuf::from).ok_or_else(|| ToolError("缺少运行时目录".into()))?;
    let runtime_directory = runtime_directory.canonicalize().map(strip_verbatim).unwrap_or(runtime_directory);
    check(runtime_directory.join("LyricIsland.exe").is_file(), format!("缺少主程序：{}", runtime_directory.display()))?;
    let report_path = arg_value(args, "--report").map(PathBuf::from);
    let artifacts = arg_value(args, "--artifacts").map(PathBuf::from);
    let version = arg_value(args, "--version").unwrap_or_else(|| default_version().to_owned());
    let workspace = temp_dir("LyricIsland.Verify13.");
    let data = workspace.join("Data");
    std::fs::create_dir_all(&data)?;

    let mut report = Map::new();
    report.insert("success".into(), json!(false));
    report.insert("version".into(), json!(version));
    report.insert("runtime".into(), json!(runtime_directory.to_string_lossy()));
    report.insert("testOverrides".into(), json!({"AllowedSources": ["LyricIsland.Verify13"], "OnlineSongInfo": false}));
    let mut runtime = Runtime::new(&runtime_directory, &data, 60_000);

    let outcome = (|| -> Result<()> {
        std::fs::write(data.join("settings.json"), "{\"AllowedSources\":[\"LyricIsland.Verify13\"],\"OnlineSongInfo\":false}")?;
        let initial = runtime.start()?;
        check(initial["version"].as_str() == Some(version.as_str()), format!("运行时版本异常：{}", initial["version"]))?;
        report.insert("nativeReportsVersion".into(), json!(true));
        report.insert("defaults".into(), check_defaults(&runtime)?);
        check(initial["todos"].is_array() && initial["pendingPage"].is_null(), "初始状态缺少待办或残留页面")?;
        report.insert("todosInState".into(), json!(true));

        check_todos(&runtime, &data)?;
        report.insert("todoContract".into(), json!(true));
        check_settings_updates(&runtime, &runtime_directory)?;
        report.insert("settingsValidation".into(), json!(true));
        report.insert("adaptiveWidth".into(), check_adaptive_width(&runtime)?);
        report.insert("tools".into(), check_tools(&mut runtime)?);
        report.insert("diagnostics".into(), check_diagnostics(&runtime)?);
        report.insert("settingsSelfCheck".into(), check_settings_selfcheck(&runtime, &workspace)?);

        report.insert("soundPersistence".into(), check_sound_persistence(&mut runtime)?);
        runtime = check_persistence(&mut runtime, &data)?;
        report.insert("todoPersistence".into(), json!(true));
        runtime.stop();

        if let Some(artifacts) = &artifacts {
            let artifacts = artifacts.canonicalize().map(strip_verbatim).unwrap_or_else(|_| artifacts.clone());
            report.insert("digests".into(), Value::Object(digests(&artifacts, &version)?));
            let share = artifacts.join(format!("DotIsle-{version}-Setup.exe"));
            if share.is_file() { report.insert("bootstrap".into(), check_bootstrap(&share, &workspace, &version)?); }
        }
        report.insert("success".into(), json!(true));
        Ok(())
    })();
    runtime.stop();

    let value = Value::Object(report);
    if let Some(path) = &report_path { write_json(path, &value)?; }
    outcome?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

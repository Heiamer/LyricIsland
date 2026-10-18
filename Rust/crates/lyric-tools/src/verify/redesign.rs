//! The redesign contract: focus count-up, direction edits, workflow stages, laps and OSD placement.

use crate::common::{check, default_version, strip_verbatim, write_json, Result, ToolError};
use crate::process::{self, Runtime};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

fn sleep(seconds: f64) { std::thread::sleep(Duration::from_secs_f64(seconds)); }

fn state(runtime: &Runtime) -> Result<Value> { process::call(runtime.pipe(), "state", None) }

fn cmd(runtime: &Runtime, name: &str, value: Option<Value>) -> Result<Value> {
    process::call(runtime.pipe(), name, value)
}

fn rejected(runtime: &Runtime, name: &str, value: Option<Value>) -> Result<bool> {
    let reply = process::request(runtime.pipe(), name, value, Duration::from_secs(15))?;
    Ok(reply.get("success").and_then(Value::as_bool) != Some(true))
}

fn timer(runtime: &Runtime, id: &str) -> Result<Value> {
    let current = state(runtime)?;
    current["timers"].as_array().and_then(|timers| timers.iter().find(|timer| timer["id"] == id)).cloned()
        .ok_or_else(|| ToolError(format!("找不到计时器 {id}")))
}

fn create(runtime: &Runtime, value: Value) -> Result<String> {
    let before: Vec<String> = state(runtime)?["timers"].as_array().into_iter().flatten()
        .filter_map(|timer| timer["id"].as_str().map(str::to_owned)).collect();
    cmd(runtime, "timerCreate", Some(value))?;
    let current = state(runtime)?;
    current["timers"].as_array().into_iter().flatten()
        .filter_map(|timer| timer["id"].as_str())
        .find(|id| !before.iter().any(|known| known == id))
        .map(str::to_owned)
        .ok_or_else(|| ToolError("计时器未创建".into()))
}

pub fn run(args: &[String]) -> Result<()> {
    let runtime_directory = args.first().map(PathBuf::from).ok_or_else(|| ToolError("缺少运行时目录".into()))?;
    let runtime_directory = runtime_directory.canonicalize().map(strip_verbatim).unwrap_or(runtime_directory);
    let out = args.get(1).map(PathBuf::from).ok_or_else(|| ToolError("缺少输出目录".into()))?;
    let out = out.canonicalize().map(strip_verbatim).unwrap_or(out);
    std::fs::create_dir_all(&out)?;
    let data = out.join("Data");
    std::fs::create_dir_all(&data)?;
    std::fs::write(data.join("settings.json"), serde_json::to_vec(&json!({
        "DefaultsOnboardingVersion": 1,
        "AllowedSources": ["DotIsle.QA"],
        "OnlineSongInfo": false,
        "NotificationsEnabled": false,
        "DeviceRemindersEnabled": false,
        "SystemStateHintsEnabled": false,
        "AutoCheckUpdatesEnabled": false
    }))?)?;

    let version = default_version().to_owned();
    let mut runtime = Runtime::new(&runtime_directory, &data, 90_000);
    let mut report = json!({"success": false, "version": version});
    let outcome = (|| -> Result<()> {
        let initial = runtime.start()?;
        check(initial["version"].as_str() == Some(version.as_str()), format!("运行时版本异常：{}", initial["version"]))?;
        check(initial["settings"]["FocusCountUp"] == false, "正计时默认应关闭")?;
        cmd(&runtime, "update", Some(json!({"key": "FocusCountUp", "value": true})))?;
        check(state(&runtime)?["settings"]["FocusCountUp"] == true, "正计时未开启")?;
        check(rejected(&runtime, "update", Some(json!({"key": "FocusCountUp", "value": "true"})))?, "正计时应拒绝非布尔值")?;

        let id = create(&runtime, json!({"mode": "pomodoro", "focus": 60, "rounds": 1}))?;
        sleep(0.25);
        cmd(&runtime, "timerAction", Some(json!({"id": id, "action": "pause"})))?;
        let before = timer(&runtime, &id)?;
        cmd(&runtime, "timerAction", Some(json!({"id": id, "action": "direction", "countUp": true, "revision": before["revision"]})))?;
        let after = timer(&runtime, &id)?;
        check(after["countUp"] == true && after["elapsedMs"] == before["elapsedMs"] && after["running"] == false && after["phase"] == before["phase"],
            format!("切换方向改变了计时状态：{before} -> {after}"))?;
        check(rejected(&runtime, "timerAction", Some(json!({"id": id, "action": "direction", "countUp": false, "revision": before["revision"]})))?, "过期修订不应被接受")?;
        check(rejected(&runtime, "timerAction", Some(json!({"id": id, "action": "direction", "countUp": "yes"})))?, "方向应拒绝非布尔值")?;
        report["directionKeepsElapsedAndRejectsStaleEdits"] = json!(true);

        let flow = create(&runtime, json!({
            "mode": "workflow", "countUp": true,
            "stages": [{"name": "专注", "seconds": 60}, {"name": "休息", "seconds": 60}]
        }))?;
        cmd(&runtime, "timerAction", Some(json!({"id": flow, "action": "pause"})))?;
        let flow_before = timer(&runtime, &flow)?;
        cmd(&runtime, "timerAction", Some(json!({"id": flow, "action": "direction", "countUp": false, "revision": flow_before["revision"]})))?;
        let flow_after = timer(&runtime, &flow)?;
        check(flow_after["elapsedMs"] == flow_before["elapsedMs"] && flow_after["workflow"] == flow_before["workflow"], "切换方向改变了流程进度")?;
        report["workflowDirectionKeepsStage"] = json!(true);

        let stopwatch = create(&runtime, json!({"mode": "stopwatch"}))?;
        sleep(0.3);
        cmd(&runtime, "timerAction", Some(json!({"id": stopwatch, "action": "lap"})))?;
        sleep(0.2);
        cmd(&runtime, "timerAction", Some(json!({"id": stopwatch, "action": "lap"})))?;
        cmd(&runtime, "timerAction", Some(json!({"id": stopwatch, "action": "pause"})))?;
        let laps = timer(&runtime, &stopwatch)?["laps"].clone();
        let values: Vec<i64> = laps.as_array().into_iter().flatten().filter_map(Value::as_i64).collect();
        let elapsed = timer(&runtime, &stopwatch)?["elapsedMs"].as_i64().unwrap_or(0);
        check(values.len() == 2 && values[0] > 0 && values[0] < values[1] && values[1] <= elapsed, format!("计次异常：{laps}"))?;
        check(rejected(&runtime, "timerAction", Some(json!({"id": stopwatch, "action": "lap"})))?, "暂停时不应记录计次")?;
        report["stopwatchLapsNativeAuthoritative"] = json!(true);

        cmd(&runtime, "islandSelect", Some(json!(format!("timer:{stopwatch}"))))?;
        cmd(&runtime, "expand", None)?;
        check(rejected(&runtime, "update", Some(json!({"key": "SystemStateHintPlacement", "value": "Other"})))?, "系统状态位置应拒绝未知值")?;
        cmd(&runtime, "update", Some(json!({"key": "SystemStateHintPlacement", "value": "Bottom"})))?;
        let before = state(&runtime)?;
        cmd(&runtime, "osdPreview", Some(json!({"kind": "caps", "on": true})))?;
        sleep(0.18);
        let after = state(&runtime)?;
        check(after["bottomOsdVisible"] == true && after["activeComponent"] == before["activeComponent"] && after["expanded"] == before["expanded"] && after["notificationVisible"] == false,
            format!("底部 OSD 干扰了灵动岛：{after}"))?;
        cmd(&runtime, "osdPreview", Some(json!({"kind": "num", "on": false})))?;
        sleep(0.08);
        check(state(&runtime)?["bottomOsdVisible"] == true, "关闭 num 不应抢走 caps")?;
        sleep(2.0);
        check(state(&runtime)?["bottomOsdVisible"] == false, "底部 OSD 未过期")?;
        report["bottomOsdSeparateReplacesAndExpires"] = json!(true);

        cmd(&runtime, "update", Some(json!({"key": "SystemStateHintPlacement", "value": "Island"})))?;
        cmd(&runtime, "osdPreview", Some(json!({"kind": "mic", "on": true})))?;
        sleep(0.12);
        let current = state(&runtime)?;
        check(current["osdVisible"] == true && current["bottomOsdVisible"] == false, format!("岛内 OSD 未生效：{current}"))?;
        cmd(&runtime, "update", Some(json!({"key": "SystemStateHintPlacement", "value": "Bottom"})))?;
        sleep(0.6);
        check(state(&runtime)?["osdVisible"] == false, "切换位置后岛内 OSD 未清除")?;
        report["osdPlacementValidationAndSwitching"] = json!(true);

        cmd(&runtime, "workflowTemplateSave", Some(json!({"name": "QA 模板", "stages": [{"name": "a", "seconds": 60}, {"name": "b", "seconds": 60}]})))?;
        let template = state(&runtime)?["templates"][0]["id"].as_str().unwrap_or("").to_owned();
        cmd(&runtime, "workflowTemplateDelete", Some(json!(template)))?;
        check(state(&runtime)?["templates"] == json!([]), "模板未删除")?;
        report["templateDeleteContract"] = json!(true);

        cmd(&runtime, "timerAction", Some(json!({"id": id, "action": "direction", "countUp": true})))?;
        let modules=json!([{"id":"qa-focus","name":"专注模块","kind":1,"minutes":25},{"id":"qa-copy","name":"专注模块 副本","kind":1,"minutes":17}]);
        cmd(&runtime,"update",Some(json!({"key":"WorkflowModules","value":modules})))?;
        for layout in ["Separated","SideBySide","LyricFocus"]{cmd(&runtime,"update",Some(json!({"key":"MusicExpandedLayout","value":layout})))?;}
        check(rejected(&runtime,"update",Some(json!({"key":"MusicExpandedLayout","value":"Bad"})))?,"布局必须校验")?;
        cmd(&runtime,"update",Some(json!({"key":"SwipeUpHide","value":false})))?;
        cmd(&runtime,"update",Some(json!({"key":"GestureTutorialSeen","value":true})))?;
        runtime.stop();
        runtime.start()?;
        check(timer(&runtime, &id)?["countUp"] == true, "方向未持久化")?;
        check(timer(&runtime, &stopwatch)?["laps"] == laps, "计次未持久化")?;
        check(state(&runtime)?["settings"]["FocusCountUp"] == true, "FocusCountUp 未持久化")?;
        check(state(&runtime)?["settings"]["SystemStateHintPlacement"] == "Bottom", "OSD 位置未持久化")?;
        cmd(&runtime, "timerAction", Some(json!({"id": stopwatch, "action": "reset"})))?;
        check(timer(&runtime, &stopwatch)?["laps"] == json!([]), "重置未清空计次")?;
        report["directionLapsAndPlacementPersist"] = json!(true);
        let saved=state(&runtime)?;
        check(saved["settings"]["WorkflowModules"]==modules,"模块库未持久化")?;
        check(saved["settings"]["MusicExpandedLayout"]=="LyricFocus"&&saved["settings"]["SwipeUpHide"]==false&&saved["settings"]["GestureTutorialSeen"]==true,"布局、手势或教程未持久化")?;
        report["modulesThreeLayoutsGestureAndTutorialPersist"]=json!(true);
        report["success"] = json!(true);
        Ok(())
    })();
    runtime.stop();
    write_json(&out.join("report.json"), &report)?;
    outcome?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

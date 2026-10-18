//! Notification reader compatibility against a synthetic SQLite store (never the user's DB).

use crate::bail;
use crate::common::{arg_value, check, sha256, strip_verbatim, write_json, Result, ToolError};
use crate::process::{self, Runtime};
use crate::sqlite::{hex, Database};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn stamp() -> i64 { (SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() / 100) as i64 + 116_444_736_000_000_000 }

fn insert(database: &Database, sequence: &mut i64, handler: i64, xml: Option<&str>, arrival: Option<i64>, identity: Option<i64>) -> Result<()> {
    *sequence += 1;
    let payload = xml.unwrap_or("<toast><visual><binding template=\"ToastGeneric\"><text>Compatibility test</text><text>Synthetic message</text></binding></visual></toast>");
    let sql = format!(
        "INSERT INTO Notification VALUES({},{},{},'toast',X'{}',{})",
        *sequence,
        identity.unwrap_or(*sequence),
        handler,
        hex(payload.as_bytes()),
        arrival.unwrap_or_else(stamp)
    );
    database.exec(&sql)
}

fn state(runtime: &Runtime) -> Result<Value> { process::request(runtime.pipe(), "state", None, Duration::from_secs(8)) }

fn update(runtime: &Runtime, key: &str, value: Value) -> Result<()> {
    let reply = process::request(runtime.pipe(), "update", Some(json!({"key": key, "value": value})), Duration::from_secs(8))?;
    check(reply["success"] == true, format!("设置 {key} 失败：{reply}"))?;
    Ok(())
}

fn wait_visible(runtime: &Runtime, want: bool, seconds: f64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    loop {
        if state(runtime)?["notificationVisible"] == want { return Ok(()); }
        if Instant::now() >= deadline { bail!("通知可见性未变为 {want}"); }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn fingerprints(db: &Path) -> Result<(String, String)> {
    let wal = PathBuf::from(format!("{}-wal", db.display()));
    Ok((sha256(db)?, sha256(&wal)?))
}

pub fn run(args: &[String]) -> Result<()> {
    let runtime_directory = arg_value(args, "--runtime").or_else(|| args.first().cloned()).map(PathBuf::from)
        .ok_or_else(|| ToolError("缺少 --runtime".into()))?;
    let work = arg_value(args, "--work").or_else(|| args.get(1).cloned()).map(PathBuf::from)
        .ok_or_else(|| ToolError("缺少 --work".into()))?;
    let runtime_directory = runtime_directory.canonicalize().map(strip_verbatim).unwrap_or(runtime_directory);
    let root = work.canonicalize().map(strip_verbatim).unwrap_or(work);
    std::fs::create_dir_all(&root)?;
    let db_path = root.join("synthetic.db");
    let data = root.join("Data");
    std::fs::create_dir_all(&data)?;
    check(!db_path.exists(), "请使用全新的测试目录")?;

    let library = runtime_directory.join("e_sqlite3.dll");
    let database = Database::open(&library, &db_path)?;
    database.exec("PRAGMA journal_mode=WAL")?;
    database.exec("CREATE TABLE NotificationHandler(RecordId INTEGER PRIMARY KEY, PrimaryId TEXT); CREATE TABLE Notification([Order] INTEGER PRIMARY KEY, Id INTEGER, HandlerId INTEGER, Type TEXT, Payload BLOB, ArrivalTime INTEGER); INSERT INTO NotificationHandler VALUES(1,'Example.Allowed'),(2,'Example.Blocked');")?;
    let mut sequence = 0;
    insert(&database, &mut sequence, 1, None, Some(stamp() - 600_000_000), None)?;

    std::fs::write(data.join("settings.json"), serde_json::to_vec(&json!({
        "NotificationsEnabled": true,
        "NotificationWhitelistEnabled": true,
        "NotificationAllowedApps": ["Example.Allowed"],
        "AllowedSources": ["LyricIsland.TestOnly"],
        "OnlineSongInfo": false
    }))?)?;

    let mut runtime = Runtime::with_pipe(&runtime_directory, &data, 60_000, "LyricIsland.Settings.test.compat.");
    runtime.set_env("LYRICISLAND_NOTIFICATION_DB", &db_path.to_string_lossy());
    let mut report = Map::new();
    let outcome = (|| -> Result<()> {
        runtime.start()?;
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut current = Value::Null;
        while Instant::now() < deadline {
            current = state(&runtime)?;
            if current["notificationState"]["access"] == "compatible" { break; }
            std::thread::sleep(Duration::from_millis(100));
        }
        check(current["notificationState"]["access"] == "compatible", format!("通知读取不兼容：{}", current["notificationState"]))?;
        let settings = &current["settings"];
        check(settings["NotificationClickOpenApp"] == true && settings["MusicClickOpenApp"] == false && settings["PauseNotifications"] == false, format!("通知默认设置异常：{settings}"))?;
        std::thread::sleep(Duration::from_millis(700));
        check(state(&runtime)?["notificationVisible"] == false, "历史通知不应回放")?;

        let before = fingerprints(&db_path)?;
        std::thread::sleep(Duration::from_millis(300));
        check(before == fingerprints(&db_path)?, "读取过程修改了数据库或 WAL")?;
        report.insert("defaultCompatibility".into(), json!(true));
        report.insert("noHistoryReplay".into(), json!(true));
        report.insert("readOnlyMainAndWal".into(), json!(true));

        insert(&database, &mut sequence, 1, None, None, None)?;
        wait_visible(&runtime, true, 4.0)?;
        report.insert("newWalNotification".into(), json!(true));

        update(&runtime, "PauseNotifications", json!(true))?;
        wait_visible(&runtime, false, 4.0)?;
        insert(&database, &mut sequence, 1, None, None, None)?;
        std::thread::sleep(Duration::from_millis(700));
        check(state(&runtime)?["notificationVisible"] == false, "暂停期间不应显示通知")?;
        update(&runtime, "PauseNotifications", json!(false))?;
        std::thread::sleep(Duration::from_millis(1000));
        check(state(&runtime)?["notificationVisible"] == false, "恢复后不应补播暂停期间的积压")?;
        report.insert("pauseAndResumeWithoutBacklog".into(), json!(true));

        insert(&database, &mut sequence, 2, None, None, None)?;
        std::thread::sleep(Duration::from_millis(800));
        check(state(&runtime)?["notificationVisible"] == false, "白名单外应用不应通知")?;
        report.insert("allowlist".into(), json!(true));

        update(&runtime, "NotificationWhitelistEnabled", json!(false))?;
        insert(&database, &mut sequence, 2, None, None, None)?;
        wait_visible(&runtime, true, 4.0)?;
        update(&runtime, "PauseNotifications", json!(true))?;
        wait_visible(&runtime, false, 4.0)?;
        update(&runtime, "PauseNotifications", json!(false))?;
        update(&runtime, "NotificationWhitelistEnabled", json!(true))?;
        report.insert("whitelistCanBeDisabled".into(), json!(true));

        insert(&database, &mut sequence, 1, Some("<broken>"), None, None)?;
        std::thread::sleep(Duration::from_millis(800));
        check(state(&runtime)?["notificationVisible"] == false, "损坏载荷不应通知")?;
        let same = stamp();
        insert(&database, &mut sequence, 1, None, Some(same), Some(999))?;
        wait_visible(&runtime, true, 4.0)?;
        update(&runtime, "PauseNotifications", json!(true))?;
        wait_visible(&runtime, false, 4.0)?;
        update(&runtime, "PauseNotifications", json!(false))?;
        std::thread::sleep(Duration::from_millis(600));
        insert(&database, &mut sequence, 1, None, Some(same), Some(999))?;
        std::thread::sleep(Duration::from_millis(800));
        check(state(&runtime)?["notificationVisible"] == false, "重复通知不应再次显示")?;
        report.insert("malformedPayloadAndDuplicateSuppressed".into(), json!(true));

        insert(&database, &mut sequence, 1, None, None, None)?;
        wait_visible(&runtime, true, 4.0)?;
        report.insert("resumesForFreshMessages".into(), json!(true));
        report.insert("success".into(), json!(true));
        Ok(())
    })();
    runtime.stop();
    drop(database);
    if outcome.is_ok() { write_json(&root.join("report.json"), &Value::Object(report.clone()))?; }
    outcome?;
    println!("{}", serde_json::to_string(&Value::Object(report))?);
    Ok(())
}

//! Runtime process control and pipe access used by the integration checks.

use crate::common::{Result, ToolError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub const PIPE_PREFIX: &str = "LyricIsland.Settings.test.verify13.";

pub fn request(pipe: &str, command: &str, value: Option<Value>, timeout: Duration) -> Result<Value> {
    let mut payload = serde_json::Map::new();
    payload.insert("version".into(), json!(1));
    payload.insert("command".into(), json!(command));
    if let Some(value) = value { payload.insert("value".into(), value); }
    let payload = Value::Object(payload);
    let deadline = Instant::now() + timeout;
    loop {
        match lyric_bootstrap::win::pipe_request(pipe, &payload, Duration::from_millis(400)) {
            Ok(reply) => return Ok(reply),
            Err(error) => {
                if Instant::now() >= deadline { return Err(ToolError(format!("{command} 无响应：{error}"))); }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    }
}

pub fn call(pipe: &str, command: &str, value: Option<Value>) -> Result<Value> {
    let reply = request(pipe, command, value.clone(), Duration::from_secs(15))?;
    if reply.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(ToolError(format!("{command} 失败：{reply}")));
    }
    Ok(reply)
}

pub fn wait_exit(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => return false,
        }
    }
}

pub struct Runtime {
    directory: PathBuf,
    data: PathBuf,
    run_ms: u64,
    pipe: String,
    child: Option<Child>,
    environment: Vec<(String, String)>,
}

impl Runtime {
    pub fn new(directory: impl Into<PathBuf>, data: impl Into<PathBuf>, run_ms: u64) -> Self {
        Self::with_pipe(directory, data, run_ms, PIPE_PREFIX)
    }

    pub fn with_pipe(directory: impl Into<PathBuf>, data: impl Into<PathBuf>, run_ms: u64, prefix: &str) -> Self {
        let mut name = prefix.to_owned();
        name.push_str(&lyric_bootstrap::paths::unique_suffix());
        Runtime { directory: directory.into(), data: data.into(), run_ms, pipe: name, child: None, environment: Vec::new() }
    }

    pub fn directory(&self) -> &Path { &self.directory }
    pub fn data(&self) -> &Path { &self.data }
    pub fn pipe(&self) -> &str { &self.pipe }
    pub fn run_ms(&self) -> u64 { self.run_ms }

    pub fn set_env(&mut self, key: &str, value: &str) { self.environment.push((key.to_owned(), value.to_owned())); }

    pub fn start(&mut self) -> Result<Value> {
        let executable = self.directory.join("LyricIsland.exe");
        if !executable.is_file() { return Err(ToolError(format!("缺少主程序：{}", executable.display()))); }
        let mut command = Command::new(&executable);
        command.arg("--data-dir").arg(&self.data).arg("--run-ms").arg(self.run_ms.to_string());
        command.env("LYRICISLAND_TEST_PIPE", &self.pipe);
        for (key, value) in &self.environment { command.env(key, value); }
        self.child = Some(command.spawn()?);
        let state = request(&self.pipe, "state", None, Duration::from_secs(20))?;
        if state.get("success").and_then(Value::as_bool) != Some(true) { return Err(ToolError(format!("启动失败：{state}"))); }
        Ok(state)
    }

    pub fn stop(&mut self) {
        let Some(mut child) = self.child.take() else { return; };
        if child.try_wait().ok().flatten().is_none() {
            let _ = request(&self.pipe, "exit", None, Duration::from_secs(3));
            if !wait_exit(&mut child, Duration::from_secs(10)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let _ = child.wait();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) { self.stop(); }
}

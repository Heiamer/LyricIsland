//! Authenticated updater progress: real byte counts, unknown length and the apply/cancel handshakes.

use crate::common::{check, write_json, Result, ToolError};
use crate::process;
use lyric_bootstrap::{http, updater};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fixture(known: bool) -> Result<String> {
    let server = TcpListener::bind(("127.0.0.1", 0))?;
    let port = server.local_addr()?.port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = server.accept() {
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let header = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{}\r\n", if known { "Content-Length: 8192\r\n" } else { "" });
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&[0u8; 8192]);
        }
    });
    Ok(format!("http://127.0.0.1:{port}/fixture"))
}

fn request(pipe: &str, command: &str) -> Result<Value> { process::request(pipe, command, None, Duration::from_secs(10)) }

pub fn run(args: &[String]) -> Result<()> {
    let pipe = args.first().cloned().ok_or_else(|| ToolError("缺少测试管道".into()))?;
    let root = args.get(1).map(PathBuf::from).ok_or_else(|| ToolError("缺少工作目录".into()))?;
    let report_path = args.get(2).map(PathBuf::from).ok_or_else(|| ToolError("缺少报告路径".into()))?;
    std::fs::create_dir_all(&root)?;
    let zip = root.join("update.zip");
    request(&pipe, "updateTestPrepare")?;

    let state = updater::shared_state();
    let mut reporter = updater::Reporter::new(true, "integration-test-job".into(), "integration-test-nonce".into(), pipe.clone(), state);

    let url = fixture(true)?;
    let (received, length) = http::download(&url, &zip, 1024 * 1024, |received, length| {
        reporter.report("downloading", received, length, "下载更新", false).map(|_| ())
    })?;
    check(received == 8192 && length == Some(8192), format!("已知长度下载异常：{received}/{length:?}"))?;
    reporter.report("verifying", received, length, "正在校验下载内容", true)?;
    let known = request(&pipe, "state")?["softwareUpdate"].clone();
    check(known["phase"] == "verifying" && known["percent"] == 100, format!("已知字节进度失败：{known}"))?;

    request(&pipe, "updateTestPrepare")?;
    let url = fixture(false)?;
    let (received, length) = http::download(&url, &zip, 1024 * 1024, |received, length| {
        reporter.report("downloading", received, length, "下载更新", false).map(|_| ())
    })?;
    check(received == 8192 && length.is_none(), format!("未知长度下载异常：{received}/{length:?}"))?;
    reporter.report("downloading", received, length, "下载完成", true)?;
    let unknown = request(&pipe, "state")?["softwareUpdate"].clone();
    check(unknown["percent"].is_null() && unknown["receivedBytes"] == 8192, format!("未知长度被虚构百分比：{unknown}"))?;

    reporter.report("ready", 0, None, "ready", true)?;
    request(&pipe, "updateApply")?;
    let directive = reporter.report("ready", 0, None, "ready", true)?;
    check(directive == "apply", format!("显式安装握手失败：{directive}"))?;

    request(&pipe, "updateTestPrepare")?;
    request(&pipe, "updateCancel")?;
    let cancelled = reporter.report("downloading", 0, None, "取消下载", true).is_err_and(|error| error.is_cancelled());
    check(cancelled, "取消请求未生效")?;

    let report = json!({
        "success": true,
        "realStreamByteProgress": true,
        "unknownLengthNoPercent": true,
        "explicitApplyHandshake": true,
        "cancel": true
    });
    write_json(Path::new(&report_path), &report)?;
    println!("{report}");
    Ok(())
}

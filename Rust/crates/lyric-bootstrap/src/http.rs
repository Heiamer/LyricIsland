use crate::error::{Error, Result};
use crate::win::wide;
use std::io::Write;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    URL_COMPONENTS, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
    WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS, WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

struct Handles {
    session: *mut core::ffi::c_void,
    connect: *mut core::ffi::c_void,
    request: *mut core::ffi::c_void,
}

impl Drop for Handles {
    fn drop(&mut self) {
        unsafe {
            if !self.request.is_null() { let _ = WinHttpCloseHandle(self.request); }
            if !self.connect.is_null() { let _ = WinHttpCloseHandle(self.connect); }
            if !self.session.is_null() { let _ = WinHttpCloseHandle(self.session); }
        }
    }
}

fn query_number(request: *mut core::ffi::c_void, header: u32) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        WinHttpQueryHeaders(
            request,
            header | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            &mut size,
            std::ptr::null_mut(),
        )
    };
    status.ok().map(|_| value)
}

struct Response {
    handles: Handles,
    length: Option<u64>,
    status: u32,
}

fn open(url: &str) -> Result<Response> {
    if !url.starts_with("https://") && !url.starts_with("http://") { return Err(Error("更新下载地址无效".into())); }
    let url_wide = wide(url);
    let mut components: URL_COMPONENTS = unsafe { std::mem::zeroed() };
    components.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
    components.dwSchemeLength = u32::MAX;
    components.dwHostNameLength = u32::MAX;
    components.dwUrlPathLength = u32::MAX;
    components.dwExtraInfoLength = u32::MAX;
    unsafe { WinHttpCrackUrl(&url_wide, 0, &mut components).map_err(|_| Error("更新下载地址无效".into()))?; }
    let host = unsafe { std::slice::from_raw_parts(components.lpszHostName.0, components.dwHostNameLength as usize) };
    let host = String::from_utf16_lossy(host.split(|c| *c == 0).next().unwrap_or(&[]));
    let path = unsafe { std::slice::from_raw_parts(components.lpszUrlPath.0, components.dwUrlPathLength as usize) };
    let mut path = String::from_utf16_lossy(path.split(|c| *c == 0).next().unwrap_or(&[]));
    if components.dwExtraInfoLength != u32::MAX && components.dwExtraInfoLength > 1 {
        let extra = unsafe { std::slice::from_raw_parts(components.lpszExtraInfo.0, components.dwExtraInfoLength as usize) };
        path.push_str(&String::from_utf16_lossy(extra.split(|c| *c == 0).next().unwrap_or(&[])));
    }
    if path.is_empty() { path.push('/'); }
    let secure = components.nScheme.0 == 2;
    let port = if components.nPort == 0 { if secure { 443 } else { 80 } } else { components.nPort };

    unsafe {
        let agent = wide("DotIsle-Updater");
        let session = WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() { return Err(Error("无法初始化网络连接".into())); }
        let mut handles = Handles { session, connect: std::ptr::null_mut(), request: std::ptr::null_mut() };
        let _ = WinHttpSetTimeouts(session, 120_000, 120_000, 120_000, 120_000);
        let host_wide = wide(&host);
        let connect = WinHttpConnect(session, PCWSTR(host_wide.as_ptr()), port, 0);
        if connect.is_null() { return Err(Error(format!("无法连接更新源：{host}"))); }
        handles.connect = connect;
        let path_wide = wide(&path);
        let request = WinHttpOpenRequest(
            connect,
            PCWSTR(wide("GET").as_ptr()),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            if secure { WINHTTP_FLAG_SECURE } else { Default::default() },
        );
        if request.is_null() { return Err(Error("无法创建更新请求".into())); }
        handles.request = request;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS.to_le_bytes();
        let _ = WinHttpSetOption(Some(request as *const core::ffi::c_void), WINHTTP_OPTION_REDIRECT_POLICY, Some(&policy));
        WinHttpSendRequest(request, None, None, 0, 0, 0).map_err(|e| Error(format!("无法请求更新源：{e}")))?;
        WinHttpReceiveResponse(request, std::ptr::null_mut()).map_err(|e| Error(format!("更新源未响应：{e}")))?;
        let status = query_number(request, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0);
        let length = query_number(request, WINHTTP_QUERY_CONTENT_LENGTH).map(|value| value as u64).filter(|value| *value > 0);
        Ok(Response { handles, length, status })
    }
}

fn read_all(response: &Response, limit: u64, mut sink: impl FnMut(&[u8], u64) -> Result<()>) -> Result<u64> {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    loop {
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(response.handles.request, &mut available) }.map_err(|e| Error(format!("读取更新源失败：{e}")))?;
        if available == 0 { break; }
        let mut read = 0u32;
        unsafe { WinHttpReadData(response.handles.request, buffer.as_mut_ptr() as *mut core::ffi::c_void, available.min(buffer.len() as u32), &mut read) }
            .map_err(|e| Error(format!("读取更新源失败：{e}")))?;
        if read == 0 { break; }
        received = received.saturating_add(read as u64);
        if received > limit { return Err(Error("更新文件超过大小限制".into())); }
        sink(&buffer[..read as usize], received)?;
    }
    Ok(received)
}

pub fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>> {
    let response = open(url)?;
    if response.status != 200 { return Err(Error(format!("更新源返回 HTTP {}：{url}", response.status))); }
    if response.length.is_some_and(|length| length > limit) { return Err(Error("更新文件超过大小限制".into())); }
    let mut output = Vec::new();
    read_all(&response, limit, |chunk, _| { output.extend_from_slice(chunk); Ok(()) })?;
    if response.length.is_some_and(|length| length != output.len() as u64) { return Err(Error("下载长度不匹配".into())); }
    Ok(output)
}

pub fn download(url: &str, path: &Path, limit: u64, mut progress: impl FnMut(u64, Option<u64>) -> Result<()>) -> Result<(u64, Option<u64>)> {
    let response = open(url)?;
    if response.status != 200 { return Err(Error(format!("更新源返回 HTTP {}：{url}", response.status))); }
    let length = response.length.filter(|value| *value <= limit);
    if response.length.is_some_and(|value| value > limit) { return Err(Error("更新文件超过大小限制".into())); }
    let mut file = std::fs::File::create(path)?;
    let result = read_all(&response, limit, |chunk, received| { file.write_all(chunk)?; progress(received, length)?; Ok(()) });
    let received = match result { Ok(value) => value, Err(error) => { let _ = std::fs::remove_file(path); return Err(error); } };
    if length.is_some_and(|value| value != received) { let _ = std::fs::remove_file(path); return Err(Error("下载长度不匹配".into())); }
    Ok((received, length))
}

pub fn status_is_success(status: u32) -> bool { status == ERROR_SUCCESS.0 }

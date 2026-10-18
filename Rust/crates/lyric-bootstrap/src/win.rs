use crate::error::{Error, Result};
use serde_json::Value;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, MessageBoxW, PostMessageW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, WM_CLOSE};

pub fn wide(value: &str) -> Vec<u16> { OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect() }

pub fn message_box(text: &str, caption: &str, error: bool) {
    let text = wide(text);
    let caption = wide(caption);
    let style = if error { MB_OK | MB_ICONERROR } else { MB_OK | MB_ICONINFORMATION };
    unsafe { MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(caption.as_ptr()), style); }
}

pub fn current_pid() -> u32 { unsafe { GetCurrentProcessId() } }

pub struct NamedMutex {
    handle: HANDLE,
}

impl NamedMutex {
    /// Returns `None` when another process already owns the mutex.
    pub fn acquire(name: &str) -> Result<Option<Self>> { Self::acquire_timeout(name, Duration::ZERO) }

    pub fn acquire_timeout(name: &str, timeout: Duration) -> Result<Option<Self>> {
        let name = wide(name);
        let handle = unsafe { windows::Win32::System::Threading::CreateMutexW(None, false, PCWSTR(name.as_ptr()))? };
        let wait = unsafe { WaitForSingleObject(handle, timeout.as_millis().min(u32::MAX as u128) as u32) };
        if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
            Ok(Some(NamedMutex { handle }))
        } else if wait == WAIT_TIMEOUT {
            unsafe { let _ = CloseHandle(handle); }
            Ok(None)
        } else {
            unsafe { let _ = CloseHandle(handle); }
            Err(Error(format!("无法获取安装互斥锁（{}）", unsafe { GetLastError() }.0)))
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Threading::ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

pub fn pipe_request(pipe: &str, request: &Value, timeout: Duration) -> Result<Value> {
    let path = format!("\\\\.\\pipe\\{pipe}");
    let deadline = Instant::now() + timeout;
    let file = loop {
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => break file,
            Err(error) => {
                if Instant::now() >= deadline { return Err(Error(format!("无法连接点屿（{error}）"))); }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    };
    let mut payload = serde_json::to_vec(request)?;
    payload.push(b'\n');
    (&file).write_all(&payload)?;
    (&file).flush()?;
    let mut reader = BufReader::new(&file);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 { return Err(Error("点屿未返回结果".into())); }
    Ok(serde_json::from_str(&line)?)
}

pub fn pipe_ready(pipe: &str) -> bool {
    std::fs::OpenOptions::new().read(true).write(true).open(format!("\\\\.\\pipe\\{pipe}")).is_ok()
}

pub fn process_image(pid: u32) -> Option<PathBuf> {
    if pid == 0 { return None; }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 32_768];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size);
        let _ = CloseHandle(handle);
        if result.is_err() { return None; }
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
    }
}

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 { return false; }
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => { let _ = CloseHandle(handle); true }
            Err(_) => false,
        }
    }
}

pub fn wait_process(pid: u32, timeout: Duration) -> bool {
    if pid == 0 { return true; }
    unsafe {
        let handle = match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => handle,
            Err(_) => return true,
        };
        let wait = WaitForSingleObject(handle, timeout.as_millis() as u32);
        let _ = CloseHandle(handle);
        wait == WAIT_OBJECT_0
    }
}

pub fn terminate_process(pid: u32) -> bool {
    unsafe {
        let handle = match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => handle,
            Err(_) => return true,
        };
        let _ = TerminateProcess(handle, 1);
        let wait = WaitForSingleObject(handle, 5_000);
        let _ = CloseHandle(handle);
        wait == WAIT_OBJECT_0
    }
}

struct EnumContext {
    pid: u32,
    sent: bool,
}

unsafe extern "system" fn enum_close(hwnd: HWND, param: LPARAM) -> BOOL {
    let context = &mut *(param.0 as *mut EnumContext);
    let mut owner = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut owner));
    if owner == context.pid {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default());
        context.sent = true;
    }
    true.into()
}

pub fn close_main_windows(pid: u32) {
    let mut context = EnumContext { pid, sent: false };
    unsafe { let _ = EnumWindows(Some(enum_close), LPARAM(&mut context as *mut _ as isize)); }
}

pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
}

pub fn processes() -> Vec<ProcessEntry> {
    let mut output = Vec::new();
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(handle) => handle,
            Err(_) => return output,
        };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(entry.szExeFile.split(|c| *c == 0).next().unwrap_or(&[]));
                output.push(ProcessEntry { pid: entry.th32ProcessID, name });
                if Process32NextW(snapshot, &mut entry).is_err() { break; }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    output
}

pub fn resource(name: &str) -> Result<Vec<u8>> {
    let name = wide(name);
    unsafe {
        let module = GetModuleHandleW(None)?;
        let info = FindResourceW(Some(module), PCWSTR(name.as_ptr()), PCWSTR(10 as *const u16));
        if info.is_invalid() { return Err(Error("程序资源为空".into())); }
        let size = SizeofResource(Some(module), info);
        if size == 0 { return Err(Error("程序资源为空".into())); }
        let loaded = LoadResource(Some(module), info)?;
        let pointer = LockResource(loaded);
        if pointer.is_null() { return Err(Error("无法读取程序资源".into())); }
        Ok(std::slice::from_raw_parts(pointer as *const u8, size as usize).to_vec())
    }
}

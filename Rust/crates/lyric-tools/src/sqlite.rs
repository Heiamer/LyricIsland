//! Minimal dynamic SQLite binding for the notification compatibility fixture.
//!
//! The test uses the e_sqlite3.dll shipped next to the runtime instead of a build-time
//! dependency, so the harness stays independent of the application's backend.

use crate::bail;
use crate::common::{Result, ToolError};
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;

type OpenV2 = unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
type Exec = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_void, *mut c_void, *mut *mut c_char) -> c_int;
type ErrMsg = unsafe extern "C" fn(*mut c_void) -> *const c_char;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> c_int;
}

fn wide(value: &Path) -> Vec<u16> { value.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect() }

unsafe fn symbol<T>(module: *mut c_void, name: &[u8]) -> Result<T> {
    let pointer = GetProcAddress(module, name.as_ptr());
    if pointer.is_null() { return Err(ToolError(format!("e_sqlite3.dll 缺少{}", String::from_utf8_lossy(&name[..name.len() - 1])))); }
    Ok(std::mem::transmute_copy(&pointer))
}

pub struct Database {
    module: *mut c_void,
    handle: *mut c_void,
    close: Close,
    exec: Exec,
    errmsg: ErrMsg,
}

impl Database {
    pub fn open(library: &Path, path: &Path) -> Result<Self> {
        let module = unsafe { LoadLibraryW(wide(library).as_ptr()) };
        if module.is_null() { bail!("无法加载 {}", library.display()); }
        let outcome = (|| -> Result<Self> {
            let open: OpenV2 = unsafe { symbol(module, b"sqlite3_open_v2\0")? };
            let close: Close = unsafe { symbol(module, b"sqlite3_close_v2\0")? };
            let exec: Exec = unsafe { symbol(module, b"sqlite3_exec\0")? };
            let errmsg: ErrMsg = unsafe { symbol(module, b"sqlite3_errmsg\0")? };
            let text = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| ToolError("数据库路径无效".into()))?;
            let mut handle = std::ptr::null_mut();
            let status = unsafe { open(text.as_ptr(), &mut handle, 0x0000_0002 | 0x0000_0004, std::ptr::null()) };
            if status != 0 {
                let message = if handle.is_null() { format!("SQLite 打开失败（{status}）") } else { unsafe { std::ffi::CStr::from_ptr(errmsg(handle)) }.to_string_lossy().to_string() };
                if !handle.is_null() { unsafe { close(handle); } }
                return Err(ToolError(message));
            }
            Ok(Database { module, handle, close, exec, errmsg })
        })();
        if outcome.is_err() { unsafe { FreeLibrary(module); } }
        outcome
    }

    pub fn exec(&self, sql: &str) -> Result<()> {
        let text = CString::new(sql).map_err(|_| ToolError("SQL 含 NUL".into()))?;
        let status = unsafe { (self.exec)(self.handle, text.as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut()) };
        if status != 0 {
            let message = unsafe { std::ffi::CStr::from_ptr((self.errmsg)(self.handle)) }.to_string_lossy().to_string();
            return Err(ToolError(format!("SQLite 执行失败：{message}")));
        }
        Ok(())
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if !self.handle.is_null() { unsafe { (self.close)(self.handle); } }
        if !self.module.is_null() { unsafe { FreeLibrary(self.module); } }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes { let _ = write!(text, "{byte:02X}"); }
    text
}

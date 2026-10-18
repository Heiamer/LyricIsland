use crate::error::{Error, Result};
use crate::win::wide;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, KEY_WOW64_64KEY, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};

pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DotIsle";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "LyricIsland";

pub struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) { unsafe { let _ = RegCloseKey(self.0); } }
}

impl Key {
    pub fn create(path: &str) -> Result<Self> {
        let path = wide(path);
        let mut key = HKEY::default();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE | KEY_WOW64_64KEY,
                None,
                &mut key,
                None,
            )
        };
        if status != ERROR_SUCCESS { return Err(Error(format!("无法写入 Windows 应用登记（{}）", status.0))); }
        Ok(Key(key))
    }

    pub fn open(path: &str) -> Result<Option<Self>> {
        let path = wide(path);
        let mut key = HKEY::default();
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, KEY_READ | KEY_WOW64_64KEY, &mut key) };
        if status == ERROR_FILE_NOT_FOUND { return Ok(None); }
        if status != ERROR_SUCCESS { return Err(Error(format!("无法读取 Windows 应用登记（{}）", status.0))); }
        Ok(Some(Key(key)))
    }

    pub fn set_string(&self, name: &str, value: &str) -> Result<()> {
        let name_wide = wide(name);
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
        let status = unsafe { RegSetValueExW(self.0, PCWSTR(name_wide.as_ptr()), Some(0), REG_SZ, Some(bytes)) };
        if status != ERROR_SUCCESS { return Err(Error(format!("无法写入登记“{name}”"))); }
        Ok(())
    }

    pub fn set_u32(&self, name: &str, value: u32) -> Result<()> {
        let name_wide = wide(name);
        let status = unsafe { RegSetValueExW(self.0, PCWSTR(name_wide.as_ptr()), Some(0), REG_DWORD, Some(&value.to_le_bytes())) };
        if status != ERROR_SUCCESS { return Err(Error(format!("无法写入登记值“{name}”"))); }
        Ok(())
    }

    pub fn string(&self, name: &str) -> Option<String> {
        let name = wide(name);
        let mut kind = Default::default();
        let mut size = 0u32;
        let status = unsafe { RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), None, Some(&mut size)) };
        if status != ERROR_SUCCESS || size == 0 { return None; }
        let mut bytes = vec![0u8; size as usize];
        let status = unsafe { RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), Some(bytes.as_mut_ptr()), Some(&mut size)) };
        if status != ERROR_SUCCESS { return None; }
        let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        Some(String::from_utf16_lossy(units.split(|c| *c == 0).next().unwrap_or(&[])))
    }

    pub fn u32(&self, name: &str) -> Option<u32> {
        let name = wide(name);
        let mut kind = Default::default();
        let mut size = 0u32;
        let status = unsafe { RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), None, Some(&mut size)) };
        if status != ERROR_SUCCESS || size < 4 { return None; }
        let mut bytes = [0u8; 4];
        let mut size = 4u32;
        let status = unsafe { RegQueryValueExW(self.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), Some(bytes.as_mut_ptr()), Some(&mut size)) };
        if status != ERROR_SUCCESS { return None; }
        Some(u32::from_le_bytes(bytes))
    }

    pub fn delete_value(&self, name: &str) -> Result<()> {
        let name_wide = wide(name);
        let status = unsafe { RegDeleteValueW(self.0, PCWSTR(name_wide.as_ptr())) };
        if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND { return Err(Error(format!("无法删除登记值“{name}”"))); }
        Ok(())
    }
}

pub fn delete_tree(path: &str) -> Result<()> {
    let path = wide(path);
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
    if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND { return Err(Error(format!("无法删除 Windows 应用登记（{}）", status.0))); }
    Ok(())
}

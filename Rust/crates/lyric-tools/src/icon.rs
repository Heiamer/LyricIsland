//! PE resource helpers: application icon and installer payload injection.

use crate::bail;
use crate::common::{Result, ToolError};
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::LibraryLoader::{BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_RCDATA: u16 = 10;
const LANG_NEUTRAL: u16 = 1033;

fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(std::iter::once(0)).collect() }

fn ordinal(id: u16) -> PCWSTR { PCWSTR(id as usize as *const u16) }

fn update(handle: HANDLE, kind: u16, name: PCWSTR, bytes: &[u8]) -> Result<()> {
    unsafe {
        UpdateResourceW(handle, ordinal(kind), name, LANG_NEUTRAL, Some(bytes.as_ptr() as *const core::ffi::c_void), bytes.len() as u32)
            .map_err(|error| ToolError(format!("无法写入程序资源：{error}")))
    }
}

fn open(executable: &Path) -> Result<HANDLE> {
    let path = wide(&executable.to_string_lossy());
    unsafe { BeginUpdateResourceW(PCWSTR(path.as_ptr()), false).map_err(|error| ToolError(format!("无法打开程序文件：{error}"))) }
}

fn finish(handle: HANDLE, discard: bool) -> Result<()> {
    unsafe { EndUpdateResourceW(handle, discard).map_err(|error| ToolError(format!("无法保存程序资源：{error}"))) }
}

fn read_u16(data: &[u8], offset: usize) -> u16 { u16::from_le_bytes([data[offset], data[offset + 1]]) }
fn read_u32(data: &[u8], offset: usize) -> u32 { u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) }

pub fn embed_icon(executable: &Path, icon: &Path) -> Result<()> {
    let data = std::fs::read(icon)?;
    if data.len() < 6 { bail!("ICO 文件无效"); }
    let reserved = read_u16(&data, 0);
    let kind = read_u16(&data, 2);
    let count = read_u16(&data, 4) as usize;
    if reserved != 0 || kind != 1 || count == 0 || data.len() < 6 + count * 16 { bail!("ICO 文件无效"); }
    let handle = open(executable)?;
    let outcome = (|| -> Result<()> {
        let mut group = Vec::with_capacity(6 + count * 14);
        group.extend_from_slice(&0u16.to_le_bytes());
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&(count as u16).to_le_bytes());
        for index in 0..count {
            let entry = 6 + index * 16;
            let width = data[entry];
            let height = data[entry + 1];
            let colors = data[entry + 2];
            let reserved = data[entry + 3];
            let planes = read_u16(&data, entry + 4);
            let bits = read_u16(&data, entry + 6);
            let size = read_u32(&data, entry + 8) as usize;
            let offset = read_u32(&data, entry + 12) as usize;
            if offset + size > data.len() { bail!("ICO 图像已截断"); }
            let id = (index + 1) as u16;
            update(handle, RT_ICON, ordinal(id), &data[offset..offset + size])?;
            group.extend_from_slice(&[width, height, colors, reserved]);
            group.extend_from_slice(&planes.to_le_bytes());
            group.extend_from_slice(&bits.to_le_bytes());
            group.extend_from_slice(&(size as u32).to_le_bytes());
            group.extend_from_slice(&id.to_le_bytes());
        }
        update(handle, RT_GROUP_ICON, ordinal(1), &group)
    })();
    if outcome.is_err() {
        let _ = finish(handle, true);
        return outcome;
    }
    finish(handle, false)
}

pub fn replace_resource(executable: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let handle = open(executable)?;
    let wide_name = wide(name);
    let outcome = update(handle, RT_RCDATA, PCWSTR(wide_name.as_ptr()), bytes);
    if outcome.is_err() {
        let _ = finish(handle, true);
        return outcome;
    }
    finish(handle, false)
}

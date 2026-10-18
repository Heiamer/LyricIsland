//! Minimal ZIP transport and SHA256 helpers shared by the app, installer and build tools.
//!
//! Only stored and deflated entries are produced or accepted. That is exactly what the
//! update bundle, the portable archive and the installer payload need, and it keeps the
//! release pipeline free of a general-purpose ZIP dependency.

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self { Error(value.to_string()) }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Self { Error(value.to_string()) }
}
impl From<std::num::TryFromIntError> for Error {
    fn from(_: std::num::TryFromIntError) -> Self { Error("archive size does not fit ZIP32".into()) }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hex(&hash.finalize())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 { break; }
        hash.update(&buffer[..read]);
    }
    Ok(hex(&hash.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes { text.push_str(&format!("{byte:02x}")); }
    text
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    data.get(offset..offset + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| Error("ZIP 目录被截断".into()))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| Error("ZIP 目录被截断".into()))
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub compressed: u64,
    method: u16,
    crc: u32,
    local_offset: u64,
}

pub struct ZipReader {
    file: File,
    entries: Vec<Entry>,
}

impl ZipReader {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        if length < 22 { return Err(Error("ZIP 文件不完整".into())); }
        let tail_length = length.min(66 * 1024) as usize;
        file.seek(SeekFrom::End(-(tail_length as i64)))?;
        let mut tail = vec![0u8; tail_length];
        file.read_exact(&mut tail)?;
        let mut eocd = None;
        for index in (0..=tail.len() - 22).rev() {
            if u32_at(&tail, index)? == EOCD {
                let comment = u16_at(&tail, index + 20)? as usize;
                if index + 22 + comment == tail.len() { eocd = Some(index); break; }
            }
        }
        let eocd = eocd.ok_or_else(|| Error("找不到 ZIP 结束记录".into()))?;
        let total = u16_at(&tail, eocd + 10)? as usize;
        let directory_size = u32_at(&tail, eocd + 12)? as u64;
        let directory_offset = u32_at(&tail, eocd + 16)? as u64;
        if directory_offset + directory_size > length { return Err(Error("ZIP 目录越界".into())); }
        file.seek(SeekFrom::Start(directory_offset))?;
        let mut directory = vec![0u8; directory_size as usize];
        file.read_exact(&mut directory)?;
        let mut entries = Vec::with_capacity(total);
        let mut cursor = 0usize;
        for _ in 0..total {
            if u32_at(&directory, cursor)? != CENTRAL { return Err(Error("ZIP 目录项无效".into())); }
            let flags = u16_at(&directory, cursor + 8)?;
            let method = u16_at(&directory, cursor + 10)?;
            let crc = u32_at(&directory, cursor + 16)?;
            let compressed = u32_at(&directory, cursor + 20)? as u64;
            let size = u32_at(&directory, cursor + 24)? as u64;
            let name_length = u16_at(&directory, cursor + 28)? as usize;
            let extra_length = u16_at(&directory, cursor + 30)? as usize;
            let comment_length = u16_at(&directory, cursor + 32)? as usize;
            let local_offset = u32_at(&directory, cursor + 42)? as u64;
            let name_start = cursor + 46;
            let name = String::from_utf8_lossy(directory.get(name_start..name_start + name_length).ok_or_else(|| Error("ZIP 名称被截断".into()))?).into_owned();
            if flags & 1 != 0 { return Err(Error(format!("ZIP 条目已加密：{name}"))); }
            if method != 0 && method != 8 { return Err(Error(format!("不支持的 ZIP 压缩方式：{name}"))); }
            if compressed == u32::MAX as u64 || size == u32::MAX as u64 || local_offset == u32::MAX as u64 { return Err(Error("不支持 ZIP64 更新包".into())); }
            entries.push(Entry { name: name.clone(), is_dir: name.ends_with('/'), size, compressed, method, crc, local_offset });
            cursor = name_start + name_length + extra_length + comment_length;
            if cursor > directory.len() { return Err(Error("ZIP 目录项越界".into())); }
        }
        Ok(ZipReader { file, entries })
    }

    pub fn entries(&self) -> &[Entry] { &self.entries }

    pub fn read(&mut self, index: usize) -> Result<Vec<u8>> {
        let entry = self.entries.get(index).ok_or_else(|| Error("ZIP 条目不存在".into()))?.clone();
        if entry.is_dir { return Ok(Vec::new()); }
        self.file.seek(SeekFrom::Start(entry.local_offset))?;
        let mut header = [0u8; 30];
        self.file.read_exact(&mut header)?;
        if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != LOCAL { return Err(Error(format!("ZIP 本地头无效：{}", entry.name))); }
        let name_length = u16::from_le_bytes([header[26], header[27]]) as u64;
        let extra_length = u16::from_le_bytes([header[28], header[29]]) as u64;
        self.file.seek(SeekFrom::Start(entry.local_offset + 30 + name_length + extra_length))?;
        let mut compressed = vec![0u8; entry.compressed as usize];
        self.file.read_exact(&mut compressed)?;
        let data = match entry.method {
            0 => compressed,
            8 => {
                let mut decoded = Vec::with_capacity(entry.size as usize);
                DeflateDecoder::new(compressed.as_slice()).read_to_end(&mut decoded)?;
                decoded
            }
            _ => unreachable!(),
        };
        if data.len() as u64 != entry.size { return Err(Error(format!("ZIP 条目长度不符：{}", entry.name))); }
        let mut crc = Crc::new();
        crc.update(&data);
        if crc.sum() != entry.crc { return Err(Error(format!("ZIP 条目校验失败：{}", entry.name))); }
        Ok(data)
    }
}

struct WrittenEntry {
    name: String,
    crc: u32,
    compressed: u32,
    size: u32,
    offset: u32,
    method: u16,
    time: u16,
    date: u16,
}

pub struct ZipWriter {
    file: File,
    entries: Vec<WrittenEntry>,
}

fn dos_time() -> (u16, u16) {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let clock = (seconds % 86_400) as u32;
    // Howard Hinnant's civil_from_days, restricted to the ZIP date range.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 { year += 1; }
    if !(1980..=2107).contains(&year) { return (0, 0x21); }
    let hour = clock / 3_600;
    let minute = (clock % 3_600) / 60;
    let second = (clock % 60) / 2;
    (((hour << 11) | (minute << 5) | second) as u16, (((year - 1980) << 9) | (month << 5) | day) as u16)
}

impl ZipWriter {
    pub fn create(path: &Path) -> Result<Self> {
        Ok(ZipWriter { file: File::create(path)?, entries: Vec::new() })
    }

    pub fn add_dir(&mut self, name: &str) -> Result<()> {
        let name = if name.ends_with('/') { name.to_owned() } else { format!("{name}/") };
        self.write_entry(&name, &[], Compression::none())
    }

    pub fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.write_entry(name, bytes, Compression::new(9))
    }

    pub fn add_file(&mut self, name: &str, path: &Path) -> Result<()> {
        let bytes = std::fs::read(path)?;
        self.add_bytes(name, &bytes)
    }

    fn write_entry(&mut self, name: &str, bytes: &[u8], compression: Compression) -> Result<()> {
        let offset = self.file.stream_position()?;
        if offset > u32::MAX as u64 { return Err(Error("ZIP 超过 4GB".into())); }
        let method: u16 = if compression.level() == 0 { 0 } else { 8 };
        let mut crc = Crc::new();
        crc.update(bytes);
        let compressed = if method == 8 {
            let mut encoder = DeflateEncoder::new(Vec::new(), compression);
            encoder.write_all(bytes)?;
            encoder.finish()?
        } else { bytes.to_vec() };
        if bytes.len() > u32::MAX as usize || compressed.len() > u32::MAX as usize { return Err(Error("ZIP 条目超过 4GB".into())); }
        let (time, date) = dos_time();
        let mut header = Vec::with_capacity(30 + name.len());
        header.extend_from_slice(&LOCAL.to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes());
        header.extend_from_slice(&0x0800u16.to_le_bytes());
        header.extend_from_slice(&method.to_le_bytes());
        header.extend_from_slice(&time.to_le_bytes());
        header.extend_from_slice(&date.to_le_bytes());
        header.extend_from_slice(&crc.sum().to_le_bytes());
        header.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        header.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(name.as_bytes());
        self.file.write_all(&header)?;
        self.file.write_all(&compressed)?;
        self.entries.push(WrittenEntry { name: name.to_owned(), crc: crc.sum(), compressed: compressed.len() as u32, size: bytes.len() as u32, offset: offset as u32, method, time, date });
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        let directory_offset = self.file.stream_position()?;
        for entry in &self.entries {
            let mut header = Vec::with_capacity(46 + entry.name.len());
            header.extend_from_slice(&CENTRAL.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes());
            header.extend_from_slice(&0x0800u16.to_le_bytes());
            header.extend_from_slice(&entry.method.to_le_bytes());
            header.extend_from_slice(&entry.time.to_le_bytes());
            header.extend_from_slice(&entry.date.to_le_bytes());
            header.extend_from_slice(&entry.crc.to_le_bytes());
            header.extend_from_slice(&entry.compressed.to_le_bytes());
            header.extend_from_slice(&entry.size.to_le_bytes());
            header.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u32.to_le_bytes());
            header.extend_from_slice(&entry.offset.to_le_bytes());
            header.extend_from_slice(entry.name.as_bytes());
            self.file.write_all(&header)?;
        }
        let directory_size = self.file.stream_position()? - directory_offset;
        let count: u16 = self.entries.len().try_into().map_err(|_| Error("ZIP 条目过多".into()))?;
        let mut eocd = Vec::with_capacity(22);
        eocd.extend_from_slice(&EOCD.to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        eocd.extend_from_slice(&count.to_le_bytes());
        eocd.extend_from_slice(&count.to_le_bytes());
        eocd.extend_from_slice(&(directory_size as u32).to_le_bytes());
        eocd.extend_from_slice(&(directory_offset as u32).to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        self.file.write_all(&eocd)?;
        self.file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("lyric-archive-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn round_trips_stored_and_deflated_entries() {
        let directory = scratch("roundtrip");
        let source = directory.join("font.bin");
        let payload = b"dotisle archive payload ".repeat(4096);
        std::fs::write(&source, &payload).unwrap();
        let zip = directory.join("bundle.zip");
        let mut writer = ZipWriter::create(&zip).unwrap();
        writer.add_dir("LyricIsland-2.0.2").unwrap();
        writer.add_file("LyricIsland-2.0.2/font.bin", &source).unwrap();
        writer.add_bytes("LyricIsland-2.0.2/说明.txt", "点屿".as_bytes()).unwrap();
        writer.finish().unwrap();
        let mut reader = ZipReader::open(&zip).unwrap();
        assert_eq!(reader.entries().len(), 3);
        assert_eq!(reader.read(1).unwrap(), payload);
        assert_eq!(reader.read(2).unwrap(), "点屿".as_bytes());
        assert!(reader.entries()[0].is_dir);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_corrupt_data_and_unsafe_archives_are_left_to_the_caller() {
        let directory = scratch("corrupt");
        let zip = directory.join("bundle.zip");
        let mut writer = ZipWriter::create(&zip).unwrap();
        writer.add_bytes("LyricIsland-9.9/../escape", b"bad").unwrap();
        writer.finish().unwrap();
        let reader = ZipReader::open(&zip).unwrap();
        assert_eq!(reader.entries()[0].name, "LyricIsland-9.9/../escape");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn hashes_files_and_bytes() {
        let directory = scratch("hash");
        let file = directory.join("settings.json");
        std::fs::write(&file, b"{\"version\":1}").unwrap();
        assert_eq!(sha256_file(&file).unwrap(), sha256_bytes(b"{\"version\":1}"));
        assert_eq!(sha256_bytes(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        std::fs::remove_dir_all(directory).unwrap();
    }
}

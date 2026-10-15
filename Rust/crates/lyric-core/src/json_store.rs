//! Opaque JSON cache compatible with the managed cache envelope format.
//! Keep payloads as compact UTF-8; deserialize only the requested entry in the host.
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

static TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn canonical(key: &str) -> String {
    // Ordinal ignore-case keys do not expand sharp-s into two characters.
    key.chars().map(|c| {
        let mut upper = c.to_uppercase();
        let first = upper.next().unwrap_or(c);
        if upper.next().is_none() { first } else { c }
    }).collect()
}

#[derive(Default)]
struct Entries(HashMap<String, Box<RawValue>>);
impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedKeys;
        impl<'de> Visitor<'de> for OrderedKeys {
            type Value = Entries;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a cache object") }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = HashMap::new();
                // Insert in file order so the last case-insensitive duplicate wins.
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    entries.insert(canonical(&key), value);
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(OrderedKeys)
    }
}
impl Serialize for Entries {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 { map.serialize_entry(key, value)?; }
        map.end()
    }
}

#[derive(Default)]
struct State { entries: Entries, disk_keys: HashSet<String>, loaded: bool }
pub struct JsonStore { path: PathBuf, envelope: bool, state: Mutex<State> }

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into(), envelope: false, state: Mutex::new(State::default()) } }
    pub fn new_enveloped(path: impl Into<PathBuf>) -> Self { Self { path: path.into(), envelope: true, state: Mutex::new(State::default()) } }
    fn load(&self, state: &mut State) {
        if state.loaded { return; }
        state.loaded = true;
        if let Ok(file) = File::open(&self.path) {
            #[derive(Deserialize)] struct Envelope { version:i32, entries:Entries }
            let entries = if self.envelope {
                serde_json::from_reader::<_,Envelope>(BufReader::new(file)).and_then(|e|if e.version==2{Ok(e.entries)}else{Err(serde_json::Error::io(std::io::Error::other("unsupported cache version")))})
            } else { serde_json::from_reader::<_, Entries>(BufReader::new(file)) };
            match entries {
                Ok(entries) => {
                    state.disk_keys = entries.0.keys().cloned().collect();
                    state.entries = entries;
                }
                Err(_) => { let _ = fs::remove_file(&self.path); }
            }
        }
    }
    /// Acquisition: 1 = memory, 2 = first disk read. Only the requested payload is copied.
    pub fn get(&self, key: &str) -> Option<(String, i32)> {
        let mut state = self.state.lock().ok()?;
        self.load(&mut state);
        let key = canonical(key);
        let payload = state.entries.0.get(&key)?.get().to_owned();
        let acquisition = if state.disk_keys.remove(&key) { 2 } else { 1 };
        Some((payload, acquisition))
    }
    /// A false result reports failed disk persistence; the memory entry stays available.
    pub fn store(&self, key: &str, payload: &str) -> Result<bool, serde_json::Error> {
        let value: Box<RawValue> = serde_json::from_str(payload)?;
        let mut state = self.state.lock().map_err(|_| serde_json::Error::io(std::io::Error::other("poisoned cache")))?;
        self.load(&mut state);
        let key = canonical(key);
        state.disk_keys.remove(&key);
        state.entries.0.insert(key, value);
        Ok(self.persist(&state.entries).is_ok())
    }
    pub fn remove(&self, key: &str) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        self.load(&mut state);
        let key = canonical(key);
        state.disk_keys.remove(&key);
        if state.entries.0.remove(&key).is_some() { return self.persist(&state.entries).is_ok(); }
        true
    }
    pub fn clear(&self) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        *state = State { loaded: true, ..State::default() };
        match fs::remove_file(&self.path) { Ok(()) => true, Err(e) => e.kind() == std::io::ErrorKind::NotFound }
    }
    fn persist(&self, entries: &Entries) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) { fs::create_dir_all(parent)?; }
        let temporary = PathBuf::from(format!("{}.{}.{}.tmp", self.path.display(), std::process::id(), TEMP_ID.fetch_add(1, Ordering::Relaxed)));
        let result = (|| {
            let file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
            let mut writer = BufWriter::new(file);
            // Serialize borrowed entries directly to disk: no whole-cache snapshot/string.
            if self.envelope {
                #[derive(Serialize)]struct Envelope<'a>{version:i32,entries:&'a Entries}
                serde_json::to_writer(&mut writer, &Envelope{version:2,entries})?;
            } else { serde_json::to_writer(&mut writer, entries)?; }
            writer.flush()?; writer.get_ref().sync_all()?; drop(writer);
            replace_file(&temporary, &self.path)
        })();
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
    }
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" { fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32; }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    // REPLACE_EXISTING | WRITE_THROUGH, matching managed atomic replace semantics.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 1 | 8) } == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}
#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> { fs::rename(source, destination) }

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf { std::env::temp_dir().join(format!("li-cache-{}-{}.json", std::process::id(), TEMP_ID.fetch_add(1, Ordering::Relaxed))) }
    #[test]
    fn reload_acquisition_remove_and_clear() {
        let path = path(); let cache = JsonStore::new(&path);
        assert!(cache.store("Track", r#"{"Value":"歌词","Lines":[1,2]}"#).unwrap());
        assert_eq!(cache.get("TRACK").unwrap().1, 1);
        let reloaded = JsonStore::new(&path);
        assert_eq!(reloaded.get("track").unwrap().1, 2);
        assert_eq!(reloaded.get("Track").unwrap().1, 1);
        assert!(reloaded.remove("tRaCk")); assert!(JsonStore::new(&path).get("track").is_none());
        assert!(reloaded.clear()); assert!(!path.exists());
    }
    #[test]
    fn last_duplicate_wins_without_unicode_key_expansion() {
        let path = path(); fs::write(&path, r#"{"Track":{"Value":1},"TRACK":{"Value":2},"ß":3,"SS":4}"#).unwrap();
        let cache = JsonStore::new(&path);
        assert_eq!(cache.get("track").unwrap().0, r#"{"Value":2}"#);
        assert_eq!(cache.get("ß").unwrap().0, "3"); assert_eq!(cache.get("ss").unwrap().0, "4"); cache.clear();
    }
    #[test]
    fn corrupt_file_recovers_and_failed_write_keeps_memory() {
        let path = path(); fs::write(&path, "broken").unwrap(); let cache = JsonStore::new(&path);
        assert!(cache.get("x").is_none()); assert!(!path.exists());
        assert!(cache.store("x", "[1,2,3]").unwrap()); cache.clear();
        fs::create_dir(&path).unwrap(); let blocked = JsonStore::new(&path);
        assert!(!blocked.store("x", "42").unwrap()); assert_eq!(blocked.get("x").unwrap().0, "42"); fs::remove_dir(&path).unwrap();
    }
    #[test]
    fn overwrites_are_valid_atomic_json_without_leftover_temporary_files() {
        let path = path(); let cache = JsonStore::new(&path);
        for i in 0..15 { assert!(cache.store("x", &i.to_string()).unwrap()); }
        let content: serde_json::Value = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert_eq!(content["X"], 14); cache.clear();
    }
}

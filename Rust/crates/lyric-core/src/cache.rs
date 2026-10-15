//! Caches.
//!
//! * `JsonLyricCacheStore<T>` — the small generic JSON store the original used for the
//!   raw and parsed pipeline stages (ported from `Services.LyricCacheStore.cs`).
//! * `LyricPipelineCache` — raw payload / parsed lyric two-stage cache.
//! * `JsonResolvedLyricCache` — the final "this track resolves to this candidate" cache,
//!   keyed by normalized title + artist (and optionally a caller supplied context).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::log_warn;
use crate::models::{
    LyricAcquisitionKind, LyricPayloadFormat, LyricProviderId, LyricTimingKind,
    LyricTimingProvenance, ParsedLyrics, RawLyricPayload, ResolvedLyrics, TrackInfo,
};

// ---------------------------------------------------------------------------
// Generic JSON store
// ---------------------------------------------------------------------------

/// The tiny persistence contract shared by both caches.
pub trait LyricCacheStore<T>: Send + Sync {
    fn try_get(&self, key: &str) -> Option<(T, LyricAcquisitionKind)>;
    fn store(&self, key: &str, value: T);
    fn remove(&self, key: &str);
    fn clear(&self);
}

/// File backed `HashMap<String, T>`; keys are case-insensitive, exactly like the original.
pub struct JsonLyricCacheStore<T> {
    path: PathBuf,
    state: Mutex<StoreState<T>>,
}

struct StoreState<T> {
    entries: Option<HashMap<String, T>>,
    loaded: bool,
    disk_keys: BTreeSet<String>,
}

impl<T> JsonLyricCacheStore<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: Mutex::new(StoreState {
                entries: None,
                loaded: false,
                disk_keys: BTreeSet::new(),
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn canonical(key: &str) -> String {
        key.to_uppercase()
    }

    fn ensure_loaded(&self, state: &mut StoreState<T>) -> bool {
        if state.loaded {
            return state.entries.is_some();
        }
        state.loaded = true;

        match fs::read_to_string(&self.path) {
            Err(_) => {
                state.entries = Some(HashMap::new());
                true
            }
            Ok(text) => match serde_json::from_str::<HashMap<String, T>>(&text) {
                Ok(entries) => {
                    let mut normalized = HashMap::new();
                    for (key, value) in entries {
                        normalized.insert(Self::canonical(&key), value);
                    }
                    state.disk_keys = normalized.keys().cloned().collect();
                    state.entries = Some(normalized);
                    true
                }
                Err(error) => {
                    // A payload contract that cannot be deserialized discards the whole
                    // cache instead of failing the caller.
                    log_warn!("Discarding lyric cache '{}': {error}", self.path.display());
                    state.entries = None;
                    false
                }
            },
        }
    }
}

impl<T> LyricCacheStore<T> for JsonLyricCacheStore<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync,
{
    fn try_get(&self, key: &str) -> Option<(T, LyricAcquisitionKind)> {
        let canonical = Self::canonical(key);
        let mut state = self.state.lock().ok()?;
        if !self.ensure_loaded(&mut state) {
            return None;
        }

        let entries = state.entries.as_ref()?;
        let value = entries.get(&canonical)?.clone();
        let acquisition = if state.disk_keys.contains(&canonical) {
            LyricAcquisitionKind::DiskCache
        } else {
            LyricAcquisitionKind::MemoryCache
        };
        Some((value, acquisition))
    }

    fn store(&self, key: &str, value: T) {
        let canonical = Self::canonical(key);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !self.ensure_loaded(&mut state) || state.entries.is_none() {
            return;
        }

        state.disk_keys.remove(&canonical);
        if let Some(entries) = state.entries.as_mut() {
            entries.insert(canonical.clone(), value);
        }
        let snapshot = state.entries.clone().unwrap_or_default();
        drop(state);
        self.persist(&snapshot);
    }

    fn remove(&self, key: &str) {
        let canonical = Self::canonical(key);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !self.ensure_loaded(&mut state) {
            return;
        }

        let removed = state
            .entries
            .as_mut()
            .map(|entries| entries.remove(&canonical).is_some())
            .unwrap_or(false);
        if !removed {
            return;
        }

        state.disk_keys.remove(&canonical);
        let snapshot = state.entries.clone().unwrap_or_default();
        drop(state);
        self.persist(&snapshot);
    }

    fn clear(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.entries = Some(HashMap::new());
        state.disk_keys.clear();
        state.loaded = true;
        let _ = fs::remove_file(&self.path);
    }
}

impl<T> JsonLyricCacheStore<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn persist(&self, entries: &HashMap<String, T>) {
        let Some(directory) = self.path.parent() else {
            return;
        };
        if let Err(error) = fs::create_dir_all(directory) {
            log_warn!("Unable to create lyric cache directory: {error}");
            return;
        }

        let temporary = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let payload = match serde_json::to_string_pretty(entries) {
            Ok(payload) => payload,
            Err(error) => {
                log_warn!("Unable to serialize lyric cache: {error}");
                return;
            }
        };

        if let Err(error) = fs::write(&temporary, payload) {
            log_warn!("Unable to write lyric cache: {error}");
            return;
        }
        if let Err(error) = fs::rename(&temporary, &self.path) {
            log_warn!("Unable to replace lyric cache: {error}");
            let _ = fs::remove_file(&temporary);
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline cache (raw + parsed)
// ---------------------------------------------------------------------------

/// `Models.LyricPipelineCache.cs` — `RawLyricCacheEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawLyricCacheEnvelope {
    pub cache_version: i32,
    pub provider_id: String,
    pub candidate_id: String,
    #[serde(deserialize_with = "crate::models::ordinal::deserialize_payload_format")]
    pub format: LyricPayloadFormat,
    pub payload_hash: String,
    pub fetched_at_utc: String,
    pub original_lyrics: Option<String>,
    pub translation_lyrics: Option<String>,
    pub is_encrypted: bool,
    pub is_pure_music: bool,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, String>,
}

/// `Models.LyricPipelineCache.cs` — `ParsedLyricCacheEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLyricCacheEnvelope {
    pub cache_version: i32,
    pub provider_id: String,
    pub candidate_id: String,
    pub raw_payload_hash: String,
    pub parser_id: String,
    pub parser_version: String,
    pub normalization_version: String,
    pub content: ParsedLyrics,
}

/// `LyricPipelineCache`.
pub struct LyricPipelineCache {
    raw_store: Box<dyn LyricCacheStore<RawLyricCacheEnvelope>>,
    parsed_store: Box<dyn LyricCacheStore<ParsedLyricCacheEnvelope>>,
}

impl LyricPipelineCache {
    pub const RAW_CACHE_VERSION: i32 = 1;
    pub const PARSED_CACHE_VERSION: i32 = 1;

    pub fn new(
        raw_store: Box<dyn LyricCacheStore<RawLyricCacheEnvelope>>,
        parsed_store: Box<dyn LyricCacheStore<ParsedLyricCacheEnvelope>>,
    ) -> Self {
        Self {
            raw_store,
            parsed_store,
        }
    }

    pub fn with_paths(raw_path: impl Into<PathBuf>, parsed_path: impl Into<PathBuf>) -> Self {
        Self::new(
            Box::new(JsonLyricCacheStore::new(raw_path)),
            Box::new(JsonLyricCacheStore::new(parsed_path)),
        )
    }

    pub fn try_get_raw(
        &self,
        provider_id: &LyricProviderId,
        candidate_id: &str,
    ) -> Option<(RawLyricPayload, LyricAcquisitionKind)> {
        let key = build_key(provider_id, candidate_id);
        let (envelope, acquisition) = self.raw_store.try_get(&key)?;

        if !is_valid_raw_envelope(&envelope, provider_id, candidate_id) {
            self.raw_store.remove(&key);
            return None;
        }

        let payload = RawLyricPayload {
            provider_id: provider_id.clone(),
            candidate_id: candidate_id.to_string(),
            format: envelope.format,
            original_lyrics: envelope.original_lyrics,
            translation_lyrics: envelope.translation_lyrics,
            is_encrypted: envelope.is_encrypted,
            is_pure_music: envelope.is_pure_music,
            diagnostics: envelope.diagnostics,
            has_stable_identity: true,
        };
        Some((payload, acquisition))
    }

    pub fn store_raw(&self, payload: &RawLyricPayload, fetched_at_utc: &str) {
        if !payload.has_stable_identity {
            return;
        }

        let envelope = RawLyricCacheEnvelope {
            cache_version: Self::RAW_CACHE_VERSION,
            provider_id: payload.provider_id.value().to_string(),
            candidate_id: payload.candidate_id.clone(),
            format: payload.format,
            payload_hash: compute_payload_hash(payload),
            fetched_at_utc: fetched_at_utc.to_string(),
            original_lyrics: payload.original_lyrics.clone(),
            translation_lyrics: payload.translation_lyrics.clone(),
            is_encrypted: payload.is_encrypted,
            is_pure_music: payload.is_pure_music,
            diagnostics: payload.diagnostics.clone(),
        };
        self.raw_store
            .store(&build_key(&payload.provider_id, &payload.candidate_id), envelope);
    }

    pub fn try_get_parsed(
        &self,
        raw_payload: &RawLyricPayload,
        parser_id: &str,
        parser_version: &str,
        normalization_version: &str,
    ) -> Option<(ParsedLyrics, LyricAcquisitionKind)> {
        if !raw_payload.has_stable_identity {
            return None;
        }

        let key = build_key(&raw_payload.provider_id, &raw_payload.candidate_id);
        let (envelope, acquisition) = self.parsed_store.try_get(&key)?;

        let expected_hash = compute_payload_hash(raw_payload);
        let identity_matches = envelope.provider_id == raw_payload.provider_id.value()
            && envelope.candidate_id == raw_payload.candidate_id;

        if !identity_matches
            || envelope.cache_version != Self::PARSED_CACHE_VERSION
            || envelope.raw_payload_hash.trim().is_empty()
            || !is_usable(&envelope.content)
        {
            self.parsed_store.remove(&key);
            return None;
        }

        if envelope.raw_payload_hash != expected_hash
            || envelope.parser_id != parser_id
            || envelope.parser_version != parser_version
            || envelope.normalization_version != normalization_version
        {
            return None;
        }

        Some((envelope.content, acquisition))
    }

    pub fn store_parsed(
        &self,
        raw_payload: &RawLyricPayload,
        parsed_lyrics: &ParsedLyrics,
        parser_id: &str,
        parser_version: &str,
        normalization_version: &str,
    ) {
        if !raw_payload.has_stable_identity || !is_usable(parsed_lyrics) {
            return;
        }

        let envelope = ParsedLyricCacheEnvelope {
            cache_version: Self::PARSED_CACHE_VERSION,
            provider_id: raw_payload.provider_id.value().to_string(),
            candidate_id: raw_payload.candidate_id.clone(),
            raw_payload_hash: compute_payload_hash(raw_payload),
            parser_id: parser_id.to_string(),
            parser_version: parser_version.to_string(),
            normalization_version: normalization_version.to_string(),
            content: parsed_lyrics.clone(),
        };
        self.parsed_store
            .store(&build_key(&raw_payload.provider_id, &raw_payload.candidate_id), envelope);
    }

    pub fn clear(&self) {
        self.raw_store.clear();
        self.parsed_store.clear();
    }
}

/// `LyricPipelineCache.ComputePayloadHash` — SHA-256 over the canonical payload string,
/// including the explicit `-1:` / `length:value` encoding of nullable fields.
pub fn compute_payload_hash(payload: &RawLyricPayload) -> String {
    let canonical = [
        payload.format.ordinal().to_string(),
        if payload.is_encrypted { "1" } else { "0" }.to_string(),
        if payload.is_pure_music { "1" } else { "0" }.to_string(),
        encode(payload.original_lyrics.as_deref()),
        encode(payload.translation_lyrics.as_deref()),
    ]
    .join("\u{1f}");

    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn encode(value: Option<&str>) -> String {
    match value {
        None => "-1:".to_string(),
        Some(text) => format!("{}:{}", text.encode_utf16().count(), text),
    }
}

fn is_valid_raw_envelope(
    envelope: &RawLyricCacheEnvelope,
    provider_id: &LyricProviderId,
    candidate_id: &str,
) -> bool {
    if envelope.cache_version != LyricPipelineCache::RAW_CACHE_VERSION
        || envelope.provider_id != provider_id.value()
        || envelope.candidate_id != candidate_id
        || envelope.payload_hash.trim().is_empty()
        || (!envelope.is_pure_music
            && envelope
                .original_lyrics
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty())
    {
        return false;
    }

    let payload = RawLyricPayload {
        provider_id: provider_id.clone(),
        candidate_id: candidate_id.to_string(),
        format: envelope.format,
        original_lyrics: envelope.original_lyrics.clone(),
        translation_lyrics: envelope.translation_lyrics.clone(),
        is_encrypted: envelope.is_encrypted,
        is_pure_music: envelope.is_pure_music,
        diagnostics: envelope.diagnostics.clone(),
        has_stable_identity: true,
    };
    envelope.payload_hash == compute_payload_hash(&payload)
}

fn is_usable(content: &ParsedLyrics) -> bool {
    content.is_pure_music || !content.lines.is_empty()
}

fn build_key(provider_id: &LyricProviderId, candidate_id: &str) -> String {
    format!("{}:{}", provider_id.value(), candidate_id)
}

// ---------------------------------------------------------------------------
// Resolved lyric cache
// ---------------------------------------------------------------------------

/// `Models.ResolvedLyricCache.cs` — `ResolvedLyricCacheEntry`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLyricCacheEntry {
    pub track_id: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub source_app: Option<String>,
    pub duration: Duration,
    pub song_id: Option<String>,
    pub provider_id: Option<String>,
    pub candidate_id: Option<String>,
    #[serde(deserialize_with = "crate::models::ordinal::deserialize_acquisition")]
    pub acquisition: LyricAcquisitionKind,
    pub diagnostics: Option<BTreeMap<String, String>>,
    pub content: Option<ParsedLyrics>,
    #[serde(default)]
    pub selection_context: Option<String>,
}

/// `Models.ResolvedLyricCache.cs` — `ResolvedLyricCacheEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLyricCacheEnvelope {
    pub version: i32,
    pub entries: Option<BTreeMap<String, ResolvedLyricCacheEntry>>,
}

/// The legacy `user-lyric-bindings-v1.json` shape, kept readable for migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStoreEnvelope {
    version: i32,
    bindings: Option<Vec<Option<LegacyBindingRecord>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyBindingRecord {
    #[serde(default)]
    track_id: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    source_app: Option<String>,
    duration: Duration,
    #[serde(default)]
    song_id: Option<String>,
    provider_id: Option<String>,
    candidate_id: Option<String>,
    #[serde(deserialize_with = "crate::models::ordinal::deserialize_acquisition")]
    acquisition: LyricAcquisitionKind,
    #[serde(default)]
    diagnostics: Option<BTreeMap<String, String>>,
    content: Option<ParsedLyrics>,
}

/// Result of a cache write, mirroring `ResolvedLyricCacheStoreResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLyricCacheStoreResult {
    pub is_saved: bool,
    pub failure_reason: Option<String>,
}

impl ResolvedLyricCacheStoreResult {
    fn saved() -> Self {
        Self {
            is_saved: true,
            failure_reason: None,
        }
    }

    fn failed(reason: impl Into<String>) -> Self {
        Self {
            is_saved: false,
            failure_reason: Some(reason.into()),
        }
    }
}

/// `JsonResolvedLyricCache`.
pub struct JsonResolvedLyricCache {
    file_path: PathBuf,
    legacy_file_path: PathBuf,
    state: Mutex<ResolvedCacheState>,
}

struct ResolvedCacheState {
    entries: Option<BTreeMap<String, ResolvedLyricCacheEntry>>,
    disk_keys: BTreeSet<String>,
    load_attempted: bool,
}

impl JsonResolvedLyricCache {
    pub const CURRENT_VERSION: i32 = 2;
    pub const DEFAULT_FILE_NAME: &'static str = "resolved-lyrics-v1.json";
    pub const LEGACY_FILE_NAME: &'static str = "user-lyric-bindings-v1.json";

    const CACHE_KEY_VERSION: &'static str = "resolved-lyrics-key-v1";
    const SCOPED_CACHE_KEY_VERSION: &'static str = "resolved-lyrics-key-v2";

    pub fn new(file_path: impl Into<PathBuf>) -> Self {
        let file_path = file_path.into();
        let legacy_file_path = file_path
            .parent()
            .map(|parent| parent.join(Self::LEGACY_FILE_NAME))
            .unwrap_or_else(|| PathBuf::from(Self::LEGACY_FILE_NAME));

        Self {
            file_path,
            legacy_file_path,
            state: Mutex::new(ResolvedCacheState {
                entries: None,
                disk_keys: BTreeSet::new(),
                load_attempted: false,
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn try_get(&self, track: &TrackInfo, context: Option<&str>) -> Option<ResolvedLyrics> {
        let key = try_create_cache_key(&track.title, &track.artist, context)?;
        let mut state = self.state.lock().ok()?;
        if !self.ensure_loaded(&mut state) {
            return None;
        }

        let entry = state.entries.as_ref()?.get(&key)?.clone();
        let acquisition = if state.disk_keys.remove(&key) {
            LyricAcquisitionKind::DiskCache
        } else {
            LyricAcquisitionKind::MemoryCache
        };

        match create_resolved_lyrics(&entry, acquisition) {
            Some(lyrics) => Some(lyrics),
            None => {
                if let Some(entries) = state.entries.as_mut() {
                    entries.remove(&key);
                }
                let snapshot = state.entries.clone();
                drop(state);
                if let Some(snapshot) = snapshot {
                    self.persist(&snapshot);
                }
                None
            }
        }
    }

    pub fn store(
        &self,
        track: &TrackInfo,
        resolved: &ResolvedLyrics,
        context: Option<&str>,
    ) -> ResolvedLyricCacheStoreResult {
        let Some(key) = try_create_cache_key(&track.title, &track.artist, context) else {
            log_warn!(
                "Resolved lyric cache rejected track '{}', candidate '{}': title or artist is invalid.",
                track.id,
                resolved.candidate_id
            );
            return ResolvedLyricCacheStoreResult::failed("当前歌曲缺少有效的标题或歌手。");
        };

        let entry = match try_create_entry(track, resolved, context) {
            Some(entry) => entry,
            None => {
                let reason = get_content_validation_failure(Some(&resolved.content))
                    .unwrap_or_else(|| "歌词来源或候选信息无效".to_string());
                log_warn!(
                    "Resolved lyric cache rejected track '{}', candidate '{}': {reason}.",
                    track.id,
                    resolved.candidate_id
                );
                return ResolvedLyricCacheStoreResult::failed(format!("{reason}。"));
            }
        };

        let Ok(mut state) = self.state.lock() else {
            return ResolvedLyricCacheStoreResult::failed("歌词缓存当前不可用，请重启应用后重试。");
        };
        if !self.ensure_loaded(&mut state) {
            return ResolvedLyricCacheStoreResult::failed("歌词缓存当前不可用，请重启应用后重试。");
        }

        if state.entries.is_none() {
            return ResolvedLyricCacheStoreResult::failed("歌词缓存当前不可用，请重启应用后重试。");
        }

        state.disk_keys.remove(&key);
        if let Some(entries) = state.entries.as_mut() {
            entries.insert(key.clone(), entry);
        }
        let snapshot = state.entries.clone().unwrap_or_default();
        drop(state);

        if !self.persist(&snapshot) {
            return ResolvedLyricCacheStoreResult::failed("缓存文件写入失败，请检查用户数据目录和磁盘空间。");
        }
        ResolvedLyricCacheStoreResult::saved()
    }

    pub fn clear(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.entries = Some(BTreeMap::new());
        state.disk_keys.clear();
        state.load_attempted = true;
        let _ = fs::remove_file(&self.file_path);
        if self.legacy_file_path != self.file_path {
            let _ = fs::remove_file(&self.legacy_file_path);
        }
    }

    fn ensure_loaded(&self, state: &mut ResolvedCacheState) -> bool {
        if state.load_attempted {
            return state.entries.is_some();
        }
        state.load_attempted = true;

        if !self.file_path.exists() {
            return self.try_load_legacy_or_initialize_empty(state);
        }

        let text = match fs::read_to_string(&self.file_path) {
            Ok(text) => text,
            Err(error) => {
                log_warn!(
                    "Failed to read resolved lyric cache '{}': {error}",
                    self.file_path.display()
                );
                state.entries = None;
                return false;
            }
        };

        let envelope: ResolvedLyricCacheEnvelope = match serde_json::from_str(&text) {
            Ok(envelope) => envelope,
            Err(error) => {
                log_warn!(
                    "Failed to read resolved lyric cache '{}': {error}",
                    self.file_path.display()
                );
                state.entries = None;
                return false;
            }
        };

        if (envelope.version != Self::CURRENT_VERSION && envelope.version != 1)
            || envelope.entries.is_none()
        {
            log_warn!(
                "Failed to read resolved lyric cache '{}': version or structure is invalid",
                self.file_path.display()
            );
            state.entries = None;
            return false;
        }

        let mut valid: BTreeMap<String, ResolvedLyricCacheEntry> = BTreeMap::new();
        let mut invalid_count = 0usize;
        for (key, entry) in envelope.entries.unwrap_or_default() {
            if !is_valid_entry(&key, Some(&entry)) {
                invalid_count += 1;
                continue;
            }
            valid.insert(key, entry);
        }

        state.disk_keys = valid.keys().cloned().collect();
        if invalid_count > 0 {
            log_warn!(
                "Removed {invalid_count} invalid record(s) from resolved lyric cache '{}'.",
                self.file_path.display()
            );
            let snapshot = valid.clone();
            state.entries = Some(valid);
            self.persist(&snapshot);
            return true;
        }

        state.entries = Some(valid);
        true
    }

    fn try_load_legacy_or_initialize_empty(&self, state: &mut ResolvedCacheState) -> bool {
        if !self.legacy_file_path.exists() {
            state.entries = Some(BTreeMap::new());
            return true;
        }

        let migrated = match self.read_legacy_entries() {
            Some(entries) => entries,
            None => {
                state.entries = None;
                return false;
            }
        };

        if !self.persist(&migrated) {
            state.entries = None;
            return false;
        }

        state.disk_keys = migrated.keys().cloned().collect();
        state.entries = Some(migrated);
        let _ = fs::remove_file(&self.legacy_file_path);
        true
    }

    fn read_legacy_entries(&self) -> Option<BTreeMap<String, ResolvedLyricCacheEntry>> {
        let text = fs::read_to_string(&self.legacy_file_path).ok()?;
        let envelope: LegacyStoreEnvelope = serde_json::from_str(&text).ok()?;
        if envelope.version != 1 {
            return None;
        }

        let mut migrated: BTreeMap<String, ResolvedLyricCacheEntry> = BTreeMap::new();
        for binding in envelope.bindings.unwrap_or_default() {
            let binding = binding?;
            let key = try_create_cache_key(
                binding.title.as_deref().unwrap_or(""),
                binding.artist.as_deref().unwrap_or(""),
                None,
            )?;
            if binding.duration.is_zero() {
                return None;
            }

            let entry = ResolvedLyricCacheEntry {
                track_id: binding.track_id,
                title: binding.title,
                artist: binding.artist,
                album: binding.album,
                source_app: binding.source_app,
                duration: binding.duration,
                song_id: binding.song_id,
                provider_id: binding.provider_id,
                candidate_id: binding.candidate_id,
                acquisition: binding.acquisition,
                diagnostics: binding.diagnostics,
                content: binding.content,
                selection_context: None,
            };
            if !is_valid_entry(&key, Some(&entry)) {
                return None;
            }
            // The old store appended newer selections, so the last record wins.
            migrated.insert(key, entry);
        }

        Some(migrated)
    }

    fn persist(&self, entries: &BTreeMap<String, ResolvedLyricCacheEntry>) -> bool {
        if let Some(directory) = self.file_path.parent() {
            if let Err(error) = fs::create_dir_all(directory) {
                log_warn!("Failed to create resolved lyric cache directory: {error}");
                return false;
            }
        }

        let envelope = ResolvedLyricCacheEnvelope {
            version: Self::CURRENT_VERSION,
            entries: Some(entries.clone()),
        };

        let payload = match serde_json::to_string_pretty(&envelope) {
            Ok(payload) => payload,
            Err(error) => {
                log_warn!("Failed to serialize resolved lyric cache: {error}");
                return false;
            }
        };

        let temporary = self
            .file_path
            .with_extension(format!("{}.tmp", std::process::id()));
        if let Err(error) = fs::write(&temporary, payload) {
            log_warn!("Failed to persist resolved lyric cache: {error}");
            let _ = fs::remove_file(&temporary);
            return false;
        }
        if let Err(error) = fs::rename(&temporary, &self.file_path) {
            log_warn!("Failed to replace resolved lyric cache: {error}");
            let _ = fs::remove_file(&temporary);
            return false;
        }
        true
    }
}

fn try_create_entry(
    track: &TrackInfo,
    resolved: &ResolvedLyrics,
    context: Option<&str>,
) -> Option<ResolvedLyricCacheEntry> {
    let entry = ResolvedLyricCacheEntry {
        track_id: Some(track.id.clone()),
        title: Some(track.title.clone()),
        artist: Some(track.artist.clone()),
        album: Some(track.album.clone()),
        source_app: Some(track.source_app.clone()),
        duration: track.duration,
        song_id: track.song_id.clone(),
        provider_id: Some(resolved.provider_id.value().to_string()),
        candidate_id: Some(resolved.candidate_id.clone()),
        acquisition: resolved.acquisition,
        diagnostics: Some(resolved.diagnostics.clone()),
        content: Some(resolved.content.clone()),
        selection_context: context.map(str::to_string),
    };

    let key = try_create_cache_key(&track.title, &track.artist, entry.selection_context.as_deref())?;
    if is_valid_entry(&key, Some(&entry)) {
        Some(entry)
    } else {
        None
    }
}

fn is_valid_entry(key: &str, entry: Option<&ResolvedLyricCacheEntry>) -> bool {
    let Some(entry) = entry else {
        return false;
    };

    let Some(expected_key) = try_create_cache_key(
        entry.title.as_deref().unwrap_or(""),
        entry.artist.as_deref().unwrap_or(""),
        entry.selection_context.as_deref(),
    ) else {
        return false;
    };

    key == expected_key
        && entry
            .provider_id
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && entry
            .candidate_id
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && entry.diagnostics.is_some()
        && get_content_validation_failure(entry.content.as_ref()).is_none()
}

fn create_resolved_lyrics(
    entry: &ResolvedLyricCacheEntry,
    acquisition: LyricAcquisitionKind,
) -> Option<ResolvedLyrics> {
    let content = entry.content.clone()?;
    if get_content_validation_failure(Some(&content)).is_some() {
        return None;
    }

    let provider_id = LyricProviderId::try_new(entry.provider_id.as_deref()?).ok()?;
    let candidate_id = entry.candidate_id.clone()?;
    if candidate_id.trim().is_empty() {
        return None;
    }

    ResolvedLyrics::try_new(
        content,
        provider_id,
        candidate_id,
        acquisition,
        entry.diagnostics.clone().unwrap_or_default(),
    )
    .ok()
}

/// `GetContentValidationFailure` — returns the (Chinese) reason the content is unusable.
pub fn get_content_validation_failure(content: Option<&ParsedLyrics>) -> Option<String> {
    let Some(content) = content else {
        return Some("歌词内容缺失".to_string());
    };

    if !matches!(
        content.timing_kind,
        LyricTimingKind::Unsynced
            | LyricTimingKind::LineTimed
            | LyricTimingKind::WordTimed
            | LyricTimingKind::CharacterTimed
            | LyricTimingKind::Mixed
    ) || !matches!(
        content.timing_provenance,
        LyricTimingProvenance::Unknown
            | LyricTimingProvenance::ProviderSupplied
            | LyricTimingProvenance::Synthetic
    ) || (!content.is_pure_music && content.lines.is_empty())
    {
        return Some("歌词结构无效".to_string());
    }

    for (line_index, line) in content.lines.iter().enumerate() {
        if line.text.trim().is_empty()
            || line.end_time.is_some_and(|end| end < line.start_time)
        {
            return Some(format!("第 {} 行无效", line_index + 1));
        }

        for (segment_index, segment) in line.segments.iter().enumerate() {
            if segment.start_time < line.start_time
                || segment.end_time <= segment.start_time
                || segment.text.is_empty()
                || line
                    .end_time
                    .is_some_and(|line_end| segment.end_time > line_end)
            {
                return Some(format!(
                    "第 {} 行的第 {} 个片段无效",
                    line_index + 1,
                    segment_index + 1
                ));
            }
        }
    }

    None
}

/// `TryCreateCacheKey` — SHA-256 over a canonical, length-prefixed key string.
pub fn try_create_cache_key(title: &str, artist: &str, context: Option<&str>) -> Option<String> {
    let normalized_title = normalize_required(title)?;
    let normalized_artist = normalize_required(artist)?;

    let canonical = if let Some(context) = context {
        format!(
            "{}:title:{}:{}:artist:{}:{}:context:{}:{}",
            JsonResolvedLyricCache::SCOPED_CACHE_KEY_VERSION,
            normalized_title.encode_utf16().count(),
            normalized_title,
            normalized_artist.encode_utf16().count(),
            normalized_artist,
            context.encode_utf16().count(),
            context
        )
    } else {
        format!(
            "{}:title:{}:{}:artist:{}:{}",
            JsonResolvedLyricCache::CACHE_KEY_VERSION,
            normalized_title.encode_utf16().count(),
            normalized_title,
            normalized_artist.encode_utf16().count(),
            normalized_artist
        )
    };

    let digest = Sha256::digest(canonical.as_bytes());
    Some(digest.iter().map(|byte| format!("{byte:02X}")).collect())
}

fn normalize_required(value: &str) -> Option<String> {
    let normalized: String = value.nfkc().collect();

    let mut builder = String::with_capacity(normalized.len());
    let mut pending_whitespace = false;
    for character in normalized.chars() {
        if character.is_whitespace() {
            pending_whitespace = !builder.is_empty();
            continue;
        }
        if pending_whitespace {
            builder.push(' ');
            pending_whitespace = false;
        }
        builder.push(character);
    }

    let result = builder.trim().to_uppercase();
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{known_providers, ms, secs, ParsedLyricLine, ParsedLyricSegment};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Every test gets its own directory: the resolved cache derives the legacy file name
    /// from the file's parent directory, so sharing a directory would race.
    fn temp_dir(prefix: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let directory = std::env::temp_dir().join(format!(
            "lyric-island-test-{prefix}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&directory);
        directory
    }

    fn temp_path(prefix: &str, name: &str) -> PathBuf {
        temp_dir(prefix).join(name)
    }

    fn sample_payload() -> RawLyricPayload {
        RawLyricPayload::try_new(
            known_providers::qq_music(),
            "candidate-1",
            LyricPayloadFormat::Qrc,
            Some("[1000,1800]qrc(1000,300)word(1300,1500)".to_string()),
            Some("translation".to_string()),
            false,
            false,
            BTreeMap::new(),
        )
        .unwrap()
    }

    fn sample_lyrics() -> ParsedLyrics {
        ParsedLyrics::new(
            vec![ParsedLyricLine::try_new(
                secs(1),
                Some(ms(2800)),
                "line",
                None,
                vec![ParsedLyricSegment::try_new(secs(1), ms(1300), "syllable").unwrap()],
                false,
            )
            .unwrap()],
            LyricTimingKind::WordTimed,
            LyricTimingProvenance::ProviderSupplied,
            LyricPayloadFormat::Qrc,
            false,
        )
    }

    fn sample_resolved() -> ResolvedLyrics {
        ResolvedLyrics::try_new(
            sample_lyrics(),
            known_providers::qq_music(),
            "candidate-1",
            LyricAcquisitionKind::Remote,
            BTreeMap::new(),
        )
        .unwrap()
    }

    fn sample_track() -> TrackInfo {
        TrackInfo::new("track-1", "Title", "Artist", "Album", "QQMusic", secs(200))
    }

    #[test]
    fn json_store_round_trips_across_instances_and_clears_persisted_state() {
        let path = temp_path("store", "store.json");
        {
            let store: JsonLyricCacheStore<RawLyricCacheEnvelope> =
                JsonLyricCacheStore::new(path.clone());
            let payload = sample_payload();
            store.store(
                "QQMusic:candidate-1",
                RawLyricCacheEnvelope {
                    cache_version: 1,
                    provider_id: "QQMusic".to_string(),
                    candidate_id: "candidate-1".to_string(),
                    format: payload.format,
                    payload_hash: compute_payload_hash(&payload),
                    fetched_at_utc: "2026-10-14T00:00:00Z".to_string(),
                    original_lyrics: payload.original_lyrics.clone(),
                    translation_lyrics: payload.translation_lyrics.clone(),
                    is_encrypted: false,
                    is_pure_music: false,
                    diagnostics: BTreeMap::new(),
                },
            );
        }

        let reopened: JsonLyricCacheStore<RawLyricCacheEnvelope> =
            JsonLyricCacheStore::new(path.clone());
        let (entry, acquisition) = reopened.try_get("qqmusic:CANDIDATE-1").expect("disk hit");
        assert_eq!(acquisition, LyricAcquisitionKind::DiskCache);
        assert_eq!(entry.candidate_id, "candidate-1");

        reopened.clear();
        assert!(reopened.try_get("QQMusic:candidate-1").is_none());
        assert!(!path.exists());
    }

    #[test]
    fn json_store_discards_cache_when_payload_contract_cannot_be_deserialized() {
        let path = temp_path("broken", "broken.json");
        fs::write(&path, "{ not json").unwrap();

        let store: JsonLyricCacheStore<RawLyricCacheEnvelope> =
            JsonLyricCacheStore::new(path.clone());
        assert!(store.try_get("any").is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn pipeline_cache_round_trips_raw_and_parsed_provider_timing() {
        let raw_path = temp_path("pipeline", "raw.json");
        let parsed_path = temp_path("pipeline", "parsed.json");
        let cache = LyricPipelineCache::with_paths(&raw_path, &parsed_path);
        let payload = sample_payload();

        assert!(cache
            .try_get_raw(&payload.provider_id, &payload.candidate_id)
            .is_none());

        cache.store_raw(&payload, "2026-10-14T00:00:00Z");

        // Same instance: the entry is still only in memory.
        let (cached, acquisition) = cache
            .try_get_raw(&payload.provider_id, &payload.candidate_id)
            .expect("raw hit");
        assert_eq!(cached.original_lyrics, payload.original_lyrics);
        assert_eq!(acquisition, LyricAcquisitionKind::MemoryCache);

        // A fresh instance reads it from disk.
        let reopened = LyricPipelineCache::with_paths(&raw_path, &parsed_path);
        let (_, acquisition) = reopened
            .try_get_raw(&payload.provider_id, &payload.candidate_id)
            .expect("raw disk hit");
        assert_eq!(acquisition, LyricAcquisitionKind::DiskCache);

        let parsed = sample_lyrics();
        cache.store_parsed(&payload, &parsed, "LyricifyPayloadParser", "1.0.0", "2");
        let (cached, _) = cache
            .try_get_parsed(&payload, "LyricifyPayloadParser", "1.0.0", "2")
            .expect("parsed hit");
        assert_eq!(cached, parsed);

        // A parser or normalization version miss leaves the raw cache reusable.
        assert!(cache
            .try_get_parsed(&payload, "LyricifyPayloadParser", "2.0.0", "2")
            .is_none());
        assert!(cache
            .try_get_raw(&payload.provider_id, &payload.candidate_id)
            .is_some());
    }

    #[test]
    fn pipeline_cache_never_persists_unstable_identity() {
        let cache = LyricPipelineCache::with_paths(
            temp_path("unstable", "raw.json"),
            temp_path("unstable", "parsed.json"),
        );
        let payload = sample_payload().with_stable_identity(false);

        cache.store_raw(&payload, "2026-10-14T00:00:00Z");
        assert!(cache
            .try_get_raw(&payload.provider_id, &payload.candidate_id)
            .is_none());
    }

    #[test]
    fn resolved_cache_ignores_player_album_duration_and_identifiers() {
        let cache = JsonResolvedLyricCache::new(temp_path("resolved", "resolved.json"));
        let track = sample_track();
        assert!(cache.store(&track, &sample_resolved(), None).is_saved);

        let mut other = sample_track();
        other.id = "track-2".to_string();
        other.album = "Other album".to_string();
        other.duration = secs(300);
        other.song_id = Some("player-song-id".to_string());

        assert!(cache.try_get(&other, None).is_some());
    }

    #[test]
    fn resolved_cache_misses_for_different_title_or_artist() {
        let cache = JsonResolvedLyricCache::new(temp_path("resolved", "resolved.json"));
        let track = sample_track();
        cache.store(&track, &sample_resolved(), None);

        let mut different_title = sample_track();
        different_title.title = "Another title".to_string();
        assert!(cache.try_get(&different_title, None).is_none());

        let mut different_artist = sample_track();
        different_artist.artist = "Another artist".to_string();
        assert!(cache.try_get(&different_artist, None).is_none());
    }

    #[test]
    fn resolved_cache_selection_contexts_do_not_reuse_default_results() {
        let cache = JsonResolvedLyricCache::new(temp_path("resolved", "resolved.json"));
        let track = sample_track();

        cache.store(&track, &sample_resolved(), None);
        assert!(cache.try_get(&track, Some("user-binding-v1")).is_none());

        cache.store(&track, &sample_resolved(), Some("user-binding-v1"));
        assert!(cache.try_get(&track, Some("user-binding-v1")).is_some());
    }

    #[test]
    fn resolved_cache_rejects_invalid_track_identity_with_a_reason() {
        let cache = JsonResolvedLyricCache::new(temp_path("resolved", "resolved.json"));
        let track = TrackInfo::new("track-1", "   ", "Artist", "Album", "QQMusic", secs(200));

        let result = cache.store(&track, &sample_resolved(), None);
        assert!(!result.is_saved);
        assert_eq!(
            result.failure_reason.as_deref(),
            Some("当前歌曲缺少有效的标题或歌手。")
        );
    }

    #[test]
    fn legacy_bindings_migrate_by_title_and_artist_with_last_record_winning() {
        let resolved_path = temp_path("resolved", "resolved.json");
        let legacy_path = resolved_path
            .parent()
            .unwrap()
            .join(JsonResolvedLyricCache::LEGACY_FILE_NAME);

        let content = sample_lyrics();
        let legacy = serde_json::json!({
            "version": 1,
            "bindings": [
                {
                    "trackId": "legacy-1",
                    "title": "Title",
                    "artist": "Artist",
                    "album": "Album",
                    "sourceApp": "QQMusic",
                    "duration": { "secs": 200, "nanos": 0 },
                    "providerId": "QQMusic",
                    "candidateId": "legacy-candidate-1",
                    "acquisition": "Remote",
                    "diagnostics": {},
                    "content": content,
                },
                {
                    "trackId": "legacy-2",
                    "title": "Title",
                    "artist": "Artist",
                    "album": "Album",
                    "sourceApp": "QQMusic",
                    "duration": { "secs": 200, "nanos": 0 },
                    "providerId": "Kugou",
                    "candidateId": "legacy-candidate-2",
                    "acquisition": "Remote",
                    "diagnostics": {},
                    "content": content,
                }
            ]
        });
        fs::write(&legacy_path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let cache = JsonResolvedLyricCache::new(resolved_path.clone());
        let lyrics = cache.try_get(&sample_track(), None).expect("migrated hit");
        assert_eq!(lyrics.candidate_id, "legacy-candidate-2");
        assert!(!legacy_path.exists());

        let _ = fs::remove_file(&resolved_path);
    }
}


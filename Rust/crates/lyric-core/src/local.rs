//! `Services.LocalLyricProvider.cs` and `Services.LocalMediaIndex.cs`.
//!
//! Scans configured folders for sidecar lyric files (or embedded lyrics in audio files),
//! scores every candidate against the playing track and parses the winner.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;

use crate::formats;
use crate::matching::LyricMatcher;
use crate::models::{
    LyricAcquisitionKind, LyricDocument, LyricLine, LyricSyllable, LyricTimingKind,
    LyricTimingProvenance, ParsedLyrics, TrackInfo,
};
use crate::payload::LyricifyPayloadParser;
use crate::policy::LyricMatchingPolicy;
use crate::projector::LyricDocumentSemanticProjector;
use crate::{log_info, log_warn};

const AUDIO_EXTENSIONS: [&str; 8] = [
    ".mp3", ".flac", ".m4a", ".aac", ".wav", ".ogg", ".opus", ".wma",
];
const LYRIC_EXTENSIONS: [&str; 3] = [".lrc", ".qrc", ".krc"];
/// `LocalLyricProvider.FindBestMatch` gives up after this budget.
const MATCH_BUDGET: Duration = Duration::from_millis(150);

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(\d+):(\d+)(?:[\.:](\d{1,3}))?\]").expect("local lrc timestamp regex")
});
static OFFSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[offset\s*:\s*([+-]?\d+)\]").expect("local offset regex")
});
static TRACK_NUMBER_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:\(?\d{1,3}\)?\s*[-._ ]*)+").expect("track number prefix regex")
});
static BRACKET_ARTIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\[([^\]]+)\]\s*(.+)$").expect("bracket artist regex")
});
static INLINE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("inline tag regex"));
static ENHANCED_LRC_BOUNDARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(?P<minutes>\d+):(?P<seconds>\d+)(?:[\.:](?P<fraction>\d{1,3}))?>")
        .expect("enhanced lrc boundary regex")
});
static CREDIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(作词|作曲|编曲|词|曲|Composer|Lyricist|Lyrics?|Music|Arranger|Producer|Written\s+by|Composed\s+by)\s*[:：]",
    )
    .expect("credit regex")
});

const OPENING_CREDIT_WINDOW_MS: i64 = 5000;

// ---------------------------------------------------------------------------
// Media index
// ---------------------------------------------------------------------------

/// `Services.LocalMediaIndex.cs` — a versioned snapshot of the scanned files.
pub struct LocalMediaIndex {
    root_folders: Vec<String>,
    version: AtomicU64,
    snapshot: Mutex<Option<(u64, Vec<String>)>>,
}

impl LocalMediaIndex {
    pub fn new(root_folders: Vec<String>) -> Self {
        Self {
            root_folders,
            version: AtomicU64::new(0),
            snapshot: Mutex::new(None),
        }
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Enumerates the index, refreshing the cached snapshot when the version changed.
    pub fn snapshot(&self) -> Vec<String> {
        if let Ok(guard) = self.snapshot.lock() {
            if let Some((version, files)) = guard.as_ref() {
                if *version == self.version() {
                    return files.clone();
                }
            }
        }

        let mut files = Vec::new();
        for root in &self.root_folders {
            enumerate_safely(Path::new(root), &mut files);
        }
        files.sort();
        files.dedup();

        let version = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut guard) = self.snapshot.lock() {
            *guard = Some((version, files.clone()));
        }
        files
    }

    /// Forces a rescan on the next snapshot call.
    pub fn invalidate(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }
}

/// Recursive walk that keeps already collected entries when a directory cannot be read.
fn enumerate_safely(directory: &Path, out: &mut Vec<String>) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            log_warn!("Local media index could not read '{}': {error}", directory.display());
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => enumerate_safely(&path, out),
            Ok(file_type) if file_type.is_file() => {
                if is_indexable(&path) {
                    out.push(path.to_string_lossy().to_string());
                }
            }
            _ => {}
        }
    }
}

fn is_indexable(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = format!(".{}", extension.to_lowercase());
    LYRIC_EXTENSIONS.contains(&extension.as_str()) || AUDIO_EXTENSIONS.contains(&extension.as_str())
}

// ---------------------------------------------------------------------------
// Local provider
// ---------------------------------------------------------------------------

struct LocalLyricEntry {
    lyric_path: String,
    stem: String,
    artist: String,
    title: String,
}

/// `LocalLyricProvider`.
pub struct LocalLyricProvider {
    root_folders: Vec<String>,
    media_index: LocalMediaIndex,
    index: Mutex<Vec<LocalLyricEntry>>,
    shared_index_version: AtomicU64,
}

impl LocalLyricProvider {
    pub const SOURCE_APP: &'static str = "Local";

    pub fn new(root_folders: Vec<String>) -> Self {
        let root_folders: Vec<String> = root_folders
            .into_iter()
            .map(|path| path.trim().trim_matches('"').to_string())
            .filter(|path| !path.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        Self {
            media_index: LocalMediaIndex::new(root_folders.clone()),
            root_folders,
            index: Mutex::new(Vec::new()),
            shared_index_version: AtomicU64::new(u64::MAX),
        }
    }

    /// `GetLyricsWithDiagnosticsAsync` — returns the document plus acquisition metadata.
    pub fn get_lyrics(&self, track: &TrackInfo) -> LocalLyricFetch {
        let started = Instant::now();
        let not_found = |elapsed: Duration| LocalLyricFetch {
            document: None,
            acquisition: LyricAcquisitionKind::NotFound,
            elapsed_millis: elapsed.as_millis() as u64,
        };

        if self.root_folders.is_empty()
            || track.title.trim().is_empty()
            || track.title.eq_ignore_ascii_case("Unknown Title")
        {
            return not_found(started.elapsed());
        }

        let index = self.snapshot_index();
        if index.is_empty() {
            return not_found(started.elapsed());
        }

        let Some(best) = find_best_match(track, &index) else {
            return not_found(started.elapsed());
        };

        let lines = match parse_lyric_file(&best.entry.lyric_path) {
            Ok(lines) => lines,
            Err(error) => {
                log_warn!(
                    "Local lyric file became unavailable: {}: {error}",
                    best.entry.lyric_path
                );
                return not_found(started.elapsed());
            }
        };

        if lines.is_empty() {
            log_info!(
                "Local lyrics matched but parsed no timed lines: {}",
                best.entry.lyric_path
            );
            return not_found(started.elapsed());
        }

        log_info!(
            "Local lyrics matched: {} - {} => {} ({})",
            track.title,
            track.artist,
            best.entry.lyric_path,
            best.score
        );

        LocalLyricFetch {
            document: Some(LyricDocument::new(
                ensure_syllables(lines),
                best.score,
                false,
            )),
            acquisition: LyricAcquisitionKind::LocalFile,
            elapsed_millis: started.elapsed().as_millis() as u64,
        }
    }

    /// Convenience wrapper used by the coordinator when it needs pipeline-model output.
    pub fn get_parsed_lyrics(&self, track: &TrackInfo) -> Option<(ParsedLyrics, LyricAcquisitionKind)> {
        let fetch = self.get_lyrics(track);
        let document = fetch.document?;
        if document.lines.is_empty() {
            return None;
        }
        Some((
            LyricDocumentSemanticProjector::to_parsed_lyrics(&document),
            fetch.acquisition,
        ))
    }

    /// Builds (or refreshes) the sidecar index and returns how many entries it now holds.
    ///
    /// The C# host calls this from a background task right after constructing the provider,
    /// mirroring the original `LocalMediaIndex` background warm-up, so the first lookup does
    /// not pay for the directory walk.
    pub fn warm_index(&self) -> usize {
        self.snapshot_index().len()
    }

    /// Forces the next [`Self::warm_index`] / lookup to rescan the root folders.
    pub fn invalidate_index(&self) {
        self.media_index.invalidate();
    }

    fn snapshot_index(&self) -> Vec<LocalLyricEntry> {
        let shared = self.media_index.snapshot();
        let shared_version = self.media_index.version();

        let Ok(mut index) = self.index.lock() else {
            return Vec::new();
        };

        if self.shared_index_version.load(Ordering::SeqCst) != shared_version {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut entries = Vec::new();
            for file in &shared {
                if !seen.insert(file.clone()) {
                    continue;
                }
                if let Some(entry) = build_entry(file) {
                    entries.push(entry);
                }
            }
            *index = entries;
            self.shared_index_version.store(shared_version, Ordering::SeqCst);
        }

        index
            .iter()
            .map(|entry| LocalLyricEntry {
                lyric_path: entry.lyric_path.clone(),
                stem: entry.stem.clone(),
                artist: entry.artist.clone(),
                title: entry.title.clone(),
            })
            .collect()
    }
}

/// Result of a local lookup, mirroring `LyricFetchResult`.
pub struct LocalLyricFetch {
    pub document: Option<LyricDocument>,
    pub acquisition: LyricAcquisitionKind,
    pub elapsed_millis: u64,
}

fn build_entry(lyric_path: &str) -> Option<LocalLyricEntry> {
    let file_name = Path::new(lyric_path).file_stem()?.to_string_lossy().to_string();
    let stem = TRACK_NUMBER_PREFIX.replace(&file_name, "").trim().to_string();
    if stem.is_empty() {
        return None;
    }

    let (artist, title) = split_artist_title(&stem);
    Some(LocalLyricEntry {
        lyric_path: lyric_path.to_string(),
        stem,
        artist,
        title,
    })
}

struct LocalLyricMatch<'a> {
    entry: &'a LocalLyricEntry,
    score: i32,
}

fn find_best_match<'a>(track: &TrackInfo, entries: &'a [LocalLyricEntry]) -> Option<LocalLyricMatch<'a>> {
    let started = Instant::now();
    let mut best: Option<LocalLyricMatch<'a>> = None;

    for entry in entries {
        if started.elapsed() > MATCH_BUDGET {
            break;
        }

        let score = score_entry(track, entry);
        if score < LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE {
            continue;
        }
        if best.as_ref().is_none_or(|current| score > current.score) {
            best = Some(LocalLyricMatch { entry, score });
        }
    }

    best
}

fn score_entry(track: &TrackInfo, entry: &LocalLyricEntry) -> i32 {
    let score = LyricMatcher::score(track, &entry.title, &entry.artist, 0, None);
    if score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE {
        return score;
    }

    let normalized_stem = LyricMatcher::normalize_search(&entry.stem);
    let normalized_title = LyricMatcher::normalize_search(&track.title);
    let normalized_artist = LyricMatcher::normalize_search(&track.artist);
    if normalized_title.is_empty() {
        return score;
    }

    let title_hit = normalized_stem.contains(&normalized_title)
        || normalized_title.contains(&normalized_stem);
    let artist_hit = normalized_artist.is_empty() || normalized_stem.contains(&normalized_artist);

    if title_hit && artist_hit {
        return score.max(88);
    }
    if title_hit {
        return score.max(82);
    }
    score
}

fn split_artist_title(stem: &str) -> (String, String) {
    if let Some(captures) = BRACKET_ARTIST.captures(stem) {
        return (captures[1].trim().to_string(), captures[2].trim().to_string());
    }

    for separator in [" - ", " – ", " — ", " _ "] {
        if let Some(index) = stem.find(separator) {
            if index == 0 || index + separator.len() >= stem.len() {
                continue;
            }
            return (
                stem[..index].trim().to_string(),
                stem[index + separator.len()..].trim().to_string(),
            );
        }
    }

    (String::new(), stem.to_string())
}

/// `ParseLyricFile`.
pub fn parse_lyric_file(path: &str) -> std::io::Result<Vec<LyricLine>> {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_lowercase()))
        .unwrap_or_default();

    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        let Some(text) = try_extract_embedded_lyric_text(path) else {
            return Ok(Vec::new());
        };
        return Ok(parse_lrc(&text));
    }

    let bytes = fs::read(path)?;
    let text = decode_text(&bytes);

    if extension == ".qrc" {
        let lines = parse_qrc_tolerant(&text);
        if !lines.is_empty() {
            return Ok(lines);
        }
    }

    if extension == ".krc" {
        let lines = parse_krc_tolerant(&text);
        if !lines.is_empty() {
            return Ok(lines);
        }
    }

    Ok(parse_lrc(&text))
}

fn parse_qrc_tolerant(raw: &str) -> Vec<LyricLine> {
    let payload = if raw.trim_start().starts_with('[') || raw.trim_start().starts_with('{') {
        raw.to_string()
    } else {
        crate::crypto::decrypt_qrc(raw).unwrap_or_else(|_| raw.to_string())
    };
    lines_from_infos(formats::parse_qrc(&payload))
}

fn parse_krc_tolerant(raw: &str) -> Vec<LyricLine> {
    let payload = if raw.trim_start().starts_with('[') {
        raw.to_string()
    } else {
        crate::crypto::decrypt_krc(raw).unwrap_or_else(|_| raw.to_string())
    };
    lines_from_infos(formats::parse_krc(&payload))
}

fn lines_from_infos(infos: Vec<formats::LineInfo>) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = infos
        .into_iter()
        .filter(|info| !info.text.trim().is_empty())
        .filter_map(|info| {
            let start = info.start_time.unwrap_or(0).max(0);
            let syllables: Option<Vec<LyricSyllable>> = if info.syllables.is_empty() {
                None
            } else {
                Some(
                    info.syllables
                        .iter()
                        .filter(|syllable| !syllable.text.is_empty())
                        .map(|syllable| LyricSyllable {
                            relative_offset: Duration::from_millis(
                                (syllable.start_time - start).max(0) as u64,
                            ),
                            duration: Duration::from_millis(
                                (syllable.end_time - syllable.start_time).max(1) as u64,
                            ),
                            text: syllable.text.clone(),
                        })
                        .collect(),
                )
            };

            Some(LyricLine {
                timestamp: Duration::from_millis(start as u64),
                text: clean_text(&info.text),
                translation: info.translation.clone(),
                syllables,
            })
        })
        .collect();

    lines.sort_by_key(|line| line.timestamp);
    lines
}

/// `ParseLrc` — timestamps expand, `[offset:]` applies, opening credits are skipped.
pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    if lrc.trim().is_empty() {
        return Vec::new();
    }

    let mut offset_ms = 0i64;
    if let Some(captures) = OFFSET.captures(lrc) {
        if let Some(value) = captures.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) {
            offset_ms = value;
        }
    }

    let mut raw_lines: Vec<LyricLine> = Vec::new();
    for line in lrc.split(['\n', '\r']).filter(|line| !line.is_empty()) {
        let matches: Vec<_> = TIMESTAMP.find_iter(line).collect();
        if matches.is_empty() {
            continue;
        }

        let last = matches.last().expect("non-empty");
        let source_text = &line[last.end()..];
        let text = clean_text(source_text);
        if text.trim().is_empty() {
            continue;
        }

        for found in matches {
            let mut timestamp =
                Duration::from_millis(parse_lrc_timestamp(found.as_str()).max(0) as u64);
            if offset_ms >= 0 {
                timestamp = timestamp
                    .checked_add(Duration::from_millis(offset_ms as u64))
                    .unwrap_or(timestamp);
            } else {
                timestamp = timestamp
                    .checked_sub(Duration::from_millis((-offset_ms) as u64))
                    .unwrap_or(Duration::ZERO);
            }

            if timestamp.as_millis() as i64 <= OPENING_CREDIT_WINDOW_MS && CREDIT.is_match(&text) {
                continue;
            }

            raw_lines.push(LyricLine {
                timestamp,
                text: text.clone(),
                translation: None,
                syllables: try_parse_enhanced_lrc_syllables(source_text, timestamp, offset_ms),
            });
        }
    }

    align_duplicate_timestamps(raw_lines)
}

fn parse_lrc_timestamp(text: &str) -> i64 {
    let captures = TIMESTAMP.captures(text);
    let (minutes, seconds, fraction) = match captures {
        Some(captures) => (
            captures[1].parse::<i64>().unwrap_or(0),
            captures[2].parse::<i64>().unwrap_or(0),
            captures
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        ),
        None => (0, 0, String::new()),
    };
    minutes * 60_000 + seconds * 1000 + parse_milliseconds(&fraction)
}

fn parse_milliseconds(fraction: &str) -> i64 {
    match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().unwrap_or(0) * 100,
        2 => fraction.parse::<i64>().unwrap_or(0) * 10,
        _ => fraction[..3].parse::<i64>().unwrap_or(0),
    }
}

fn try_parse_enhanced_lrc_syllables(
    source_text: &str,
    line_timestamp: Duration,
    offset_ms: i64,
) -> Option<Vec<LyricSyllable>> {
    let boundaries: Vec<_> = ENHANCED_LRC_BOUNDARY.find_iter(source_text).collect();
    if boundaries.is_empty() || !are_boundary_spans_complete(source_text, &boundaries) {
        return None;
    }

    let mut syllables: Vec<LyricSyllable> = Vec::new();
    let mut cursor = line_timestamp;
    let mut text_start = 0usize;

    for boundary_match in &boundaries {
        let boundary = parse_boundary_time(boundary_match.as_str(), offset_ms)?;
        if boundary < cursor {
            return None;
        }

        let fragment = clean_text(&source_text[text_start..boundary_match.start()]);
        if !fragment.is_empty() {
            if boundary == cursor {
                return None;
            }
            syllables.push(LyricSyllable {
                relative_offset: cursor.saturating_sub(line_timestamp),
                duration: boundary - cursor,
                text: fragment,
            });
        }

        cursor = boundary;
        text_start = boundary_match.end();
    }

    let trailing = clean_text(&source_text[text_start..]);
    if trailing.is_empty() && !syllables.is_empty() {
        Some(syllables)
    } else {
        None
    }
}

fn parse_boundary_time(text: &str, offset_ms: i64) -> Option<Duration> {
    let captures = ENHANCED_LRC_BOUNDARY.captures(text)?;
    let minutes = captures.name("minutes")?.as_str().parse::<i64>().ok()?;
    let seconds = captures.name("seconds")?.as_str().parse::<i64>().ok()?;
    let fraction = captures
        .name("fraction")
        .map(|m| m.as_str())
        .unwrap_or_default();

    let mut timestamp = minutes * 60_000 + seconds * 1000 + parse_milliseconds(fraction);
    timestamp += offset_ms;
    Some(Duration::from_millis(timestamp.max(0) as u64))
}

fn are_boundary_spans_complete(source_text: &str, boundaries: &[regex::Match<'_>]) -> bool {
    let inline_tags: Vec<_> = INLINE_TAG.find_iter(source_text).collect();
    if inline_tags.len() != boundaries.len() {
        return false;
    }

    let mut cursor = 0usize;
    for (boundary, inline_tag) in boundaries.iter().zip(inline_tags.iter()) {
        if boundary.start() != inline_tag.start()
            || boundary.end() != inline_tag.end()
            || boundary.start() < cursor
        {
            return false;
        }

        let before = &source_text[cursor..boundary.start()];
        if before.contains('<') || before.contains('>') {
            return false;
        }
        cursor = boundary.end();
    }

    let after = &source_text[cursor..];
    !after.contains('<') && !after.contains('>')
}

fn clean_text(text: &str) -> String {
    INLINE_TAG.replace_all(text, "").trim().to_string()
}

fn align_duplicate_timestamps(raw_lines: Vec<LyricLine>) -> Vec<LyricLine> {
    let mut grouped: Vec<(Duration, Vec<LyricLine>)> = Vec::new();
    for line in raw_lines {
        match grouped.last_mut() {
            Some((timestamp, bucket)) if *timestamp == line.timestamp => bucket.push(line),
            _ => grouped.push((line.timestamp, vec![line])),
        }
    }
    grouped.sort_by_key(|(timestamp, _)| *timestamp);

    grouped
        .into_iter()
        .filter_map(|(_, mut bucket)| {
            if bucket.len() > 1 {
                let translation = bucket[1].text.clone();
                let mut primary = bucket.remove(0);
                primary.translation = Some(translation);
                Some(primary)
            } else {
                bucket.pop()
            }
        })
        .filter(|line| !line.text.trim().is_empty())
        .collect()
}

/// `EnsureSyllables` — synthesizes per-character syllables when a provider gave none.
pub fn ensure_syllables(mut lines: Vec<LyricLine>) -> Vec<LyricLine> {
    for index in 0..lines.len() {
        let line = lines[index].clone();
        if line.syllables.as_ref().is_some_and(|value| !value.is_empty())
            || line.text.is_empty()
        {
            continue;
        }

        let next_timestamp = lines
            .get(index + 1)
            .map(|next| next.timestamp)
            .unwrap_or(line.timestamp + Duration::from_secs(5));
        let duration_ms = (next_timestamp.saturating_sub(line.timestamp).as_millis() as i64)
            .clamp(500, 10_000);
        let characters: Vec<char> = line.text.chars().collect();
        let ms_per_char = duration_ms as f64 / characters.len() as f64;

        let syllables: Vec<LyricSyllable> = characters
            .iter()
            .enumerate()
            .map(|(character_index, character)| LyricSyllable {
                relative_offset: Duration::from_millis((character_index as f64 * ms_per_char) as u64),
                duration: Duration::from_millis(ms_per_char as u64),
                text: character.to_string(),
            })
            .collect();

        lines[index].syllables = Some(syllables);
    }
    lines
}

// ---------------------------------------------------------------------------
// Embedded lyrics + text decoding
// ---------------------------------------------------------------------------

fn try_extract_embedded_lyric_text(path: &str) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    for key in ["LYRICS", "SYNCEDLYRICS", "UNSYNCEDLYRICS"] {
        if let Some(text) = try_extract_vorbis_comment(&bytes, key) {
            return Some(text);
        }
    }
    None
}

fn try_extract_vorbis_comment(bytes: &[u8], key: &str) -> Option<String> {
    let marker = format!("{key}=").into_bytes();
    let mut search_from = 0usize;

    while let Some(relative) = index_of_ascii_case_insensitive(&bytes[search_from..], &marker) {
        let index = search_from + relative;
        if index < 4 {
            search_from = index + marker.len();
            continue;
        }

        let length = i32::from_le_bytes([bytes[index - 4], bytes[index - 3], bytes[index - 2], bytes[index - 1]]);
        if length >= marker.len() as i32 && length <= 5 * 1024 * 1024 && index + length as usize <= bytes.len()
        {
            let comment = decode_text(&bytes[index..index + length as usize]);
            if let Some(separator) = comment.find('=') {
                if comment[..separator].eq_ignore_ascii_case(key) {
                    return Some(comment[separator + 1..].to_string());
                }
            }
        }

        search_from = index + marker.len();
    }

    None
}

fn index_of_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }

    (0..=haystack.len() - needle.len()).find(|&index| {
        needle.iter().enumerate().all(|(offset, byte)| {
            haystack[index + offset].to_ascii_uppercase() == byte.to_ascii_uppercase()
        })
    })
}

/// `DecodeText` — strict UTF-8 first, then the system code page (GBK on zh-CN Windows).
pub fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => decode_fallback(bytes),
    }
}

#[cfg(feature = "gbk")]
fn decode_fallback(bytes: &[u8]) -> String {
    let (text, _, _) = encoding_rs::GBK.decode(bytes);
    text.to_string()
}

#[cfg(not(feature = "gbk"))]
fn decode_fallback(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

/// Parses a decoded payload for formats the local provider understands.
pub fn parse_local_payload(
    format: crate::models::LyricPayloadFormat,
    content: &str,
) -> Option<ParsedLyrics> {
    let payload = crate::models::DecodedLyricPayload {
        provider_id: crate::models::known_providers::local(),
        candidate_id: "local".to_string(),
        format,
        original_lyrics: Some(content.to_string()),
        translation_lyrics: None,
        is_pure_music: false,
        diagnostics: Default::default(),
    };

    let parsed = LyricifyPayloadParser::parse(&payload).ok()?;
    if parsed.timing_kind == LyricTimingKind::LineTimed && parsed.lines.is_empty() {
        return None;
    }
    let _ = (LyricTimingProvenance::Unknown, PathBuf::new());
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::secs;

    #[test]
    fn lrc_sidecar_is_parsed_with_offset_and_credits_filtered() {
        let text = "[offset:+250]\n[00:00.10]作词: Someone\n[00:03.00][00:01.00]alpha line\n[00:02.500]\n[00:04.00]beta line";
        let lines = parse_lrc(text);
        let times: Vec<u128> = lines.iter().map(|line| line.timestamp.as_millis()).collect();
        assert_eq!(times, vec![1250, 3250, 4250]);
        assert_eq!(lines[0].text, "alpha line");
    }

    #[test]
    fn ensure_syllables_synthesizes_per_character_timing() {
        let lines = vec![LyricLine {
            timestamp: secs(1),
            text: "abc".to_string(),
            translation: None,
            syllables: None,
        }];
        let lines = ensure_syllables(lines);
        let syllables = lines[0].syllables.as_ref().unwrap();
        assert_eq!(syllables.len(), 3);
        assert_eq!(syllables[0].text, "a");
        assert!(syllables[0].duration >= Duration::from_millis(160));
    }

    #[test]
    fn artist_title_split_supports_bracket_and_dash_styles() {
        assert_eq!(
            split_artist_title("[Vocaloid] Tell Your World"),
            ("Vocaloid".to_string(), "Tell Your World".to_string())
        );
        assert_eq!(
            split_artist_title("M83 - Midnight City"),
            ("M83".to_string(), "Midnight City".to_string())
        );
        assert_eq!(
            split_artist_title("Midnight City"),
            (String::new(), "Midnight City".to_string())
        );
    }

    #[test]
    fn local_provider_finds_and_parses_a_sidecar_file() {
        let directory = std::env::temp_dir().join(format!("lyric-island-local-{}", std::process::id()));
        let _ = fs::create_dir_all(&directory);
        let lyric_path = directory.join("M83 - Midnight City.lrc");
        fs::write(&lyric_path, "[00:01.00]first line\n[00:03.00]second line").unwrap();

        let provider = LocalLyricProvider::new(vec![directory.to_string_lossy().to_string()]);
        let track = TrackInfo::new(
            "track-1",
            "Midnight City",
            "M83",
            "Album",
            "Spotify",
            secs(244),
        );

        let fetch = provider.get_lyrics(&track);
        assert!(fetch.document.is_some());
        assert_eq!(fetch.acquisition, LyricAcquisitionKind::LocalFile);
        assert!(fetch.document.unwrap().lines.len() >= 2);

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn decode_text_handles_utf8_and_gbk() {
        assert_eq!(decode_text("中文".as_bytes()), "中文");
        // "中文" encoded as GBK.
        assert_eq!(decode_text(&[0xD6, 0xD0, 0xCE, 0xC4]), "中文");
    }
}

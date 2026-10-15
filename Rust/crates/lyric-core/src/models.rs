//! Domain models, ported from `TaskbarLyrics.Core/Models.*.cs`.
//!
//! The C# original validates its records by throwing (`ArgumentOutOfRangeException`,
//! `ArgumentException`). Rust has no exceptions, so every validating constructor is
//! exposed as `try_new` returning [`ModelError`]. The invariants themselves are kept
//! identical, including the ones the original test-suite asserts on.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Model-level validation failure. Mirrors the exception types of the C# original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// `ArgumentOutOfRangeException` for negative start times.
    NegativeStartTime,
    /// `ArgumentOutOfRangeException` for an end time at or before the start.
    EndNotAfterStart,
    /// `ArgumentException` for empty required text.
    EmptyText(&'static str),
    /// `ArgumentException` when a segment escapes its line timing range.
    SegmentOutsideLine,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NegativeStartTime => write!(f, "start time cannot be negative"),
            ModelError::EndNotAfterStart => write!(f, "end time must be after its start time"),
            ModelError::EmptyText(what) => write!(f, "{what} cannot be empty"),
            ModelError::SegmentOutsideLine => write!(f, "segments must stay within the line timing range"),
        }
    }
}

impl std::error::Error for ModelError {}

pub const fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

pub const fn secs(value: u64) -> Duration {
    Duration::from_secs(value)
}

// ---------------------------------------------------------------------------
// Provider identity
// ---------------------------------------------------------------------------

/// `LyricProviderId` — a non-empty provider name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LyricProviderId(String);

impl LyricProviderId {
    pub fn try_new(value: &str) -> Result<Self, ModelError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ModelError::EmptyText("Provider ID"));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn value(&self) -> &str {
        &self.0
    }

    pub fn eq_ignore_ascii_case(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}

impl fmt::Display for LyricProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `KnownLyricProviders`.
pub mod known_providers {
    pub const LOCAL: &str = "Local";
    pub const QQ_MUSIC: &str = "QQMusic";
    pub const KUGOU: &str = "Kugou";
    pub const NETEASE: &str = "Netease";
    pub const LRCLIB: &str = "LRCLIB";

    use super::LyricProviderId;

    pub fn local() -> LyricProviderId {
        LyricProviderId::try_new(LOCAL).expect("constant provider id")
    }

    pub fn qq_music() -> LyricProviderId {
        LyricProviderId::try_new(QQ_MUSIC).expect("constant provider id")
    }

    pub fn kugou() -> LyricProviderId {
        LyricProviderId::try_new(KUGOU).expect("constant provider id")
    }

    pub fn netease() -> LyricProviderId {
        LyricProviderId::try_new(NETEASE).expect("constant provider id")
    }

    pub fn lrclib() -> LyricProviderId {
        LyricProviderId::try_new(LRCLIB).expect("constant provider id")
    }

    /// `KnownLyricProviders.OnlineTrustOrder` — note Kugou precedes Netease here.
    pub fn online_trust_order() -> Vec<LyricProviderId> {
        vec![qq_music(), kugou(), netease(), lrclib()]
    }
}

// ---------------------------------------------------------------------------
// Track identity
// ---------------------------------------------------------------------------

/// `TrackInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub duration: Duration,
    #[serde(default)]
    pub song_id: Option<String>,
}

impl TrackInfo {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        source_app: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            source_app: source_app.into(),
            duration,
            song_id: None,
        }
    }

    pub fn with_song_id(mut self, song_id: impl Into<String>) -> Self {
        self.song_id = Some(song_id.into());
        self
    }

    /// C# `TimeSpan < TimeSpan.Zero ? TimeSpan.Zero : duration` is unrepresentable with
    /// `Duration`, so this only clamps nothing and exists to keep call sites explicit.
    pub fn normalized_duration(&self) -> Duration {
        self.duration
    }
}

/// `TrackIdentity`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackIdentity {
    pub track_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration: Duration,
    pub source_app: String,
    pub song_id: Option<String>,
    #[serde(default)]
    pub version_markers: Vec<String>,
}

impl TrackIdentity {
    pub fn from_track_info(track: &TrackInfo) -> Self {
        Self {
            track_id: track.id.clone(),
            title: track.title.clone(),
            artists: if track.artist.trim().is_empty() {
                Vec::new()
            } else {
                vec![track.artist.clone()]
            },
            album: track.album.clone(),
            duration: track.duration,
            source_app: track.source_app.clone(),
            song_id: track.song_id.clone(),
            version_markers: Vec::new(),
        }
    }

    pub fn primary_artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or("")
    }

    /// Rebuilds the `TrackInfo` shape the matcher consumes (`string.Join(", ", Artists)`).
    pub fn to_track_info(&self) -> TrackInfo {
        TrackInfo {
            id: self.track_id.clone(),
            title: self.title.clone(),
            artist: self.artists.join(", "),
            album: self.album.clone(),
            source_app: self.source_app.clone(),
            duration: self.duration,
            song_id: self.song_id.clone(),
        }
    }
}

/// `SearchQueryVariant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQueryVariant {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration: Duration,
    #[serde(default)]
    pub relaxation_reasons: Vec<String>,
}

impl SearchQueryVariant {
    pub fn try_new(
        id: impl Into<String>,
        title: impl Into<String>,
        artists: Vec<String>,
        album: impl Into<String>,
        duration: Duration,
        relaxation_reasons: Vec<String>,
    ) -> Result<Self, ModelError> {
        let id = id.into();
        let title = title.into();
        if id.trim().is_empty() {
            return Err(ModelError::EmptyText("Query variant ID"));
        }
        if title.trim().is_empty() {
            return Err(ModelError::EmptyText("Query title"));
        }
        Ok(Self {
            id,
            title,
            artists,
            album: album.into(),
            duration,
            relaxation_reasons,
        })
    }
}

/// `LyricSearchPlan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricSearchPlan {
    pub original_track: TrackIdentity,
    pub variants: Vec<SearchQueryVariant>,
}

impl LyricSearchPlan {
    pub fn try_new(
        original_track: TrackIdentity,
        variants: Vec<SearchQueryVariant>,
    ) -> Result<Self, ModelError> {
        if variants.is_empty() {
            return Err(ModelError::EmptyText("A search plan requires at least one query variant"));
        }
        Ok(Self {
            original_track,
            variants,
        })
    }
}

/// `SourceTrackCandidate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTrackCandidate {
    pub provider_id: LyricProviderId,
    pub candidate_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration: Duration,
    pub query_variant_id: String,
    #[serde(default)]
    pub fetch_metadata: BTreeMap<String, String>,
}

impl SourceTrackCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        provider_id: LyricProviderId,
        candidate_id: impl Into<String>,
        title: impl Into<String>,
        artists: Vec<String>,
        album: impl Into<String>,
        duration: Duration,
        query_variant_id: impl Into<String>,
        fetch_metadata: BTreeMap<String, String>,
    ) -> Result<Self, ModelError> {
        let candidate_id = candidate_id.into();
        let query_variant_id = query_variant_id.into();
        if candidate_id.trim().is_empty() {
            return Err(ModelError::EmptyText("Candidate ID"));
        }
        if query_variant_id.trim().is_empty() {
            return Err(ModelError::EmptyText("Query variant ID"));
        }
        Ok(Self {
            provider_id,
            candidate_id,
            title: title.into(),
            artists,
            album: album.into(),
            duration,
            query_variant_id,
            fetch_metadata,
        })
    }

    pub fn joined_artists(&self) -> String {
        self.artists.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Lyric payloads
// ---------------------------------------------------------------------------

/// `LyricPayloadFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LyricPayloadFormat {
    Unknown,
    Lrc,
    Qrc,
    Krc,
    Yrc,
    Ttml,
    PlainText,
}

impl LyricPayloadFormat {
    /// Ordinal value used by the pipeline cache payload hash.
    pub fn ordinal(self) -> i32 {
        match self {
            LyricPayloadFormat::Unknown => 0,
            LyricPayloadFormat::Lrc => 1,
            LyricPayloadFormat::Qrc => 2,
            LyricPayloadFormat::Krc => 3,
            LyricPayloadFormat::Yrc => 4,
            LyricPayloadFormat::Ttml => 5,
            LyricPayloadFormat::PlainText => 6,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LyricPayloadFormat::Unknown => "Unknown",
            LyricPayloadFormat::Lrc => "Lrc",
            LyricPayloadFormat::Qrc => "Qrc",
            LyricPayloadFormat::Krc => "Krc",
            LyricPayloadFormat::Yrc => "Yrc",
            LyricPayloadFormat::Ttml => "Ttml",
            LyricPayloadFormat::PlainText => "PlainText",
        }
    }
}

/// `RawLyricPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLyricPayload {
    pub provider_id: LyricProviderId,
    pub candidate_id: String,
    pub format: LyricPayloadFormat,
    pub original_lyrics: Option<String>,
    pub translation_lyrics: Option<String>,
    pub is_encrypted: bool,
    pub is_pure_music: bool,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub has_stable_identity: bool,
}

fn default_true() -> bool {
    true
}

impl RawLyricPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        provider_id: LyricProviderId,
        candidate_id: impl Into<String>,
        format: LyricPayloadFormat,
        original_lyrics: Option<String>,
        translation_lyrics: Option<String>,
        is_encrypted: bool,
        is_pure_music: bool,
        diagnostics: BTreeMap<String, String>,
    ) -> Result<Self, ModelError> {
        let candidate_id = candidate_id.into();
        if candidate_id.trim().is_empty() {
            return Err(ModelError::EmptyText("Candidate ID"));
        }
        Ok(Self {
            provider_id,
            candidate_id,
            format,
            original_lyrics,
            translation_lyrics,
            is_encrypted,
            is_pure_music,
            diagnostics,
            has_stable_identity: true,
        })
    }

    pub fn with_stable_identity(mut self, stable: bool) -> Self {
        self.has_stable_identity = stable;
        self
    }
}

/// `DecodedLyricPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedLyricPayload {
    pub provider_id: LyricProviderId,
    pub candidate_id: String,
    pub format: LyricPayloadFormat,
    pub original_lyrics: Option<String>,
    pub translation_lyrics: Option<String>,
    pub is_pure_music: bool,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, String>,
}

impl DecodedLyricPayload {
    pub fn from_raw(raw: &RawLyricPayload, original: Option<String>) -> Self {
        Self {
            provider_id: raw.provider_id.clone(),
            candidate_id: raw.candidate_id.clone(),
            format: raw.format,
            original_lyrics: original,
            translation_lyrics: raw.translation_lyrics.clone(),
            is_pure_music: raw.is_pure_music,
            diagnostics: raw.diagnostics.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsed lyrics
// ---------------------------------------------------------------------------

/// `LyricTimingKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricTimingKind {
    Unsynced,
    LineTimed,
    WordTimed,
    CharacterTimed,
    Mixed,
}

/// `LyricTimingProvenance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricTimingProvenance {
    Unknown,
    ProviderSupplied,
    Synthetic,
}

/// Serialization helpers that accept **both** shapes the original produced.
///
/// `System.Text.Json` (the C# side) writes enums as *numbers* by default, while the Rust
/// port writes them as names. Deserializers below accept either, so cache files written by
/// the .NET build stay readable by the port.
pub mod ordinal {
    use super::*;
    use serde::de::{self, Deserializer};
    use serde::Deserialize;

    fn as_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<serde_json::Value, D::Error> {
        serde_json::Value::deserialize(deserializer)
    }

    fn from_value<T, E>(value: serde_json::Value, from_name: impl Fn(&str) -> Option<T>, from_ordinal: impl Fn(i64) -> Option<T>) -> Result<T, E>
    where
        E: de::Error,
    {
        match value {
            serde_json::Value::String(name) => from_name(&name)
                .ok_or_else(|| E::custom(format!("unknown enum name '{name}'"))),
            serde_json::Value::Number(number) => number
                .as_i64()
                .and_then(&from_ordinal)
                .ok_or_else(|| E::custom(format!("unknown enum ordinal '{number}'"))),
            other => Err(E::custom(format!("unexpected enum value '{other}'"))),
        }
    }

    pub fn deserialize_payload_format<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<LyricPayloadFormat, D::Error> {
        let value = as_value(deserializer)?;
        from_value(
            value,
            |name| match name {
                "Unknown" => Some(LyricPayloadFormat::Unknown),
                "Lrc" => Some(LyricPayloadFormat::Lrc),
                "Qrc" => Some(LyricPayloadFormat::Qrc),
                "Krc" => Some(LyricPayloadFormat::Krc),
                "Yrc" => Some(LyricPayloadFormat::Yrc),
                "Ttml" => Some(LyricPayloadFormat::Ttml),
                "PlainText" => Some(LyricPayloadFormat::PlainText),
                _ => None,
            },
            payload_format_from_ordinal,
        )
    }

    pub fn payload_format_from_ordinal(ordinal: i64) -> Option<LyricPayloadFormat> {
        Some(match ordinal {
            0 => LyricPayloadFormat::Unknown,
            1 => LyricPayloadFormat::Lrc,
            2 => LyricPayloadFormat::Qrc,
            3 => LyricPayloadFormat::Krc,
            4 => LyricPayloadFormat::Yrc,
            5 => LyricPayloadFormat::Ttml,
            6 => LyricPayloadFormat::PlainText,
            _ => return None,
        })
    }

    pub fn deserialize_timing_kind<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<LyricTimingKind, D::Error> {
        let value = as_value(deserializer)?;
        from_value(
            value,
            |name| match name {
                "Unsynced" => Some(LyricTimingKind::Unsynced),
                "LineTimed" => Some(LyricTimingKind::LineTimed),
                "WordTimed" => Some(LyricTimingKind::WordTimed),
                "CharacterTimed" => Some(LyricTimingKind::CharacterTimed),
                "Mixed" => Some(LyricTimingKind::Mixed),
                _ => None,
            },
            |ordinal| match ordinal {
                0 => Some(LyricTimingKind::Unsynced),
                1 => Some(LyricTimingKind::LineTimed),
                2 => Some(LyricTimingKind::WordTimed),
                3 => Some(LyricTimingKind::CharacterTimed),
                4 => Some(LyricTimingKind::Mixed),
                _ => None,
            },
        )
    }

    pub fn deserialize_timing_provenance<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<LyricTimingProvenance, D::Error> {
        let value = as_value(deserializer)?;
        from_value(
            value,
            |name| match name {
                "Unknown" => Some(LyricTimingProvenance::Unknown),
                "ProviderSupplied" => Some(LyricTimingProvenance::ProviderSupplied),
                "Synthetic" => Some(LyricTimingProvenance::Synthetic),
                _ => None,
            },
            |ordinal| match ordinal {
                0 => Some(LyricTimingProvenance::Unknown),
                1 => Some(LyricTimingProvenance::ProviderSupplied),
                2 => Some(LyricTimingProvenance::Synthetic),
                _ => None,
            },
        )
    }

    pub fn acquisition_from_ordinal(ordinal: i64) -> Option<LyricAcquisitionKind> {
        Some(match ordinal {
            0 => LyricAcquisitionKind::Unknown,
            1 => LyricAcquisitionKind::Searching,
            2 => LyricAcquisitionKind::MemoryCache,
            3 => LyricAcquisitionKind::DiskCache,
            4 => LyricAcquisitionKind::Remote,
            5 => LyricAcquisitionKind::LocalFile,
            6 => LyricAcquisitionKind::SongMapping,
            7 => LyricAcquisitionKind::NotFound,
            8 => LyricAcquisitionKind::UserBinding,
            _ => return None,
        })
    }

    pub fn deserialize_acquisition<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<LyricAcquisitionKind, D::Error> {
        let value = as_value(deserializer)?;
        from_value(
            value,
            |name| match name {
                "Unknown" => Some(LyricAcquisitionKind::Unknown),
                "Searching" => Some(LyricAcquisitionKind::Searching),
                "MemoryCache" => Some(LyricAcquisitionKind::MemoryCache),
                "DiskCache" => Some(LyricAcquisitionKind::DiskCache),
                "Remote" => Some(LyricAcquisitionKind::Remote),
                "LocalFile" => Some(LyricAcquisitionKind::LocalFile),
                "SongMapping" => Some(LyricAcquisitionKind::SongMapping),
                "NotFound" => Some(LyricAcquisitionKind::NotFound),
                "UserBinding" => Some(LyricAcquisitionKind::UserBinding),
                _ => None,
            },
            acquisition_from_ordinal,
        )
    }
}

/// `ParsedLyricSegment`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLyricSegment {
    pub start_time: Duration,
    pub end_time: Duration,
    pub text: String,
}

impl ParsedLyricSegment {
    pub fn try_new(
        start_time: Duration,
        end_time: Duration,
        text: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let text = text.into();
        if end_time <= start_time {
            return Err(ModelError::EndNotAfterStart);
        }
        if text.is_empty() {
            return Err(ModelError::EmptyText("Segment text"));
        }
        Ok(Self {
            start_time,
            end_time,
            text,
        })
    }

    /// Validating constructor from millisecond values, mirroring the negative-start check
    /// of the C# record (`startTime < TimeSpan.Zero` throws).
    pub fn try_from_millis(
        start_ms: i64,
        end_ms: i64,
        text: impl Into<String>,
    ) -> Result<Self, ModelError> {
        if start_ms < 0 {
            return Err(ModelError::NegativeStartTime);
        }
        if end_ms <= start_ms {
            return Err(ModelError::EndNotAfterStart);
        }
        Self::try_new(ms(start_ms as u64), ms(end_ms as u64), text)
    }

    pub fn duration(&self) -> Duration {
        self.end_time - self.start_time
    }
}

/// `ParsedLyricLine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLyricLine {
    pub start_time: Duration,
    pub end_time: Option<Duration>,
    pub text: String,
    #[serde(default)]
    pub translation: Option<String>,
    #[serde(default)]
    pub segments: Vec<ParsedLyricSegment>,
    #[serde(default)]
    pub is_information_line: bool,
}

impl ParsedLyricLine {
    pub fn try_new(
        start_time: Duration,
        end_time: Option<Duration>,
        text: impl Into<String>,
        translation: Option<String>,
        segments: Vec<ParsedLyricSegment>,
        is_information_line: bool,
    ) -> Result<Self, ModelError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(ModelError::EmptyText("Line text"));
        }
        if let Some(end) = end_time {
            if end < start_time {
                return Err(ModelError::EndNotAfterStart);
            }
        }

        let mut ordered = segments;
        ordered.sort_by_key(|segment| segment.start_time);
        let escapes = ordered.iter().any(|segment| {
            segment.start_time < start_time
                || end_time.is_some_and(|end| segment.end_time > end)
        });
        if escapes {
            return Err(ModelError::SegmentOutsideLine);
        }

        let translation = normalize_translation_internal(translation);
        Ok(Self {
            start_time,
            end_time,
            text: text.trim().to_string(),
            translation,
            segments: ordered,
            is_information_line,
        })
    }

    pub fn simple(
        start_time: Duration,
        end_time: Option<Duration>,
        text: impl Into<String>,
    ) -> Result<Self, ModelError> {
        Self::try_new(start_time, end_time, text, None, Vec::new(), false)
    }
}

/// The C# rule: blank, `"//"` and `"///"` are "no translation" markers.
pub fn normalize_translation(translation: Option<String>) -> Option<String> {
    let trimmed = translation?.trim().to_string();
    if trimmed.is_empty() || trimmed == "//" || trimmed == "///" {
        return None;
    }
    Some(trimmed)
}

fn normalize_translation_internal(translation: Option<String>) -> Option<String> {
    normalize_translation(translation)
}

/// `ParsedLyrics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLyrics {
    pub lines: Vec<ParsedLyricLine>,
    #[serde(deserialize_with = "ordinal::deserialize_timing_kind")]
    pub timing_kind: LyricTimingKind,
    #[serde(deserialize_with = "ordinal::deserialize_timing_provenance")]
    pub timing_provenance: LyricTimingProvenance,
    #[serde(deserialize_with = "ordinal::deserialize_payload_format")]
    pub format: LyricPayloadFormat,
    #[serde(default)]
    pub is_pure_music: bool,
}

impl ParsedLyrics {
    /// Lines are ordered by start time, exactly like the C# constructor.
    pub fn new(
        mut lines: Vec<ParsedLyricLine>,
        timing_kind: LyricTimingKind,
        timing_provenance: LyricTimingProvenance,
        format: LyricPayloadFormat,
        is_pure_music: bool,
    ) -> Self {
        lines.sort_by_key(|line| line.start_time);
        Self {
            lines,
            timing_kind,
            timing_provenance,
            format,
            is_pure_music,
        }
    }

    /// `LyricifyPayloadParser` placeholder for an instrumental track.
    pub fn pure_music_placeholder(format: LyricPayloadFormat) -> Self {
        Self::new(
            vec![
                ParsedLyricLine::simple(Duration::ZERO, None, "\u{1F3B6}\u{1F3B6}\u{1F3B6}")
                    .expect("placeholder line is valid"),
            ],
            LyricTimingKind::Unsynced,
            LyricTimingProvenance::Unknown,
            format,
            true,
        )
    }
}

/// `LyricCandidateEvaluation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricCandidateEvaluation {
    pub score: i32,
    pub is_admitted: bool,
    pub rejection_reasons: Vec<String>,
}

impl LyricCandidateEvaluation {
    pub fn accepted(score: i32) -> Self {
        Self {
            score,
            is_admitted: true,
            rejection_reasons: Vec::new(),
        }
    }

    pub fn rejected(score: i32, reasons: Vec<String>) -> Self {
        Self {
            score,
            is_admitted: false,
            rejection_reasons: reasons,
        }
    }
}

/// `LyricAcquisitionKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricAcquisitionKind {
    Unknown,
    Searching,
    MemoryCache,
    DiskCache,
    Remote,
    LocalFile,
    SongMapping,
    NotFound,
    UserBinding,
}

/// `ResolvedLyrics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLyrics {
    pub content: ParsedLyrics,
    pub provider_id: LyricProviderId,
    pub candidate_id: String,
    pub acquisition: LyricAcquisitionKind,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, String>,
}

impl ResolvedLyrics {
    pub fn try_new(
        content: ParsedLyrics,
        provider_id: LyricProviderId,
        candidate_id: impl Into<String>,
        acquisition: LyricAcquisitionKind,
        diagnostics: BTreeMap<String, String>,
    ) -> Result<Self, ModelError> {
        let candidate_id = candidate_id.into();
        if candidate_id.trim().is_empty() {
            return Err(ModelError::EmptyText("Candidate ID"));
        }
        Ok(Self {
            content,
            provider_id,
            candidate_id,
            acquisition,
            diagnostics,
        })
    }

    pub fn identity_score(&self) -> Option<i32> {
        self.diagnostics
            .get("identityScore")
            .and_then(|value| value.parse::<i32>().ok())
    }
}

/// `LyricResolutionStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricResolutionStatus {
    Succeeded,
    NotFound,
    TransientFailure,
    Failed,
    Unavailable,
}

/// `LyricResolutionResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricResolutionResult {
    pub lyrics: Option<ResolvedLyrics>,
    pub status: LyricResolutionStatus,
}

impl LyricResolutionResult {
    pub fn from_lyrics(lyrics: Option<ResolvedLyrics>) -> Self {
        let status = if lyrics.is_none() {
            LyricResolutionStatus::NotFound
        } else {
            LyricResolutionStatus::Succeeded
        };
        Self { lyrics, status }
    }

    pub fn unavailable() -> Self {
        Self {
            lyrics: None,
            status: LyricResolutionStatus::Unavailable,
        }
    }
}

/// `LyricSourceSelection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricSourceSelection {
    pub order: Vec<LyricProviderId>,
    pub cache_context: Option<String>,
}

/// `LyricSourceSelection.ManualCacheContext`.
pub const MANUAL_CACHE_CONTEXT: &str = "user-binding-v1";

impl LyricSourceSelection {
    pub fn new(order: Vec<LyricProviderId>, cache_context: Option<String>) -> Self {
        Self {
            order,
            cache_context,
        }
    }
}

/// `LyricSourceTerminalState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricSourceTerminalState {
    Succeeded,
    IdentityRejected,
    NoLyrics,
    InvalidContent,
    Failed,
    TimedOut,
    Disabled,
    Canceled,
}

// ---------------------------------------------------------------------------
// Legacy document model (still used by the local provider and the projector)
// ---------------------------------------------------------------------------

/// `LyricSyllable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricSyllable {
    pub relative_offset: Duration,
    pub duration: Duration,
    pub text: String,
}

/// `LyricLine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub timestamp: Duration,
    pub text: String,
    #[serde(default)]
    pub translation: Option<String>,
    #[serde(default)]
    pub syllables: Option<Vec<LyricSyllable>>,
}

/// `LyricDocument`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricDocument {
    pub lines: Vec<LyricLine>,
    #[serde(default)]
    pub best_score: i32,
    pub is_pure_music: bool,
}

impl LyricDocument {
    pub fn new(mut lines: Vec<LyricLine>, best_score: i32, is_pure_music: bool) -> Self {
        lines.sort_by_key(|line| line.timestamp);
        let is_pure_music = is_pure_music || looks_like_pure_music(&lines);
        Self {
            lines,
            best_score,
            is_pure_music,
        }
    }
}

fn looks_like_pure_music(lines: &[LyricLine]) -> bool {
    let content: Vec<String> = lines
        .iter()
        .map(|line| normalize_document_text(&line.text))
        .filter(|text| !text.is_empty() && !is_informational_line(text))
        .collect();

    content.len() == 1 && content[0].contains("纯音乐")
}

fn normalize_document_text(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '[' | ']' | '【' | '】' | '(' | ')' | '（' | '）' | ' ' | '\t'
            )
        })
        .to_string()
}

fn is_informational_line(text: &str) -> bool {
    if text.contains("纯音乐") {
        return false;
    }
    const KEYWORDS: [&str; 23] = [
        "获取", "来源", "提供", "贡献", "上传", "制作", "校对", "翻译", "作词", "作曲", "编曲",
        "歌词", "歌詞", "lyric", "lyrics", "provided", "generated", "synced", "composer", "music",
        "arranger", "producer", "written",
    ];
    KEYWORDS
        .iter()
        .any(|keyword| text.to_lowercase().contains(&keyword.to_lowercase()))
}

/// `LyricDisplayFrame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricDisplayFrame {
    pub current_line: String,
    pub next_line: String,
    pub title: String,
    pub line_progress: f64,
    pub current_line_index: i32,
    pub is_pure_music: bool,
    pub word_scan_progress: Option<f64>,
    pub current_translation: Option<String>,
    pub next_translation: Option<String>,
    pub has_track_translation: bool,
}

impl LyricDisplayFrame {
    pub fn empty() -> Self {
        Self {
            current_line: String::new(),
            next_line: String::new(),
            title: String::new(),
            line_progress: 0.0,
            current_line_index: -1,
            is_pure_music: false,
            word_scan_progress: None,
            current_translation: None,
            next_translation: None,
            has_track_translation: false,
        }
    }

    pub fn message(text: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            current_line: text.into(),
            title: title.into(),
            current_line_index: -1,
            ..Self::empty()
        }
    }
}

/// `PlaybackSnapshot` — the UI-facing cover bytes are kept as an opaque blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackSnapshot {
    pub is_playing: bool,
    pub position: Duration,
    pub track: Option<TrackInfo>,
    #[serde(default)]
    pub cover_image_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub raw_position: Option<Duration>,
    #[serde(default)]
    pub extrapolated_position: Option<Duration>,
    #[serde(default)]
    pub is_cover_loading: bool,
    #[serde(default)]
    pub source_app_user_model_id: String,
    #[serde(default)]
    pub can_seek: bool,
}

impl PlaybackSnapshot {
    pub fn idle() -> Self {
        Self {
            is_playing: false,
            position: Duration::ZERO,
            track: None,
            cover_image_bytes: None,
            raw_position: None,
            extrapolated_position: None,
            is_cover_loading: false,
            source_app_user_model_id: String::new(),
            can_seek: false,
        }
    }

    pub fn playing(track: TrackInfo, position: Duration) -> Self {
        Self {
            is_playing: true,
            position,
            track: Some(track),
            ..Self::idle()
        }
    }
}

// ---------------------------------------------------------------------------
// Trace records (`Models.LyricResolutionTrace.cs`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricResolutionRequestTrace {
    pub request_id: String,
    pub original_track: TrackInfo,
    pub effective_track: TrackInfo,
    pub search_plan: LyricSearchPlan,
    pub is_pure_music: bool,
    pub preferred_provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricResolutionCandidateTrace {
    pub request_id: String,
    pub candidate: SourceTrackCandidate,
    pub evaluation: LyricCandidateEvaluation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricResolutionSourceTrace {
    pub request_id: String,
    pub provider_id: LyricProviderId,
    pub state: LyricSourceTerminalState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricResolutionSelectionTrace {
    pub request_id: String,
    pub lyrics: Option<ResolvedLyrics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_reject_empty_values_and_expose_stable_online_trust_order() {
        assert!(LyricProviderId::try_new("  ").is_err());

        for provider in [
            known_providers::local(),
            known_providers::qq_music(),
            known_providers::kugou(),
            known_providers::netease(),
            known_providers::lrclib(),
        ] {
            assert!(!provider.value().trim().is_empty());
        }

        let order: Vec<String> = known_providers::online_trust_order()
            .iter()
            .map(|provider| provider.value().to_string())
            .collect();
        assert_eq!(order, vec!["QQMusic", "Kugou", "Netease", "LRCLIB"]);
    }

    #[test]
    fn parsed_timing_rejects_invalid_values_and_normalizes_segment_order() {
        assert_eq!(
            ParsedLyricSegment::try_from_millis(-1, 100, "invalid"),
            Err(ModelError::NegativeStartTime)
        );
        assert_eq!(
            ParsedLyricSegment::try_from_millis(100, 100, "invalid"),
            Err(ModelError::EndNotAfterStart)
        );
        assert_eq!(
            ParsedLyricSegment::try_new(Duration::ZERO, ms(100), ""),
            Err(ModelError::EmptyText("Segment text"))
        );

        let late = ParsedLyricSegment::try_from_millis(1500, 1700, "late").unwrap();
        let early = ParsedLyricSegment::try_from_millis(1100, 1300, "early").unwrap();
        let line = ParsedLyricLine::try_new(
            secs(1),
            Some(secs(2)),
            "line",
            None,
            vec![late, early],
            false,
        )
        .unwrap();

        assert_eq!(line.segments[0].text, "early");
        assert_eq!(line.segments[1].text, "late");

        let outside = ParsedLyricSegment::try_from_millis(900, 1100, "outside").unwrap();
        assert_eq!(
            ParsedLyricLine::try_new(secs(1), Some(secs(2)), "line", None, vec![outside], false),
            Err(ModelError::SegmentOutsideLine)
        );
        assert_eq!(
            ParsedLyricLine::try_new(Duration::ZERO, None, "   ", None, Vec::new(), false),
            Err(ModelError::EmptyText("Line text"))
        );
    }

    #[test]
    fn parsed_lyric_line_normalizes_missing_translation_markers() {
        for translation in [
            None,
            Some(String::new()),
            Some("   ".to_string()),
            Some("//".to_string()),
            Some("///".to_string()),
        ] {
            let line = ParsedLyricLine::try_new(
                Duration::ZERO,
                Some(secs(1)),
                "original",
                translation,
                Vec::new(),
                false,
            )
            .unwrap();
            assert!(line.translation.is_none());
        }
    }

    #[test]
    fn parsed_lyric_line_preserves_real_translation_containing_slashes() {
        let line = ParsedLyricLine::try_new(
            Duration::ZERO,
            Some(secs(1)),
            "original",
            Some("AC/DC translation".into()),
            Vec::new(),
            false,
        )
        .unwrap();
        assert_eq!(line.translation.as_deref(), Some("AC/DC translation"));
    }

    #[test]
    fn parsed_lyrics_preserves_mixed_granularity_and_provider_timing_provenance() {
        let word_line = ParsedLyricLine::try_new(
            Duration::ZERO,
            Some(secs(1)),
            "word line",
            None,
            vec![ParsedLyricSegment::try_new(Duration::ZERO, ms(400), "word").unwrap()],
            false,
        )
        .unwrap();
        let line_timed = ParsedLyricLine::try_new(secs(1), None, "line timed", None, Vec::new(), false)
            .unwrap();

        let parsed = ParsedLyrics::new(
            vec![line_timed, word_line],
            LyricTimingKind::Mixed,
            LyricTimingProvenance::ProviderSupplied,
            LyricPayloadFormat::Qrc,
            false,
        );

        assert_eq!(parsed.timing_kind, LyricTimingKind::Mixed);
        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::ProviderSupplied);
        assert_eq!(parsed.format, LyricPayloadFormat::Qrc);
        assert_eq!(parsed.lines[0].text, "word line");
        assert_eq!(parsed.lines[1].text, "line timed");
    }

    #[test]
    fn track_identity_retains_original_source_app_and_song_id() {
        let track = TrackInfo::new(
            "track-1",
            "Contract song",
            "Contract artist",
            "Contract album",
            "QQMusic",
            secs(180),
        )
        .with_song_id("qq-song-1");

        let identity = TrackIdentity::from_track_info(&track);

        assert_eq!(identity.track_id, track.id);
        assert_eq!(identity.source_app, track.source_app);
        assert_eq!(identity.song_id, track.song_id);
        assert_eq!(identity.title, track.title);
        assert_eq!(identity.primary_artist(), track.artist);
    }
}

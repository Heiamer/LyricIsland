//! `lyric-ffi` — a small, stable C ABI over `lyric-core`.
//!
//! The C# side (`TaskbarLyrics.Core/RustLyricEngine.cs`) P/Invokes these entry points so
//! the published LyricIsland executable runs the Rust lyric engine instead of the
//! original C# implementation.
//!
//! ## Wire format
//! Legacy text functions return a heap allocated, NUL terminated UTF-8 JSON string that the
//! caller must release with [`li_free_string`]. Failures are reported as
//! `{"error":"..."}` rather than by returning NULL, so the caller never has to guess.
//! Times are **integer milliseconds** and enums are **names** (`"LineTimed"`), which keeps
//! the contract readable and independent of any serializer default.
//!
//! Every entry point catches panics: unwinding across the FFI boundary is undefined
//! behaviour, so a panic becomes an `{"error":...}` response instead.

use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use lyric_core::local::LocalLyricProvider;
use lyric_core::matching::LyricMatcher;
use lyric_core::models::{
    DecodedLyricPayload, LyricAcquisitionKind, LyricDocument, LyricLine, LyricPayloadFormat,
    LyricProviderId, LyricSearchPlan, LyricSyllable, ParsedLyrics, PlaybackSnapshot,
    ResolvedLyrics, SourceTrackCandidate, TrackIdentity, TrackInfo,
};
use lyric_core::payload::LyricifyPayloadParser;
use lyric_core::planner::LyricSearchPlanner;
use lyric_core::policy::LyricIdentityEvaluator;
use lyric_core::projector::LyricDocumentSemanticProjector;
use lyric_core::resolution::{select_best_scored, ScoredCandidate};
use lyric_core::sync::LyricSyncService;
use serde::{Deserialize, Serialize};
mod json_cache;
mod backend;

// ---------------------------------------------------------------------------
// C ABI helpers
// ---------------------------------------------------------------------------

fn to_c_string(value: String) -> *mut c_char {
    CString::new(value)
        .unwrap_or_else(|_| CString::new("{\"error\":\"nul byte in output\"}").expect("static"))
        .into_raw()
}

/// # Safety
/// Strings must be valid UTF-8 C strings and target must be writable.
#[no_mangle]
pub unsafe extern "C" fn li_scan_rip(path: *const c_char, offset: u64, length: u64, section_rva: i64, signature: *const c_char, displacement: u32, target: *mut i64) -> i32 {
    if path.is_null() || signature.is_null() || target.is_null() { return 0; }
    catch_unwind(AssertUnwindSafe(|| {
        match lyric_core::memory_scan::scan_rip(&from_c_str(path),offset,length,section_rva,&from_c_str(signature),displacement as usize) {
            Ok(value) => { target.write(value); 1 }, Err(_) => 0,
        }
    })).unwrap_or(0)
}

/// Releases a string previously returned by this library.
///
/// # Safety
/// `ptr` must be a pointer returned by one of the `li_*` functions, or NULL.
#[no_mangle]
pub unsafe extern "C" fn li_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(CString::from_raw(ptr));
}

/// # Safety
/// `ptr` must be NULL or a valid NUL terminated UTF-8 string.
unsafe fn from_c_str(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

fn respond<T: FnOnce() -> String>(body: T) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(json) => to_c_string(json),
        Err(_) => to_c_string("{\"error\":\"panic in lyric_ffi\"}".to_string()),
    }
}

fn error_json(message: impl Into<String>) -> String {
    #[derive(Serialize)]
    struct Error {
        error: String,
    }
    serde_json::to_string(&Error {
        error: message.into(),
    })
    .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string())
}

fn ok_json<T: Serialize>(value: &T) -> String {
    #[derive(Serialize)]
    struct Ok1<T> {
        ok: T,
    }
    serde_json::to_string(&Ok1 { ok: value })
        .unwrap_or_else(|error| error_json(format!("serialization failed: {error}")))
}

fn parse_format(name: &str) -> LyricPayloadFormat {
    match name.to_ascii_lowercase().as_str() {
        "lrc" => LyricPayloadFormat::Lrc,
        "qrc" => LyricPayloadFormat::Qrc,
        "krc" => LyricPayloadFormat::Krc,
        "yrc" => LyricPayloadFormat::Yrc,
        "ttml" => LyricPayloadFormat::Ttml,
        "plaintext" | "plain" | "text" => LyricPayloadFormat::PlainText,
        _ => LyricPayloadFormat::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Wire DTOs (camelCase, milliseconds, enum names)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SegmentDto {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LineDto {
    start_ms: u64,
    end_ms: Option<u64>,
    text: String,
    translation: Option<String>,
    segments: Vec<SegmentDto>,
    is_information_line: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ParsedLyricsDto {
    lines: Vec<LineDto>,
    timing_kind: String,
    timing_provenance: String,
    format: String,
    is_pure_music: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyllableDto {
    relative_offset_ms: u64,
    duration_ms: u64,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentLineDto {
    timestamp_ms: u64,
    text: String,
    translation: Option<String>,
    syllables: Option<Vec<SyllableDto>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricDocumentDto {
    lines: Vec<DocumentLineDto>,
    best_score: i32,
    is_pure_music: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalDocumentResponse {
    found: bool,
    document: Option<LyricDocumentDto>,
    score: i32,
    acquisition: String,
    elapsed_millis: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedLocalResponse {
    found: bool,
    provider_id: String,
    candidate_id: String,
    acquisition: String,
    parsed: Option<ParsedLyricsDto>,
}

// -- search plan / admission / selection ------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityIn {
    #[serde(default)]
    track_id: Option<String>,
    title: String,
    #[serde(default)]
    artists: Vec<String>,
    #[serde(default)]
    album: String,
    #[serde(default)]
    duration_ms: i64,
    #[serde(default)]
    source_app: String,
    #[serde(default)]
    song_id: Option<String>,
    #[serde(default)]
    version_markers: Vec<String>,
}

impl IdentityIn {
    fn into_identity(self) -> TrackIdentity {
        TrackIdentity {
            track_id: self.track_id.unwrap_or_else(|| "ffi-track".to_string()),
            title: self.title,
            artists: self.artists,
            album: self.album,
            duration: Duration::from_millis(self.duration_ms.max(0) as u64),
            source_app: self.source_app,
            song_id: self.song_id.filter(|value| !value.trim().is_empty()),
            version_markers: self.version_markers,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateIn {
    provider_id: String,
    candidate_id: String,
    title: String,
    #[serde(default)]
    artists: Vec<String>,
    #[serde(default)]
    album: String,
    #[serde(default)]
    duration_ms: i64,
    #[serde(default)]
    query_variant_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoredCandidateIn {
    provider_id: String,
    #[serde(default)]
    candidate_id: String,
    #[serde(default)]
    identity_score: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VariantOut {
    id: String,
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: i64,
    relaxation_reasons: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityOut {
    track_id: String,
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: i64,
    source_app: String,
    song_id: Option<String>,
    version_markers: Vec<String>,
}

impl From<&TrackIdentity> for IdentityOut {
    fn from(identity: &TrackIdentity) -> Self {
        Self {
            track_id: identity.track_id.clone(),
            title: identity.title.clone(),
            artists: identity.artists.clone(),
            album: identity.album.clone(),
            duration_ms: identity.duration.as_millis() as i64,
            source_app: identity.source_app.clone(),
            song_id: identity.song_id.clone(),
            version_markers: identity.version_markers.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanOut {
    original_track: IdentityOut,
    variants: Vec<VariantOut>,
}

impl From<&LyricSearchPlan> for PlanOut {
    fn from(plan: &LyricSearchPlan) -> Self {
        Self {
            original_track: IdentityOut::from(&plan.original_track),
            variants: plan
                .variants
                .iter()
                .map(|variant| VariantOut {
                    id: variant.id.clone(),
                    title: variant.title.clone(),
                    artists: variant.artists.clone(),
                    album: variant.album.clone(),
                    duration_ms: variant.duration.as_millis() as i64,
                    relaxation_reasons: variant.relaxation_reasons.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentSyllableIn {
    #[serde(default)]
    relative_offset_ms: i64,
    #[serde(default)]
    duration_ms: i64,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentLineIn {
    #[serde(default)]
    timestamp_ms: i64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    translation: Option<String>,
    #[serde(default)]
    syllables: Option<Vec<DocumentSyllableIn>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentIn {
    #[serde(default)]
    lines: Vec<DocumentLineIn>,
    #[serde(default)]
    is_pure_music: bool,
}

impl DocumentIn {
    fn into_document(self) -> Result<LyricDocument, String> {
        if self.lines.is_empty() {
            return Err("document has no lines".to_string());
        }

        let lines: Vec<LyricLine> = self
            .lines
            .into_iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| LyricLine {
                timestamp: Duration::from_millis(line.timestamp_ms.max(0) as u64),
                text: line.text,
                translation: line.translation.filter(|value| !value.trim().is_empty()),
                syllables: line.syllables.map(|syllables| {
                    syllables
                        .into_iter()
                        .filter(|syllable| !syllable.text.is_empty())
                        .map(|syllable| LyricSyllable {
                            relative_offset: Duration::from_millis(
                                syllable.relative_offset_ms.max(0) as u64,
                            ),
                            duration: Duration::from_millis(syllable.duration_ms.max(0) as u64),
                            text: syllable.text,
                        })
                        .collect()
                }),
            })
            .collect();

        if lines.is_empty() {
            return Err("document has no usable lines".to_string());
        }

        Ok(LyricDocument::new(lines, 0, self.is_pure_music))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationOut {
    score: i32,
    is_admitted: bool,
    rejection_reasons: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameDto {
    current_line: String,
    next_line: String,
    title: String,
    line_progress: f64,
    current_line_index: i32,
    is_pure_music: bool,
    word_scan_progress: Option<f64>,
    current_translation: Option<String>,
    next_translation: Option<String>,
    has_track_translation: bool,
}

fn parsed_lyrics_dto(parsed: &ParsedLyrics) -> ParsedLyricsDto {
    ParsedLyricsDto {
        lines: parsed
            .lines
            .iter()
            .map(|line| LineDto {
                start_ms: line.start_time.as_millis() as u64,
                end_ms: line.end_time.map(|end| end.as_millis() as u64),
                text: line.text.clone(),
                translation: line.translation.clone(),
                segments: line
                    .segments
                    .iter()
                    .map(|segment| SegmentDto {
                        start_ms: segment.start_time.as_millis() as u64,
                        end_ms: segment.end_time.as_millis() as u64,
                        text: segment.text.clone(),
                    })
                    .collect(),
                is_information_line: line.is_information_line,
            })
            .collect(),
        timing_kind: format!("{:?}", parsed.timing_kind),
        timing_provenance: format!("{:?}", parsed.timing_provenance),
        format: parsed.format.as_str().to_string(),
        is_pure_music: parsed.is_pure_music,
    }
}

fn local_document_dto(
    fetch: &lyric_core::local::LocalLyricFetch,
) -> LocalDocumentResponse {
    let document = fetch.document.as_ref().map(|document| LyricDocumentDto {
        lines: document
            .lines
            .iter()
            .map(|line| DocumentLineDto {
                timestamp_ms: line.timestamp.as_millis() as u64,
                text: line.text.clone(),
                translation: line.translation.clone(),
                syllables: line.syllables.as_ref().map(|syllables| {
                    syllables
                        .iter()
                        .map(|syllable| SyllableDto {
                            relative_offset_ms: syllable.relative_offset.as_millis() as u64,
                            duration_ms: syllable.duration.as_millis() as u64,
                            text: syllable.text.clone(),
                        })
                        .collect()
                }),
            })
            .collect(),
        best_score: document.best_score,
        is_pure_music: document.is_pure_music,
    });

    LocalDocumentResponse {
        found: document.is_some(),
        score: document.as_ref().map(|document| document.best_score).unwrap_or(0),
        document,
        acquisition: format!("{:?}", fetch.acquisition),
        elapsed_millis: fetch.elapsed_millis,
    }
}

fn build_track(title: &str, artist: &str, album: &str, duration_seconds: i32, song_id: &str) -> TrackInfo {
    build_track_with_source(title, artist, album, "FFI", duration_seconds, song_id)
}

fn build_track_with_source(
    title: &str,
    artist: &str,
    album: &str,
    source_app: &str,
    duration_seconds: i32,
    song_id: &str,
) -> TrackInfo {
    let mut track = TrackInfo::new(
        "ffi-track",
        title,
        artist,
        album,
        if source_app.trim().is_empty() { "FFI" } else { source_app },
        Duration::from_secs(duration_seconds.max(0) as u64),
    );
    if !song_id.trim().is_empty() {
        track = track.with_song_id(song_id);
    }
    track
}

// ---------------------------------------------------------------------------
// Local provider cache
// ---------------------------------------------------------------------------

/// One provider (and therefore one scanned sidecar index) per folder set, so repeated
/// lookups do not rescan the disk. The C# host warms the cache in the background right
/// after building its provider, which mirrors the original `LocalMediaIndex` behaviour.
fn local_provider_cache() -> &'static Mutex<BTreeMap<String, Arc<LocalLyricProvider>>> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, Arc<LocalLyricProvider>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn provider_key(dirs: &[String]) -> String {
    dirs.join("|").to_lowercase()
}

fn provider_for(dirs: Vec<String>) -> Arc<LocalLyricProvider> {
    let key = provider_key(&dirs);
    if let Ok(mut cache) = local_provider_cache().lock() {
        if let Some(existing) = cache.get(&key) {
            return existing.clone();
        }
        let provider = Arc::new(LocalLyricProvider::new(dirs));
        cache.insert(key, provider.clone());
        return provider;
    }

    Arc::new(LocalLyricProvider::new(dirs))
}

fn parse_dirs(json: &str) -> Result<Vec<String>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(json).map_err(|error| format!("invalid dirs json: {error}"))
}

// ---------------------------------------------------------------------------
// Public C API
// ---------------------------------------------------------------------------

/// Semver-ish release name of the Rust engine (`1.0`).
#[no_mangle]
pub extern "C" fn li_version() -> *mut c_char {
    respond(|| lyric_core::VERSION.to_string())
}

/// Version plus build details, useful to confirm in the app that Rust is really loaded.
#[no_mangle]
pub extern "C" fn li_build_info() -> *mut c_char {
    respond(|| {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct BuildInfo {
            version: &'static str,
            crate_version: &'static str,
            compiler: &'static str,
            abi: u32,
        }
        serde_json::to_string(&BuildInfo {
            version: lyric_core::VERSION,
            crate_version: env!("CARGO_PKG_VERSION"),
            compiler: "rustc",
            abi: 1,
        })
        .unwrap_or_else(|error| error_json(format!("serialization failed: {error}")))
    })
}

/// `LyricMatcher.Score` over the FFI boundary. Returns the score directly (`0` = conflict).
///
/// # Safety
/// All pointer arguments must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_match_score(
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
    duration_seconds: i32,
    candidate_title: *const c_char,
    candidate_artist: *const c_char,
    candidate_duration_seconds: i32,
    candidate_album: *const c_char,
) -> i32 {
    let title = from_c_str(title);
    let artist = from_c_str(artist);
    let album = from_c_str(album);
    let candidate_title = from_c_str(candidate_title);
    let candidate_artist = from_c_str(candidate_artist);
    let candidate_album = from_c_str(candidate_album);

    catch_unwind(AssertUnwindSafe(|| {
        let track = build_track(&title, &artist, &album, duration_seconds, "");
        LyricMatcher::score(
            &track,
            &candidate_title,
            &candidate_artist,
            candidate_duration_seconds,
            if candidate_album.is_empty() {
                None
            } else {
                Some(candidate_album.as_str())
            },
        )
    }))
    .unwrap_or(0)
}

/// Decrypts (when needed) and parses a lyric payload, returning `{"ok":ParsedLyrics}`.
///
/// `diagnostics_json` is a JSON object of string pairs (the same dictionary the C# payload
/// carries); it is forwarded so diagnostics such as
/// `informationLineStartTimesMs` keep working.
///
/// # Safety
/// All pointer arguments must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_parse_payload(
    format: *const c_char,
    original: *const c_char,
    translation: *const c_char,
    is_pure_music: i32,
    diagnostics_json: *const c_char,
) -> *mut c_char {
    let format = parse_format(&from_c_str(format));
    let original = from_c_str(original);
    let translation = from_c_str(translation);
    let diagnostics_json = from_c_str(diagnostics_json);
    let is_pure_music = is_pure_music != 0;

    respond(move || {
        let diagnostics = if diagnostics_json.trim().is_empty() {
            Default::default()
        } else {
            match serde_json::from_str(&diagnostics_json) {
                Ok(diagnostics) => diagnostics,
                Err(error) => return error_json(format!("invalid diagnostics json: {error}")),
            }
        };

        let payload = DecodedLyricPayload {
            provider_id: LyricProviderId::try_new("FFI").expect("constant provider id"),
            candidate_id: "ffi-candidate".to_string(),
            format,
            original_lyrics: if original.is_empty() {
                None
            } else {
                Some(original)
            },
            translation_lyrics: if translation.is_empty() {
                None
            } else {
                Some(translation)
            },
            is_pure_music,
            diagnostics,
        };

        match LyricifyPayloadParser::parse(&payload) {
            Ok(parsed) => ok_json(&parsed_lyrics_dto(&parsed)),
            Err(error) => error_json(error.to_string()),
        }
    })
}

/// Decrypts an encrypted QRC/KRC payload, returning `{"ok":"<plain lyrics>"}`.
/// Callers that receive a payload with `IsEncrypted = false` do not need this.
///
/// # Safety
/// `format` and `encrypted` must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_decrypt(
    format: *const c_char,
    encrypted: *const c_char,
) -> *mut c_char {
    let format = parse_format(&from_c_str(format));
    let encrypted = from_c_str(encrypted);

    respond(move || {
        #[derive(Serialize)]
        struct Plain {
            ok: String,
        }

        let result = match format {
            LyricPayloadFormat::Qrc => lyric_core::crypto::decrypt_qrc(&encrypted),
            LyricPayloadFormat::Krc => lyric_core::crypto::decrypt_krc(&encrypted),
            other => {
                return error_json(format!(
                    "no decrypter is registered for {} payloads",
                    other.as_str()
                ))
            }
        };

        match result {
            Ok(plain) => serde_json::to_string(&Plain { ok: plain })
                .unwrap_or_else(|error| error_json(format!("serialization failed: {error}"))),
            Err(error) => error_json(error.to_string()),
        }
    })
}

/// Builds the search plan (query variants) for a track. `identity_json` describes the
/// `TrackIdentity`: `{"trackId","title","artists":[],"album","durationMs","sourceApp","songId"}`.
///
/// # Safety
/// `identity_json` must be NULL or a valid NUL terminated UTF-8 JSON object.
#[no_mangle]
pub unsafe extern "C" fn li_plan_search(identity_json: *const c_char) -> *mut c_char {
    let identity_json = from_c_str(identity_json);

    respond(move || {
        let identity = match serde_json::from_str::<IdentityIn>(&identity_json) {
            Ok(input) => input.into_identity(),
            Err(error) => return error_json(format!("invalid identity json: {error}")),
        };

        match LyricSearchPlanner::create_plan(&identity) {
            Ok(plan) => ok_json(&PlanOut::from(&plan)),
            Err(error) => error_json(error),
        }
    })
}

/// Scores one candidate against the track (`TrackIdentity`), returning
/// `{"ok":{"score":n,"isAdmitted":bool,"rejectionReasons":[]}}`.
///
/// # Safety
/// All pointers must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_evaluate_candidate(
    identity_json: *const c_char,
    candidate_json: *const c_char,
) -> *mut c_char {
    let identity_json = from_c_str(identity_json);
    let candidate_json = from_c_str(candidate_json);

    respond(move || {
        let identity = match serde_json::from_str::<IdentityIn>(&identity_json) {
            Ok(input) => input.into_identity(),
            Err(error) => return error_json(format!("invalid identity json: {error}")),
        };
        let candidate: CandidateIn = match serde_json::from_str(&candidate_json) {
            Ok(candidate) => candidate,
            Err(error) => return error_json(format!("invalid candidate json: {error}")),
        };

        let provider_id = match LyricProviderId::try_new(&candidate.provider_id) {
            Ok(provider_id) => provider_id,
            Err(error) => return error_json(error.to_string()),
        };

        let source_candidate = match SourceTrackCandidate::try_new(
            provider_id,
            candidate.candidate_id,
            candidate.title,
            candidate.artists,
            candidate.album,
            Duration::from_millis(candidate.duration_ms.max(0) as u64),
            if candidate.query_variant_id.trim().is_empty() {
                "ffi".to_string()
            } else {
                candidate.query_variant_id
            },
            Default::default(),
        ) {
            Ok(candidate) => candidate,
            Err(error) => return error_json(error.to_string()),
        };

        let evaluation = LyricIdentityEvaluator::evaluate(&identity, &source_candidate);
        ok_json(&EvaluationOut {
            score: evaluation.score,
            is_admitted: evaluation.is_admitted,
            rejection_reasons: evaluation.rejection_reasons,
        })
    })
}

/// Applies the resolution selection rule to already scored provider results:
/// `outcomes_json` = `[{"providerId","candidateId","identityScore"}]`,
/// `trust_order_json` = `["QQMusic","Kugou","Netease","LRCLIB"]`.
///
/// # Safety
/// All pointers must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_select_best(
    outcomes_json: *const c_char,
    trust_order_json: *const c_char,
) -> *mut c_char {
    let outcomes_json = from_c_str(outcomes_json);
    let trust_order_json = from_c_str(trust_order_json);

    respond(move || {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Selected {
            found: bool,
            provider_id: Option<String>,
            candidate_id: Option<String>,
        }

        let inputs: Vec<ScoredCandidateIn> = match serde_json::from_str(&outcomes_json) {
            Ok(inputs) => inputs,
            Err(error) => return error_json(format!("invalid outcomes json: {error}")),
        };

        let order_names: Vec<String> = if trust_order_json.trim().is_empty() {
            Vec::new()
        } else {
            match serde_json::from_str(&trust_order_json) {
                Ok(names) => names,
                Err(error) => return error_json(format!("invalid trust order json: {error}")),
            }
        };

        let mut trust_order: Vec<LyricProviderId> = Vec::new();
        for name in &order_names {
            match LyricProviderId::try_new(name) {
                Ok(provider_id) => trust_order.push(provider_id),
                Err(error) => return error_json(error.to_string()),
            }
        }

        let mut scored: Vec<ScoredCandidate> = Vec::with_capacity(inputs.len());
        for input in inputs {
            let provider_id = match LyricProviderId::try_new(&input.provider_id) {
                Ok(provider_id) => provider_id,
                Err(error) => return error_json(error.to_string()),
            };
            scored.push(ScoredCandidate {
                provider_id,
                candidate_id: input.candidate_id,
                identity_score: input.identity_score,
            });
        }

        match select_best_scored(&scored, &trust_order) {
            Some(best) => ok_json(&Selected {
                found: true,
                provider_id: Some(best.provider_id.value().to_string()),
                candidate_id: Some(best.candidate_id.clone()),
            }),
            None => ok_json(&Selected {
                found: false,
                provider_id: None,
                candidate_id: None,
            }),
        }
    })
}

/// Scans (or reuses) the sidecar index for `dirs_json`, returning
/// `{"ok":{"entries":n,"elapsedMs":n}}`. Call it from a background task at start-up.
///
/// # Safety
/// `dirs_json` must be NULL or a valid NUL terminated UTF-8 JSON array of folders.
#[no_mangle]
pub unsafe extern "C" fn li_warm_local_index(dirs_json: *const c_char) -> *mut c_char {
    let dirs_json = from_c_str(dirs_json);

    respond(move || {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct WarmResponse {
            entries: usize,
            elapsed_millis: u64,
        }

        let dirs = match parse_dirs(&dirs_json) {
            Ok(dirs) => dirs,
            Err(error) => return error_json(error),
        };

        let started = std::time::Instant::now();
        let provider = provider_for(dirs);
        let entries = provider.warm_index();
        ok_json(&WarmResponse {
            entries,
            elapsed_millis: started.elapsed().as_millis() as u64,
        })
    })
}

/// Drops the cached index. An empty string drops every cached folder set, otherwise only the
/// folder set given in `dirs_json` is dropped (it is rescanned on the next lookup).
///
/// # Safety
/// `dirs_json` must be NULL or a valid NUL terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn li_invalidate_local_index(dirs_json: *const c_char) -> *mut c_char {
    let dirs_json = from_c_str(dirs_json);

    respond(move || {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InvalidateResponse {
            cleared: usize,
        }

        let Ok(mut cache) = local_provider_cache().lock() else {
            return error_json("local index cache is poisoned");
        };

        let cleared = if dirs_json.trim().is_empty() {
            let count = cache.len();
            cache.clear();
            count
        } else {
            match parse_dirs(&dirs_json) {
                Ok(dirs) => usize::from(cache.remove(&provider_key(&dirs)).is_some()),
                Err(error) => return error_json(error),
            }
        };

        ok_json(&InvalidateResponse { cleared })
    })
}

/// Looks up sidecar lyric files in `dirs_json` (a JSON array of folders) and returns
/// `{"ok":{"found":bool,"document":{...},"score":n,...}}`.
///
/// # Safety
/// All pointer arguments must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_local_document(
    dirs_json: *const c_char,
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
    duration_seconds: i32,
) -> *mut c_char {
    let dirs_json = from_c_str(dirs_json);
    let title = from_c_str(title);
    let artist = from_c_str(artist);
    let album = from_c_str(album);

    respond(move || {
        let dirs = match parse_dirs(&dirs_json) {
            Ok(dirs) => dirs,
            Err(error) => return error_json(error),
        };

        let provider = provider_for(dirs);
        let track = build_track(&title, &artist, &album, duration_seconds, "");
        let fetch = provider.get_lyrics(&track);
        ok_json(&local_document_dto(&fetch))
    })
}

/// Full local resolution (provider metadata + parsed lyrics) in one call.
///
/// # Safety
/// All pointer arguments must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_resolve_local(
    dirs_json: *const c_char,
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
    duration_seconds: i32,
    song_id: *const c_char,
) -> *mut c_char {
    let dirs_json = from_c_str(dirs_json);
    let title = from_c_str(title);
    let artist = from_c_str(artist);
    let album = from_c_str(album);
    let song_id = from_c_str(song_id);

    respond(move || {
        let dirs = match parse_dirs(&dirs_json) {
            Ok(dirs) => dirs,
            Err(error) => return error_json(error),
        };

        let provider = provider_for(dirs);
        let track = build_track(&title, &artist, &album, duration_seconds, &song_id);

        let Some((parsed, acquisition)) = provider.get_parsed_lyrics(&track) else {
            return ok_json(&ResolvedLocalResponse {
                found: false,
                provider_id: "Local".to_string(),
                candidate_id: String::new(),
                acquisition: "NotFound".to_string(),
                parsed: None,
            });
        };

        let candidate_id = format!(
            "local:{}",
            match track.song_id.as_deref().filter(|value| !value.trim().is_empty()) {
                Some(song_id) => song_id,
                None => &track.id,
            }
        );

        ok_json(&ResolvedLocalResponse {
            found: true,
            provider_id: "Local".to_string(),
            candidate_id,
            acquisition: format!("{acquisition:?}"),
            parsed: Some(parsed_lyrics_dto(&parsed)),
        })
    })
}

/// Parses a lyric file from disk and returns the display frame at `at_seconds`.
/// Used by the CLI/diagnostics; the app itself drives frames from its own sync service.
///
/// # Safety
/// `file_path` must be NULL or a valid NUL terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn li_display_frame(file_path: *const c_char, at_seconds: f64) -> *mut c_char {
    let file_path = from_c_str(file_path);

    respond(move || {
        let content = match std::fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(error) => return error_json(format!("cannot read '{file_path}': {error}")),
        };

        let format = match std::path::Path::new(&file_path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("lrc")
            .to_ascii_lowercase()
            .as_str()
        {
            "qrc" => LyricPayloadFormat::Qrc,
            "krc" => LyricPayloadFormat::Krc,
            "yrc" => LyricPayloadFormat::Yrc,
            "ttml" => LyricPayloadFormat::Ttml,
            _ => LyricPayloadFormat::Lrc,
        };

        let payload = DecodedLyricPayload {
            provider_id: LyricProviderId::try_new("Local").expect("constant provider id"),
            candidate_id: "ffi-frame".to_string(),
            format,
            original_lyrics: Some(content),
            translation_lyrics: None,
            is_pure_music: false,
            diagnostics: Default::default(),
        };

        let parsed = match LyricifyPayloadParser::parse(&payload) {
            Ok(parsed) => parsed,
            Err(error) => return error_json(error.to_string()),
        };

        let resolved = match ResolvedLyrics::try_new(
            parsed,
            LyricProviderId::try_new("Local").expect("constant provider id"),
            "ffi-frame",
            lyric_core::models::LyricAcquisitionKind::LocalFile,
            Default::default(),
        ) {
            Ok(resolved) => resolved,
            Err(error) => return error_json(error.to_string()),
        };

        let track = build_track(&file_path, "ffi", "", at_seconds.max(1.0) as i32, "");
        let mut service = LyricSyncService::new();
        service.begin_track(&track);
        service.complete_search(&track, Some(&resolved));

        let snapshot = lyric_core::models::PlaybackSnapshot::playing(
            track,
            Duration::from_secs_f64(at_seconds.max(0.0)),
        );
        let frame = service.get_display_frame(&snapshot);

        ok_json(&FrameDto {
            current_line: frame.current_line,
            next_line: frame.next_line,
            title: frame.title,
            line_progress: frame.line_progress,
            current_line_index: frame.current_line_index,
            is_pure_music: frame.is_pure_music,
            word_scan_progress: frame.word_scan_progress,
            current_translation: frame.current_translation,
            next_translation: frame.next_translation,
            has_track_translation: frame.has_track_translation,
        })
    })
}

// ---------------------------------------------------------------------------
// Display frame sync (handle based)
// ---------------------------------------------------------------------------

/// Per-handle state for the frame computation. The document and the loaded-at timestamp
/// live in Rust, so a frame tick only carries the playback position.
struct SyncState {
    service: LyricSyncService,
    track: TrackInfo,
    frame_text: FrameTextCache,
}

#[derive(Default)]
struct FrameTextCache {
    version: u64,
    values: [Option<String>; 5],
    encoded: [Option<CString>; 5],
}

impl FrameTextCache {
    fn update(&mut self, values: [Option<String>; 5]) -> Result<(), ()> {
        if self.version != 0 && self.values == values { return Ok(()); }
        // Stage all conversions before replacing any borrowed buffer. A NUL in
        // provider text must fall back to JSON without corrupting the old cache.
        let mut replacements: [Option<Option<CString>>; 5] = std::array::from_fn(|_| None);
        for (index, value) in values.iter().enumerate() {
            if self.values[index] != *value {
                replacements[index] = Some(match value { Some(text) => Some(CString::new(text.as_bytes()).map_err(|_| ())?), None => None });
            }
        }
        for (index, replacement) in replacements.into_iter().enumerate() {
            if let Some(encoded) = replacement { self.encoded[index] = encoded; }
        }
        self.values = values;
        self.version = self.version.wrapping_add(1).max(1);
        Ok(())
    }
    fn pointer(&self, index: usize) -> *const c_char {
        self.encoded[index].as_ref().map_or(std::ptr::null(), |value| value.as_ptr())
    }
}

#[repr(C)]
pub struct NativeDisplayFrame {
    line_progress: f64,
    word_progress: f64,
    line_index: i32,
    pure_music: i32,
    has_word_progress: i32,
    has_translation: i32,
    text_version: u64,
    current: *const c_char,
    next: *const c_char,
    title: *const c_char,
    translation: *const c_char,
    next_translation: *const c_char,
}

fn sync_handles() -> &'static Mutex<BTreeMap<u64, SyncState>> {
    static HANDLES: OnceLock<Mutex<BTreeMap<u64, SyncState>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Creates a sync handle. Returns 0 when the handle table cannot be locked.
#[no_mangle]
pub extern "C" fn li_sync_create() -> u64 {
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let Ok(mut handles) = sync_handles().lock() else {
        return 0;
    };

    handles.insert(
        handle,
        SyncState {
            service: LyricSyncService::new(),
            track: build_track("", "", "", 0, ""),
            frame_text: FrameTextCache::default(),
        },
    );
    handle
}

/// Releases a sync handle.
#[no_mangle]
pub extern "C" fn li_sync_destroy(handle: u64) {
    if let Ok(mut handles) = sync_handles().lock() {
        handles.remove(&handle);
    }
}

/// Registers the playing track. Returns `{"ok":{"isNewTrack":bool}}`; `isNewTrack` mirrors
/// the identity change detection (same source app + title + artist).
///
/// # Safety
/// All pointers must be NULL or valid NUL terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn li_sync_begin_track(
    handle: u64,
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
    source_app: *const c_char,
    duration_seconds: i32,
    song_id: *const c_char,
) -> *mut c_char {
    let title = from_c_str(title);
    let artist = from_c_str(artist);
    let album = from_c_str(album);
    let source_app = from_c_str(source_app);
    let song_id = from_c_str(song_id);

    respond(move || {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct BeginResponse {
            is_new_track: bool,
        }

        let Ok(mut handles) = sync_handles().lock() else {
            return error_json("sync handle table is poisoned");
        };
        let Some(state) = handles.get_mut(&handle) else {
            return error_json("unknown sync handle");
        };

        let track = build_track_with_source(&title, &artist, &album, &source_app, duration_seconds, &song_id);
        let is_new_track = state.service.begin_track(&track);
        state.track = track;

        ok_json(&BeginResponse { is_new_track })
    })
}

/// Installs the lyrics document (see the `LyricDocument` wire format) for the current track.
///
/// # Safety
/// `document_json` must be NULL or a valid NUL terminated UTF-8 JSON object.
#[no_mangle]
pub unsafe extern "C" fn li_sync_set_document(
    handle: u64,
    document_json: *const c_char,
) -> *mut c_char {
    let document_json = from_c_str(document_json);

    respond(move || {
        let input: DocumentIn = match serde_json::from_str(&document_json) {
            Ok(input) => input,
            Err(error) => return error_json(format!("invalid document json: {error}")),
        };

        let Ok(mut handles) = sync_handles().lock() else {
            return error_json("sync handle table is poisoned");
        };
        let Some(state) = handles.get_mut(&handle) else {
            return error_json("unknown sync handle");
        };

        let document = match input.into_document() {
            Ok(document) => document,
            Err(error) => return error_json(error),
        };

        let content = LyricDocumentSemanticProjector::to_parsed_lyrics(&document);
        let resolved = match ResolvedLyrics::try_new(
            content,
            LyricProviderId::try_new("FFI").expect("constant provider id"),
            "ffi-document",
            LyricAcquisitionKind::LocalFile,
            Default::default(),
        ) {
            Ok(resolved) => resolved,
            Err(error) => return error_json(error.to_string()),
        };

        let track = state.track.clone();
        let applied = state.service.complete_search(&track, Some(&resolved));

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SetDocumentResponse {
            applied: bool,
        }
        ok_json(&SetDocumentResponse { applied })
    })
}

/// Marks the current track as "no lyrics found".
#[no_mangle]
pub extern "C" fn li_sync_mark_not_found(handle: u64) -> *mut c_char {
    respond(move || {
        let Ok(mut handles) = sync_handles().lock() else {
            return error_json("sync handle table is poisoned");
        };
        let Some(state) = handles.get_mut(&handle) else {
            return error_json("unknown sync handle");
        };

        let track = state.track.clone();
        let applied = state.service.complete_search(&track, None);

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct NotFoundResponse {
            applied: bool,
        }
        ok_json(&NotFoundResponse { applied })
    })
}

/// Clears the handle (used when playback stops / the snapshot has no track).
#[no_mangle]
pub extern "C" fn li_sync_reset(handle: u64) -> *mut c_char {
    respond(move || {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ResetResponse {
            reset: bool,
        }

        let Ok(mut handles) = sync_handles().lock() else {
            return error_json("sync handle table is poisoned");
        };
        let Some(state) = handles.get_mut(&handle) else {
            return error_json("unknown sync handle");
        };

        let idle = PlaybackSnapshot::idle();
        let _ = state.service.get_display_frame(&idle);

        ok_json(&ResetResponse { reset: true })
    })
}

/// Computes the display frame for `position_ms` (already offset by the player/track lead
/// times). This is the per-frame hot path: no document data crosses the boundary here, and
/// the frame title comes from the track registered by [`li_sync_begin_track`].
#[no_mangle]
pub extern "C" fn li_sync_frame(handle: u64, position_ms: i64) -> *mut c_char {
    respond(move || {
        let Ok(mut handles) = sync_handles().lock() else {
            return error_json("sync handle table is poisoned");
        };
        let Some(state) = handles.get_mut(&handle) else {
            return error_json("unknown sync handle");
        };

        let snapshot = PlaybackSnapshot::playing(
            state.track.clone(),
            Duration::from_millis(position_ms.max(0) as u64),
        );
        let frame = state.service.get_display_frame(&snapshot);

        ok_json(&FrameDto {
            current_line: frame.current_line,
            next_line: frame.next_line,
            title: frame.title,
            line_progress: frame.line_progress,
            current_line_index: frame.current_line_index,
            is_pure_music: frame.is_pure_music,
            word_scan_progress: frame.word_scan_progress,
            current_translation: frame.current_translation,
            next_translation: frame.next_translation,
            has_track_translation: frame.has_track_translation,
        })
    })
}

/// Writes numeric fields directly and borrows cached UTF-8 text until the next
/// frame call or handle destruction. Callers must serialize calls per handle.
/// # Safety
/// `output` must point to a writable NativeDisplayFrame.
#[no_mangle]
pub unsafe extern "C" fn li_sync_frame_v2(handle: u64, position_ms: i64, output: *mut NativeDisplayFrame) -> i32 {
    if output.is_null() { return 0; }
    catch_unwind(AssertUnwindSafe(|| {
        let Ok(mut handles) = sync_handles().lock() else { return 0; };
        let Some(state) = handles.get_mut(&handle) else { return 0; };
        let snapshot = PlaybackSnapshot::playing(state.track.clone(), Duration::from_millis(position_ms.max(0) as u64));
        let frame = state.service.get_display_frame(&snapshot);
        if state.frame_text.update([Some(frame.current_line), Some(frame.next_line), Some(frame.title), frame.current_translation, frame.next_translation]).is_err() { return 0; }
        output.write(NativeDisplayFrame {
            line_progress: frame.line_progress, word_progress: frame.word_scan_progress.unwrap_or(0.0),
            line_index: frame.current_line_index, pure_music: i32::from(frame.is_pure_music),
            has_word_progress: i32::from(frame.word_scan_progress.is_some()), has_translation: i32::from(frame.has_track_translation),
            text_version: state.frame_text.version, current: state.frame_text.pointer(0), next: state.frame_text.pointer(1),
            title: state.frame_text.pointer(2), translation: state.frame_text.pointer(3), next_translation: state.frame_text.pointer(4),
        });
        1
    })).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_text_does_not_partially_replace_cached_pointers() {
        let mut cache = FrameTextCache::default();
        let old = [Some("old".to_string()), Some("next".to_string()), None, None, None];
        cache.update(old.clone()).unwrap();
        let pointer = cache.pointer(0);
        let version = cache.version;
        assert!(cache.update([Some("changed".to_string()), Some("bad\0text".to_string()), None, None, None]).is_err());
        assert_eq!(cache.version, version);
        cache.update(old).unwrap();
        assert_eq!(cache.pointer(0), pointer);
        assert_eq!(unsafe { CStr::from_ptr(pointer) }.to_str().unwrap(), "old");
    }

    fn c(value: &str) -> CString {
        CString::new(value).expect("no interior nul")
    }

    fn take(ptr: *mut c_char) -> String {
        let text = unsafe { from_c_str(ptr) };
        unsafe { li_free_string(ptr) };
        text
    }

    #[test]
    fn version_and_build_info_are_serialized() {
        let version = take(li_version());
        assert_eq!(version, "1.0");

        let build = take(li_build_info());
        assert!(build.contains("\"version\":\"1.0\""));
        assert!(build.contains("\"abi\":1"));
    }

    #[test]
    fn match_score_matches_the_core_rules() {
        let title = c("Midnight City");
        let artist = c("M83");
        let album = c("Hurry Up, We're Dreaming");
        let cand_title = c("Midnight City (Live)");
        let cand_artist = c("M83");

        let score = unsafe {
            li_match_score(
                title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
                244,
                cand_title.as_ptr(),
                cand_artist.as_ptr(),
                244,
                std::ptr::null(),
            )
        };
        assert_eq!(score, 0, "version conflict must hard reject");

        let cand_title = c("Midnight City");
        let score = unsafe {
            li_match_score(
                title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
                244,
                cand_title.as_ptr(),
                cand_artist.as_ptr(),
                244,
                std::ptr::null(),
            )
        };
        assert!(score >= 95, "expected immediate acceptance, got {score}");
    }

    #[test]
    fn parse_payload_returns_camel_case_milliseconds() {
        let format = c("qrc");
        let original = c("[1000,1800]qrc(1000,300)zero(1300,0)word(1300,1500)");

        let json = take(unsafe {
            li_parse_payload(format.as_ptr(), original.as_ptr(), std::ptr::null(), 0, std::ptr::null())
        });

        assert!(json.contains("\"startMs\":1000"), "{json}");
        assert!(json.contains("\"endMs\":2800"), "{json}");
        assert!(json.contains("\"timingKind\":\"WordTimed\""), "{json}");
    }

    #[test]
    fn parse_payload_reports_errors_as_json() {
        let format = c("lrc");
        let original = c("");
        let json = take(unsafe {
            li_parse_payload(format.as_ptr(), original.as_ptr(), std::ptr::null(), 0, std::ptr::null())
        });
        assert!(json.starts_with("{\"error\":"), "{json}");
    }

    #[test]
    fn decrypt_round_trips_a_qrc_payload() {
        let plain = "[1000,1800]qrc(1000,300)word(1300,1500)";
        let encrypted = lyric_core::crypto::encrypt_qrc(plain).expect("encrypt");

        let format = c("qrc");
        let encrypted_c = c(&encrypted);
        let json = take(unsafe { li_decrypt(format.as_ptr(), encrypted_c.as_ptr()) });

        assert!(json.contains(plain), "{json}");
    }

    #[test]
    fn decrypt_reports_errors_for_unsupported_formats() {
        let format = c("lrc");
        let payload = c("anything");
        let json = take(unsafe { li_decrypt(format.as_ptr(), payload.as_ptr()) });

        assert!(json.starts_with("{\"error\":"), "{json}");
    }

    #[test]
    fn parse_payload_forwards_information_line_diagnostics() {
        let format = c("lrc");
        let original = c("[00:01.00]metadata line\n[00:02.00]Composer: The Band");
        let diagnostics = c("{\"informationLineStartTimesMs\":\"1000\"}");

        let json = take(unsafe {
            li_parse_payload(
                format.as_ptr(),
                original.as_ptr(),
                std::ptr::null(),
                0,
                diagnostics.as_ptr(),
            )
        });

        assert!(json.contains("\"isInformationLine\":true"), "{json}");
    }

    #[test]
    fn parse_payload_rejects_malformed_diagnostics_json() {
        let format = c("lrc");
        let original = c("[00:01.00]line");
        let diagnostics = c("{oops");

        let json = take(unsafe {
            li_parse_payload(
                format.as_ptr(),
                original.as_ptr(),
                std::ptr::null(),
                0,
                diagnostics.as_ptr(),
            )
        });

        assert!(json.starts_with("{\"error\":"), "{json}");
    }

    #[test]
    fn sync_handle_computes_frames_without_resending_the_document() {
        let handle = li_sync_create();
        assert_ne!(handle, 0);

        let title = c("Title");
        let artist = c("Artist");
        let album = c("Album");
        let source = c("Spotify");
        let begin = take(unsafe {
            li_sync_begin_track(
                handle,
                title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
                source.as_ptr(),
                200,
                std::ptr::null(),
            )
        });
        assert!(begin.contains("\"isNewTrack\":true"), "{begin}");

        // The frame before lyrics arrive reports the search state.
        let searching = take(li_sync_frame(handle, 1000));
        assert!(searching.contains("\"currentLine\":\"正在检索歌词...\""), "{searching}");

        let document = c(r#"{"lines":[{"timestampMs":1000,"text":"one","translation":"第一"},{"timestampMs":3000,"text":"two"},{"timestampMs":5000,"text":"three"}],"isPureMusic":false}"#);
        let set = take(unsafe { li_sync_set_document(handle, document.as_ptr()) });
        assert!(set.contains("\"applied\":true"), "{set}");

        let frame = take(li_sync_frame(handle, 4000));
        assert!(frame.contains("\"currentLine\":\"two\""), "{frame}");
        assert!(frame.contains("\"nextLine\":\"three\""), "{frame}");
        assert!(frame.contains("\"currentLineIndex\":1"), "{frame}");
        assert!(frame.contains("\"hasTrackTranslation\":true"), "{frame}");
        assert!(frame.contains("\"title\":\"Title\""), "{frame}");

        // Before the first line the frame prepares line 0.
        let early = take(li_sync_frame(handle, 100));
        assert!(early.contains("\"currentLine\":\"one\""), "{early}");

        // Line progress is half way between the first two lines.
        let progress = take(li_sync_frame(handle, 2000));
        assert!(progress.contains("\"lineProgress\":0.5"), "{progress}");

        // A new track resets the document, so the "searching" and "not found" states are the
        // ones the host can observe before lyrics arrive (same flow as the C# service).
        let other_title = c("Another Title");
        let other_track = take(unsafe {
            li_sync_begin_track(
                handle,
                other_title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
                source.as_ptr(),
                200,
                std::ptr::null(),
            )
        });
        assert!(other_track.contains("\"isNewTrack\":true"), "{other_track}");

        let missing = take(li_sync_mark_not_found(handle));
        assert!(missing.contains("\"applied\""), "{missing}");
        let no_lyrics = take(li_sync_frame(handle, 4000));
        assert!(no_lyrics.contains("\"currentLine\":\"暂未找到歌词\""), "{no_lyrics}");

        li_sync_destroy(handle);
        let unknown = take(li_sync_frame(handle, 1000));
        assert!(unknown.starts_with("{\"error\":"), "{unknown}");
    }


    #[test]
    fn binary_frame_matches_legacy_and_reuses_text_until_line_changes() {
        let handle = li_sync_create();
        let title = c("Title");
        take(unsafe { li_sync_begin_track(handle, title.as_ptr(), c("Artist").as_ptr(), c("Album").as_ptr(), c("Spotify").as_ptr(), 200, std::ptr::null()) });
        let document = c(r#"{"lines":[{"timestampMs":1000,"text":"one","translation":"第一"},{"timestampMs":3000,"text":"two"},{"timestampMs":5000,"text":"three"}],"isPureMusic":false}"#);
        take(unsafe { li_sync_set_document(handle, document.as_ptr()) });
        let mut result = std::mem::MaybeUninit::<NativeDisplayFrame>::uninit();
        assert_eq!(std::mem::size_of::<NativeDisplayFrame>(), 80);
        assert_eq!(unsafe { li_sync_frame_v2(handle, 2000, result.as_mut_ptr()) }, 1);
        let first = unsafe { result.assume_init_read() };
        assert_eq!(unsafe { CStr::from_ptr(first.current) }.to_str().unwrap(), "one");
        assert_eq!(unsafe { CStr::from_ptr(first.translation) }.to_str().unwrap(), "第一");
        assert!(first.next_translation.is_null());
        assert_eq!(first.has_translation, 1);
        assert_eq!(first.has_word_progress, 0);
        let json: serde_json::Value = serde_json::from_str(&take(li_sync_frame(handle, 2000))).unwrap();
        let old = &json["ok"];
        assert_eq!(first.line_progress, old["lineProgress"].as_f64().unwrap());
        assert_eq!(first.line_index as i64, old["currentLineIndex"].as_i64().unwrap());
        assert_eq!(unsafe { li_sync_frame_v2(handle, 2500, result.as_mut_ptr()) }, 1);
        let second = unsafe { result.assume_init_read() };
        assert_eq!(first.text_version, second.text_version);
        assert_eq!(first.current, second.current);
        assert_eq!(second.line_progress, 0.75);
        assert_eq!(unsafe { li_sync_frame_v2(handle, 4000, result.as_mut_ptr()) }, 1);
        let third = unsafe { result.assume_init_read() };
        assert_ne!(third.text_version, second.text_version);
        assert_eq!(unsafe { CStr::from_ptr(third.current) }.to_str().unwrap(), "two");
        assert!(third.translation.is_null());
        assert_eq!(unsafe { li_sync_frame_v2(handle, 0, std::ptr::null_mut()) }, 0);
        li_sync_destroy(handle);
        assert_eq!(unsafe { li_sync_frame_v2(handle, 0, result.as_mut_ptr()) }, 0);
    }

    #[test]
    fn plan_search_produces_the_expected_variants() {
        let identity = c(r#"{"trackId":"t","title":"Signal (Deluxe) feat. Guest","artists":["Beyoncé feat. Zoë","Another Artist"],"album":"Planner album","durationMs":210000,"sourceApp":"Spotify","songId":"sid"}"#);

        let json = take(unsafe { li_plan_search(identity.as_ptr()) });

        assert!(json.contains("\"id\":\"exact\""), "{json}");
        assert!(json.contains("\"id\":\"normalized\""), "{json}");
        assert!(json.contains("\"id\":\"primary-artist\""), "{json}");
        assert!(json.contains("\"id\":\"relaxed-title\""), "{json}");
        assert!(json.contains("\"durationMs\":210000"), "{json}");
    }

    #[test]
    fn evaluate_candidate_applies_admission_rules() {
        let identity = c(r#"{"title":"Midnight City","artists":["M83"],"album":"Album","durationMs":244000,"sourceApp":"Spotify"}"#);
        let candidate = c(r#"{"providerId":"QQMusic","candidateId":"c1","title":"Midnight City (Live)","artists":["M83"],"album":"Album","durationMs":244000}"#);

        let rejected = take(unsafe {
            li_evaluate_candidate(identity.as_ptr(), candidate.as_ptr())
        });
        assert!(rejected.contains("\"score\":0"), "{rejected}");
        assert!(rejected.contains("\"isAdmitted\":false"), "{rejected}");
        assert!(rejected.contains("identity-conflict"), "{rejected}");

        let good = c(r#"{"providerId":"QQMusic","candidateId":"c2","title":"Midnight City","artists":["M83"],"album":"Album","durationMs":244000}"#);
        let admitted = take(unsafe { li_evaluate_candidate(identity.as_ptr(), good.as_ptr()) });
        assert!(admitted.contains("\"isAdmitted\":true"), "{admitted}");
    }

    #[test]
    fn select_best_follows_trust_order_and_high_confidence_pool() {
        let outcomes = c(r#"[{"providerId":"Netease","candidateId":"n1","identityScore":90},{"providerId":"QQMusic","candidateId":"q1","identityScore":85}]"#);
        let order = c(r#"["QQMusic","Kugou","Netease","LRCLIB"]"#);

        let json = take(unsafe { li_select_best(outcomes.as_ptr(), order.as_ptr()) });
        assert!(json.contains("\"candidateId\":\"q1\""), "{json}");

        let high = c(r#"[{"providerId":"Netease","candidateId":"n1","identityScore":98},{"providerId":"QQMusic","candidateId":"q1","identityScore":85}]"#);
        let json = take(unsafe { li_select_best(high.as_ptr(), order.as_ptr()) });
        assert!(json.contains("\"candidateId\":\"n1\""), "{json}");

        let empty = take(unsafe {
            li_select_best(c("[]").as_ptr(), order.as_ptr())
        });
        assert!(empty.contains("\"found\":false"), "{empty}");
    }

    #[test]
    fn warm_local_index_caches_and_can_be_invalidated() {
        let directory =
            std::env::temp_dir().join(format!("lyric-ffi-warm-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        std::fs::write(
            directory.join("03. M83 - Midnight City.lrc"),
            "[00:01.00]first line",
        )
        .expect("write fixture");

        let dirs_json = serde_json::to_string(&vec![directory.to_string_lossy().to_string()]).unwrap();
        let dirs = c(&dirs_json);

        let first = take(unsafe { li_warm_local_index(dirs.as_ptr()) });
        assert!(first.contains("\"entries\":1"), "{first}");

        let second = take(unsafe { li_warm_local_index(dirs.as_ptr()) });
        assert!(second.contains("\"entries\":1"), "{second}");

        let title = c("Midnight City");
        let artist = c("M83");
        let album = c("Album");
        let document = take(unsafe {
            li_local_document(dirs.as_ptr(), title.as_ptr(), artist.as_ptr(), album.as_ptr(), 244)
        });
        assert!(document.contains("\"found\":true"), "{document}");
        assert!(document.contains("first line"), "{document}");

        let invalidated = take(unsafe { li_invalidate_local_index(dirs.as_ptr()) });
        assert!(invalidated.contains("\"cleared\":1"), "{invalidated}");

        // A lookup after invalidation rescans and still finds the file.
        let document = take(unsafe {
            li_local_document(dirs.as_ptr(), title.as_ptr(), artist.as_ptr(), album.as_ptr(), 244)
        });
        assert!(document.contains("\"found\":true"), "{document}");

        let emptied = take(unsafe { li_invalidate_local_index(std::ptr::null()) });
        assert!(emptied.contains("\"cleared\":"), "{emptied}");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn local_document_finds_and_reports_a_sidecar_file() {
        let directory = std::env::temp_dir().join(format!("lyric-ffi-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        std::fs::write(
            directory.join("M83 - Midnight City.lrc"),
            "[00:01.00]first line\n[00:03.00]second line",
        )
        .expect("write fixture");

        let dirs = c(&serde_json::to_string(&vec![directory.to_string_lossy().to_string()]).unwrap());
        let title = c("Midnight City");
        let artist = c("M83");
        let album = c("Album");

        let json = take(unsafe {
            li_local_document(dirs.as_ptr(), title.as_ptr(), artist.as_ptr(), album.as_ptr(), 244)
        });

        assert!(json.contains("\"found\":true"), "{json}");
        assert!(json.contains("first line"), "{json}");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn display_frame_reports_the_line_at_the_position() {
        let directory = std::env::temp_dir().join(format!("lyric-ffi-frame-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        let path = directory.join("frame.lrc");
        std::fs::write(&path, "[00:01.00]one\n[00:03.00]two\n[00:05.00]three").expect("write fixture");

        let path_c = c(&path.to_string_lossy());
        let json = take(unsafe { li_display_frame(path_c.as_ptr(), 3.6) });

        assert!(json.contains("\"currentLine\":\"two\""), "{json}");
        assert!(json.contains("\"nextLine\":\"three\""), "{json}");

        let _ = std::fs::remove_dir_all(&directory);
    }
}

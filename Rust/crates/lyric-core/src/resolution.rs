//! `Services.LyricResolutionCoordinator.cs`, `Services.SongSearchMapResolver.cs` and the
//! `LyricSearchStageExecutor` seam.
//!
//! ### Deviation from the C# original
//! The original is asynchronous and resolves every enabled provider **concurrently**,
//! cancelling the lower-priority batch once the primary provider yields an immediate
//! acceptance score. This port is deliberately synchronous: every source seam is a plain
//! `Result`, and providers are walked in trust order. The *selection rules* are unchanged
//! (immediate acceptance short circuit, high-confidence pool, trust order then score), so
//! the observable result is identical; only the wall-clock behaviour differs.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::cache::{try_create_cache_key, JsonResolvedLyricCache};
use crate::models::{
    known_providers, LyricAcquisitionKind, LyricPayloadFormat, LyricProviderId,
    LyricResolutionResult, LyricResolutionStatus, LyricSearchPlan, LyricSourceSelection,
    LyricSourceTerminalState, LyricTimingKind, LyricTimingProvenance, ParsedLyricLine,
    ParsedLyrics, RawLyricPayload, ResolvedLyrics, SourceTrackCandidate, TrackIdentity, TrackInfo,
};
use crate::payload::{LyricifyPayloadDecoder, LyricifyPayloadParser};
use crate::planner::LyricSearchPlanner;
use crate::policy::{
    LyricIdentityEvaluator, LyricMatchingPolicy, LyricProviderTrustPolicy, SourceFailure,
};
use crate::{log_diagnostic, log_warn};

/// Normalization version of the pipeline, used as a parsed-cache key component.
pub const NORMALIZATION_VERSION: &str = "2";

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    let value = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{:08x}{:04x}", std::process::id(), value)
}

// ---------------------------------------------------------------------------
// Seams
// ---------------------------------------------------------------------------

/// `ILyricSource`.
pub trait LyricSource: Send + Sync {
    fn provider_id(&self) -> &LyricProviderId;

    /// `ILyricSource.SearchAsync`.
    fn search(&self, plan: &LyricSearchPlan) -> Result<Vec<SourceTrackCandidate>, SourceFailure>;

    /// `ILyricSource.FetchAsync`. `Ok(None)` means "this candidate has no lyrics".
    fn fetch(
        &self,
        candidate: &SourceTrackCandidate,
    ) -> Result<Option<RawLyricPayload>, SourceFailure>;
}

/// `ILyricMappingResolver` + `LyricMapping`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricMapping {
    pub title: String,
    pub artist: String,
    pub preferred_provider: Option<String>,
    pub is_pure_music: bool,
    pub album: Option<String>,
}

impl LyricMapping {
    pub fn unchanged(track: &TrackInfo) -> Self {
        Self {
            title: track.title.clone(),
            artist: track.artist.clone(),
            preferred_provider: None,
            is_pure_music: false,
            album: Some(track.album.clone()),
        }
    }
}

pub trait LyricMappingResolver: Send + Sync {
    fn resolve(&self, track: &TrackInfo) -> LyricMapping;
}

/// The default resolver: no mapping at all.
pub struct IdentityMappingResolver;

impl LyricMappingResolver for IdentityMappingResolver {
    fn resolve(&self, track: &TrackInfo) -> LyricMapping {
        LyricMapping::unchanged(track)
    }
}

/// One row of the alias table (`Database/SongSearchMap.cs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongSearchMap {
    pub original_title: String,
    pub original_artist: String,
    pub original_album: String,
    pub mapped_title: String,
    pub mapped_artist: String,
    pub mapped_album: String,
    pub preferred_provider: Option<String>,
    pub is_marked_as_pure_music: bool,
}

/// `SongSearchMapResolver`.
///
/// The C# original queries an EF Core SQLite database. The port keeps the *selection*
/// rules (`SelectBestMapping`) and sources the rows from an explicitly supplied list
/// instead, so the lyric core has no database dependency.
pub struct SongSearchMapResolver {
    entries: Vec<SongSearchMap>,
}

impl SongSearchMapResolver {
    pub fn new(entries: Vec<SongSearchMap>) -> Self {
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `SongSearchMapResolver.SelectBestMapping`.
    pub fn select_best_mapping<'a>(
        candidates: &'a [SongSearchMap],
        album: &str,
    ) -> Option<&'a SongSearchMap> {
        if let Some(exact) = candidates
            .iter()
            .find(|candidate| candidate.original_album == album)
        {
            return Some(exact);
        }
        if let Some(legacy) = candidates
            .iter()
            .find(|candidate| candidate.original_album.trim().is_empty())
        {
            return Some(legacy);
        }
        if candidates.len() == 1 {
            return candidates.first();
        }
        None
    }
}

impl LyricMappingResolver for SongSearchMapResolver {
    fn resolve(&self, track: &TrackInfo) -> LyricMapping {
        let candidates: Vec<SongSearchMap> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.original_title == track.title && entry.original_artist == track.artist
            })
            .cloned()
            .collect();

        let Some(map) = Self::select_best_mapping(&candidates, &track.album) else {
            return LyricMapping::unchanged(track);
        };

        LyricMapping {
            title: if map.mapped_title.trim().is_empty() {
                track.title.clone()
            } else {
                map.mapped_title.clone()
            },
            artist: if map.mapped_artist.trim().is_empty() {
                track.artist.clone()
            } else {
                map.mapped_artist.clone()
            },
            preferred_provider: map.preferred_provider.clone(),
            is_pure_music: map.is_marked_as_pure_music,
            album: Some(if map.mapped_album.trim().is_empty() {
                track.album.clone()
            } else {
                map.mapped_album.clone()
            }),
        }
    }
}

/// `ILyricResolutionTraceSink`.
pub trait LyricResolutionTraceSink: Send + Sync {
    fn request_prepared(&self, trace: &crate::models::LyricResolutionRequestTrace);
    fn candidate_evaluated(&self, trace: &crate::models::LyricResolutionCandidateTrace);
    fn source_completed(&self, trace: &crate::models::LyricResolutionSourceTrace);
    fn selection_completed(&self, trace: &crate::models::LyricResolutionSelectionTrace);
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

struct CandidateAdmission {
    candidate: SourceTrackCandidate,
    evaluation: crate::models::LyricCandidateEvaluation,
}

struct SourceOutcome {
    provider_id: LyricProviderId,
    state: LyricSourceTerminalState,
    lyrics: Option<ResolvedLyrics>,
    detail: Option<String>,
    is_transient_failure: bool,
}

struct CandidateResolution {
    lyrics: Option<ResolvedLyrics>,
    invalid_content: bool,
}

impl CandidateResolution {
    fn no_lyrics() -> Self {
        Self {
            lyrics: None,
            invalid_content: false,
        }
    }

    fn invalid() -> Self {
        Self {
            lyrics: None,
            invalid_content: true,
        }
    }
}

/// `LyricResolutionCoordinator` plus its optional caches.
pub struct LyricResolutionCoordinator {
    sources: Vec<Box<dyn LyricSource>>,
    source_index: BTreeMap<String, usize>,
    source_gates: Mutex<HashSet<String>>,
    mapping_resolver: Box<dyn LyricMappingResolver>,
    pipeline_cache: Option<crate::cache::LyricPipelineCache>,
    trust_policy: LyricProviderTrustPolicy,
    source_selection_resolver: Option<Box<dyn Fn(&TrackInfo) -> LyricSourceSelection + Send + Sync>>,
    local_provider: Option<crate::local::LocalLyricProvider>,
    resolved_cache: Option<JsonResolvedLyricCache>,
    trace_sink: Option<Box<dyn LyricResolutionTraceSink>>,
    complete_all_sources_for_trace: bool,
    source_timeout: Duration,
    disposed: bool,
}

/// Builder + constructor, mirroring the original constructor's optional parameters.
pub struct LyricResolutionCoordinatorBuilder {
    pub sources: Vec<Box<dyn LyricSource>>,
    pub mapping_resolver: Option<Box<dyn LyricMappingResolver>>,
    pub pipeline_cache: Option<crate::cache::LyricPipelineCache>,
    pub trust_policy: Option<LyricProviderTrustPolicy>,
    pub local_provider: Option<crate::local::LocalLyricProvider>,
    pub resolved_cache: Option<JsonResolvedLyricCache>,
    pub trace_sink: Option<Box<dyn LyricResolutionTraceSink>>,
    pub source_selection_resolver:
        Option<Box<dyn Fn(&TrackInfo) -> LyricSourceSelection + Send + Sync>>,
    pub source_timeout: Option<Duration>,
    pub complete_all_sources_for_trace: bool,
}

impl Default for LyricResolutionCoordinatorBuilder {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            mapping_resolver: None,
            pipeline_cache: None,
            trust_policy: None,
            local_provider: None,
            resolved_cache: None,
            trace_sink: None,
            source_selection_resolver: None,
            source_timeout: None,
            complete_all_sources_for_trace: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorError {
    DuplicateSource(String),
    TrustPolicy(String),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::DuplicateSource(id) => write!(f, "Duplicate lyric source '{id}'."),
            CoordinatorError::TrustPolicy(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl LyricResolutionCoordinator {
    pub fn new(builder: LyricResolutionCoordinatorBuilder) -> Result<Self, CoordinatorError> {
        let mut source_index: BTreeMap<String, usize> = BTreeMap::new();
        for (index, source) in builder.sources.iter().enumerate() {
            let key = source.provider_id().value().to_lowercase();
            if source_index.insert(key.clone(), index).is_some() {
                return Err(CoordinatorError::DuplicateSource(key));
            }
        }

        let trust_policy = match builder.trust_policy {
            Some(policy) => policy,
            None => LyricProviderTrustPolicy::create_default(
                builder
                    .sources
                    .iter()
                    .map(|source| source.provider_id().clone())
                    .collect(),
            )
            .map_err(|error| CoordinatorError::TrustPolicy(error.to_string()))?,
        };

        let source_timeout = builder
            .source_timeout
            .unwrap_or(LyricMatchingPolicy::ONLINE_SOURCE_TIMEOUT);

        Ok(Self {
            sources: builder.sources,
            source_index,
            source_gates: Mutex::new(HashSet::new()),
            mapping_resolver: builder
                .mapping_resolver
                .unwrap_or_else(|| Box::new(IdentityMappingResolver)),
            pipeline_cache: builder.pipeline_cache,
            trust_policy,
            source_selection_resolver: builder.source_selection_resolver,
            local_provider: builder.local_provider,
            resolved_cache: builder.resolved_cache,
            trace_sink: builder.trace_sink,
            complete_all_sources_for_trace: builder.complete_all_sources_for_trace,
            source_timeout,
            disposed: false,
        })
    }

    pub fn provider_trust_order(&self) -> &[LyricProviderId] {
        self.trust_policy.order()
    }

    /// `ResolveAsync`.
    pub fn resolve(&self, track: &TrackInfo) -> LyricResolutionResult {
        if self.disposed {
            return LyricResolutionResult::unavailable();
        }

        let request_id = next_request_id();
        log_diagnostic!(
            "LYRIC_PIPELINE",
            "Request='{request_id}' Track='{}' State='Started'.",
            track.id
        );

        let selection = match &self.source_selection_resolver {
            Some(resolver) => resolver(track),
            None => LyricSourceSelection::new(self.trust_policy.order().to_vec(), None),
        };

        if track.title.trim().is_empty() || track.title.eq_ignore_ascii_case("Unknown Title") {
            self.log_selection(&request_id, None);
            return LyricResolutionResult::from_lyrics(None);
        }

        if let Some(cached) = self.try_get_cached_lyrics(track, &selection) {
            self.log_selection(&request_id, Some(&cached));
            return LyricResolutionResult::from_lyrics(Some(cached));
        }

        let mapping = self.mapping_resolver.resolve(track);
        let mut mapped_track = track.clone();
        mapped_track.title = mapping.title.clone();
        mapped_track.artist = mapping.artist.clone();
        if let Some(album) = mapping.album.as_ref().filter(|album| !album.trim().is_empty()) {
            mapped_track.album = album.clone();
        }

        let identity = TrackIdentity::from_track_info(&mapped_track);
        let search_plan = match LyricSearchPlanner::create_plan(&identity) {
            Ok(plan) => plan,
            Err(error) => {
                log_warn!("Lyric search plan rejected: {error}");
                return LyricResolutionResult::from_lyrics(None);
            }
        };

        self.trace_request(
            &request_id,
            track,
            &mapped_track,
            &search_plan,
            mapping.is_pure_music,
            mapping.preferred_provider.clone(),
        );

        if mapping.is_pure_music {
            let pure_music = create_mapped_pure_music(&mapped_track);
            self.log_selection(&request_id, Some(&pure_music));
            return LyricResolutionResult::from_lyrics(Some(pure_music));
        }

        if let Some(preferred) = mapping
            .preferred_provider
            .as_deref()
            .filter(|provider| {
                selection
                    .order
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(provider))
            })
        {
            let result = self.resolve_preferred(&mapped_track, &search_plan, preferred, &request_id);
            self.try_store_automatic_resolution(track, result.lyrics.as_ref(), &selection);
            self.log_selection(&request_id, result.lyrics.as_ref());
            return result;
        }

        if let Some(local) = self.resolve_local(&mapped_track) {
            self.log_selection(&request_id, Some(&local));
            return LyricResolutionResult::from_lyrics(Some(local));
        }

        let result = self.resolve_online(&mapped_track, &search_plan, &request_id, &selection.order);
        self.try_store_automatic_resolution(track, result.lyrics.as_ref(), &selection);
        self.log_selection(&request_id, result.lyrics.as_ref());
        result
    }

    /// `ResolveCandidateAsync` — resolves a specific, already known candidate.
    pub fn resolve_candidate(
        &self,
        track: &TrackInfo,
        candidate: &SourceTrackCandidate,
    ) -> Option<ResolvedLyrics> {
        if self.disposed {
            return None;
        }

        let request_id = next_request_id();
        let key = candidate.provider_id.value().to_lowercase();
        let index = *self.source_index.get(&key)?;
        let source = self.sources.get(index)?;

        if !self.try_enter_source(&key) {
            return None;
        }
        let result = (|| {
            let admission = CandidateAdmission {
                candidate: candidate.clone(),
                evaluation: LyricIdentityEvaluator::evaluate(
                    &TrackIdentity::from_track_info(track),
                    candidate,
                ),
            };
            self.resolve_candidate_for_source(source, admission, &request_id)
                .lyrics
        })();
        self.exit_source(&key);
        result
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
    }

    // -- internals ---------------------------------------------------------

    fn try_enter_source(&self, key: &str) -> bool {
        match self.source_gates.lock() {
            Ok(mut gates) => gates.insert(key.to_string()),
            Err(_) => false,
        }
    }

    fn exit_source(&self, key: &str) {
        if let Ok(mut gates) = self.source_gates.lock() {
            gates.remove(key);
        }
    }

    fn try_get_cached_lyrics(
        &self,
        track: &TrackInfo,
        selection: &LyricSourceSelection,
    ) -> Option<ResolvedLyrics> {
        let cache = self.resolved_cache.as_ref()?;

        if let Some(manual) = cache.try_get(track, Some(crate::models::MANUAL_CACHE_CONTEXT)) {
            return Some(manual);
        }

        let found = cache.try_get(track, selection.cache_context.as_deref())?;
        if selection.order.iter().any(|provider| *provider == found.provider_id) {
            Some(found)
        } else {
            None
        }
    }

    fn try_store_automatic_resolution(
        &self,
        track: &TrackInfo,
        lyrics: Option<&ResolvedLyrics>,
        selection: &LyricSourceSelection,
    ) {
        let Some(cache) = self.resolved_cache.as_ref() else {
            return;
        };
        let Some(lyrics) = lyrics else {
            return;
        };
        let Some(score) = lyrics.identity_score() else {
            return;
        };
        if score < LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE {
            return;
        }

        let result = cache.store(track, lyrics, selection.cache_context.as_deref());
        if !result.is_saved {
            log_warn!("Resolved lyric cache rejected an automatic lyric selection.");
        }
    }

    fn resolve_local(&self, track: &TrackInfo) -> Option<ResolvedLyrics> {
        let provider = self.local_provider.as_ref()?;
        let (content, acquisition) = provider.get_parsed_lyrics(track)?;
        if content.lines.is_empty() {
            return None;
        }

        let mut diagnostics = BTreeMap::new();
        diagnostics.insert("source".to_string(), "local".to_string());

        ResolvedLyrics::try_new(
            content,
            known_providers::local(),
            build_local_candidate_id(track),
            acquisition,
            diagnostics,
        )
        .ok()
    }

    fn resolve_preferred(
        &self,
        track: &TrackInfo,
        search_plan: &LyricSearchPlan,
        provider_name: &str,
        request_id: &str,
    ) -> LyricResolutionResult {
        let Some(index) = self.source_index.get(&provider_name.to_lowercase()).copied() else {
            log_warn!("Mapped preferred lyric provider '{provider_name}' is not registered.");
            return LyricResolutionResult::unavailable();
        };
        let source = &self.sources[index];

        let outcome = self.resolve_source_with_trace(source, track, search_plan, request_id);
        summarize_outcomes(&[outcome])
    }

    fn resolve_online(
        &self,
        track: &TrackInfo,
        search_plan: &LyricSearchPlan,
        request_id: &str,
        trust_order: &[LyricProviderId],
    ) -> LyricResolutionResult {
        if trust_order.is_empty() {
            return LyricResolutionResult::from_lyrics(None);
        }

        let mut outcomes: Vec<(LyricProviderId, SourceOutcome)> = Vec::new();
        let mut all_outcomes: Vec<SourceOutcome> = Vec::new();

        for (position, provider_id) in trust_order.iter().enumerate() {
            let Some(index) = self.source_index.get(&provider_id.value().to_lowercase()).copied()
            else {
                continue;
            };
            let source = &self.sources[index];
            let outcome = self.resolve_source_with_trace(source, track, search_plan, request_id);

            // Immediate acceptance short circuits the lower priority providers, exactly
            // like cancelling the batch in the original. Trace mode keeps walking.
            if !self.complete_all_sources_for_trace
                && position == 0
                && outcome.state == LyricSourceTerminalState::Succeeded
                && outcome
                    .lyrics
                    .as_ref()
                    .and_then(|lyrics| lyrics.identity_score())
                    .is_some_and(|score| score >= LyricMatchingPolicy::IMMEDIATE_ACCEPTANCE_SCORE)
            {
                let lyrics = outcome.lyrics.clone();
                return LyricResolutionResult::from_lyrics(lyrics);
            }

            if outcome.state == LyricSourceTerminalState::Succeeded && outcome.lyrics.is_some() {
                outcomes.push((provider_id.clone(), outcome));
            } else {
                all_outcomes.push(outcome);
            }
        }

        if outcomes.is_empty() {
            return summarize_outcomes(&all_outcomes);
        }

        select_best_outcome(&outcomes, trust_order)
    }

    fn resolve_source_with_trace(
        &self,
        source: &Box<dyn LyricSource>,
        track: &TrackInfo,
        search_plan: &LyricSearchPlan,
        request_id: &str,
    ) -> SourceOutcome {
        let outcome = self.resolve_source(source, track, search_plan, request_id);
        self.trace_source(request_id, &outcome);
        outcome
    }

    fn resolve_source(
        &self,
        source: &Box<dyn LyricSource>,
        _track: &TrackInfo,
        search_plan: &LyricSearchPlan,
        request_id: &str,
    ) -> SourceOutcome {
        let provider_id = source.provider_id().clone();
        let key = provider_id.value().to_lowercase();
        let started = Instant::now();

        if !self.try_enter_source(&key) {
            return SourceOutcome {
                provider_id,
                state: LyricSourceTerminalState::Disabled,
                lyrics: None,
                detail: Some("provider-busy".to_string()),
                is_transient_failure: false,
            };
        }

        let outcome = (|| {
            let candidates = match source.search(search_plan) {
                Ok(candidates) => candidates,
                Err(failure) => {
                    return failure_outcome(&provider_id, failure, request_id);
                }
            };

            if candidates.is_empty() {
                return SourceOutcome {
                    provider_id: provider_id.clone(),
                    state: LyricSourceTerminalState::NoLyrics,
                    lyrics: None,
                    detail: Some("no-candidates".to_string()),
                    is_transient_failure: false,
                };
            }

            let identity = &search_plan.original_track;
            let evaluated: Vec<CandidateAdmission> = candidates
                .into_iter()
                .map(|candidate| CandidateAdmission {
                    evaluation: LyricIdentityEvaluator::evaluate(identity, &candidate),
                    candidate,
                })
                .collect();

            for admission in &evaluated {
                self.trace_candidate(request_id, admission);
            }

            let mut admitted: Vec<CandidateAdmission> = evaluated
                .into_iter()
                .filter(|admission| admission.evaluation.is_admitted)
                .collect();
            admitted.sort_by(|left, right| right.evaluation.score.cmp(&left.evaluation.score));

            if admitted.is_empty() {
                return SourceOutcome {
                    provider_id: provider_id.clone(),
                    state: LyricSourceTerminalState::IdentityRejected,
                    lyrics: None,
                    detail: Some("all-candidates-rejected".to_string()),
                    is_transient_failure: false,
                };
            }

            let mut saw_invalid_content = false;
            for admission in admitted {
                if started.elapsed() > self.source_timeout {
                    return SourceOutcome {
                        provider_id: provider_id.clone(),
                        state: LyricSourceTerminalState::TimedOut,
                        lyrics: None,
                        detail: Some("source-timeout".to_string()),
                        is_transient_failure: true,
                    };
                }

                let resolved = self.resolve_candidate_for_source(source, admission, request_id);
                if let Some(lyrics) = resolved.lyrics {
                    return SourceOutcome {
                        provider_id: provider_id.clone(),
                        state: LyricSourceTerminalState::Succeeded,
                        lyrics: Some(lyrics),
                        detail: None,
                        is_transient_failure: false,
                    };
                }
                saw_invalid_content |= resolved.invalid_content;
            }

            SourceOutcome {
                provider_id: provider_id.clone(),
                state: if saw_invalid_content {
                    LyricSourceTerminalState::InvalidContent
                } else {
                    LyricSourceTerminalState::NoLyrics
                },
                lyrics: None,
                detail: Some(if saw_invalid_content {
                    "candidate-content-invalid".to_string()
                } else {
                    "candidate-has-no-lyrics".to_string()
                }),
                is_transient_failure: false,
            }
        })();

        self.exit_source(&key);
        outcome
    }

    fn resolve_candidate_for_source(
        &self,
        source: &Box<dyn LyricSource>,
        admission: CandidateAdmission,
        request_id: &str,
    ) -> CandidateResolution {
        let candidate = &admission.candidate;
        let started = Instant::now();

        let (raw_payload, _acquisition) = match self.pipeline_cache.as_ref().and_then(|cache| {
            cache.try_get_raw(&source.provider_id().clone(), &candidate.candidate_id)
        }) {
            Some((payload, acquisition)) => (payload, acquisition),
            None => {
                let payload = match source.fetch(candidate) {
                    Ok(Some(payload)) => payload,
                    Ok(None) => return CandidateResolution::no_lyrics(),
                    Err(failure) => {
                        log_warn!(
                            "Lyric request '{request_id}' provider '{}' fetch failed: {failure:?}",
                            source.provider_id()
                        );
                        return CandidateResolution::no_lyrics();
                    }
                };

                if let Some(cache) = self.pipeline_cache.as_ref() {
                    cache.store_raw(&payload, &current_timestamp());
                }
                (payload, LyricAcquisitionKind::Remote)
            }
        };

        if raw_payload.provider_id != *source.provider_id()
            || raw_payload.candidate_id != candidate.candidate_id
        {
            return CandidateResolution::invalid();
        }

        if !LyricifyPayloadParser::can_parse(raw_payload.format) {
            return CandidateResolution::invalid();
        }

        let parser_id = "lyric-core::LyricifyPayloadParser";
        let parser_version = env!("CARGO_PKG_VERSION");

        let cached = self.pipeline_cache.as_ref().and_then(|cache| {
            cache.try_get_parsed(&raw_payload, parser_id, parser_version, NORMALIZATION_VERSION)
        });

        let (parsed, acquisition) = match cached {
            Some((parsed, acquisition)) => (parsed, acquisition),
            None => {
                let decoded = match LyricifyPayloadDecoder::decode(&raw_payload) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        log_warn!(
                            "Lyric payload rejected. Request='{request_id}' Provider='{}' Candidate='{}' Format='{}' Error='{error}'",
                            source.provider_id(),
                            candidate.candidate_id,
                            raw_payload.format.as_str()
                        );
                        return CandidateResolution::invalid();
                    }
                };

                if decoded.provider_id != *source.provider_id()
                    || decoded.candidate_id != candidate.candidate_id
                {
                    return CandidateResolution::invalid();
                }

                let parsed = match LyricifyPayloadParser::parse(&decoded) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        log_warn!(
                            "Lyric payload rejected. Request='{request_id}' Provider='{}' Candidate='{}' Format='{}' Error='{error}'",
                            source.provider_id(),
                            candidate.candidate_id,
                            raw_payload.format.as_str()
                        );
                        return CandidateResolution::invalid();
                    }
                };

                if parsed.lines.is_empty() && !parsed.is_pure_music {
                    return CandidateResolution::invalid();
                }

                if let Some(cache) = self.pipeline_cache.as_ref() {
                    cache.store_parsed(
                        &raw_payload,
                        &parsed,
                        parser_id,
                        parser_version,
                        NORMALIZATION_VERSION,
                    );
                }
                (parsed, LyricAcquisitionKind::Remote)
            }
        };

        let mut diagnostics = raw_payload.diagnostics.clone();
        diagnostics.insert("requestId".to_string(), request_id.to_string());
        diagnostics.insert("queryVariant".to_string(), candidate.query_variant_id.clone());
        diagnostics.insert(
            "identityScore".to_string(),
            admission.evaluation.score.to_string(),
        );
        diagnostics.insert(
            "format".to_string(),
            raw_payload.format.as_str().to_string(),
        );
        diagnostics.insert(
            "elapsedMs".to_string(),
            started.elapsed().as_millis().to_string(),
        );

        match ResolvedLyrics::try_new(
            parsed,
            source.provider_id().clone(),
            candidate.candidate_id.clone(),
            acquisition,
            diagnostics,
        ) {
            Ok(lyrics) => CandidateResolution {
                lyrics: Some(lyrics),
                invalid_content: false,
            },
            Err(_) => CandidateResolution::invalid(),
        }
    }

    // -- tracing -----------------------------------------------------------

    fn trace_request(
        &self,
        request_id: &str,
        original: &TrackInfo,
        effective: &TrackInfo,
        search_plan: &LyricSearchPlan,
        is_pure_music: bool,
        preferred_provider: Option<String>,
    ) {
        let Some(sink) = self.trace_sink.as_ref() else {
            return;
        };
        sink.request_prepared(&crate::models::LyricResolutionRequestTrace {
            request_id: request_id.to_string(),
            original_track: original.clone(),
            effective_track: effective.clone(),
            search_plan: search_plan.clone(),
            is_pure_music,
            preferred_provider,
        });
    }

    fn trace_candidate(&self, request_id: &str, admission: &CandidateAdmission) {
        log_diagnostic!(
            "LYRIC_PIPELINE",
            "Request='{request_id}' Provider='{}' Candidate='{}' QueryVariant='{}' IdentityAdmitted='{}' IdentityScore='{}'.",
            admission.candidate.provider_id,
            admission.candidate.candidate_id,
            admission.candidate.query_variant_id,
            admission.evaluation.is_admitted,
            admission.evaluation.score
        );

        let Some(sink) = self.trace_sink.as_ref() else {
            return;
        };
        sink.candidate_evaluated(&crate::models::LyricResolutionCandidateTrace {
            request_id: request_id.to_string(),
            candidate: admission.candidate.clone(),
            evaluation: admission.evaluation.clone(),
        });
    }

    fn trace_source(&self, request_id: &str, outcome: &SourceOutcome) {
        log_diagnostic!(
            "LYRIC_PIPELINE",
            "Request='{request_id}' Provider='{}' TerminalState='{}' Detail='{}' Retryable='{}' Selected='{}'.",
            outcome.provider_id,
            format!("{:?}", outcome.state),
            outcome.detail.clone().unwrap_or_else(|| "none".to_string()),
            outcome.is_transient_failure,
            outcome.lyrics.is_some()
        );

        let Some(sink) = self.trace_sink.as_ref() else {
            return;
        };
        sink.source_completed(&crate::models::LyricResolutionSourceTrace {
            request_id: request_id.to_string(),
            provider_id: outcome.provider_id.clone(),
            state: outcome.state,
            detail: outcome.detail.clone(),
        });
    }

    fn log_selection(&self, request_id: &str, lyrics: Option<&ResolvedLyrics>) {
        if let Some(sink) = self.trace_sink.as_ref() {
            sink.selection_completed(&crate::models::LyricResolutionSelectionTrace {
                request_id: request_id.to_string(),
                lyrics: lyrics.cloned(),
            });
        }

        match lyrics {
            None => log_diagnostic!(
                "LYRIC_PIPELINE",
                "Request='{request_id}' Selection='None'."
            ),
            Some(lyrics) => log_diagnostic!(
                "LYRIC_PIPELINE",
                "Request='{request_id}' Selection='Accepted' Provider='{}' Candidate='{}' QueryVariant='{}' IdentityScore='{}' Format='{}' TimingProvenance='{}' CacheAcquisition='{}'.",
                lyrics.provider_id,
                lyrics.candidate_id,
                lyrics.diagnostics.get("queryVariant").cloned().unwrap_or_else(|| "none".to_string()),
                lyrics.diagnostics.get("identityScore").cloned().unwrap_or_else(|| "none".to_string()),
                lyrics.content.format.as_str(),
                format!("{:?}", lyrics.content.timing_provenance),
                format!("{:?}", lyrics.acquisition)
            ),
        }
    }
}

fn failure_outcome(
    provider_id: &LyricProviderId,
    failure: SourceFailure,
    _request_id: &str,
) -> SourceOutcome {
    let transient = crate::policy::LyricTransientFailurePolicy::is_transient(&failure);
    SourceOutcome {
        provider_id: provider_id.clone(),
        state: LyricSourceTerminalState::Failed,
        lyrics: None,
        detail: Some(format!("{failure:?}")),
        is_transient_failure: transient,
    }
}

fn summarize_outcomes(outcomes: &[SourceOutcome]) -> LyricResolutionResult {
    if let Some(succeeded) = outcomes.iter().find(|outcome| outcome.lyrics.is_some()) {
        return LyricResolutionResult::from_lyrics(succeeded.lyrics.clone());
    }

    let status = if outcomes.iter().any(|outcome| outcome.is_transient_failure) {
        LyricResolutionStatus::TransientFailure
    } else if outcomes.iter().any(|outcome| {
        matches!(
            outcome.state,
            LyricSourceTerminalState::Failed | LyricSourceTerminalState::InvalidContent
        )
    }) {
        LyricResolutionStatus::Failed
    } else {
        LyricResolutionStatus::NotFound
    };

    LyricResolutionResult {
        lyrics: None,
        status,
    }
}

fn select_best_outcome(
    outcomes: &[(LyricProviderId, SourceOutcome)],
    trust_order: &[LyricProviderId],
) -> LyricResolutionResult {
    let scored: Vec<ScoredCandidate> = outcomes
        .iter()
        .map(|(provider_id, outcome)| ScoredCandidate {
            provider_id: provider_id.clone(),
            candidate_id: outcome
                .lyrics
                .as_ref()
                .map(|lyrics| lyrics.candidate_id.clone())
                .unwrap_or_default(),
            identity_score: outcome
                .lyrics
                .as_ref()
                .and_then(|lyrics| lyrics.identity_score())
                .unwrap_or(0),
        })
        .collect();

    let Some(best) = select_best_scored(&scored, trust_order) else {
        return LyricResolutionResult::from_lyrics(None);
    };

    LyricResolutionResult::from_lyrics(
        outcomes
            .iter()
            .find(|(provider_id, outcome)| {
                *provider_id == best.provider_id
                    && outcome
                        .lyrics
                        .as_ref()
                        .is_some_and(|lyrics| lyrics.candidate_id == best.candidate_id)
            })
            .and_then(|(_, outcome)| outcome.lyrics.clone()),
    )
}

/// A provider result reduced to what the selection rule needs.
///
/// Exposed so hosts can run the *same* selection rule
/// without re-implementing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredCandidate {
    pub provider_id: LyricProviderId,
    pub candidate_id: String,
    pub identity_score: i32,
}

/// The original `SelectBestOutcome` rule:
/// high-confidence candidates (>= immediate acceptance) win over everything else;
/// within the pool, trust order decides, then the higher score.
pub fn select_best_scored<'a>(
    candidates: &'a [ScoredCandidate],
    trust_order: &[LyricProviderId],
) -> Option<&'a ScoredCandidate> {
    if candidates.is_empty() {
        return None;
    }

    let high_confidence: Vec<&ScoredCandidate> = candidates
        .iter()
        .filter(|candidate| {
            candidate.identity_score >= LyricMatchingPolicy::IMMEDIATE_ACCEPTANCE_SCORE
        })
        .collect();

    let pool: Vec<&ScoredCandidate> = if high_confidence.is_empty() {
        candidates.iter().collect()
    } else {
        high_confidence
    };

    let mut ranked = pool;
    ranked.sort_by(|left, right| {
        let left_index = trust_order
            .iter()
            .position(|provider| *provider == left.provider_id)
            .unwrap_or(usize::MAX);
        let right_index = trust_order
            .iter()
            .position(|provider| *provider == right.provider_id)
            .unwrap_or(usize::MAX);
        left_index
            .cmp(&right_index)
            .then_with(|| right.identity_score.cmp(&left.identity_score))
    });

    ranked.first().copied()
}

fn create_mapped_pure_music(track: &TrackInfo) -> ResolvedLyrics {
    let content = ParsedLyrics::new(
        vec![ParsedLyricLine::simple(
            Duration::ZERO,
            None,
            "\u{1F3B6}\u{1F3B6}\u{1F3B6}",
        )
        .expect("placeholder line is valid")],
        LyricTimingKind::Unsynced,
        LyricTimingProvenance::Unknown,
        LyricPayloadFormat::PlainText,
        true,
    );

    let mut diagnostics = BTreeMap::new();
    diagnostics.insert("source".to_string(), "mapping".to_string());

    ResolvedLyrics::try_new(
        content,
        known_providers::local(),
        format!("mapping:{}", track.id),
        LyricAcquisitionKind::SongMapping,
        diagnostics,
    )
    .expect("mapped pure music is always valid")
}

fn build_local_candidate_id(track: &TrackInfo) -> String {
    format!(
        "local:{}",
        match track.song_id.as_deref().filter(|id| !id.trim().is_empty()) {
            Some(song_id) => song_id,
            None => &track.id,
        }
    )
}

fn current_timestamp() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

/// Exposed so callers can pre-compute cache keys exactly like the resolved cache does.
pub fn resolved_cache_key(track: &TrackInfo, context: Option<&str>) -> Option<String> {
    try_create_cache_key(&track.title, &track.artist, context)
}

/// Helper mirroring `LyricSourceSelection.ManualCacheContext` usage.
pub fn manual_cache_context() -> &'static str {
    crate::models::MANUAL_CACHE_CONTEXT
}

/// Provider ids the routing policy knows about, for diagnostics.
pub fn adapted_provider_names() -> Vec<String> {
    ["QQMusic", "Netease", "Kugou", "LRCLIB"]
        .iter()
        .map(|value| value.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{secs, LyricResolutionStatus};
    use crate::policy::SourceFailure;
    use std::sync::Arc;

    struct FixedSource {
        provider_id: LyricProviderId,
        format: LyricPayloadFormat,
        payload: Option<String>,
    }

    impl LyricSource for FixedSource {
        fn provider_id(&self) -> &LyricProviderId {
            &self.provider_id
        }

        fn search(&self, plan: &LyricSearchPlan) -> Result<Vec<SourceTrackCandidate>, SourceFailure> {
            let identity = &plan.original_track;
            Ok(vec![SourceTrackCandidate::try_new(
                self.provider_id.clone(),
                format!("{}-candidate", self.provider_id.value()),
                identity.title.clone(),
                identity.artists.clone(),
                identity.album.clone(),
                identity.duration,
                plan.variants[0].id.clone(),
                Default::default(),
            )
            .unwrap()])
        }

        fn fetch(
            &self,
            candidate: &SourceTrackCandidate,
        ) -> Result<Option<RawLyricPayload>, SourceFailure> {
            let Some(payload) = self.payload.clone() else {
                return Ok(None);
            };
            Ok(Some(
                RawLyricPayload::try_new(
                    self.provider_id.clone(),
                    candidate.candidate_id.clone(),
                    self.format,
                    Some(payload),
                    None,
                    false,
                    false,
                    Default::default(),
                )
                .unwrap(),
            ))
        }
    }

    struct FailingSource {
        provider_id: LyricProviderId,
        failure: SourceFailure,
    }

    impl LyricSource for FailingSource {
        fn provider_id(&self) -> &LyricProviderId {
            &self.provider_id
        }

        fn search(&self, _plan: &LyricSearchPlan) -> Result<Vec<SourceTrackCandidate>, SourceFailure> {
            Err(self.failure.clone())
        }

        fn fetch(
            &self,
            _candidate: &SourceTrackCandidate,
        ) -> Result<Option<RawLyricPayload>, SourceFailure> {
            Err(self.failure.clone())
        }
    }

    fn track() -> TrackInfo {
        TrackInfo::new("track-1", "A0 Fixture", "Synthetic Artist", "Album", "Spotify", secs(200))
    }

    fn coordinator_with(sources: Vec<Box<dyn LyricSource>>) -> LyricResolutionCoordinator {
        // The trust order must be a permutation of the registered providers, so a test
        // with a single source uses a single entry trust order.
        let registered: Vec<LyricProviderId> = sources
            .iter()
            .map(|source| source.provider_id().clone())
            .collect();
        let trust_policy =
            LyricProviderTrustPolicy::new(registered.clone(), registered).expect("valid trust order");

        LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder {
            sources,
            trust_policy: Some(trust_policy),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn resolves_from_the_first_trusted_provider_with_immediate_acceptance() {
        let coordinator = coordinator_with(vec![Box::new(FixedSource {
            provider_id: known_providers::qq_music(),
            format: LyricPayloadFormat::Lrc,
            payload: Some("[00:01.00]hello world".to_string()),
        })]);

        let result = coordinator.resolve(&track());

        assert_eq!(result.status, LyricResolutionStatus::Succeeded);
        let lyrics = result.lyrics.unwrap();
        assert_eq!(lyrics.provider_id.value(), "QQMusic");
        assert_eq!(lyrics.content.lines.len(), 1);
        assert_eq!(lyrics.content.lines[0].text, "hello world");
        assert_eq!(lyrics.identity_score(), Some(100));
    }

    #[test]
    fn falls_back_to_the_next_provider_when_the_first_fails_transiently() {
        let coordinator = coordinator_with(vec![
            Box::new(FailingSource {
                provider_id: known_providers::qq_music(),
                failure: SourceFailure::Timeout,
            }),
            Box::new(FixedSource {
                provider_id: known_providers::kugou(),
                format: LyricPayloadFormat::Krc,
                payload: Some("[1000,1800]<0,300,0>hi<300,1500,0>there".to_string()),
            }),
        ]);

        let result = coordinator.resolve(&track());

        assert_eq!(result.status, LyricResolutionStatus::Succeeded);
        assert_eq!(result.lyrics.unwrap().provider_id.value(), "Kugou");
    }

    #[test]
    fn reports_transient_failure_when_every_provider_is_unreachable() {
        let coordinator = coordinator_with(vec![Box::new(FailingSource {
            provider_id: known_providers::qq_music(),
            failure: SourceFailure::Connection,
        })]);

        let result = coordinator.resolve(&track());

        assert_eq!(result.status, LyricResolutionStatus::TransientFailure);
        assert!(result.lyrics.is_none());
    }

    #[test]
    fn unknown_title_short_circuits_to_not_found() {
        let coordinator = coordinator_with(vec![Box::new(FixedSource {
            provider_id: known_providers::qq_music(),
            format: LyricPayloadFormat::Lrc,
            payload: Some("[00:01.00]hello".to_string()),
        })]);

        let mut unknown = track();
        unknown.title = "Unknown Title".to_string();

        let result = coordinator.resolve(&unknown);
        assert_eq!(result.status, LyricResolutionStatus::NotFound);
        assert!(result.lyrics.is_none());
    }

    #[test]
    fn resolved_cache_short_circuits_the_second_resolution() {
        let directory = std::env::temp_dir().join(format!(
            "lyric-island-coordinator-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&directory);
        let cache_path = directory.join("resolved.json");

        let source = Arc::new(FixedSource {
            provider_id: known_providers::qq_music(),
            format: LyricPayloadFormat::Lrc,
            payload: Some("[00:01.00]hello world".to_string()),
        });

        struct SharedSource(Arc<FixedSource>);
        impl LyricSource for SharedSource {
            fn provider_id(&self) -> &LyricProviderId {
                self.0.provider_id()
            }
            fn search(
                &self,
                plan: &LyricSearchPlan,
            ) -> Result<Vec<SourceTrackCandidate>, SourceFailure> {
                self.0.search(plan)
            }
            fn fetch(
                &self,
                candidate: &SourceTrackCandidate,
            ) -> Result<Option<RawLyricPayload>, SourceFailure> {
                self.0.fetch(candidate)
            }
        }

        let trust_policy = LyricProviderTrustPolicy::new(
            vec![known_providers::qq_music()],
            vec![known_providers::qq_music()],
        )
        .unwrap();

        let coordinator = LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder {
            sources: vec![Box::new(SharedSource(source))],
            trust_policy: Some(trust_policy),
            resolved_cache: Some(JsonResolvedLyricCache::new(cache_path.clone())),
            ..Default::default()
        })
        .unwrap();

        let first = coordinator.resolve(&track());
        assert_eq!(first.status, LyricResolutionStatus::Succeeded);

        let second = coordinator.resolve(&track());
        let lyrics = second.lyrics.expect("cached");
        assert!(matches!(
            lyrics.acquisition,
            LyricAcquisitionKind::DiskCache | LyricAcquisitionKind::MemoryCache
        ));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn selection_prefers_trust_order_then_the_higher_score() {
        let qq = known_providers::qq_music();
        let kugou = known_providers::kugou();
        let netease = known_providers::netease();

        let candidates = vec![
            ScoredCandidate {
                provider_id: netease.clone(),
                candidate_id: "netease-high".to_string(),
                identity_score: 90,
            },
            ScoredCandidate {
                provider_id: qq.clone(),
                candidate_id: "qq-lower".to_string(),
                identity_score: 85,
            },
            ScoredCandidate {
                provider_id: kugou.clone(),
                candidate_id: "kugou-same".to_string(),
                identity_score: 85,
            },
        ];

        // All below the immediate-acceptance line, so the whole set is the pool and the
        // trust order (QQ -> Kugou -> Netease -> LRCLIB) decides.
        let order = vec![qq.clone(), kugou.clone(), netease.clone()];
        let best = select_best_scored(&candidates, &order).expect("a candidate");
        assert_eq!(best.candidate_id, "qq-lower");

        // Two providers with the same score: the earlier one in the trust order wins.
        let order = vec![kugou.clone(), qq.clone(), netease.clone()];
        let best = select_best_scored(&candidates, &order).expect("a candidate");
        assert_eq!(best.candidate_id, "kugou-same");
    }

    #[test]
    fn selection_prefers_the_high_confidence_pool_over_trust_order() {
        let qq = known_providers::qq_music();
        let kugou = known_providers::kugou();

        let candidates = vec![
            ScoredCandidate {
                provider_id: qq.clone(),
                candidate_id: "qq-medium".to_string(),
                identity_score: 85,
            },
            ScoredCandidate {
                provider_id: kugou.clone(),
                candidate_id: "kugou-immediate".to_string(),
                identity_score: 98,
            },
        ];

        let order = vec![qq.clone(), kugou.clone()];
        let best = select_best_scored(&candidates, &order).expect("a candidate");
        assert_eq!(best.candidate_id, "kugou-immediate");

        assert!(select_best_scored(&[], &order).is_none());
    }

    #[test]
    fn song_search_map_selects_exact_album_then_legacy_then_single() {
        let map = |album: &str, id: &str| SongSearchMap {
            original_title: "Original".to_string(),
            original_artist: "Artist".to_string(),
            original_album: album.to_string(),
            mapped_title: format!("Mapped {id}"),
            mapped_artist: "Mapped artist".to_string(),
            mapped_album: "Mapped album".to_string(),
            preferred_provider: Some("Netease".to_string()),
            is_marked_as_pure_music: false,
        };

        let exact = map("Album", "exact");
        let legacy = map("", "legacy");
        let candidates = vec![exact.clone(), legacy.clone()];
        assert_eq!(
            SongSearchMapResolver::select_best_mapping(&candidates, "Album").unwrap().mapped_title,
            "Mapped exact"
        );
        assert_eq!(
            SongSearchMapResolver::select_best_mapping(&candidates, "Other").unwrap().mapped_title,
            "Mapped legacy"
        );

        let ambiguous = vec![map("A", "a"), map("B", "b")];
        assert!(SongSearchMapResolver::select_best_mapping(&ambiguous, "C").is_none());
        assert_eq!(
            SongSearchMapResolver::select_best_mapping(&ambiguous[..1], "C").unwrap().mapped_title,
            "Mapped a"
        );
    }

    #[test]
    fn song_search_map_resolver_prefers_a_mapped_provider() {
        let resolver = SongSearchMapResolver::new(vec![SongSearchMap {
            original_title: "Original".to_string(),
            original_artist: "Artist".to_string(),
            original_album: "Album".to_string(),
            mapped_title: "Mapped title".to_string(),
            mapped_artist: "Mapped artist".to_string(),
            mapped_album: "Mapped album".to_string(),
            preferred_provider: Some("Netease".to_string()),
            is_marked_as_pure_music: true,
        }]);

        let track = TrackInfo::new("id", "Original", "Artist", "Album", "Spotify", secs(100));
        let mapping = resolver.resolve(&track);

        assert_eq!(mapping.title, "Mapped title");
        assert_eq!(mapping.artist, "Mapped artist");
        assert_eq!(mapping.preferred_provider.as_deref(), Some("Netease"));
        assert!(mapping.is_pure_music);
    }
}

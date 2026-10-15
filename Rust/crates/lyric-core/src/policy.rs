//! Policy layer: admission thresholds, provider trust order, source routing and
//! transient-failure classification.
//!
//! Ported from `Services.LyricMatchingPolicy.cs`, `Services.LyricIdentityEvaluator.cs`,
//! `Services.LyricProviderTrustPolicy.cs`, `Services.LyricSourceRoutingPolicy.cs` and
//! `Services.LyricTransientFailurePolicy.cs`.

use std::collections::BTreeSet;
use std::time::Duration;

use crate::matching::LyricMatcher;
use crate::models::{
    known_providers, LyricCandidateEvaluation, LyricProviderId, SourceTrackCandidate, TrackIdentity,
    TrackInfo,
};

/// `LyricMatchingPolicy`.
pub struct LyricMatchingPolicy;

impl LyricMatchingPolicy {
    pub const MINIMUM_ACCEPTED_MATCH_SCORE: i32 = 80;
    pub const IMMEDIATE_ACCEPTANCE_SCORE: i32 = 95;
    /// Cross-script artist candidates stay admissible but can never reach immediate
    /// acceptance, so they cannot outrank a same-script candidate with real artist evidence.
    pub const CROSS_SCRIPT_ARTIST_MAX_SCORE: i32 = Self::IMMEDIATE_ACCEPTANCE_SCORE - 1;
    pub const DURATION_CONFLICT_THRESHOLD: Duration = Duration::from_secs(20);
    pub const ONLINE_SOURCE_TIMEOUT: Duration = Duration::from_secs(5);
}

/// `LyricIdentityEvaluator`.
pub struct LyricIdentityEvaluator;

impl LyricIdentityEvaluator {
    pub fn evaluate(
        original_track: &TrackIdentity,
        candidate: &SourceTrackCandidate,
    ) -> LyricCandidateEvaluation {
        let track = original_track.to_track_info();
        let score = LyricMatcher::score(
            &track,
            &candidate.title,
            &candidate.joined_artists(),
            candidate.duration.as_secs_f64().round() as i32,
            Some(&candidate.album),
        );

        if score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE {
            LyricCandidateEvaluation::accepted(score)
        } else {
            LyricCandidateEvaluation::rejected(
                score,
                vec![if score == 0 {
                    "identity-conflict".to_string()
                } else {
                    "below-admission-threshold".to_string()
                }],
            )
        }
    }

    /// Convenience overload preserving the C# `Evaluate(TrackInfo, candidate)` call shape.
    pub fn evaluate_track(
        track: &TrackInfo,
        candidate: &SourceTrackCandidate,
    ) -> LyricCandidateEvaluation {
        Self::evaluate(&TrackIdentity::from_track_info(track), candidate)
    }
}

/// `ProviderSongIdPolicy`.
pub struct ProviderSongIdPolicy;

impl ProviderSongIdPolicy {
    pub fn can_use_direct_song_id(track: &TrackIdentity, provider_id: &LyricProviderId) -> bool {
        if track
            .song_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return false;
        }

        match LyricSourceRoutingPolicy::try_get_official_provider(Some(&track.source_app)) {
            Some(official) => official.eq_ignore_ascii_case(provider_id.value()),
            None => false,
        }
    }
}

/// `LyricProviderTrustPolicy` — validates that the trust order is a permutation of the
/// registered providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricProviderTrustPolicy {
    order: Vec<LyricProviderId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustPolicyError {
    DuplicateProvider(String),
    UnregisteredProvider(String),
    MissingProvider(String),
}

impl std::fmt::Display for TrustPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustPolicyError::DuplicateProvider(id) => {
                write!(f, "Trust order contains duplicate provider '{id}'.")
            }
            TrustPolicyError::UnregisteredProvider(id) => {
                write!(f, "Trust order contains unregistered provider '{id}'.")
            }
            TrustPolicyError::MissingProvider(id) => {
                write!(f, "Trust order omits registered provider '{id}'.")
            }
        }
    }
}

impl std::error::Error for TrustPolicyError {}

impl LyricProviderTrustPolicy {
    pub fn new(
        trust_order: Vec<LyricProviderId>,
        registered_providers: Vec<LyricProviderId>,
    ) -> Result<Self, TrustPolicyError> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for provider in &trust_order {
            if !seen.insert(provider.value().to_lowercase()) {
                return Err(TrustPolicyError::DuplicateProvider(provider.value().to_string()));
            }
        }

        let registered: BTreeSet<String> = registered_providers
            .iter()
            .map(|provider| provider.value().to_lowercase())
            .collect();
        let ordered: BTreeSet<String> = trust_order
            .iter()
            .map(|provider| provider.value().to_lowercase())
            .collect();

        for provider in &ordered {
            if !registered.contains(provider) {
                return Err(TrustPolicyError::UnregisteredProvider(provider.clone()));
            }
        }
        for provider in &registered {
            if !ordered.contains(provider) {
                return Err(TrustPolicyError::MissingProvider(provider.clone()));
            }
        }

        Ok(Self { order: trust_order })
    }

    pub fn create_default(
        registered_providers: Vec<LyricProviderId>,
    ) -> Result<Self, TrustPolicyError> {
        Self::new(known_providers::online_trust_order(), registered_providers)
    }

    pub fn order(&self) -> &[LyricProviderId] {
        &self.order
    }
}

/// `LyricSourceRoutingPolicy`.
pub struct LyricSourceRoutingPolicy;

impl LyricSourceRoutingPolicy {
    const ADAPTED_PROVIDERS: [&'static str; 3] = ["QQMusic", "Netease", "Kugou"];
    const UNKNOWN_PRIMARY_FALLBACK: [&'static str; 3] = ["QQMusic", "Netease", "LRCLIB"];
    const UNKNOWN_SECONDARY_FALLBACK: [&'static str; 1] = ["Kugou"];

    /// Maps a player identity onto its own lyric provider, when one exists.
    pub fn try_get_official_provider(source_app: Option<&str>) -> Option<LyricProviderId> {
        let source = source_app.unwrap_or("").trim().to_lowercase();
        if source.contains("qqmusic") {
            return LyricProviderId::try_new(known_providers::QQ_MUSIC).ok();
        }
        if source.contains("netease")
            || source.contains("cloudmusic")
            || source.contains("163music")
            || source.contains("music.163")
            || source.contains("wyy")
        {
            return LyricProviderId::try_new(known_providers::NETEASE).ok();
        }
        if source.contains("kugou") {
            return LyricProviderId::try_new(known_providers::KUGOU).ok();
        }
        None
    }

    /// Provider batches to try, in order. Owned-player sources are excluded from their
    /// own fallback list.
    pub fn build_fallback_batches(track: &TrackInfo) -> Vec<Vec<LyricProviderId>> {
        if let Some(official) = Self::try_get_official_provider(Some(&track.source_app)) {
            let batch = Self::ADAPTED_PROVIDERS
                .iter()
                .filter(|provider| !official.eq_ignore_ascii_case(provider))
                .copied()
                .chain(std::iter::once(known_providers::LRCLIB))
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|provider| LyricProviderId::try_new(provider).ok())
                .collect();
            return vec![Self::dedupe(batch)];
        }

        vec![
            Self::dedupe(
                Self::UNKNOWN_PRIMARY_FALLBACK
                    .iter()
                    .filter_map(|provider| LyricProviderId::try_new(provider).ok())
                    .collect(),
            ),
            Self::dedupe(
                Self::UNKNOWN_SECONDARY_FALLBACK
                    .iter()
                    .filter_map(|provider| LyricProviderId::try_new(provider).ok())
                    .collect(),
            ),
        ]
    }

    fn dedupe(providers: Vec<LyricProviderId>) -> Vec<LyricProviderId> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        providers
            .into_iter()
            .filter(|provider| !provider.value().trim().is_empty())
            .filter(|provider| seen.insert(provider.value().to_lowercase()))
            .collect()
    }
}

/// Transport-level failure of a lyric source.
///
/// The C# original classified raw `Exception` instances (`SocketException`,
/// `HttpRequestException`, `TimeoutException`, ...). The port keeps the same *categories*
/// as an explicit enum so the policy is testable without a live network stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFailure {
    /// `TimeoutException`.
    Timeout,
    /// `SocketError.HostNotFound` / `TryAgain` / `NoData` / `HostDown` / `HostUnreachable`.
    NameResolution,
    /// `SocketError.ConnectionReset` / `Aborted` / `Refused` / `NetworkDown` /
    /// `NetworkUnreachable` / `TimedOut` / `HttpRequestError.ConnectionError`.
    Connection,
    /// `HttpRequestError.ResponseEnded`.
    ResponseEnded,
    /// `HttpRequestError.Unknown`.
    Protocol,
    /// HTTP response status.
    HttpStatus(u16),
    /// TLS handshake/parsing failure — never retried.
    TlsParsing,
    /// Anything else.
    Other(String),
}

/// `LyricTransientFailurePolicy`.
pub struct LyricTransientFailurePolicy;

impl LyricTransientFailurePolicy {
    pub fn is_transient(failure: &SourceFailure) -> bool {
        match failure {
            SourceFailure::Timeout | SourceFailure::NameResolution => true,
            SourceFailure::Connection | SourceFailure::ResponseEnded | SourceFailure::Protocol => {
                true
            }
            SourceFailure::HttpStatus(status) => matches!(status, 408 | 500 | 502 | 503 | 504),
            SourceFailure::TlsParsing | SourceFailure::Other(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ms, secs, LyricProviderId, SourceTrackCandidate};
    use std::collections::BTreeMap;

    fn create_identity(title: &str) -> TrackIdentity {
        TrackIdentity::from_track_info(&TrackInfo::new(
            "identity-track",
            title,
            "Run Away With Me artist",
            "Identity album",
            "Spotify",
            secs(210),
        ))
    }

    fn create_identity_candidate(identity: &TrackIdentity, title: &str) -> SourceTrackCandidate {
        SourceTrackCandidate::try_new(
            known_providers::qq_music(),
            "identity-candidate",
            title,
            identity.artists.clone(),
            identity.album.clone(),
            identity.duration,
            "exact",
            BTreeMap::new(),
        )
        .unwrap()
    }

    #[test]
    fn identity_evaluation_rejects_mismatched_simlish_versions() {
        for (original, candidate) in [
            ("Run Away With Me", "Run Away With Me (Simlish Version)"),
            ("Run Away With Me (Simlish Version)", "Run Away With Me"),
        ] {
            let identity = create_identity(original);
            let evaluation =
                LyricIdentityEvaluator::evaluate(&identity, &create_identity_candidate(&identity, candidate));

            assert!(!evaluation.is_admitted);
            assert_eq!(evaluation.score, 0);
            assert!(evaluation.rejection_reasons.contains(&"identity-conflict".to_string()));
        }
    }

    #[test]
    fn identity_evaluation_allows_candidates_when_both_titles_are_simlish() {
        let identity = create_identity("Run Away With Me (Simlish Version)");
        let evaluation = LyricIdentityEvaluator::evaluate(
            &identity,
            &create_identity_candidate(&identity, "Run Away With Me (Simlish Version)"),
        );

        assert!(evaluation.is_admitted);
        assert!((95..=100).contains(&evaluation.score));
        assert!(evaluation.rejection_reasons.is_empty());
    }

    #[test]
    fn identity_evaluation_does_not_reject_ordinary_bracketed_metadata() {
        for (original, candidate) in [
            ("Run Away With Me", "Run Away With Me (Deluxe Edition)"),
            ("Run Away With Me (Deluxe Edition)", "Run Away With Me"),
            ("Run Away With Me", "Run Away With Me (Remastered 2024)"),
        ] {
            let identity = create_identity(original);
            let evaluation =
                LyricIdentityEvaluator::evaluate(&identity, &create_identity_candidate(&identity, candidate));

            assert!(evaluation.is_admitted, "{original} vs {candidate}");
            assert!((95..=100).contains(&evaluation.score));
        }
    }

    #[test]
    fn identity_score_only_controls_admission_and_does_not_expose_cross_source_ranking() {
        let identity = TrackIdentity::from_track_info(&TrackInfo::new(
            "score-track",
            "Score song",
            "Score artist",
            "Score album",
            "Spotify",
            secs(200),
        ));
        let exact = create_candidate(known_providers::qq_music(), "Score song", "score-exact");
        let relaxed = create_candidate(known_providers::kugou(), "Score song extended", "score-relaxed");

        let exact_evaluation = LyricIdentityEvaluator::evaluate(&identity, &exact);
        let relaxed_evaluation = LyricIdentityEvaluator::evaluate(&identity, &relaxed);

        assert!(exact_evaluation.is_admitted);
        assert!(relaxed_evaluation.is_admitted);
        assert_ne!(exact_evaluation.score, relaxed_evaluation.score);
    }

    #[test]
    fn identity_evaluation_prefers_matching_album_without_rejecting_another_album() {
        let identity = TrackIdentity::from_track_info(&TrackInfo::new(
            "album-track",
            "Album song",
            "Album artist",
            "Studio album",
            "Spotify",
            secs(200),
        ));
        let matching = SourceTrackCandidate::try_new(
            known_providers::qq_music(),
            "matching-album",
            identity.title.clone(),
            identity.artists.clone(),
            identity.album.clone(),
            identity.duration,
            "exact",
            BTreeMap::new(),
        )
        .unwrap();
        let different = SourceTrackCandidate::try_new(
            known_providers::qq_music(),
            "different-album",
            identity.title.clone(),
            identity.artists.clone(),
            "Compilation",
            identity.duration,
            "exact",
            BTreeMap::new(),
        )
        .unwrap();

        let matching_evaluation = LyricIdentityEvaluator::evaluate(&identity, &matching);
        let different_evaluation = LyricIdentityEvaluator::evaluate(&identity, &different);

        assert!(matching_evaluation.is_admitted);
        assert!(different_evaluation.is_admitted);
        assert!(matching_evaluation.score > different_evaluation.score);
    }

    #[test]
    fn song_id_direct_lookup_requires_owning_source_provider() {
        for (source_app, provider, expected) in [
            ("QQMusic", "QQMusic", true),
            ("QQMusic", "Netease", false),
            ("Netease", "Netease", true),
            ("Kugou", "Kugou", true),
            ("Spotify", "QQMusic", false),
        ] {
            let identity = TrackIdentity::from_track_info(
                &TrackInfo::new(
                    "song-id-track",
                    "Song ID track",
                    "Song ID artist",
                    "Song ID album",
                    source_app,
                    secs(180),
                )
                .with_song_id("provider-song-id"),
            );

            assert_eq!(
                ProviderSongIdPolicy::can_use_direct_song_id(
                    &identity,
                    &LyricProviderId::try_new(provider).unwrap()
                ),
                expected,
                "{source_app} / {provider}"
            );
        }
    }

    #[test]
    fn official_provider_mapping_rejects_unknown_players() {
        assert_eq!(
            LyricSourceRoutingPolicy::try_get_official_provider(Some("QQMusic.exe"))
                .map(|provider| provider.value().to_string()),
            Some("QQMusic".to_string())
        );
        assert_eq!(
            LyricSourceRoutingPolicy::try_get_official_provider(Some("CloudMusic"))
                .map(|provider| provider.value().to_string()),
            Some("Netease".to_string())
        );
        assert_eq!(
            LyricSourceRoutingPolicy::try_get_official_provider(Some("music.163.com"))
                .map(|provider| provider.value().to_string()),
            Some("Netease".to_string())
        );
        assert_eq!(
            LyricSourceRoutingPolicy::try_get_official_provider(Some("KuGou Music"))
                .map(|provider| provider.value().to_string()),
            Some("Kugou".to_string())
        );
        assert!(LyricSourceRoutingPolicy::try_get_official_provider(Some("Spotify")).is_none());
    }

    #[test]
    fn fallback_batches_exclude_the_owning_provider() {
        let batches =
            LyricSourceRoutingPolicy::build_fallback_batches(&create_track("QQMusic"));
        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0]
                .iter()
                .map(|provider| provider.value().to_string())
                .collect::<Vec<_>>(),
            vec!["Netease", "Kugou", "LRCLIB"]
        );

        let other = LyricSourceRoutingPolicy::build_fallback_batches(&create_track("Spotify"));
        assert_eq!(other.len(), 2);
        assert_eq!(
            other[0]
                .iter()
                .map(|provider| provider.value().to_string())
                .collect::<Vec<_>>(),
            vec!["QQMusic", "Netease", "LRCLIB"]
        );
        assert_eq!(
            other[1]
                .iter()
                .map(|provider| provider.value().to_string())
                .collect::<Vec<_>>(),
            vec!["Kugou"]
        );
    }

    #[test]
    fn trust_policy_rejects_duplicate_missing_and_unknown_provider_ids() {
        let registered = known_providers::online_trust_order();

        assert_eq!(
            LyricProviderTrustPolicy::new(
                vec![
                    known_providers::qq_music(),
                    known_providers::qq_music(),
                    known_providers::kugou(),
                    known_providers::netease(),
                ],
                registered.clone()
            ),
            Err(TrustPolicyError::DuplicateProvider("QQMusic".to_string()))
        );

        // Omitting a registered provider is rejected.
        let mut incomplete = registered.clone();
        incomplete.pop();
        assert_eq!(
            LyricProviderTrustPolicy::new(incomplete, registered.clone()),
            Err(TrustPolicyError::MissingProvider("lrclib".to_string()))
        );

        // An unregistered provider in the trust order is rejected.
        let mut with_unknown = registered.clone();
        with_unknown.pop();
        with_unknown.push(LyricProviderId::try_new("FifthSource").unwrap());
        assert_eq!(
            LyricProviderTrustPolicy::new(with_unknown, registered.clone()),
            Err(TrustPolicyError::UnregisteredProvider("fifthsource".to_string()))
        );

        // The canonical order is accepted.
        assert!(LyricProviderTrustPolicy::new(registered.clone(), registered).is_ok());
    }

    #[test]
    fn transient_failure_policy_classifies_like_the_original() {
        assert!(LyricTransientFailurePolicy::is_transient(&SourceFailure::Timeout));
        assert!(LyricTransientFailurePolicy::is_transient(&SourceFailure::NameResolution));
        assert!(LyricTransientFailurePolicy::is_transient(&SourceFailure::Connection));
        assert!(LyricTransientFailurePolicy::is_transient(&SourceFailure::HttpStatus(408)));
        assert!(LyricTransientFailurePolicy::is_transient(&SourceFailure::HttpStatus(503)));
        assert!(!LyricTransientFailurePolicy::is_transient(&SourceFailure::HttpStatus(404)));
        assert!(!LyricTransientFailurePolicy::is_transient(&SourceFailure::HttpStatus(401)));
        assert!(!LyricTransientFailurePolicy::is_transient(&SourceFailure::TlsParsing));
        assert!(!LyricTransientFailurePolicy::is_transient(&SourceFailure::Other(
            "bug".to_string()
        )));
    }

    fn create_candidate(
        provider: LyricProviderId,
        title: &str,
        candidate_id: &str,
    ) -> SourceTrackCandidate {
        SourceTrackCandidate::try_new(
            provider,
            candidate_id,
            title,
            vec!["Score artist".to_string()],
            "Score album",
            secs(200),
            "exact",
            BTreeMap::new(),
        )
        .unwrap()
    }

    fn create_track(source_app: &str) -> TrackInfo {
        let mut track = TrackInfo::new(
            "track-id",
            "Midnight City",
            "M83",
            "Hurry Up, We're Dreaming",
            source_app,
            secs(244),
        );
        track.duration = secs(244);
        assert!(track.duration > ms(0));
        track
    }
}

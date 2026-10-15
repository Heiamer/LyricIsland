//! `Services.LyricSearchPlanner.cs` and the internal `LyricSearchStageExecutor`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::matching::LyricMatcher;
use crate::models::{
    LyricSearchPlan, SearchQueryVariant, SourceTrackCandidate, TrackIdentity,
};
use crate::policy::LyricIdentityEvaluator;
use crate::text::contains_content_version_marker;

static ARTIST_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*(?:,|&|/|、|，|;|；|\bfeat\.?\b|\bft\.?\b|\bwith\b)\s*")
        .expect("artist separator regex")
});

static BRACKET_OR_FEATURE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*(?:[\(\[\{（【].*?[\)\]\}）】]|\b(?:feat\.?|ft\.?|with)\b.*)$")
        .expect("relaxed title regex")
});

static DELIMITED_SUFFIX_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+[-–—]\s+").expect("delimited suffix regex"));

/// `LyricSearchPlanner`.
pub struct LyricSearchPlanner;

impl LyricSearchPlanner {
    pub fn create_plan(track: &TrackIdentity) -> Result<LyricSearchPlan, String> {
        if track.title.trim().is_empty() {
            return Err("Track title cannot be empty.".to_string());
        }

        let artists = expand_artists(&track.artists);
        let mut variants: Vec<SearchQueryVariant> = Vec::new();

        add_variant(&mut variants, "exact", &track.title, &artists, track, &[]);

        let normalized_title = LyricMatcher::normalize_search(&track.title);
        let normalized_artists: Vec<String> = artists
            .iter()
            .map(|artist| LyricMatcher::normalize_search(artist))
            .filter(|value| !value.is_empty())
            .collect();

        let artists_changed = artists.len() != normalized_artists.len()
            || artists
                .iter()
                .zip(normalized_artists.iter())
                .any(|(left, right)| left != right);

        if normalized_title != track.title || artists_changed {
            add_variant(
                &mut variants,
                "normalized",
                &normalized_title,
                &normalized_artists,
                track,
                &["punctuation-script-diacritic-normalization"],
            );
        }

        if artists.len() > 1 {
            add_variant(
                &mut variants,
                "primary-artist",
                &track.title,
                &artists[..1],
                track,
                &["primary-artist-only"],
            );
        }

        let relaxed_title = relax_title(&track.title);
        if !relaxed_title.is_empty() && relaxed_title != track.title {
            add_variant(
                &mut variants,
                "relaxed-title",
                &relaxed_title,
                &artists,
                track,
                &["removed-feature-or-bracket-suffix"],
            );
        }

        let version_relaxed = relax_delimited_version_suffix(&track.title);
        if !version_relaxed.is_empty() && version_relaxed != track.title {
            add_variant(
                &mut variants,
                "version-relaxed-title",
                &version_relaxed,
                &artists,
                track,
                &["removed-delimited-version-suffix"],
            );
        }

        LyricSearchPlan::try_new(track.clone(), variants).map_err(|error| error.to_string())
    }
}

fn expand_artists(artists: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for artist in artists {
        for part in ARTIST_SEPARATOR.split(artist) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(trimmed))
            {
                continue;
            }
            seen.push(trimmed.to_string());
        }
    }
    seen
}

fn add_variant(
    variants: &mut Vec<SearchQueryVariant>,
    id: &str,
    title: &str,
    artists: &[String],
    track: &TrackIdentity,
    reasons: &[&str],
) {
    if title.trim().is_empty() {
        return;
    }

    let duplicate = variants.iter().any(|variant| {
        variant.title == title
            && variant.artists.len() == artists.len()
            && variant
                .artists
                .iter()
                .zip(artists.iter())
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    });
    if duplicate {
        return;
    }

    if let Ok(variant) = SearchQueryVariant::try_new(
        id,
        title,
        artists.to_vec(),
        track.album.clone(),
        track.duration,
        reasons.iter().map(|reason| reason.to_string()).collect(),
    ) {
        variants.push(variant);
    }
}

fn relax_title(title: &str) -> String {
    let mut relaxed = title.trim().to_string();
    loop {
        let next = BRACKET_OR_FEATURE_SUFFIX
            .replace(&relaxed, "")
            .trim()
            .to_string();
        if next == relaxed {
            return relaxed;
        }
        relaxed = next;
    }
}

fn relax_delimited_version_suffix(title: &str) -> String {
    let last_match = DELIMITED_SUFFIX_SEPARATOR.find_iter(title).last();
    let Some(delimiter) = last_match else {
        return title.to_string();
    };

    let suffix = &title[delimiter.end()..];
    if contains_content_version_marker(suffix) {
        title[..delimiter.start()].trim().to_string()
    } else {
        title.to_string()
    }
}

/// `LyricSearchStageExecutor` — walks the plan and stops at the first stage that already
/// contains an admissible candidate.
pub struct LyricSearchStageExecutor;

impl LyricSearchStageExecutor {
    pub fn execute<F>(
        plan: &LyricSearchPlan,
        mut retrieve: F,
    ) -> Vec<SourceTrackCandidate>
    where
        F: FnMut(&SearchQueryVariant) -> Vec<SourceTrackCandidate>,
    {
        let mut all: BTreeMap<String, SourceTrackCandidate> = BTreeMap::new();

        for variant in &plan.variants {
            let candidates = deduplicate(retrieve(variant));
            for candidate in &candidates {
                all.entry(candidate.candidate_id.clone())
                    .or_insert_with(|| candidate.clone());
            }

            if has_admitted_candidate(&plan.original_track, &candidates) {
                return candidates;
            }
        }

        all.into_values().collect()
    }
}

fn deduplicate(candidates: Vec<SourceTrackCandidate>) -> Vec<SourceTrackCandidate> {
    let mut unique: BTreeMap<String, SourceTrackCandidate> = BTreeMap::new();
    for candidate in candidates {
        unique
            .entry(candidate.candidate_id.clone())
            .or_insert(candidate);
    }
    unique.into_values().collect()
}

fn has_admitted_candidate(
    original_track: &TrackIdentity,
    candidates: &[SourceTrackCandidate],
) -> bool {
    candidates
        .iter()
        .any(|candidate| LyricIdentityEvaluator::evaluate(original_track, candidate).is_admitted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{known_providers, secs, TrackInfo};

    fn create_identity(title: &str) -> TrackIdentity {
        TrackIdentity::from_track_info(&TrackInfo::new(
            "planner-track",
            title,
            "Planner artist",
            "Planner album",
            "Spotify",
            secs(210),
        ))
    }

    #[test]
    fn plan_preserves_original_identity_and_produces_feature_and_relaxed_variants() {
        let track = TrackInfo::new(
            "planner-track",
            "Signal (Deluxe) feat. Guest",
            "Beyoncé feat. Zoë, Another Artist",
            "Planner album",
            "Spotify",
            secs(210),
        )
        .with_song_id("player-song-id");
        let identity = TrackIdentity::from_track_info(&track);

        let plan = LyricSearchPlanner::create_plan(&identity).unwrap();

        assert_eq!(plan.original_track.title, track.title);
        assert_eq!(plan.original_track.source_app, track.source_app);
        assert_eq!(plan.original_track.song_id, track.song_id);

        let ids: Vec<&str> = plan.variants.iter().map(|v| v.id.as_str()).collect();
        for expected in ["exact", "normalized", "primary-artist", "relaxed-title"] {
            assert!(ids.contains(&expected), "missing variant {expected} in {ids:?}");
        }

        let primary_artist = plan
            .variants
            .iter()
            .find(|variant| variant.id == "primary-artist")
            .unwrap();
        assert_eq!(primary_artist.artists[0], "Beyoncé");

        let relaxed = plan
            .variants
            .iter()
            .find(|variant| variant.id == "relaxed-title")
            .unwrap();
        assert!(!relaxed.title.to_lowercase().contains("deluxe"));
        assert!(!relaxed.title.to_lowercase().contains("feat"));
        assert!(relaxed
            .relaxation_reasons
            .contains(&"removed-feature-or-bracket-suffix".to_string()));
    }

    #[test]
    fn plan_normalizes_punctuation_diacritics_and_script_without_replacing_original_track() {
        let track = TrackInfo::new(
            "planner-normalization",
            "Café, déjà vu!",
            "Beyoncé & Zoë",
            "繁體專輯",
            "Spotify",
            secs(180),
        );
        let identity = TrackIdentity::from_track_info(&track);

        let plan = LyricSearchPlanner::create_plan(&identity).unwrap();
        let normalized = plan
            .variants
            .iter()
            .find(|variant| variant.id == "normalized")
            .unwrap();

        assert_eq!(normalized.title, "cafe deja vu");
        assert_eq!(normalized.artists, vec!["beyonce", "zoe"]);
        assert_eq!(plan.original_track.title, track.title);
        assert_eq!(plan.original_track.primary_artist(), track.artist);
    }

    #[test]
    fn plan_produces_delimited_version_relaxed_variant() {
        for (title, expected) in [
            ("Anti-Hero - ILLENIUM Remix", "Anti-Hero"),
            ("Versioned song – Live", "Versioned song"),
            ("Versioned song — Acoustic Version", "Versioned song"),
        ] {
            let plan = LyricSearchPlanner::create_plan(&create_identity(title)).unwrap();
            let exact = plan
                .variants
                .iter()
                .find(|variant| variant.id == "exact")
                .unwrap();
            let relaxed = plan
                .variants
                .iter()
                .find(|variant| variant.id == "version-relaxed-title")
                .unwrap();

            assert_eq!(relaxed.title, expected);
            assert_eq!(relaxed.artists, exact.artists);
            assert!(relaxed
                .relaxation_reasons
                .contains(&"removed-delimited-version-suffix".to_string()));
            assert_eq!(plan.original_track.title, title);
        }
    }

    #[test]
    fn plan_does_not_relax_ordinary_hyphenated_title() {
        for title in ["Anti-Hero", "Versioned song - Chapter Two"] {
            let plan = LyricSearchPlanner::create_plan(&create_identity(title)).unwrap();
            assert!(!plan
                .variants
                .iter()
                .any(|variant| variant.id == "version-relaxed-title"));
        }
    }

    #[test]
    fn delimited_version_relaxed_query_still_uses_original_version_identity_for_admission() {
        let identity = create_identity("Anti-Hero - ILLENIUM Remix");
        let plan = LyricSearchPlanner::create_plan(&identity).unwrap();
        let relaxed = plan
            .variants
            .iter()
            .find(|variant| variant.id == "version-relaxed-title")
            .unwrap();

        let candidate = candidate_for(&identity, &relaxed.title, &relaxed.id);
        let evaluation = LyricIdentityEvaluator::evaluate(&identity, &candidate);

        assert!(!evaluation.is_admitted);
        assert_eq!(evaluation.score, 0);
        assert!(evaluation.rejection_reasons.contains(&"identity-conflict".to_string()));
    }

    #[test]
    fn relaxed_query_still_uses_original_version_identity_for_admission() {
        for marker in ["Live", "Remix", "Acoustic", "Instrumental", "Karaoke"] {
            let track = TrackInfo::new(
                "version-track",
                format!("Versioned song ({marker})"),
                "Version artist",
                "Version album",
                "Spotify",
                secs(200),
            );
            let identity = TrackIdentity::from_track_info(&track);
            let plan = LyricSearchPlanner::create_plan(&identity).unwrap();
            let relaxed = plan
                .variants
                .iter()
                .find(|variant| variant.id == "relaxed-title")
                .unwrap();

            let candidate = candidate_for(&identity, &relaxed.title, &relaxed.id);
            let evaluation = LyricIdentityEvaluator::evaluate(&identity, &candidate);

            assert!(!evaluation.is_admitted, "{marker}");
            assert_eq!(evaluation.score, 0, "{marker}");
            assert!(evaluation
                .rejection_reasons
                .contains(&"identity-conflict".to_string()));
        }
    }

    #[test]
    fn stage_executor_stops_at_the_first_stage_with_an_admitted_candidate() {
        let identity = create_identity("A0 Fixture");
        let plan = LyricSearchPlanner::create_plan(&identity).unwrap();

        let mut calls = 0;
        let result = LyricSearchStageExecutor::execute(&plan, |variant| {
            calls += 1;
            if variant.id == "exact" {
                vec![candidate_for(&identity, &identity.title, &variant.id)]
            } else {
                Vec::new()
            }
        });

        assert_eq!(calls, 1);
        assert_eq!(result.len(), 1);

        let mut calls = 0;
        let none = LyricSearchStageExecutor::execute(&plan, |_variant| {
            calls += 1;
            Vec::new()
        });
        assert_eq!(calls, plan.variants.len());
        assert!(none.is_empty());
    }

    fn candidate_for(
        identity: &TrackIdentity,
        title: &str,
        query_variant_id: &str,
    ) -> SourceTrackCandidate {
        SourceTrackCandidate::try_new(
            known_providers::qq_music(),
            "candidate",
            title,
            identity.artists.clone(),
            identity.album.clone(),
            identity.duration,
            query_variant_id,
            BTreeMap::new(),
        )
        .unwrap()
    }
}

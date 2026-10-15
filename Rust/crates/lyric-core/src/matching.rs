//! `Utilities.LyricMatcher.Score` — the identity scorer.

use crate::models::TrackInfo;
use crate::text::{
    is_cross_script, normalize_for_search, normalize_title_for_comparison, CONFLICT_KEYWORDS,
};

/// Port of `LyricMatcher`. Kept as an associated-function façade so call sites read
/// like the original (`LyricMatcher::score(...)`).
pub struct LyricMatcher;

impl LyricMatcher {
    /// `LyricMatcher.NormalizeForSearch`.
    pub fn normalize_search(value: &str) -> String {
        normalize_for_search(value)
    }

    /// `LyricMatcher.NormalizeTitleForComparison`.
    pub fn normalize_title_for_comparison(value: &str) -> String {
        normalize_title_for_comparison(value)
    }

    /// `LyricMatcher.ContainsContentVersionMarker`.
    pub fn contains_content_version_marker(value: &str) -> bool {
        crate::text::contains_content_version_marker(value)
    }

    /// Scores a candidate against the local track. `0` means a hard identity conflict.
    pub fn score(
        target: &TrackInfo,
        result_title: &str,
        result_artist: &str,
        result_duration_seconds: i32,
        result_album: Option<&str>,
    ) -> i32 {
        if is_unknown_title(&target.title) || is_unknown_title(result_title) {
            return 0;
        }

        for keyword in CONFLICT_KEYWORDS {
            if has_version_conflict(&target.title, result_title, keyword) {
                return 0;
            }
        }

        let normalized_target_title = normalize_title_for_comparison(&target.title);
        let normalized_result_title = normalize_title_for_comparison(result_title);
        let normalized_target_artist = normalize_for_search(&target.artist);
        let normalized_result_artist = normalize_for_search(result_artist);
        let normalized_target_album = normalize_for_search(&target.album);
        let normalized_result_album = normalize_for_search(result_album.unwrap_or(""));

        let title_sim = get_string_similarity(&normalized_target_title, &normalized_result_title);
        let mut artist_sim =
            get_string_similarity(&normalized_target_artist, &normalized_result_artist);
        let artist_token_overlap = get_token_overlap_similarity(
            &normalized_target_artist,
            &normalized_result_artist,
        );
        if artist_token_overlap > 0.0 {
            artist_sim = artist_sim.max(artist_token_overlap);
        }
        let duration_sim =
            get_duration_similarity(target.duration.as_secs_f64(), result_duration_seconds as f64);
        let album_sim = get_string_similarity(&normalized_target_album, &normalized_result_album);

        let has_target_artist = has_useful_artist(&normalized_target_artist);
        let has_result_artist = has_useful_artist(&normalized_result_artist);
        let has_duration = target.duration.as_secs_f64() > 0.0 && result_duration_seconds > 0;

        // A cross-script artist pair (魚韻 vs sakanaction) is an unverified alias
        // hypothesis: it is treated as *missing* evidence rather than as a zero score.
        let artists_cross_script = has_target_artist
            && has_result_artist
            && is_cross_script(&normalized_target_artist, &normalized_result_artist);
        let score_artist = has_target_artist && has_result_artist && !artists_cross_script;

        let mut total = if score_artist && has_duration {
            (title_sim * 0.50) + (artist_sim * 0.30) + (duration_sim * 0.20)
        } else if score_artist {
            (title_sim * 0.60) + (artist_sim * 0.40)
        } else if has_duration {
            (title_sim * 0.75) + (duration_sim * 0.25)
        } else {
            title_sim
        };

        if has_useful_album(&normalized_target_album) && has_useful_album(&normalized_result_album) {
            const ALBUM_WEIGHT: f64 = 0.05;
            total = (total * (1.0 - ALBUM_WEIGHT)) + (album_sim * ALBUM_WEIGHT);
        }

        if artists_cross_script {
            total = total.min(0.94);
        }

        (total * 100.0).round() as i32
    }
}

fn get_string_similarity(s1: &str, s2: &str) -> f64 {
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    crate::text::JaroWinkler::default().similarity(s1, s2)
}

fn get_duration_similarity(local_seconds: f64, remote_seconds: f64) -> f64 {
    if remote_seconds == 0.0 || local_seconds <= 0.0 {
        return 0.0;
    }

    const PERFECT_TOLERANCE: f64 = 1.0;
    const MAX_TOLERANCE: f64 = 10.0;

    let diff = (local_seconds - remote_seconds).abs();
    if diff <= PERFECT_TOLERANCE {
        return 1.0;
    }
    if diff >= MAX_TOLERANCE {
        return 0.0;
    }
    1.0 - ((diff - PERFECT_TOLERANCE) / (MAX_TOLERANCE - PERFECT_TOLERANCE))
}

fn has_useful_artist(normalized_artist: &str) -> bool {
    !normalized_artist.trim().is_empty() && normalized_artist != "unknown artist"
}

fn has_useful_album(normalized_album: &str) -> bool {
    !normalized_album.trim().is_empty() && normalized_album != "unknown album"
}

fn is_unknown_title(title: &str) -> bool {
    title.trim().is_empty() || title.trim().eq_ignore_ascii_case("unknown title")
}

fn get_token_overlap_similarity(s1: &str, s2: &str) -> f64 {
    let tokens1 = split_comparable_tokens(s1);
    let tokens2 = split_comparable_tokens(s2);
    if tokens1.is_empty() || tokens2.is_empty() {
        return 0.0;
    }

    let overlap = tokens1.iter().filter(|token| tokens2.contains(*token)).count();
    if overlap == 0 {
        return 0.0;
    }

    ((overlap as f64 / tokens1.len() as f64) + (overlap as f64 / tokens2.len() as f64)) / 2.0
}

fn split_comparable_tokens(value: &str) -> Vec<String> {
    let mut tokens: Vec<String> = value
        .split(' ')
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .filter(|token| token.chars().count() >= 2)
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens
}

fn has_version_conflict(target: &str, result: &str, keyword: &str) -> bool {
    let lowered_keyword = keyword.to_lowercase();
    let target_has = target.to_lowercase().contains(&lowered_keyword);
    let result_has = result.to_lowercase().contains(&lowered_keyword);
    target_has != result_has
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{secs, TrackInfo};
    use crate::policy::LyricMatchingPolicy;

    fn create_track(title: &str, artist: &str, duration_seconds: u64) -> TrackInfo {
        TrackInfo::new(
            "track-id",
            title,
            artist,
            "Album",
            "Spotify",
            secs(duration_seconds),
        )
    }

    #[test]
    fn score_when_title_artist_and_duration_match_returns_high_confidence() {
        let track = create_track("Midnight City", "M83", 244);
        assert!(LyricMatcher::score(&track, "Midnight City", "M83", 244, None) >= 95);
    }

    #[test]
    fn score_when_only_one_title_has_live_version_marker_rejects_the_match() {
        let track = create_track("Midnight City", "M83", 244);
        assert_eq!(
            LyricMatcher::score(&track, "Midnight City (Live)", "M83", 244, None),
            0
        );
    }

    #[test]
    fn score_treats_bracketed_version_marker_as_equivalent_to_delimited_version_marker() {
        let mut track = create_track("Anti-Hero - ILLENIUM Remix", "Taylor Swift", 267);
        track.album = "Anti-Hero (Remixes) [Explicit]".to_string();

        let score = LyricMatcher::score(
            &track,
            "Anti-Hero (Illenium Remix)",
            "Taylor Swift / ILLENIUM",
            267,
            Some("Anti-Hero (Remixes) [Explicit]"),
        );
        assert_eq!(score, 98);
    }

    #[test]
    fn score_rejects_different_version_markers_inside_brackets() {
        let track = create_track("Anti-Hero - ILLENIUM Remix", "Taylor Swift", 267);
        assert_eq!(
            LyricMatcher::score(&track, "Anti-Hero (Live)", "Taylor Swift", 267, None),
            0
        );
    }

    #[test]
    fn score_still_ignores_non_version_bracketed_title_content() {
        let track = create_track("Midnight City", "M83", 244);
        assert_eq!(
            LyricMatcher::score(&track, "Midnight City (Radio Edit)", "M83", 244, None),
            100
        );
    }

    #[test]
    fn score_treats_bracketed_from_qualifier_as_equivalent_to_delimited_qualifier() {
        let mut track = create_track("Nobody - from Kaiju No. 8", "OneRepublic", 153);
        track.album = "Nobody (from Kaiju No. 8)".to_string();

        let score = LyricMatcher::score(
            &track,
            "Nobody (from Kaiju No. 8)",
            "OneRepublic",
            153,
            Some("Nobody (from Kaiju No. 8)"),
        );
        assert_eq!(score, 100);
    }

    #[test]
    fn score_rejects_from_qualifier_when_only_one_title_contains_it() {
        let track = create_track("Nobody - from Kaiju No. 8", "OneRepublic", 153);
        assert_eq!(
            LyricMatcher::score(&track, "Nobody", "OneRepublic", 153, None),
            0
        );
    }

    #[test]
    fn score_when_duration_differs_by_twenty_seconds_reduces_score_but_does_not_reject() {
        let track = create_track("Midnight City", "M83", 244);
        let score = LyricMatcher::score(&track, "Midnight City", "M83", 270, None);
        assert!((1..=100).contains(&score));
        assert!(score < 100);
    }

    #[test]
    fn score_uses_album_as_low_weight_evidence_without_hard_rejection() {
        let track = create_track("Midnight City", "M83", 244);
        let matching_album = LyricMatcher::score(&track, "Midnight City", "M83", 244, Some("Album"));
        let different_album =
            LyricMatcher::score(&track, "Midnight City", "M83", 244, Some("Compilation"));

        assert_eq!(matching_album, 100);
        assert!((90..=99).contains(&different_album));
    }

    #[test]
    fn score_admits_cross_script_artist_alias_without_immediate_acceptance() {
        let mut track = create_track("新宝島", "魚韻", 305);
        track.album = "834.194".to_string();

        let score = LyricMatcher::score(&track, "新宝島", "sakanaction", 306, Some("新宝島"));
        assert!(score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
        assert!(score < LyricMatchingPolicy::IMMEDIATE_ACCEPTANCE_SCORE);
    }

    #[test]
    fn score_treats_kana_versus_latin_artist_names_as_not_comparable() {
        let track = create_track("新宝島", "サカナクション", 305);
        let score = LyricMatcher::score(&track, "新宝島", "sakanaction", 305, None);
        assert!(score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
    }

    #[test]
    fn score_admits_cross_script_artist_alias_with_duration_drift() {
        let track = create_track("新宝島", "魚韻", 300);
        let score = LyricMatcher::score(&track, "新宝島", "sakanaction", 305, None);
        assert!(score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
    }

    #[test]
    fn score_still_penalizes_different_artists_within_same_script() {
        let track = create_track("Midnight City", "M83", 244);
        let score = LyricMatcher::score(&track, "Midnight City", "Coldplay", 244, None);
        assert!(score < LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
    }

    #[test]
    fn score_compares_mixed_script_artist_names_normally() {
        let track = create_track("新宝島", "X玖少年团", 305);
        let score = LyricMatcher::score(&track, "新宝島", "sakanaction", 305, None);
        assert!(score < LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
    }

    #[test]
    fn score_ranks_verified_same_script_artist_above_cross_script_candidate() {
        let track = create_track("Love Me Back", "RITUAL / Tove Styrke", 178);

        let verified = LyricMatcher::score(
            &track,
            "Love Me Back",
            "R I T U A L / Tove Styrke",
            178,
            Some("Love Me Back"),
        );
        let cross_script = LyricMatcher::score(
            &track,
            "Love Me Back (爱我)",
            "倖田來未",
            177,
            Some("Love Me Back"),
        );

        assert!(verified > cross_script);
    }

    #[test]
    fn score_treats_spaced_letter_artist_styling_as_exact_match() {
        let mut track = create_track("Love Me Back", "RITUAL / Tove Styrke", 178);
        track.album = "Love Me Back".to_string();

        let score = LyricMatcher::score(
            &track,
            "Love Me Back",
            "R I T U A L / Tove Styrke",
            178,
            Some("Love Me Back"),
        );
        assert_eq!(score, 100);
    }

    #[test]
    fn score_keeps_cross_script_artist_below_immediate_acceptance() {
        let track = create_track("Love Me Back", "RITUAL / Tove Styrke", 178);
        let score = LyricMatcher::score(
            &track,
            "Love Me Back",
            "倖田來未",
            178,
            Some("Love Me Back"),
        );
        assert!(score >= LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE);
        assert!(score < LyricMatchingPolicy::IMMEDIATE_ACCEPTANCE_SCORE);
    }
}

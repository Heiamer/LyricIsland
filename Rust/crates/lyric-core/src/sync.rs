//! `Services.LyricSyncService.cs` — turns a playback snapshot into a display frame.
//!
//! ### Deviation from the C# original
//! The original owns a background search task (`StartLyricsUpdate`) with cancellation,
//! stabilization delay and network retries. The port keeps the *state machine* and the
//! frame maths, but exposes [`LyricSyncService::begin_track`] /
//! [`LyricSyncService::complete_search`] so the caller drives the resolution (single
//! threaded, no runtime dependency). Retry timing lives in the caller.

use std::time::{Duration, Instant};

use crate::models::{
    LyricAcquisitionKind, LyricDocument, LyricDisplayFrame, LyricLine, LyricResolutionStatus,
    PlaybackSnapshot, ResolvedLyrics, TrackInfo,
};
use crate::projector::ResolvedLyricsCompatibilityProjector;
use crate::policy::LyricMatchingPolicy;

/// `LyricSyncService.SearchingText` / `NoLyricsText`.
pub const SEARCHING_TEXT: &str = "正在检索歌词...";
pub const NO_LYRICS_TEXT: &str = "暂未找到歌词";

const STARTUP_LINE_GUARD_POSITION_THRESHOLD_MS: u128 = 500;
const STARTUP_LINE_GUARD_LOAD_WINDOW_MS: u128 = 300;
pub const DEFAULT_METADATA_STABILIZATION_DELAY: Duration = Duration::from_millis(750);
pub const NETWORK_RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

/// `LyricSyncService`.
pub struct LyricSyncService {
    player_lead_time: Box<dyn Fn(Option<&str>) -> Duration + Send + Sync>,
    track_lead_time: Box<dyn Fn(Option<&TrackInfo>, Option<&str>) -> Duration + Send + Sync>,
    metadata_stabilization_delay: Duration,

    current_track: Option<TrackInfo>,
    current_track_id: Option<String>,
    current_document: Option<LyricDocument>,
    current_document_has_translation: bool,
    current_lyric_source_app: Option<String>,
    current_lyric_acquisition: LyricAcquisitionKind,
    current_lyric_fetch_elapsed_millis: u64,
    current_lyric_resolved_at: Option<Instant>,
    duration_correction_consumed: bool,
    last_emitted_line_index: i32,
    document_loaded_at: Option<Instant>,
    last_search_duration: Option<Duration>,
    status: LyricResolutionStatus,
}

impl Default for LyricSyncService {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricSyncService {
    pub fn new() -> Self {
        Self {
            player_lead_time: Box::new(|_| Duration::ZERO),
            track_lead_time: Box::new(|_, _| Duration::ZERO),
            metadata_stabilization_delay: DEFAULT_METADATA_STABILIZATION_DELAY,
            current_track: None,
            current_track_id: None,
            current_document: None,
            current_document_has_translation: false,
            current_lyric_source_app: None,
            current_lyric_acquisition: LyricAcquisitionKind::Unknown,
            current_lyric_fetch_elapsed_millis: 0,
            current_lyric_resolved_at: None,
            duration_correction_consumed: false,
            last_emitted_line_index: -1,
            document_loaded_at: None,
            last_search_duration: None,
            status: LyricResolutionStatus::NotFound,
        }
    }

    pub fn with_lead_times(
        player_lead_time: impl Fn(Option<&str>) -> Duration + Send + Sync + 'static,
        track_lead_time: impl Fn(Option<&TrackInfo>, Option<&str>) -> Duration + Send + Sync + 'static,
        metadata_stabilization_delay: Duration,
    ) -> Self {
        let mut service = Self::new();
        service.player_lead_time = Box::new(player_lead_time);
        service.track_lead_time = Box::new(track_lead_time);
        service.metadata_stabilization_delay = metadata_stabilization_delay;
        service
    }

    pub fn metadata_stabilization_delay(&self) -> Duration {
        self.metadata_stabilization_delay
    }

    pub fn current_lyric_source_app(&self) -> Option<&str> {
        self.current_lyric_source_app.as_deref()
    }

    pub fn current_lyric_acquisition(&self) -> LyricAcquisitionKind {
        self.current_lyric_acquisition
    }

    pub fn current_lyric_fetch_elapsed_millis(&self) -> u64 {
        self.current_lyric_fetch_elapsed_millis
    }

    pub fn current_lyric_resolved_at(&self) -> Option<Instant> {
        self.current_lyric_resolved_at
    }

    pub fn current_document(&self) -> Option<&LyricDocument> {
        self.current_document.as_ref()
    }

    pub fn is_searching(&self) -> bool {
        self.current_lyric_acquisition == LyricAcquisitionKind::Searching
    }

    /// `BuildStableTrackIdentity` — stays stable while SMTC fills in SongId and duration.
    pub fn build_stable_track_identity(track: &TrackInfo) -> String {
        format!(
            "{}|{}|{}",
            normalize_identity_part(&track.source_app),
            normalize_identity_part(&track.title),
            normalize_identity_part(&track.artist)
        )
    }

    /// Starts a lyric search for `track`. Returns `true` when the track is new, in which
    /// case the caller should resolve lyrics and hand them to [`Self::complete_search`].
    pub fn begin_track(&mut self, track: &TrackInfo) -> bool {
        let track_id = Self::build_stable_track_identity(track);

        if self.current_track_id.as_deref() == Some(track_id.as_str()) {
            self.current_track = Some(track.clone());
            if self.should_correct_duration(track.duration) {
                self.duration_correction_consumed = true;
                self.reset_document();
                self.status = LyricResolutionStatus::Succeeded;
                return true;
            }
            return false;
        }

        self.current_track = Some(track.clone());
        self.current_track_id = Some(track_id);
        self.reset_document();
        true
    }

    fn reset_document(&mut self) {
        self.current_document = None;
        self.current_document_has_translation = false;
        self.current_lyric_source_app = None;
        self.current_lyric_acquisition = LyricAcquisitionKind::Searching;
        self.current_lyric_fetch_elapsed_millis = 0;
        self.current_lyric_resolved_at = None;
        self.last_emitted_line_index = -1;
        self.last_search_duration = None;
        self.duration_correction_consumed = false;
    }

    /// `TryApplyResolvedLyrics` — installs lyrics resolved elsewhere.
    pub fn complete_search(&mut self, track: &TrackInfo, resolved: Option<&ResolvedLyrics>) -> bool {
        let Some(current_track_id) = self.current_track_id.clone() else {
            return false;
        };
        if current_track_id != Self::build_stable_track_identity(track) {
            return false;
        }

        let Some(resolved) = resolved else {
            self.current_lyric_acquisition = LyricAcquisitionKind::NotFound;
            self.current_lyric_fetch_elapsed_millis = 0;
            self.current_lyric_resolved_at = Some(Instant::now());
            self.last_search_duration = Some(track.duration);
            return false;
        };

        if resolved.content.lines.is_empty() {
            return false;
        }

        let document = ResolvedLyricsCompatibilityProjector::to_lyric_document(resolved, false);
        if document.lines.is_empty() {
            return false;
        }

        self.current_track = Some(track.clone());
        self.current_document_has_translation = document
            .lines
            .iter()
            .any(|line| line.translation.as_deref().is_some_and(|t| !t.trim().is_empty()));
        self.current_document = Some(document);
        self.current_lyric_source_app = Some(resolved.provider_id.value().to_string());
        self.current_lyric_acquisition = resolved.acquisition;
        self.current_lyric_fetch_elapsed_millis = read_elapsed_millis(resolved);
        self.current_lyric_resolved_at = Some(Instant::now());
        self.document_loaded_at = Some(Instant::now());
        self.last_emitted_line_index = -1;
        self.last_search_duration = Some(track.duration);
        self.duration_correction_consumed = true;
        self.status = LyricResolutionStatus::Succeeded;
        true
    }

    /// `GetDisplayFrameAsync`.
    pub fn get_display_frame(&mut self, snapshot: &PlaybackSnapshot) -> LyricDisplayFrame {
        let Some(track) = snapshot.track.as_ref() else {
            self.current_track = None;
            self.current_track_id = None;
            self.current_document = None;
            self.current_document_has_translation = false;
            self.current_lyric_source_app = None;
            self.current_lyric_acquisition = LyricAcquisitionKind::Unknown;
            self.current_lyric_fetch_elapsed_millis = 0;
            self.current_lyric_resolved_at = None;
            self.last_search_duration = None;
            self.duration_correction_consumed = false;
            self.last_emitted_line_index = -1;
            return LyricDisplayFrame::empty();
        };

        // `begin_track` may start a new search; the frame then reports the search state.
        self.begin_track(track);

        let Some(document) = self.current_document.as_ref() else {
            return LyricDisplayFrame::message(
                if self.is_searching() {
                    SEARCHING_TEXT
                } else {
                    NO_LYRICS_TEXT
                },
                self.current_track
                    .as_ref()
                    .map(|track| track.title.clone())
                    .unwrap_or_default(),
            );
        };

        if document.lines.is_empty() {
            return LyricDisplayFrame::message(
                if self.is_searching() {
                    SEARCHING_TEXT
                } else {
                    NO_LYRICS_TEXT
                },
                self.current_track
                    .as_ref()
                    .map(|track| track.title.clone())
                    .unwrap_or_default(),
            );
        }

        // Both offsets shift the playback position; lyric timestamps stay immutable.
        let source_lead = (self.player_lead_time)(
            self.current_track.as_ref().map(|track| track.source_app.as_str()),
        );
        let track_lead = (self.track_lead_time)(
            self.current_track.as_ref(),
            self.current_lyric_source_app.as_deref(),
        );
        let position = snapshot.position + source_lead + track_lead;

        let lines = &document.lines;
        let mut current_index = find_current_line_index(lines, position);

        // Startup grace period: for 300ms after lyrics load the SMTC position is often
        // stale; force the first line instead of showing the wrong one.
        if let Some(loaded_at) = self.document_loaded_at {
            if loaded_at.elapsed().as_millis() < STARTUP_LINE_GUARD_LOAD_WINDOW_MS
                && self.last_emitted_line_index < 0
                && position.as_millis() <= STARTUP_LINE_GUARD_POSITION_THRESHOLD_MS
                && current_index >= 0
            {
                current_index = 0;
            }
        }

        if current_index >= 0 {
            self.last_emitted_line_index = current_index;
        }

        let title = self
            .current_track
            .as_ref()
            .map(|track| track.title.clone())
            .unwrap_or_default();

        if current_index < 0 {
            let first = &lines[0];
            return LyricDisplayFrame {
                current_line: first.text.clone(),
                next_line: lines.get(1).map(|line| line.text.clone()).unwrap_or_default(),
                title,
                line_progress: 0.0,
                current_line_index: 0,
                is_pure_music: document.is_pure_music,
                word_scan_progress: calculate_word_scan_progress(
                    first,
                    position,
                    first.text.chars().count(),
                ),
                current_translation: first.translation.clone(),
                next_translation: lines.get(1).and_then(|line| line.translation.clone()),
                has_track_translation: self.current_document_has_translation,
            };
        }

        let display_index = current_index as usize;
        let current_line = &lines[display_index];
        let next_line = lines.get(display_index + 1);

        let mut progress = 0.0;
        if let Some(next) = next_line {
            let duration = next.timestamp.saturating_sub(current_line.timestamp);
            if !duration.is_zero() {
                let elapsed = position.saturating_sub(current_line.timestamp);
                progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
            }
        }

        LyricDisplayFrame {
            current_line: current_line.text.clone(),
            next_line: next_line.map(|line| line.text.clone()).unwrap_or_default(),
            title,
            line_progress: progress,
            current_line_index: current_index,
            is_pure_music: document.is_pure_music,
            word_scan_progress: calculate_word_scan_progress(
                current_line,
                position,
                current_line.text.chars().count(),
            ),
            current_translation: current_line.translation.clone(),
            next_translation: next_line.and_then(|line| line.translation.clone()),
            has_track_translation: self.current_document_has_translation,
        }
    }

    fn should_correct_duration(&self, latest_duration: Duration) -> bool {
        if self.duration_correction_consumed {
            return false;
        }
        let Some(searched_duration) = self.last_search_duration else {
            return false;
        };
        if latest_duration.is_zero() {
            return false;
        }

        searched_duration.is_zero()
            || latest_duration.abs_diff(searched_duration)
                >= LyricMatchingPolicy::DURATION_CONFLICT_THRESHOLD
    }
}

fn normalize_identity_part(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        value.trim().to_uppercase()
    }
}

fn read_elapsed_millis(resolved: &ResolvedLyrics) -> u64 {
    resolved
        .diagnostics
        .get("elapsedMs")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

/// `FindCurrentLineIndex` — the last line whose timestamp is not after `position`; a run
/// of duplicate timestamps collapses onto the first entry of that run.
pub fn find_current_line_index(lines: &[LyricLine], position: Duration) -> i32 {
    let mut current_index: i32 = -1;
    let mut current_timestamp: Option<Duration> = None;

    for (index, line) in lines.iter().enumerate() {
        if line.timestamp > position {
            break;
        }
        if current_timestamp != Some(line.timestamp) {
            current_timestamp = Some(line.timestamp);
            current_index = index as i32;
        }
    }

    current_index
}

/// `CalculateWordScanProgress` — the fraction of the line covered by karaoke highlighting.
pub fn calculate_word_scan_progress(
    line: &LyricLine,
    position: Duration,
    display_text_length: usize,
) -> Option<f64> {
    let syllables = line.syllables.as_ref().filter(|value| !value.is_empty())?;

    let elapsed = position.saturating_sub(line.timestamp);
    let mut scanned_text_length = 0.0f64;
    let mut text_search_index = 0usize;
    let mut has_usable_syllable = false;
    let mut last_usable_syllable_end = Duration::ZERO;

    // Character offsets of the line text, so byte/char indexing stays consistent.
    let characters: Vec<char> = line.text.chars().collect();

    for syllable in syllables {
        if syllable.text.is_empty() {
            continue;
        }

        let mut syllable_text_length = syllable.text.chars().count();
        let syllable_text_index = find_from(
            &characters,
            &syllable.text.chars().collect::<Vec<_>>(),
            text_search_index,
        )
        .unwrap_or_else(|| text_search_index.min(characters.len()));

        syllable_text_length = syllable_text_length.min(characters.len() - syllable_text_index);
        if syllable_text_length == 0 {
            continue;
        }

        has_usable_syllable = true;
        last_usable_syllable_end = syllable.relative_offset + syllable.duration;

        if elapsed <= syllable.relative_offset {
            break;
        }

        if syllable.duration.is_zero() {
            scanned_text_length = (syllable_text_index + syllable_text_length) as f64;
            text_search_index = syllable_text_index + syllable_text_length;
            continue;
        }

        let syllable_progress = ((elapsed.as_secs_f64() - syllable.relative_offset.as_secs_f64())
            / syllable.duration.as_secs_f64())
        .clamp(0.0, 1.0);

        scanned_text_length =
            syllable_text_index as f64 + (syllable_text_length as f64 * syllable_progress);
        if syllable_progress < 1.0 {
            break;
        }

        text_search_index = syllable_text_index + syllable_text_length;
    }

    if !has_usable_syllable {
        return None;
    }

    if scanned_text_length >= text_search_index as f64
        && text_search_index > 0
        && elapsed >= last_usable_syllable_end
    {
        return Some(1.0);
    }

    Some((scanned_text_length / display_text_length.max(1) as f64).clamp(0.0, 1.0))
}

fn find_from(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || from > haystack.len() - needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()] == *needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{known_providers, ms, secs, LyricPayloadFormat, LyricSyllable, LyricTimingKind,
        LyricTimingProvenance, ParsedLyricLine, ParsedLyricSegment, ParsedLyrics};
    use crate::projector::ResolvedLyricsCompatibilityProjector;

    fn line(timestamp: Duration, text: &str) -> LyricLine {
        LyricLine {
            timestamp,
            text: text.to_string(),
            translation: None,
            syllables: None,
        }
    }

    fn resolved_from(lines: Vec<LyricLine>) -> ResolvedLyrics {
        let document = LyricDocument::new(lines, 0, false);
        let content = crate::projector::LyricDocumentSemanticProjector::to_parsed_lyrics(&document);
        ResolvedLyrics::try_new(
            content,
            known_providers::local(),
            "local:track",
            LyricAcquisitionKind::LocalFile,
            Default::default(),
        )
        .unwrap()
    }

    #[test]
    fn stable_identity_ignores_song_id_and_duration() {
        let mut track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify", secs(200));
        let first = LyricSyncService::build_stable_track_identity(&track);
        track.song_id = Some("song".to_string());
        track.duration = secs(210);

        assert_eq!(first, LyricSyncService::build_stable_track_identity(&track));
    }

    #[test]
    fn current_line_is_the_last_line_at_or_before_the_position() {
        let lines = vec![
            line(secs(1), "one"),
            line(secs(3), "two"),
            line(secs(5), "three"),
        ];

        assert_eq!(find_current_line_index(&lines, ms(500)), -1);
        assert_eq!(find_current_line_index(&lines, secs(1)), 0);
        assert_eq!(find_current_line_index(&lines, ms(2999)), 0);
        assert_eq!(find_current_line_index(&lines, secs(3)), 1);
        assert_eq!(find_current_line_index(&lines, secs(10)), 2);
    }

    #[test]
    fn duplicate_timestamps_collapse_onto_the_first_entry() {
        let lines = vec![
            line(secs(1), "one"),
            line(secs(1), "one translation"),
            line(secs(4), "two"),
        ];
        assert_eq!(find_current_line_index(&lines, secs(2)), 0);
    }

    #[test]
    fn display_frame_reports_searching_then_lines() {
        let track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify", secs(200));
        let mut service = LyricSyncService::new();

        assert!(service.begin_track(&track));
        let frame = service.get_display_frame(&PlaybackSnapshot::playing(track.clone(), secs(1)));
        assert_eq!(frame.current_line, SEARCHING_TEXT);

        let resolved = resolved_from(vec![
            line(secs(1), "one"),
            line(secs(3), "two"),
            line(secs(5), "three"),
        ]);
        assert!(service.complete_search(&track, Some(&resolved)));

        let frame = service.get_display_frame(&PlaybackSnapshot::playing(track.clone(), ms(4000)));
        assert_eq!(frame.current_line, "two");
        assert_eq!(frame.next_line, "three");
        assert_eq!(frame.current_line_index, 1);
        assert!((frame.line_progress - 0.5).abs() < 0.001);
        assert_eq!(frame.title, "Title");
    }

    #[test]
    fn display_frame_before_the_first_line_prepares_the_first_line() {
        let track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify", secs(200));
        let mut service = LyricSyncService::new();
        service.begin_track(&track);
        let resolved = resolved_from(vec![line(secs(2), "one"), line(secs(4), "two")]);
        service.complete_search(&track, Some(&resolved));

        let frame = service.get_display_frame(&PlaybackSnapshot::playing(track, ms(500)));
        assert_eq!(frame.current_line, "one");
        assert_eq!(frame.next_line, "two");
        assert_eq!(frame.current_line_index, 0);
        assert_eq!(frame.line_progress, 0.0);
    }

    #[test]
    fn idle_snapshot_clears_everything() {
        let track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify", secs(200));
        let mut service = LyricSyncService::new();
        service.begin_track(&track);
        service.complete_search(&track, Some(&resolved_from(vec![line(secs(1), "one")])));

        let frame = service.get_display_frame(&PlaybackSnapshot::idle());

        assert_eq!(frame.current_line, "");
        assert!(service.current_document().is_none());
        assert_eq!(service.current_lyric_acquisition(), LyricAcquisitionKind::Unknown);
    }

    #[test]
    fn missing_lyrics_reports_the_not_found_state() {
        let track = TrackInfo::new("id", "Title", "Artist", "Album", "Spotify", secs(200));
        let mut service = LyricSyncService::new();
        service.begin_track(&track);
        assert!(!service.complete_search(&track, None));

        let frame = service.get_display_frame(&PlaybackSnapshot::playing(track, secs(1)));
        assert_eq!(frame.current_line, NO_LYRICS_TEXT);
        assert_eq!(service.current_lyric_acquisition(), LyricAcquisitionKind::NotFound);
    }

    #[test]
    fn word_scan_progress_follows_syllable_timing() {
        let karaoke_line = LyricLine {
            timestamp: secs(1),
            text: "abcd".to_string(),
            translation: None,
            syllables: Some(vec![
                LyricSyllable {
                    relative_offset: Duration::ZERO,
                    duration: ms(500),
                    text: "ab".to_string(),
                },
                LyricSyllable {
                    relative_offset: ms(500),
                    duration: ms(500),
                    text: "cd".to_string(),
                },
            ]),
        };

        assert_eq!(calculate_word_scan_progress(&karaoke_line, secs(1), 4), Some(0.0));
        let half = calculate_word_scan_progress(&karaoke_line, ms(1250), 4).unwrap();
        assert!((half - 0.25).abs() < 0.001, "{half}");
        assert_eq!(calculate_word_scan_progress(&karaoke_line, ms(3000), 4), Some(1.0));
        assert!(calculate_word_scan_progress(&line(secs(1), "plain"), secs(1), 5).is_none());
    }

    #[test]
    fn duration_correction_triggers_at_most_once() {
        let mut service = LyricSyncService::new();
        service.last_search_duration = Some(secs(200));
        service.duration_correction_consumed = false;

        assert!(service.should_correct_duration(secs(230)));
        assert!(!service.should_correct_duration(secs(205)));
        service.duration_correction_consumed = true;
        assert!(!service.should_correct_duration(secs(230)));
    }

    #[test]
    fn compatibility_projector_round_trip_feeds_the_sync_service() {
        let start = secs(2);
        let content = ParsedLyrics::new(
            vec![ParsedLyricLine::try_new(
                start,
                Some(start + secs(2)),
                "original",
                Some("translated".to_string()),
                vec![ParsedLyricSegment::try_new(start, start + ms(350), "syllable").unwrap()],
                false,
            )
            .unwrap()],
            LyricTimingKind::WordTimed,
            LyricTimingProvenance::ProviderSupplied,
            LyricPayloadFormat::Krc,
            false,
        );
        let resolved = ResolvedLyrics::try_new(
            content,
            known_providers::kugou(),
            "kugou-candidate",
            LyricAcquisitionKind::Remote,
            Default::default(),
        )
        .unwrap();

        let document = ResolvedLyricsCompatibilityProjector::to_lyric_document(&resolved, true);
        assert_eq!(document.lines[0].text, "original");
        assert_eq!(document.lines[0].translation.as_deref(), Some("translated"));
    }
}

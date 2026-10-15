//! Projectors between the legacy document model and the pipeline model.
//!
//! * `Services.LyricDocumentSemanticProjector.cs`
//! * `Services.ResolvedLyricsCompatibilityProjector.cs`

use std::time::Duration;

use crate::models::{
    LyricDocument, LyricLine, LyricPayloadFormat, LyricSyllable, LyricTimingKind,
    LyricTimingProvenance, ParsedLyricLine, ParsedLyricSegment, ParsedLyrics, ResolvedLyrics,
};

/// `LyricDocumentSemanticProjector`.
pub struct LyricDocumentSemanticProjector;

impl LyricDocumentSemanticProjector {
    pub fn to_parsed_lyrics(document: &LyricDocument) -> ParsedLyrics {
        let lines: Vec<ParsedLyricLine> = document
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let end_time = document
                    .lines
                    .get(index + 1)
                    .map(|next| next.timestamp)
                    .filter(|end| *end > line.timestamp);
                to_parsed_lyric_line(line, end_time)
            })
            .collect();

        // `LyricDocument` keeps syllables but not whether they were supplied or synthesized.
        let timing_kind = if lines.is_empty() {
            LyricTimingKind::Unsynced
        } else if lines.iter().any(|line| !line.segments.is_empty()) {
            LyricTimingKind::Mixed
        } else {
            LyricTimingKind::LineTimed
        };

        ParsedLyrics::new(
            lines,
            timing_kind,
            LyricTimingProvenance::Unknown,
            LyricPayloadFormat::Lrc,
            document.is_pure_music,
        )
    }
}

fn to_parsed_lyric_line(line: &LyricLine, end_time: Option<Duration>) -> ParsedLyricLine {
    let text = if line.text.trim().is_empty() {
        // The document model tolerates empty text; the pipeline model does not. Fall back to
        // a single space so callers never see a dropped line.
        " ".to_string()
    } else {
        line.text.clone()
    };

    ParsedLyricLine {
        start_time: line.timestamp,
        end_time,
        text: text.trim().to_string(),
        translation: crate::models::normalize_translation(line.translation.clone()),
        segments: project_syllables(line, end_time),
        is_information_line: false,
    }
}

fn project_syllables(line: &LyricLine, line_end: Option<Duration>) -> Vec<ParsedLyricSegment> {
    let Some(syllables) = line.syllables.as_ref().filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let mut segments = Vec::with_capacity(syllables.len());
    for syllable in syllables {
        match try_project_syllable(line.timestamp, line_end, syllable) {
            Some(segment) => segments.push(segment),
            None => return Vec::new(),
        }
    }
    segments
}

fn try_project_syllable(
    line_start: Duration,
    line_end: Option<Duration>,
    syllable: &LyricSyllable,
) -> Option<ParsedLyricSegment> {
    if syllable.duration.is_zero() || syllable.text.trim().is_empty() {
        return None;
    }

    let start_time = line_start.checked_add(syllable.relative_offset)?;
    let end_time = start_time.checked_add(syllable.duration)?;

    if start_time < line_start {
        return None;
    }
    if end_time <= start_time {
        return None;
    }
    if line_end.is_some_and(|end| end_time > end) {
        return None;
    }

    ParsedLyricSegment::try_new(start_time, end_time, syllable.text.clone()).ok()
}

/// `ResolvedLyricsCompatibilityProjector`.
pub struct ResolvedLyricsCompatibilityProjector;

impl ResolvedLyricsCompatibilityProjector {
    pub fn to_lyric_document(resolved: &ResolvedLyrics, include_information_lines: bool) -> LyricDocument {
        let lines: Vec<LyricLine> = resolved
            .content
            .lines
            .iter()
            .filter(|line| include_information_lines || !line.is_information_line)
            .map(to_lyric_line)
            .collect();

        LyricDocument::new(lines, 0, resolved.content.is_pure_music)
    }
}

fn to_lyric_line(line: &ParsedLyricLine) -> LyricLine {
    let syllables = if line.segments.is_empty() {
        None
    } else {
        Some(
            line.segments
                .iter()
                .map(|segment| LyricSyllable {
                    relative_offset: segment.start_time.saturating_sub(line.start_time),
                    duration: segment.end_time - segment.start_time,
                    text: segment.text.clone(),
                })
                .collect(),
        )
    };

    LyricLine {
        timestamp: line.start_time,
        text: line.text.clone(),
        translation: line.translation.clone(),
        syllables,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{known_providers, ms, secs, LyricAcquisitionKind};

    #[test]
    fn compatibility_projector_preserves_text_syllables_translation_and_pure_music() {
        let start = secs(2);
        let segment = ParsedLyricSegment::try_new(start, start + ms(350), "syllable").unwrap();
        let content = ParsedLyrics::new(
            vec![ParsedLyricLine::try_new(
                start,
                Some(start + secs(2)),
                "original",
                Some("translated".to_string()),
                vec![segment],
                false,
            )
            .unwrap()],
            LyricTimingKind::WordTimed,
            LyricTimingProvenance::ProviderSupplied,
            LyricPayloadFormat::Krc,
            true,
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
        assert_eq!(document.lines.len(), 1);
        let line = &document.lines[0];
        let syllable = line.syllables.as_ref().unwrap().first().unwrap();

        assert!(document.is_pure_music);
        assert_eq!(line.text, "original");
        assert_eq!(line.translation.as_deref(), Some("translated"));
        assert_eq!(syllable.relative_offset, Duration::ZERO);
        assert_eq!(syllable.duration, ms(350));
        assert_eq!(syllable.text, "syllable");
    }

    #[test]
    fn semantic_projector_derives_end_times_from_the_next_line() {
        let document = LyricDocument::new(
            vec![
                LyricLine {
                    timestamp: secs(1),
                    text: "first".to_string(),
                    translation: None,
                    syllables: None,
                },
                LyricLine {
                    timestamp: ms(2500),
                    text: "second".to_string(),
                    translation: None,
                    syllables: None,
                },
            ],
            0,
            false,
        );

        let parsed = LyricDocumentSemanticProjector::to_parsed_lyrics(&document);

        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].end_time, Some(ms(2500)));
        assert_eq!(parsed.lines[1].end_time, None);
        assert_eq!(parsed.timing_kind, LyricTimingKind::LineTimed);
        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::Unknown);
    }

    #[test]
    fn information_lines_can_be_filtered_out() {
        let content = ParsedLyrics::new(
            vec![
                ParsedLyricLine::try_new(secs(1), None, "credits", None, Vec::new(), true).unwrap(),
                ParsedLyricLine::try_new(secs(2), None, "lyric", None, Vec::new(), false).unwrap(),
            ],
            LyricTimingKind::LineTimed,
            LyricTimingProvenance::ProviderSupplied,
            LyricPayloadFormat::Yrc,
            false,
        );
        let resolved = ResolvedLyrics::try_new(
            content,
            known_providers::netease(),
            "netease-candidate",
            LyricAcquisitionKind::Remote,
            Default::default(),
        )
        .unwrap();

        let with_info = ResolvedLyricsCompatibilityProjector::to_lyric_document(&resolved, true);
        let without_info = ResolvedLyricsCompatibilityProjector::to_lyric_document(&resolved, false);

        assert_eq!(with_info.lines.len(), 2);
        assert_eq!(without_info.lines.len(), 1);
        assert_eq!(without_info.lines[0].text, "lyric");
    }
}

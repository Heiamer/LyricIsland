//! `Services.LyricifyPayloadDecoder.cs` and `Services.LyricifyPayloadParser.cs`.
//!
//! The decoder handles the encrypted QRC/KRC containers; the parser turns a decoded
//! payload into the pipeline's [`ParsedLyrics`] model, including LRC translation
//! alignment, information-line marking and timing-kind classification.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::crypto;
use crate::formats::{self, LineInfo};
use crate::models::{
    DecodedLyricPayload, LyricPayloadFormat, LyricTimingKind, LyricTimingProvenance, ParsedLyricLine,
    ParsedLyricSegment, ParsedLyrics, RawLyricPayload,
};

/// Diagnostic key consumed by `ApplyInformationLineMarkers`.
pub const INFORMATION_LINE_START_TIMES_DIAGNOSTIC: &str = "informationLineStartTimesMs";

/// Translation lines are matched to originals within this tolerance.
const TRANSLATION_TOLERANCE_MS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    EmptyPayload,
    NoUsableLines(String),
    NoPlainText,
    MissingDecoder(LyricPayloadFormat),
    Decrypt(crypto::DecryptError),
    Model(String),
}

impl std::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::EmptyPayload => write!(f, "Lyric payload is empty."),
            PayloadError::NoUsableLines(format) => write!(
                f,
                "The {format} parser returned no usable lyric lines."
            ),
            PayloadError::NoPlainText => {
                write!(f, "Plain lyric payload contains no usable text.")
            }
            PayloadError::MissingDecoder(format) => write!(
                f,
                "No decoder is registered for encrypted {} payloads.",
                format.as_str()
            ),
            PayloadError::Decrypt(error) => write!(f, "{error}"),
            PayloadError::Model(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for PayloadError {}

/// `LyricifyPayloadDecoder`.
pub struct LyricifyPayloadDecoder;

impl LyricifyPayloadDecoder {
    pub fn can_decode(format: LyricPayloadFormat) -> bool {
        matches!(format, LyricPayloadFormat::Qrc | LyricPayloadFormat::Krc)
    }

    pub fn decode(payload: &RawLyricPayload) -> Result<DecodedLyricPayload, PayloadError> {
        let original = payload.original_lyrics.clone();

        let decrypted = match (payload.is_encrypted, original.as_deref()) {
            (true, Some(text)) if !text.trim().is_empty() => Some(match payload.format {
                LyricPayloadFormat::Qrc => {
                    crypto::decrypt_qrc(text).map_err(PayloadError::Decrypt)?
                }
                LyricPayloadFormat::Krc => {
                    crypto::decrypt_krc(text).map_err(PayloadError::Decrypt)?
                }
                other => return Err(PayloadError::MissingDecoder(other)),
            }),
            _ => None,
        };

        Ok(DecodedLyricPayload::from_raw(
            payload,
            decrypted.or(original),
        ))
    }

    /// Pass-through used by the coordinator when `IsEncrypted` is false.
    pub fn identity(payload: &RawLyricPayload) -> DecodedLyricPayload {
        DecodedLyricPayload::from_raw(payload, payload.original_lyrics.clone())
    }
}

/// `LyricifyPayloadParser`.
pub struct LyricifyPayloadParser;

impl LyricifyPayloadParser {
    pub fn can_parse(format: LyricPayloadFormat) -> bool {
        matches!(
            format,
            LyricPayloadFormat::Lrc
                | LyricPayloadFormat::Qrc
                | LyricPayloadFormat::Krc
                | LyricPayloadFormat::Yrc
                | LyricPayloadFormat::Ttml
                | LyricPayloadFormat::PlainText
        )
    }

    pub fn parse(payload: &DecodedLyricPayload) -> Result<ParsedLyrics, PayloadError> {
        let original = payload.original_lyrics.as_deref().unwrap_or("");

        if payload.is_pure_music && original.trim().is_empty() {
            return Ok(ParsedLyrics::pure_music_placeholder(payload.format));
        }

        if original.trim().is_empty() {
            return Err(PayloadError::EmptyPayload);
        }

        if payload.format == LyricPayloadFormat::PlainText {
            return Self::parse_plain_text(payload);
        }

        let lines = if payload.format == LyricPayloadFormat::Lrc {
            formats::parse_lrc(original)
        } else {
            parse_explicit(payload.format, original)?
        };

        let mut converted = convert_lines(&lines)?;

        if let Some(translation_content) = payload
            .translation_lyrics
            .as_deref()
            .filter(|content| !content.trim().is_empty())
        {
            converted = apply_lrc_translations(converted, translation_content);
        }

        converted = apply_information_line_markers(converted, &payload.diagnostics);

        if converted.is_empty() {
            return Err(PayloadError::NoUsableLines(payload.format.as_str().to_string()));
        }

        let timing_kind = determine_timing_kind(&converted);
        Ok(ParsedLyrics::new(
            converted,
            timing_kind,
            LyricTimingProvenance::ProviderSupplied,
            payload.format,
            payload.is_pure_music,
        ))
    }

    fn parse_plain_text(payload: &DecodedLyricPayload) -> Result<ParsedLyrics, PayloadError> {
        let lines: Vec<ParsedLyricLine> = payload
            .original_lyrics
            .as_deref()
            .unwrap_or("")
            .replace('\r', "")
            .split('\n')
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .filter_map(|text| ParsedLyricLine::simple(Duration::ZERO, None, text).ok())
            .collect();

        if lines.is_empty() {
            return Err(PayloadError::NoPlainText);
        }

        Ok(ParsedLyrics::new(
            lines,
            LyricTimingKind::Unsynced,
            LyricTimingProvenance::Unknown,
            LyricPayloadFormat::PlainText,
            payload.is_pure_music,
        ))
    }
}

fn parse_explicit(
    format: LyricPayloadFormat,
    content: &str,
) -> Result<Vec<LineInfo>, PayloadError> {
    let lines = match format {
        LyricPayloadFormat::Qrc => formats::parse_qrc(content),
        LyricPayloadFormat::Krc => formats::parse_krc(content),
        LyricPayloadFormat::Yrc => formats::parse_yrc(content),
        LyricPayloadFormat::Ttml => formats::parse_ttml(content),
        _ => {
            return Err(PayloadError::NoUsableLines(format.as_str().to_string()));
        }
    };
    Ok(lines)
}

/// `ConvertLines` — source lines become validated [`ParsedLyricLine`]s, blank text is
/// dropped and word/character segments are clipped to the line range.
fn convert_lines(source_lines: &[LineInfo]) -> Result<Vec<ParsedLyricLine>, PayloadError> {
    let mut lines: Vec<ParsedLyricLine> = Vec::new();

    for source_line in source_lines {
        if source_line.text.trim().is_empty() {
            continue;
        }

        let start_ms = source_line.start_time.unwrap_or(0).max(0);
        let segments = convert_segments(source_line, start_ms, source_line.end_time);

        let end_ms = match source_line.end_time.filter(|end| *end > 0) {
            Some(end) => Some(end.max(start_ms)),
            None => segments
                .iter()
                .map(|segment| segment.end_time.as_millis() as i64)
                .max(),
        };

        let line = ParsedLyricLine::try_new(
            Duration::from_millis(start_ms as u64),
            end_ms.map(|end| Duration::from_millis(end.max(start_ms) as u64)),
            normalize_line_text(&source_line.text),
            source_line.translation.clone(),
            segments,
            source_line.is_information_line,
        )
        .map_err(|error| PayloadError::Model(error.to_string()))?;

        lines.push(line);
    }

    lines.sort_by_key(|line| line.start_time);
    Ok(lines)
}

fn convert_segments(
    source_line: &LineInfo,
    line_start_ms: i64,
    line_end_ms: Option<i64>,
) -> Vec<ParsedLyricSegment> {
    let mut segments: Vec<ParsedLyricSegment> = source_line
        .syllables
        .iter()
        .filter(|syllable| {
            !syllable.text.is_empty()
                && syllable.start_time >= line_start_ms
                && syllable.end_time > syllable.start_time
                && line_end_ms.is_none_or(|end| syllable.end_time <= end)
        })
        .filter_map(|syllable| {
            ParsedLyricSegment::try_new(
                Duration::from_millis(syllable.start_time as u64),
                Duration::from_millis(syllable.end_time as u64),
                syllable.text.clone(),
            )
            .ok()
        })
        .collect();

    segments.sort_by_key(|segment| segment.start_time);
    segments
}

fn apply_lrc_translations(
    original_lines: Vec<ParsedLyricLine>,
    translation_content: &str,
) -> Vec<ParsedLyricLine> {
    let translation_lines = convert_lines(&formats::parse_lrc(translation_content))
        .unwrap_or_default();

    original_lines
        .into_iter()
        .map(|line| {
            let matched = translation_lines.iter().find(|candidate| {
                let diff = abs_diff_ms(candidate.start_time, line.start_time);
                diff <= TRANSLATION_TOLERANCE_MS as u128
            });

            match matched {
                None => line,
                Some(translation) => ParsedLyricLine {
                    start_time: line.start_time,
                    end_time: line.end_time,
                    text: line.text.clone(),
                    translation: crate::models::normalize_translation(Some(
                        translation.text.clone(),
                    )),
                    segments: line.segments.clone(),
                    is_information_line: line.is_information_line,
                },
            }
        })
        .collect()
}

fn abs_diff_ms(left: Duration, right: Duration) -> u128 {
    if left >= right {
        (left - right).as_millis()
    } else {
        (right - left).as_millis()
    }
}

fn apply_information_line_markers(
    lines: Vec<ParsedLyricLine>,
    diagnostics: &BTreeMap<String, String>,
) -> Vec<ParsedLyricLine> {
    let Some(serialized) = diagnostics.get(INFORMATION_LINE_START_TIMES_DIAGNOSTIC) else {
        return lines;
    };

    let marked: Vec<Duration> = serialized
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .map(|value| Duration::from_millis(value as u64))
        .collect();

    if marked.is_empty() {
        return lines;
    }

    lines
        .into_iter()
        .map(|line| {
            if marked.contains(&line.start_time) {
                ParsedLyricLine {
                    is_information_line: true,
                    ..line
                }
            } else {
                line
            }
        })
        .collect()
}

fn determine_timing_kind(lines: &[ParsedLyricLine]) -> LyricTimingKind {
    let segmented: Vec<&ParsedLyricLine> = lines
        .iter()
        .filter(|line| !line.segments.is_empty())
        .collect();

    if segmented.is_empty() {
        return LyricTimingKind::LineTimed;
    }
    if segmented.len() != lines.len() {
        return LyricTimingKind::Mixed;
    }

    let has_character = segmented.iter().flat_map(|line| &line.segments).any(|segment| {
        segment.text.chars().count() == 1
    });
    let has_word = segmented
        .iter()
        .flat_map(|line| &line.segments)
        .any(|segment| segment.text.chars().count() > 1);

    match (has_character, has_word) {
        (true, true) => LyricTimingKind::Mixed,
        (true, false) => LyricTimingKind::CharacterTimed,
        _ => LyricTimingKind::WordTimed,
    }
}

/// Whitespace collapse used by `NormalizeLineText`.
fn normalize_line_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{known_providers, ms, secs};

    fn decoded(
        format: LyricPayloadFormat,
        original: &str,
        translation: Option<&str>,
    ) -> DecodedLyricPayload {
        DecodedLyricPayload {
            provider_id: known_providers::qq_music(),
            candidate_id: format!("a2-{}", format.as_str()),
            format,
            original_lyrics: Some(original.to_string()),
            translation_lyrics: translation.map(str::to_string),
            is_pure_music: false,
            diagnostics: BTreeMap::new(),
        }
    }

    const LRC_FIXTURE: &str = "[offset:+250]\n[00:03.00][00:01.00]alpha fixture line\n[00:02.500]\n[xx:yy]malformed fixture line\n[00:04.00]beta fixture line";

    #[test]
    fn lrc_fixture_applies_offset_expands_timestamps_and_aligns_translation() {
        let payload = decoded(
            LyricPayloadFormat::Lrc,
            LRC_FIXTURE,
            Some("[offset:+250]\n[00:01.00]translated fixture line\n[00:03.00]//"),
        );

        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        assert_eq!(parsed.format, LyricPayloadFormat::Lrc);
        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::ProviderSupplied);
        assert_eq!(parsed.timing_kind, LyricTimingKind::LineTimed);
        assert_eq!(parsed.lines.len(), 3);
        assert_eq!(parsed.lines[0].start_time, ms(1250));
        assert_eq!(parsed.lines[1].start_time, ms(3250));
        assert_eq!(parsed.lines[2].start_time, ms(4250));
        assert_eq!(parsed.lines[0].text, "alpha fixture line");
        assert_eq!(
            parsed.lines[0].translation.as_deref(),
            Some("translated fixture line")
        );
        assert!(parsed.lines[1].translation.is_none());
        assert!(parsed.lines.iter().all(|line| line.segments.is_empty()));
    }

    #[test]
    fn qrc_fixture_retains_absolute_line_and_segment_timing() {
        let payload = decoded(
            LyricPayloadFormat::Qrc,
            "[1000,1800]qrc(1000,300)zero(1300,0)word(1300,1500)",
            None,
        );
        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::ProviderSupplied);
        let line = parsed
            .lines
            .iter()
            .find(|line| line.start_time == secs(1))
            .expect("line at 1s");
        assert_eq!(line.end_time, Some(ms(2800)));
        assert!(!line.segments.is_empty());
        assert_eq!(line.segments.len(), 2);
        assert_eq!(line.segments[0].start_time, secs(1));
        assert_eq!(line.segments[0].end_time, ms(1300));
        assert_eq!(line.segments[1].start_time, ms(1300));
        assert_eq!(line.segments[1].end_time, ms(2800));
    }

    #[test]
    fn krc_fixture_retains_absolute_line_and_segment_timing() {
        let payload = decoded(
            LyricPayloadFormat::Krc,
            "[1000,1800]<0,300,0>krc<300,0,0>zero<300,1500,0>word",
            None,
        );
        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        let line = parsed
            .lines
            .iter()
            .find(|line| line.start_time == secs(1))
            .expect("line at 1s");
        assert_eq!(line.end_time, Some(ms(2800)));
        assert_eq!(line.segments.len(), 2);
        assert_eq!(line.segments[0].start_time, secs(1));
        assert_eq!(line.segments[0].end_time, ms(1300));
        assert_eq!(line.segments[1].start_time, ms(1300));
        assert_eq!(line.segments[1].end_time, ms(2800));
    }

    #[test]
    fn yrc_fixture_retains_absolute_line_and_segment_timing() {
        let payload = decoded(
            LyricPayloadFormat::Yrc,
            "[1000,1800](1000,300,0)yrc(1300,0,0)zero(1300,1500,0)word",
            None,
        );
        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        let line = parsed
            .lines
            .iter()
            .find(|line| line.start_time == secs(1))
            .expect("line at 1s");
        assert_eq!(line.end_time, Some(ms(2800)));
        assert_eq!(line.segments.len(), 2);
        assert_eq!(line.segments[0].start_time, secs(1));
        assert_eq!(line.segments[1].end_time, ms(2800));
    }

    #[test]
    fn ttml_fixture_uses_explicit_ttml_parser() {
        let payload = decoded(LyricPayloadFormat::Ttml, TTML_FIXTURE, None);
        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        assert_eq!(parsed.format, LyricPayloadFormat::Ttml);
        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::ProviderSupplied);
        assert!(parsed.lines.iter().any(|line| line.text == "ttml zero fixture"));
        assert!(parsed.lines.iter().any(|line| line.text == "metadata line"));
    }

    #[test]
    fn plain_text_returns_unsynced_lines() {
        let payload = decoded(LyricPayloadFormat::PlainText, "plain first\n\nplain second", None);
        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        assert_eq!(parsed.timing_kind, LyricTimingKind::Unsynced);
        assert_eq!(parsed.timing_provenance, LyricTimingProvenance::Unknown);
        assert_eq!(
            parsed.lines.iter().map(|line| line.text.clone()).collect::<Vec<_>>(),
            vec!["plain first", "plain second"]
        );
        assert!(parsed.lines.iter().all(|line| line.end_time.is_none()));
    }

    #[test]
    fn explicit_information_line_marker_is_preserved_without_text_heuristics() {
        let mut payload = decoded(
            LyricPayloadFormat::Lrc,
            "[00:01.00]metadata line\n[00:02.00]Composer: The Band",
            None,
        );
        payload
            .diagnostics
            .insert(INFORMATION_LINE_START_TIMES_DIAGNOSTIC.to_string(), "1000".to_string());

        let parsed = LyricifyPayloadParser::parse(&payload).unwrap();

        assert!(parsed.lines[0].is_information_line);
        assert!(!parsed.lines[1].is_information_line);
    }

    #[test]
    fn empty_or_invalid_payload_fails_explicitly() {
        assert_eq!(
            LyricifyPayloadParser::parse(&decoded(LyricPayloadFormat::Lrc, "", None)),
            Err(PayloadError::EmptyPayload)
        );
        assert!(matches!(
            LyricifyPayloadParser::parse(&decoded(
                LyricPayloadFormat::Lrc,
                "[not-a-time]invalid",
                None
            )),
            Err(PayloadError::NoUsableLines(_))
        ));
        // The original only guarantees that *a* format exception is raised here; both
        // "payload is empty" and "plain payload has no usable text" are acceptable.
        assert!(LyricifyPayloadParser::parse(&decoded(
            LyricPayloadFormat::PlainText,
            "\n  \r\n",
            None
        ))
        .is_err());
    }

    #[test]
    fn decoder_decrypts_qrc_and_krc_but_passes_plain_payloads_through() {
        let plain = RawLyricPayload::try_new(
            known_providers::qq_music(),
            "plain",
            LyricPayloadFormat::Qrc,
            Some("[1000,100]hi(1000,100)".to_string()),
            None,
            false,
            false,
            BTreeMap::new(),
        )
        .unwrap();
        let decoded = LyricifyPayloadDecoder::decode(&plain).unwrap();
        assert_eq!(decoded.original_lyrics.as_deref(), Some("[1000,100]hi(1000,100)"));

        let encrypted_text = crypto::encrypt_qrc("[1000,100]hi(1000,100)").unwrap();
        let encrypted = RawLyricPayload::try_new(
            known_providers::qq_music(),
            "encrypted",
            LyricPayloadFormat::Qrc,
            Some(encrypted_text),
            None,
            true,
            false,
            BTreeMap::new(),
        )
        .unwrap();
        let decoded = LyricifyPayloadDecoder::decode(&encrypted).unwrap();
        assert_eq!(decoded.original_lyrics.as_deref(), Some("[1000,100]hi(1000,100)"));
    }

    const TTML_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tl="urn:taskbarlyrics:test">
  <body>
    <div>
      <p xml:id="line-1" begin="00:00:01.000" end="00:00:02.800">
        <span begin="00:00:01.000" end="00:00:01.300">ttml</span>
        <span begin="00:00:01.300" end="00:00:01.300">zero</span>
        <span begin="00:00:01.300" end="00:00:02.800">fixture</span>
      </p>
      <p xml:id="info-1" begin="00:00:03.000" end="00:00:03.500" tl:information-line="true">metadata line</p>
      <p xml:id="line-2" begin="00:00:04.000" end="00:00:05.000">line timed fallback</p>
    </div>
  </body>
</tt>"#;
}

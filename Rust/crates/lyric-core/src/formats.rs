//! Lyric format parsers.
//!
//! The C# original delegated every concrete format to `Lyricify.Lyrics.Parsers`
//! (`LrcParser`, `QrcParser`, `KrcParser`, `YrcParser`, `TtmlParser`). Those parsers are
//! reproduced here as a self-contained, dependency-free implementation, checked against
//! the fixtures that ship with the original test project
//! (`TaskbarLyrics.Core.Tests/TestData/Lyrics`).

use std::sync::LazyLock;

use regex::Regex;

/// A timed word/character inside a line, in absolute milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyllableInfo {
    pub text: String,
    pub start_time: i64,
    pub end_time: i64,
}

impl SyllableInfo {
    pub fn new(text: impl Into<String>, start_time: i64, end_time: i64) -> Self {
        Self {
            text: text.into(),
            start_time,
            end_time,
        }
    }
}

/// The neutral per-line model every format parser produces.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LineInfo {
    pub text: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub syllables: Vec<SyllableInfo>,
    pub translation: Option<String>,
    pub is_information_line: bool,
}

impl LineInfo {
    pub fn text_from_syllables(&self) -> String {
        self.syllables
            .iter()
            .map(|syllable| syllable.text.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(\d+):(\d+)(?:[.:](\d{1,3}))?\]").expect("lrc timestamp regex")
});

static OFFSET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*\[offset\s*:\s*([+-]?\d+)\]\s*$").expect("offset regex"));

// ---------------------------------------------------------------------------
// LRC
// ---------------------------------------------------------------------------

/// `LrcParser.Parse` plus the wrapper's own `[offset:]` handling: the offset is applied
/// *inside* this function, so translated payloads get the same shift.
pub fn parse_lrc(content: &str) -> Vec<LineInfo> {
    let mut offset_ms: i64 = 0;
    let mut lyric_lines: Vec<&str> = Vec::new();

    let normalized_content = content.replace('\r', "");
    for raw in normalized_content.split('\n') {
        if let Some(captures) = OFFSET.captures(raw) {
            if let Ok(value) = captures[1].parse::<i64>() {
                offset_ms = value;
                continue;
            }
        }
        lyric_lines.push(raw);
    }

    let mut lines: Vec<LineInfo> = Vec::new();
    for raw in lyric_lines {
        let matches: Vec<_> = TIMESTAMP.find_iter(raw).collect();
        if matches.is_empty() {
            continue;
        }

        let last = matches.last().expect("non-empty");
        let text_start = last.end();
        let text = if text_start < raw.len() {
            raw[text_start..].to_string()
        } else {
            String::new()
        };

        for capture in matches {
            let timestamp = parse_lrc_timestamp(capture.as_str()).saturating_add(offset_ms).max(0);
            lines.push(LineInfo {
                text: text.clone(),
                start_time: Some(timestamp),
                end_time: None,
                syllables: Vec::new(),
                translation: None,
                is_information_line: false,
            });
        }
    }

    lines
}

fn parse_lrc_timestamp(text: &str) -> i64 {
    let inner = text.trim_start_matches('[').trim_end_matches(']');
    let mut parts = inner.split([':', '.']);
    let minutes: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let seconds: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let fraction = parts.next().unwrap_or("");
    let millis = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().unwrap_or(0) * 100,
        2 => fraction.parse::<i64>().unwrap_or(0) * 10,
        _ => fraction[..3].parse::<i64>().unwrap_or(0),
    };
    minutes * 60_000 + seconds * 1000 + millis
}

// ---------------------------------------------------------------------------
// QRC (QQ Music)
// ---------------------------------------------------------------------------

/// `QrcParser.Parse` on an already decrypted payload.
///
/// Line header `[start,duration]`, then `(start,duration)text` word stamps. A parenthesis
/// whose content is not a pair of integers belongs to the lyric text (QQ Music uses
/// parentheses for repeated phrases).
pub fn parse_qrc(content: &str) -> Vec<LineInfo> {
    content
        .trim()
        .lines()
        .map(parse_qrc_line)
        .collect()
}

fn parse_qrc_line(line: &str) -> LineInfo {
    let (header, body) = split_line_header(line);
    let (start_time, end_time) = match header {
        Some((start, duration)) => (Some(start), Some(start + duration)),
        None => (None, None),
    };

    let syllables = parse_paren_syllables(body, false);
    let mut info = LineInfo {
        text: syllables
            .iter()
            .map(|syllable| syllable.text.as_str())
            .collect(),
        start_time,
        end_time,
        syllables,
        translation: None,
        is_information_line: false,
    };

    if info.start_time.is_none() {
        info.start_time = info.syllables.first().map(|syllable| syllable.start_time);
        info.end_time = info.syllables.last().map(|syllable| syllable.end_time);
    }
    info
}

/// Splits a `[start,duration]` header from the line body when both fields are integers.
fn split_line_header(line: &str) -> (Option<(i64, i64)>, &str) {
    let Some(close) = line.find(']') else {
        return (None, line);
    };

    let header = line[..close]
        .strip_prefix('[')
        .and_then(parse_time_pair);

    (header, &line[close + 1..])
}

fn parse_time_pair(tag: &str) -> Option<(i64, i64)> {
    let (start, duration) = tag.split_once(',')?;
    Some((start.trim().parse().ok()?, duration.trim().parse().ok()?))
}

/// `paren` form: `(start,duration)text`.
fn parse_paren_syllables(body: &str, absolute_relative_to_line: bool) -> Vec<SyllableInfo> {
    let _ = absolute_relative_to_line;
    let mut syllables: Vec<SyllableInfo> = Vec::new();
    let mut pending = String::new();
    let mut rest = body;

    while let Some(open) = rest.find('(') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(')') else {
            break;
        };

        match parse_time_pair(&after_open[..close]) {
            None => {
                pending.push_str(&rest[..=open]);
                rest = after_open;
                continue;
            }
            Some((start, duration)) => {
                pending.push_str(&rest[..open]);
                syllables.push(SyllableInfo::new(
                    std::mem::take(&mut pending),
                    start,
                    start + duration,
                ));
                rest = &after_open[close + 1..];
            }
        }
    }

    pending.push_str(rest);
    if let Some(last) = syllables.last_mut() {
        last.text.push_str(&pending);
    }

    syllables
}

// ---------------------------------------------------------------------------
// KRC (Kugou)
// ---------------------------------------------------------------------------

/// `KrcParser.ParseLyrics` on an already decrypted payload, including the embedded
/// base64 translation table (`[language:...]`).
pub fn parse_krc(content: &str) -> Vec<LineInfo> {
    let mut lines: Vec<LineInfo> = Vec::new();
    for raw in content.split('\n').map(|line| line.trim_end_matches('\r')) {
        if !raw.starts_with('[') {
            continue;
        }
        if let Some(info) = parse_krc_line(raw) {
            lines.push(info);
        }
    }

    if let Some(translations) = krc_translations(content) {
        for (line, translation) in lines.iter_mut().zip(translations) {
            if !translation.is_empty() && translation != "//" {
                line.translation = Some(translation);
            }
        }
    }

    lines
}

/// `[start,duration]<relStart,duration,0>text<...>text`
pub fn parse_krc_line(line: &str) -> Option<LineInfo> {
    let (header, body) = line.strip_prefix('[')?.split_once(']')?;
    let mut header = header.split(',');
    let line_start: i64 = header.next()?.parse().ok()?;
    let line_end = header
        .next()
        .and_then(|duration| duration.parse::<i64>().ok())
        .map(|duration| line_start + duration);

    let mut chunks = body.split(",0>");
    let first = chunks.next()?.strip_prefix('<')?;
    let mut first = first.split(',');
    let mut start: i64 = first.next()?.parse().ok()?;
    let mut duration: i64 = first.next()?.parse().ok()?;

    let mut syllables: Vec<SyllableInfo> = Vec::new();
    for chunk in chunks {
        let (text, next_time) = match chunk.rsplit_once('<') {
            Some((text, time)) => (text, Some(time)),
            None => (chunk, None),
        };

        syllables.push(SyllableInfo::new(
            text,
            line_start + start,
            line_start + start + duration,
        ));

        if let Some(time) = next_time {
            let mut time = time.split(',');
            start = time.next().and_then(|value| value.parse().ok()).unwrap_or(start);
            duration = time
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(duration);
        }
    }

    let text: String = syllables
        .iter()
        .map(|syllable| syllable.text.as_str())
        .collect();

    Some(LineInfo {
        text,
        start_time: Some(line_start),
        end_time: line_end,
        syllables,
        translation: None,
        is_information_line: false,
    })
}

/// KRC embeds a base64 encoded JSON translation table behind `[language:...]`.
fn krc_translations(content: &str) -> Option<Vec<String>> {
    let start = content.find("[language:")? + "[language:".len();
    let end = content[start..].find(']')? + start;
    let decoded = base64_decode(&content[start..end])?;
    let value: serde_json::Value = serde_json::from_str(&decoded).ok()?;
    let content_items = value.get("content")?.as_array()?;
    let item = content_items
        .iter()
        .find(|item| item.get("type").and_then(|t| t.as_i64()) == Some(1))?;
    let lyric_content = item.get("lyricContent")?.as_array()?;

    Some(
        lyric_content
            .iter()
            .filter_map(|entry| {
                entry
                    .as_array()
                    .and_then(|lines| lines.first())
                    .and_then(|first| first.as_str())
                    .map(|text| text.to_string())
            })
            .collect(),
    )
}

fn base64_decode(value: &str) -> Option<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()?;
    String::from_utf8(bytes).ok()
}

// ---------------------------------------------------------------------------
// YRC (Netease)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum YrcState {
    None,
    Lyric,
    InTimestamp,
    InDuration,
    InWordTimestamp,
    InWordDuration,
    InWordUnknown,
}

/// `YrcParser.Parse` on an already decrypted payload.
///
/// YRC mixes two line kinds: leading/trailing `{...}` credit JSON lines (information
/// lines) and `[lineStart,lineDuration](wordStart,wordDuration,0)text` lyric lines.
pub fn parse_yrc(content: &str) -> Vec<LineInfo> {
    let mut lines: Vec<LineInfo> = Vec::new();
    for raw in content.split('\n') {
        let trimmed = raw.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('{') {
            if let Some(info) = parse_yrc_credits(trimmed) {
                lines.push(info);
            }
            continue;
        }

        if trimmed.starts_with('[') {
            lines.push(parse_yrc_line(trimmed));
        }
    }
    lines
}

fn parse_yrc_credits(line: &str) -> Option<LineInfo> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let timestamp = value.get("t").and_then(|t| t.as_i64()).unwrap_or(0);
    let credits = value.get("c")?.as_array()?;
    let text: String = credits
        .iter()
        .filter_map(|credit| credit.get("tx").and_then(|tx| tx.as_str()))
        .collect();

    if text.is_empty() {
        return None;
    }

    Some(LineInfo {
        text,
        start_time: Some(timestamp),
        end_time: None,
        syllables: Vec::new(),
        translation: None,
        is_information_line: true,
    })
}

fn parse_yrc_line(line: &str) -> LineInfo {
    let (header, body) = split_line_header(line);
    let (line_start, line_end) = match header {
        Some((start, duration)) => (Some(start), Some(start + duration)),
        None => (None, None),
    };

    let mut syllables: Vec<SyllableInfo> = Vec::new();
    let mut state = YrcState::None;
    let mut number: i64 = 0;
    let mut word_start: i64 = 0;
    let mut word_duration: i64 = 0;
    let mut pending_text = String::new();

    let chars: Vec<char> = body.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];

        // State machine mirroring the original: `(`, `,` and `)` are timestamps only
        // when they are followed by a digit, otherwise they are lyric text.
        let next_is_digit = chars
            .get(index + 1)
            .map(|next| next.is_ascii_digit())
            .unwrap_or(false);

        match current {
            '[' | ',' | '(' | ')' => {
                if state == YrcState::Lyric && !next_is_digit {
                    pending_text.push(current);
                    index += 1;
                    continue;
                }

                match state {
                    YrcState::InTimestamp => {
                        word_start = number;
                        number = 0;
                        state = YrcState::InDuration;
                    }
                    YrcState::InWordTimestamp => {
                        word_start = number;
                        number = 0;
                        state = YrcState::InWordDuration;
                    }
                    YrcState::InWordDuration => {
                        word_duration = number;
                        number = 0;
                        state = YrcState::InWordUnknown;
                    }
                    _ => {
                        if current == '(' {
                            // A new word starts: flush whatever text accumulated so far.
                            if !pending_text.is_empty() || !syllables.is_empty() {
                                syllables.push(SyllableInfo::new(
                                    std::mem::take(&mut pending_text),
                                    word_start,
                                    word_start + word_duration,
                                ));
                            }
                            state = YrcState::InWordTimestamp;
                        } else {
                            number = 0;
                            state = YrcState::None;
                        }
                    }
                }
                index += 1;
                continue;
            }
            _ => {}
        }

        match state {
            YrcState::None => {
                if current.is_ascii_digit() {
                    state = YrcState::InTimestamp;
                    number = current.to_digit(10).unwrap() as i64;
                } else {
                    state = YrcState::Lyric;
                    pending_text.push(current);
                }
            }
            YrcState::InTimestamp | YrcState::InDuration => {
                if current.is_ascii_digit() {
                    number = number * 10 + current.to_digit(10).unwrap() as i64;
                } else {
                    state = YrcState::Lyric;
                    pending_text.push(current);
                }
            }
            YrcState::InWordTimestamp | YrcState::InWordDuration | YrcState::InWordUnknown => {
                if current.is_ascii_digit() {
                    number = number * 10 + current.to_digit(10).unwrap() as i64;
                } else {
                    state = YrcState::Lyric;
                    pending_text.push(current);
                }
            }
            YrcState::Lyric => pending_text.push(current),
        }

        index += 1;
    }

    if !pending_text.is_empty() {
        syllables.push(SyllableInfo::new(
            pending_text,
            word_start,
            word_start + word_duration,
        ));
    }

    let text: String = syllables
        .iter()
        .map(|syllable| syllable.text.as_str())
        .collect();

    let start_time = line_start.or_else(|| syllables.first().map(|syllable| syllable.start_time));
    let end_time = line_end.or_else(|| syllables.last().map(|syllable| syllable.end_time));

    LineInfo {
        text,
        start_time,
        end_time,
        syllables,
        translation: None,
        is_information_line: false,
    }
}

// ---------------------------------------------------------------------------
// TTML (Apple Music)
// ---------------------------------------------------------------------------

static TTML_PARAGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<p\b([^>]*)>(.*?)</p\s*>").expect("ttml paragraph regex"));

static TTML_SPAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<span\b([^>]*)>(.*?)</span\s*>").expect("ttml span regex"));

static XML_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z_][\w:.-]*)\s*=\s*"([^"]*)""#).expect("xml attribute regex")
});

static XML_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").expect("xml tag regex"));

/// `TtmlParser.Parse` — a pragmatic subset covering `p`/`span` timing, lyric text and the
/// `information-line` marker the original test fixture uses.
pub fn parse_ttml(content: &str) -> Vec<LineInfo> {
    let mut lines = Vec::new();

    for paragraph in TTML_PARAGRAPH.captures_iter(content) {
        let attributes = parse_attributes(&paragraph[1]);
        let body = &paragraph[2];

        let Some(start) = attribute_time(&attributes, "begin") else {
            continue;
        };
        let end = attribute_time(&attributes, "end").or_else(|| {
            attribute_time(&attributes, "dur").map(|duration| start + duration)
        });

        let mut syllables = Vec::new();
        for span in TTML_SPAN.captures_iter(body) {
            let span_attributes = parse_attributes(&span[1]);
            let span_start = attribute_time(&span_attributes, "begin").unwrap_or(start);
            let span_end = attribute_time(&span_attributes, "end")
                .or_else(|| attribute_time(&span_attributes, "dur").map(|d| span_start + d))
                .unwrap_or(span_end_default(span_start, start));
            let text = strip_tags(&span[2]);
            if !text.is_empty() {
                syllables.push(SyllableInfo::new(text.clone(), span_start, span_end));
            }
        }

        let text = strip_tags(body);
        if text.trim().is_empty() {
            continue;
        }

        let is_information_line = attributes
            .iter()
            .any(|(name, value)| name.ends_with("information-line") && value == "true");

        lines.push(LineInfo {
            text,
            start_time: Some(start),
            end_time: Some(end.unwrap_or(start)),
            syllables,
            translation: None,
            is_information_line,
        });
    }

    lines
}

fn span_end_default(span_start: i64, line_start: i64) -> i64 {
    if span_start > line_start {
        span_start
    } else {
        line_start
    }
}

fn parse_attributes(fragment: &str) -> Vec<(String, String)> {
    XML_ATTRIBUTE
        .captures_iter(fragment)
        .map(|capture| (capture[1].to_string(), capture[2].to_string()))
        .collect()
}

fn attribute_time(attributes: &[(String, String)], name: &str) -> Option<i64> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .and_then(|(_, value)| parse_ttml_time(value))
}

/// Supports `hh:mm:ss.mmm`, `hh:mm:ss:fff` and `Ns` / `Nms` offset times.
pub fn parse_ttml_time(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(rest) = value.strip_suffix("ms") {
        return rest.trim().parse::<f64>().ok().map(|ms| ms.round() as i64);
    }
    if let Some(rest) = value.strip_suffix('s') {
        return rest.trim().parse::<f64>().ok().map(|secs| (secs * 1000.0).round() as i64);
    }

    let normalized = value.replace(',', ".");
    let parts: Vec<&str> = normalized.split(':').collect();
    if parts.len() < 3 {
        return None;
    }

    let hours: i64 = parts[0].parse().ok()?;
    let minutes: i64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;
    let frames: i64 = if parts.len() > 3 {
        // `hh:mm:ss:ff` frame notation. 25 fps is the EBU default TTML uses in practice.
        parts[3].parse::<i64>().ok().map(|frames| frames * 40).unwrap_or(0)
    } else {
        0
    };

    Some(hours * 3_600_000 + minutes * 60_000 + (seconds * 1000.0).round() as i64 + frames)
}

fn strip_tags(fragment: &str) -> String {
    XML_TAG.replace_all(fragment, "").to_string()
}

// ---------------------------------------------------------------------------
// Plain text
// ---------------------------------------------------------------------------

/// Unsynced plain text: every non-empty line, all stamped at zero.
pub fn parse_plain_text(content: &str) -> Vec<LineInfo> {
    content
        .replace('\r', "")
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| LineInfo {
            text: line.to_string(),
            start_time: Some(0),
            end_time: None,
            syllables: Vec::new(),
            translation: None,
            is_information_line: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const QRC_FIXTURE: &str = "[1000,1800]qrc(1000,300)zero(1300,0)word(1300,1500)\n[3000,1000]second(3000,500)";
    const KRC_FIXTURE: &str =
        "[1000,1800]<0,300,0>krc<300,0,0>zero<300,1500,0>word\n[3000,1000]<0,500,0>second";
    const YRC_FIXTURE: &str =
        "[1000,1800](1000,300,0)yrc(1300,0,0)zero(1300,1500,0)word\n[3000,1000](3000,500,0)second";
    const LRC_FIXTURE: &str = "[offset:+250]\n[00:03.00][00:01.00]alpha fixture line\n[00:02.500]\n[xx:yy]malformed fixture line\n[00:04.00]beta fixture line";
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

    #[test]
    fn lrc_expands_timestamps_and_applies_offset() {
        let lines = parse_lrc(LRC_FIXTURE);
        let mut times: Vec<i64> = lines
            .iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| line.start_time.unwrap())
            .collect();
        times.sort();
        assert_eq!(times, vec![1250, 3250, 4250]);
    }

    #[test]
    fn qrc_fixture_keeps_absolute_line_and_segment_timing() {
        let lines = parse_qrc(QRC_FIXTURE);
        let line = lines
            .iter()
            .find(|line| line.start_time == Some(1000))
            .unwrap();
        assert_eq!(line.end_time, Some(2800));
        assert_eq!(line.text, "qrczeroword");
        assert_eq!(line.syllables.len(), 3);
        assert_eq!(line.syllables[0].start_time, 1000);
        assert_eq!(line.syllables[0].end_time, 1300);
        assert_eq!(line.syllables[2].start_time, 1300);
        assert_eq!(line.syllables[2].end_time, 2800);
    }

    #[test]
    fn krc_fixture_keeps_relative_syllable_timing() {
        let lines = parse_krc(KRC_FIXTURE);
        let line = lines
            .iter()
            .find(|line| line.start_time == Some(1000))
            .unwrap();
        assert_eq!(line.end_time, Some(2800));
        assert_eq!(line.syllables[0].text, "krc");
        assert_eq!(line.syllables[1].end_time, line.syllables[1].start_time);
        assert_eq!(line.syllables[2].start_time, 1300);
        assert_eq!(line.syllables[2].end_time, 2800);
    }

    #[test]
    fn krc_rejects_malformed_lines_without_panicking() {
        for line in ["", "]", "[", "[]", "[0,1]", "[x,1]<0,1,0>a", "[0,1]<é"] {
            assert!(parse_krc_line(line).is_none(), "{line:?}");
        }
    }

    #[test]
    fn yrc_fixture_keeps_absolute_word_timing() {
        let lines = parse_yrc(YRC_FIXTURE);
        let line = lines
            .iter()
            .find(|line| line.start_time == Some(1000))
            .unwrap();
        assert_eq!(line.end_time, Some(2800));
        assert_eq!(line.text, "yrczeroword");
        assert_eq!(line.syllables[0].start_time, 1000);
        assert_eq!(line.syllables[0].end_time, 1300);
        assert_eq!(line.syllables[2].start_time, 1300);
        assert_eq!(line.syllables[2].end_time, 2800);
    }

    #[test]
    fn ttml_fixture_keeps_spans_and_information_line() {
        let lines = parse_ttml(TTML_FIXTURE);
        let first = &lines[0];
        assert_eq!(first.start_time, Some(1000));
        assert_eq!(first.end_time, Some(2800));
        assert_eq!(first.syllables.len(), 3);
        assert!(first.text.contains("ttml"));

        let info = lines
            .iter()
            .find(|line| line.text.contains("metadata line"))
            .unwrap();
        assert!(info.is_information_line);
    }

    #[test]
    fn ttml_time_formats() {
        assert_eq!(parse_ttml_time("00:00:01.500"), Some(1500));
        assert_eq!(parse_ttml_time("00:01:02:00"), Some(62_000));
        assert_eq!(parse_ttml_time("1.5s"), Some(1500));
        assert_eq!(parse_ttml_time("250ms"), Some(250));
    }
}

//! `Utilities.LyricLineNormalizer.cs` — merges standalone speaker labels such as
//! `"男："` into the line they introduce.

use std::sync::LazyLock;

use regex::Regex;

use crate::models::LyricLine;

static SINGER_SLOT_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:歌手\s*)?[A-Za-z0-9Ａ-Ｚａ-ｚ０-９一二三四五六七八九十甲乙丙丁]+$")
        .expect("singer slot label regex")
});

const MAX_SPEAKER_LABEL_LEAD_MS: u128 = 1500;

const COMMON_SPEAKER_LABELS: [&str; 17] = [
    "男", "女", "合", "齐", "男声", "女声", "合唱", "齐唱", "独白", "旁白", "主唱", "和声", "说唱",
    "rap", "rapper", "vocal", "vocals",
];

const SECTION_LABELS: [&str; 10] = [
    "intro",
    "outro",
    "verse",
    "chorus",
    "pre-chorus",
    "prechorus",
    "bridge",
    "hook",
    "refrain",
    "interlude",
];

/// `LyricLineNormalizer`.
pub struct LyricLineNormalizer;

impl LyricLineNormalizer {
    pub fn merge_standalone_speaker_labels(lines: &[LyricLine]) -> Vec<LyricLine> {
        let mut sorted: Vec<LyricLine> = lines.to_vec();
        sorted.sort_by_key(|line| line.timestamp);

        if sorted.len() < 2 {
            return sorted;
        }

        let mut merged: Vec<LyricLine> = Vec::with_capacity(sorted.len());
        for index in 0..sorted.len() {
            let current = sorted[index].clone();
            let label = match try_get_standalone_speaker_label(&current.text) {
                Some(label) => label,
                None => {
                    merged.push(current);
                    continue;
                }
            };

            match try_find_merge_target(&sorted, index) {
                Some(target_index) => {
                    let target = sorted[target_index].clone();
                    let mut updated = target;
                    updated.text = format!("{label}{}", updated.text.trim_start());
                    updated.syllables = None;
                    sorted[target_index] = updated;
                }
                None => merged.push(current),
            }
        }

        merged
    }
}

fn try_find_merge_target(lines: &[LyricLine], speaker_index: usize) -> Option<usize> {
    let speaker = &lines[speaker_index];
    for (offset, candidate) in lines.iter().enumerate().skip(speaker_index + 1) {
        let lead = candidate
            .timestamp
            .saturating_sub(speaker.timestamp)
            .as_millis();
        if lead > MAX_SPEAKER_LABEL_LEAD_MS {
            break;
        }
        if try_get_standalone_speaker_label(&candidate.text).is_some() {
            continue;
        }
        return Some(offset);
    }
    None
}

fn try_get_standalone_speaker_label(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let without_colon = trimmed
        .strip_suffix(':')
        .or_else(|| trimmed.strip_suffix('：'))?;

    let name: String = without_colon
        .trim()
        .trim_matches(|c: char| {
            matches!(c, '[' | ']' | '(' | ')' | '（' | '）' | '【' | '】' | '「' | '」')
        })
        .to_string();

    let name_len = name.chars().count();
    if !(1..=16).contains(&name_len) {
        return None;
    }

    if SECTION_LABELS
        .iter()
        .any(|label| name.eq_ignore_ascii_case(label))
    {
        return None;
    }

    let is_common = COMMON_SPEAKER_LABELS
        .iter()
        .any(|label| name.eq_ignore_ascii_case(label));
    let has_speaker_hint = name_len <= 8
        && ["男", "女", "合", "唱"]
            .iter()
            .any(|keyword| name.to_lowercase().contains(&keyword.to_lowercase()));
    let matches_slot = SINGER_SLOT_LABEL.is_match(&name);

    if is_common || has_speaker_hint || matches_slot {
        Some(format!("{name}："))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ms, secs};

    #[test]
    fn standalone_speaker_label_is_folded_into_the_next_line() {
        let lines = vec![
            LyricLine {
                timestamp: secs(1),
                text: "女：".to_string(),
                translation: None,
                syllables: None,
            },
            LyricLine {
                timestamp: ms(1200),
                text: "第一句".to_string(),
                translation: None,
                syllables: None,
            },
            LyricLine {
                timestamp: secs(5),
                text: "第二句".to_string(),
                translation: None,
                syllables: None,
            },
        ];

        let merged = LyricLineNormalizer::merge_standalone_speaker_labels(&lines);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].text, "女：第一句");
        assert_eq!(merged[1].text, "第二句");
    }

    #[test]
    fn section_labels_and_far_away_labels_are_left_alone() {
        let lines = vec![
            LyricLine {
                timestamp: secs(1),
                text: "Chorus:".to_string(),
                translation: None,
                syllables: None,
            },
            LyricLine {
                timestamp: secs(9),
                text: "正文".to_string(),
                translation: None,
                syllables: None,
            },
        ];

        let merged = LyricLineNormalizer::merge_standalone_speaker_labels(&lines);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].text, "Chorus:");
    }
}

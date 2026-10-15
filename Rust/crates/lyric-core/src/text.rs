//! Text utilities ported from `Utilities.ChineseScriptConverter.cs`,
//! `Utilities.LyricMatcher.cs` (normalization half) and the Jaro-Winkler
//! similarity helper that the original consumed from `F23.StringSimilarity`.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

// ---------------------------------------------------------------------------
// Chinese script conversion
// ---------------------------------------------------------------------------

/// Traditional → Simplified fallback table.
///
/// The C# original cascades three sources: the OpenCC `T2cn` preset, the Win32
/// `LCMapStringEx(LCMAP_SIMPLIFIED_CHINESE)` API and finally a 29 entry fallback map.
/// A Rust rebuild cannot take either platform dependency:
///
/// * `OpenCC.Presets.T2cn` pulls in the native OpenCC library, and the pure-Rust
///   OpenCC re-implementations all depend on `getrandom`/`windows-sys`, which cannot be
///   linked by the toolchain used here (no usable `dlltool`).
/// * `LCMapStringEx` is a Win32 API; the port is dependency-free and cross-platform.
///
/// So the port ships a larger built-in table that covers the characters that actually
/// show up in song metadata. Characters outside the table pass through unchanged, which
/// is exactly what the original does when OpenCC and the Win32 API are both unavailable.
const TRADITIONAL_TO_SIMPLIFIED: &[(char, char)] = &[
    ('後', '后'), ('愛', '爱'), ('錯', '错'), ('過', '过'), ('靜', '静'), ('倫', '伦'),
    ('聽', '听'), ('國', '国'), ('體', '体'), ('語', '语'), ('夢', '梦'), ('時', '时'),
    ('間', '间'), ('歡', '欢'), ('與', '与'), ('為', '为'), ('會', '会'), ('來', '来'),
    ('見', '见'), ('這', '这'), ('樣', '样'), ('點', '点'), ('聲', '声'), ('樂', '乐'),
    ('別', '别'), ('裡', '里'), ('臺', '台'), ('妳', '你'), ('們', '们'), ('個', '个'),
    ('說', '说'), ('話', '话'), ('沒', '没'), ('對', '对'), ('開', '开'), ('關', '关'),
    ('電', '电'), ('車', '车'), ('東', '东'), ('馬', '马'), ('鳥', '鸟'), ('魚', '鱼'),
    ('龍', '龙'), ('鳳', '凤'), ('華', '华'), ('雲', '云'), ('霧', '雾'), ('風', '风'),
    ('陽', '阳'), ('陰', '阴'), ('銀', '银'), ('錢', '钱'), ('鐵', '铁'), ('銅', '铜'),
    ('鋼', '钢'), ('針', '针'), ('門', '门'), ('問', '问'), ('聞', '闻'), ('閉', '闭'),
    ('閒', '闲'), ('閃', '闪'), ('長', '长'), ('張', '张'), ('帳', '帐'), ('寫', '写'),
    ('讀', '读'), ('書', '书'), ('畫', '画'), ('圖', '图'), ('課', '课'), ('詞', '词'),
    ('詩', '诗'), ('誰', '谁'), ('認', '认'), ('識', '识'), ('請', '请'), ('謝', '谢'),
    ('讓', '让'), ('記', '记'), ('許', '许'), ('論', '论'), ('講', '讲'), ('誤', '误'),
    ('譯', '译'), ('戀', '恋'), ('變', '变'), ('邊', '边'), ('還', '还'), ('進', '进'),
    ('遠', '远'), ('運', '运'), ('連', '连'), ('違', '违'), ('遺', '遗'), ('選', '选'),
    ('適', '适'), ('遷', '迁'), ('達', '达'), ('邁', '迈'), ('遲', '迟'), ('現', '现'),
    ('觀', '观'), ('覺', '觉'), ('規', '规'), ('視', '视'), ('親', '亲'), ('覽', '览'),
    ('訂', '订'), ('計', '计'), ('討', '讨'), ('訊', '讯'), ('訪', '访'), ('試', '试'),
    ('該', '该'), ('詳', '详'), ('誌', '志'), ('調', '调'), ('談', '谈'), ('諾', '诺'),
    ('證', '证'), ('議', '议'), ('護', '护'), ('讚', '赞'), ('額', '额'), ('願', '愿'),
    ('顧', '顾'), ('顯', '显'), ('飛', '飞'), ('飯', '饭'), ('飼', '饲'), ('飽', '饱'),
    ('飾', '饰'), ('餅', '饼'), ('養', '养'), ('館', '馆'), ('駐', '驻'), ('駛', '驶'),
    ('駕', '驾'), ('騎', '骑'), ('驚', '惊'), ('髮', '发'), ('鬥', '斗'), ('麗', '丽'),
    ('黃', '黄'), ('黨', '党'), ('齊', '齐'), ('齒', '齿'), ('於', '于'), ('應', '应'),
    ('麼', '么'), ('幾', '几'), ('內', '内'), ('兩', '两'), ('從', '从'), ('眾', '众'),
    ('優', '优'), ('傳', '传'), ('價', '价'), ('億', '亿'), ('儀', '仪'), ('兒', '儿'),
    ('冊', '册'), ('沖', '冲'), ('決', '决'), ('況', '况'), ('淨', '净'), ('凍', '冻'),
    ('劃', '划'), ('剛', '刚'), ('創', '创'), ('劇', '剧'), ('劉', '刘'), ('動', '动'),
    ('務', '务'), ('勝', '胜'), ('勞', '劳'), ('勢', '势'), ('勵', '励'), ('勸', '劝'),
    ('區', '区'), ('醫', '医'), ('協', '协'), ('單', '单'), ('賣', '卖'), ('買', '买'),
    ('費', '费'), ('資', '资'), ('質', '质'), ('財', '财'), ('貴', '贵'), ('購', '购'),
    ('貨', '货'), ('貼', '贴'), ('貸', '贷'), ('賠', '赔'), ('賺', '赚'), ('贏', '赢'),
    ('賦', '赋'), ('賬', '账'), ('賞', '赏'), ('贈', '赠'), ('趕', '赶'), ('載', '载'),
    ('輔', '辅'), ('輕', '轻'), ('輛', '辆'), ('輪', '轮'), ('轉', '转'), ('輝', '辉'),
    ('輩', '辈'), ('輯', '辑'), ('輸', '输'), ('辭', '辞'), ('農', '农'), ('遼', '辽'),
    ('郵', '邮'), ('鄉', '乡'), ('釀', '酿'), ('釋', '释'), ('鐘', '钟'), ('錄', '录'),
    ('鍋', '锅'), ('鎮', '镇'), ('鏡', '镜'), ('陣', '阵'), ('陳', '陈'), ('陸', '陆'),
    ('隊', '队'), ('隨', '随'), ('隱', '隐'), ('難', '难'), ('靈', '灵'), ('響', '响'),
    ('頻', '频'), ('題', '题'), ('顏', '颜'), ('類', '类'), ('飲', '饮'), ('龜', '龟'),
    ('聖', '圣'), ('聯', '联'), ('職', '职'), ('聰', '聪'), ('腫', '肿'), ('臉', '脸'),
    ('臨', '临'), ('舉', '举'), ('舊', '旧'), ('艷', '艳'), ('蘇', '苏'), ('藝', '艺'),
    ('處', '处'), ('號', '号'), ('蟲', '虫'), ('衝', '冲'), ('補', '补'), ('裝', '装'),
    ('製', '制'), ('複', '复'), ('訓', '训'), ('設', '设'), ('訴', '诉'), ('診', '诊'),
    ('註', '注'), ('評', '评'), ('誠', '诚'), ('貝', '贝'), ('頁', '页'), ('麥', '麦'),
    ('亂', '乱'), ('爭', '争'), ('亞', '亚'), ('產', '产'), ('眾', '众'), ('術', '术'),
    ('葉', '叶'), ('蘭', '兰'), ('憶', '忆'), ('驚', '惊'), ('獻', '献'), ('純', '纯'),
    ('絕', '绝'), ('絲', '丝'), ('織', '织'), ('縫', '缝'), ('紅', '红'), ('綠', '绿'),
    ('藍', '蓝'), ('紫', '紫'), ('夢', '梦'), ('顆', '颗'), ('暫', '暂'), ('樸', '朴'),
    ('機', '机'), ('構', '构'), ('檢', '检'), ('權', '权'), ('歡', '欢'), ('殘', '残'),
    ('歸', '归'), ('歲', '岁'), ('歷', '历'), ('羅', '罗'), ('罰', '罚'), ('纖', '纤'),
    ('續', '续'), ('罷', '罢'), ('聽', '听'), ('膚', '肤'), ('臉', '脸'), ('臨', '临'),
    ('舉', '举'), ('藥', '药'), ('虛', '虚'), ('號', '号'), ('覽', '览'), ('贈', '赠'),
    ('跡', '迹'), ('踐', '践'), ('蹤', '踪'), ('躍', '跃'), ('軟', '软'), ('較', '较'),
    ('軋', '轧'), ('輝', '辉'), ('輩', '辈'), ('辭', '辞'), ('農', '农'), ('遞', '递'),
    ('遲', '迟'), ('鄰', '邻'), ('醫', '医'), ('釘', '钉'), ('鈔', '钞'), ('鉤', '钩'),
    ('銳', '锐'), ('鋒', '锋'), ('鋪', '铺'), ('鏈', '链'), ('鎖', '锁'), ('鏡', '镜'),
    ('鑽', '钻'), ('閉', '闭'), ('閏', '闰'), ('閱', '阅'), ('關', '关'), ('陣', '阵'),
    ('陝', '陕'), ('險', '险'), ('隱', '隐'), ('雖', '虽'), ('雙', '双'), ('雜', '杂'),
    ('雞', '鸡'), ('離', '离'), ('電', '电'), ('靜', '静'), ('頁', '页'), ('顧', '顾'),
    ('願', '愿'), ('顫', '颤'), ('風', '风'), ('飄', '飘'), ('飯', '饭'), ('館', '馆'),
];

/// Simplified-conversion façade matching `ChineseScriptConverter.ToSimplified`.
///
/// Blank input is returned untouched, exactly like the original early return.
pub fn to_simplified(text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }

    if text
        .chars()
        .any(|ch| TRADITIONAL_TO_SIMPLIFIED.iter().any(|(trad, _)| *trad == ch))
    {
        text.chars()
            .map(|ch| {
                TRADITIONAL_TO_SIMPLIFIED
                    .iter()
                    .find(|(trad, _)| *trad == ch)
                    .map(|(_, simplified)| *simplified)
                    .unwrap_or(ch)
            })
            .collect()
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// Diacritics
// ---------------------------------------------------------------------------

/// `LyricMatcher.RemoveDiacritics` — FormD split, drop non-spacing marks, FormC merge.
pub fn remove_diacritics(text: &str) -> String {
    let decomposed: String = text.nfd().collect();
    let stripped: String = decomposed
        .chars()
        .filter(|ch| !is_non_spacing_mark(*ch))
        .collect();
    stripped.nfc().collect()
}

fn is_non_spacing_mark(ch: char) -> bool {
    use unicode_normalization::char::is_combining_mark;
    // The C# filter used `UnicodeCategory.NonSpacingMark`; combining marks are the
    // closest equivalent available without shipping a Unicode category table.
    is_combining_mark(ch)
}

// ---------------------------------------------------------------------------
// Version markers
// ---------------------------------------------------------------------------

/// `LyricMatcher.ConflictKeywords` — deliberately does *not* contain the bracket
/// metadata terms (`deluxe`, `remastered`, `edit`, ...); those are stripped as noise
/// instead of being treated as a version conflict.
pub const CONFLICT_KEYWORDS: [&str; 14] = [
    "live",
    "remix",
    "acoustic",
    "demo",
    "instrumental",
    "karaoke",
    "simlish",
    "from",
    "vma",
    "award",
    "现场",
    "演唱会",
    "颁奖",
    "典礼",
];

/// `LyricMatcher.ContainsContentVersionMarker`.
pub fn contains_content_version_marker(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }
    CONFLICT_KEYWORDS
        .iter()
        .any(|keyword| value.to_lowercase().contains(&keyword.to_lowercase()))
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

static BRACKET_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*[\(\[\{（【].*?[\)\]\}）】]\s*").expect("bracket suffix regex")
});

static FEATURE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(feat\.?|ft\.?|with)\s+.*$").expect("feature suffix regex")
});

static NOISE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\s*[\(\[（【](explicit|deluxe|digital|premium|album|edit|version|special|anniversary|studio|remastered)[\)\]）】]\s*",
    )
    .expect("noise tag regex")
});

static WHITESPACE_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("whitespace regex"));

/// `LyricMatcher.NormalizeForSearch` — bracketed content is always dropped.
pub fn normalize_for_search(value: &str) -> String {
    normalize(value, false)
}

/// `LyricMatcher.NormalizeTitleForComparison` — a bracketed *version* marker survives,
/// so `"Anti-Hero - ILLENIUM Remix"` and `"Anti-Hero (Illenium Remix)"` compare equal.
pub fn normalize_title_for_comparison(value: &str) -> String {
    normalize(value, true)
}

fn normalize(value: &str, preserve_version_markers: bool) -> String {
    if value.trim().is_empty() {
        return String::new();
    }

    let simplified = to_simplified(value).to_lowercase();
    let without_diacritics = remove_diacritics(&simplified);
    let no_noise = NOISE_TAG.replace_all(&without_diacritics, " ");

    let pure_title = BRACKET_SUFFIX.replace_all(&no_noise, |caps: &regex::Captures<'_>| {
        if preserve_version_markers && contains_content_version_marker(&caps[0]) {
            let content = caps[0].trim().trim_matches(|c: char| {
                matches!(c, '(' | '[' | '{' | '（' | '【' | ')' | ']' | '}' | '）' | '】')
            });
            format!(" {content} ")
        } else {
            " ".to_string()
        }
    });

    let no_features = FEATURE_SUFFIX.replace_all(&pure_title, "");

    let mapped: String = no_features
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect();

    let collapsed = WHITESPACE_RUN.replace_all(&mapped, " ").trim().to_string();
    collapse_spaced_character_runs(&collapsed)
}

/// Quote-music style artist styling (`"R I T U A L"`) is folded back into one token so
/// that Jaro-Winkler and token overlap both see the artist as written.
pub fn collapse_spaced_character_runs(value: &str) -> String {
    let mut collapsed: Vec<String> = Vec::new();
    let mut run = String::new();
    for token in value.split(' ') {
        if token.chars().count() != 1 {
            if !run.is_empty() {
                collapsed.push(std::mem::take(&mut run));
            }
            collapsed.push(token.to_string());
            continue;
        }
        run.push_str(token);
    }
    if !run.is_empty() {
        collapsed.push(run);
    }
    collapsed.join(" ")
}

// ---------------------------------------------------------------------------
// Script classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextScriptKind {
    Latin,
    Cjk,
}

fn letter_script(ch: char) -> Option<TextScriptKind> {
    match ch {
        'A'..='Z' | 'a'..='z' | '\u{00C0}'..='\u{024F}' => Some(TextScriptKind::Latin),
        '\u{3040}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{F900}'..='\u{FAFF}' => Some(TextScriptKind::Cjk),
        _ => None,
    }
}

fn try_get_pure_script(value: &str) -> Option<TextScriptKind> {
    let mut found: Option<TextScriptKind> = None;
    for ch in value.chars() {
        if !ch.is_alphabetic() {
            continue;
        }
        let script = letter_script(ch)?;
        match found {
            None => found = Some(script),
            Some(existing) if existing != script => return None,
            Some(_) => {}
        }
    }
    found
}

/// `LyricMatcher.IsCrossScript` — both sides must be *purely* one script and differ.
pub fn is_cross_script(left: &str, right: &str) -> bool {
    match (try_get_pure_script(left), try_get_pure_script(right)) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Jaro-Winkler
// ---------------------------------------------------------------------------

/// Jaro-Winkler with the exact defaults used by `F23.StringSimilarity`:
/// boost threshold `0.7`, prefix scaling `0.1`, prefix capped at 4 characters.
pub struct JaroWinkler {
    threshold: f64,
    scaling: f64,
}

impl Default for JaroWinkler {
    fn default() -> Self {
        Self {
            threshold: 0.7,
            scaling: 0.1,
        }
    }
}

impl JaroWinkler {
    pub fn new(threshold: f64, scaling: f64) -> Self {
        Self { threshold, scaling }
    }

    pub fn similarity(&self, s1: &str, s2: &str) -> f64 {
        if s1 == s2 {
            return 1.0;
        }
        let a: Vec<char> = s1.chars().collect();
        let b: Vec<char> = s2.chars().collect();
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        let jaro = jaro(&a, &b);
        if jaro <= self.threshold {
            return jaro;
        }

        let prefix = a
            .iter()
            .zip(b.iter())
            .take(4)
            .take_while(|(left, right)| left == right)
            .count();

        jaro + prefix as f64 * self.scaling * (1.0 - jaro)
    }
}

fn jaro(a: &[char], b: &[char]) -> f64 {
    let window = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_matched = vec![false; a.len()];
    let mut b_matched = vec![false; b.len()];
    let mut matches = 0usize;

    for i in 0..a.len() {
        let start = i.saturating_sub(window);
        let end = (i + window + 1).min(b.len());
        for j in start..end {
            if !b_matched[j] && a[i] == b[j] {
                a_matched[i] = true;
                b_matched[j] = true;
                matches += 1;
                break;
            }
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut transpositions = 0usize;
    let mut cursor = 0usize;
    for i in 0..a.len() {
        if !a_matched[i] {
            continue;
        }
        while !b_matched[cursor] {
            cursor += 1;
        }
        if a[i] != b[cursor] {
            transpositions += 1;
        }
        cursor += 1;
    }

    let m = matches as f64;
    let t = transpositions as f64 / 2.0;
    (m / a.len() as f64 + m / b.len() as f64 + (m - t) / m) / 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn des_style_known_jaro_winkler_pairs() {
        let jw = JaroWinkler::default();
        assert_eq!(jw.similarity("m83", "m83"), 1.0);
        assert_eq!(jw.similarity("", "abc"), 0.0);
        assert!(jw.similarity("martha", "marhta") > 0.96);
    }

    #[test]
    fn simplified_conversion_keeps_untabled_text_intact() {
        assert_eq!(to_simplified("愛與夢"), "爱与梦");
        assert_eq!(to_simplified("plain ascii"), "plain ascii");
        assert_eq!(to_simplified("   "), "   ");
    }

    #[test]
    fn diacritics_are_stripped() {
        assert_eq!(remove_diacritics("Café déjà vu"), "Cafe deja vu");
        assert_eq!(normalize_for_search("Café, déjà vu!"), "cafe deja vu");
    }

    #[test]
    fn spaced_character_runs_collapse() {
        assert_eq!(
            normalize_for_search("R I T U A L / Tove Styrke"),
            "ritual tove styrke"
        );
    }

    #[test]
    fn version_markers_survive_title_normalization_but_not_search_normalization() {
        assert_eq!(
            normalize_title_for_comparison("Anti-Hero (Illenium Remix)"),
            "anti hero illenium remix"
        );
        assert_eq!(normalize_for_search("Anti-Hero (Illenium Remix)"), "anti hero");
    }

    #[test]
    fn cross_script_detection_matches_the_original_rules() {
        assert!(is_cross_script("魚韻", "sakanaction"));
        assert!(is_cross_script("サカナクション", "sakanaction"));
        assert!(!is_cross_script("X玖少年团", "sakanaction"));
        assert!(!is_cross_script("m83", "coldplay"));
    }
}

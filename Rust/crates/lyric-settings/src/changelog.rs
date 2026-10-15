//! Release-note parsing, kept free of any UI types.
//!
//! The history page renders whatever this returns; splitting the two keeps the parsing testable
//! without dragging gpui's element macros into a test harness (their expansion is deep enough to
//! hit the crate's recursion limit).
use std::sync::OnceLock;

const CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

/// One version: display title, date, and the flattened body.
pub struct Entry {
    pub title: String,
    pub date: String,
    pub body: String,
}

/// Strip markdown emphasis and turn bullets into a printable list item.
pub fn plain(line: &str) -> String {
    let line = line.trim_start_matches('#').trim().replace("**", "").replace('`', "");
    if let Some(text) = line.strip_prefix("- ") {
        format!("• {text}")
    } else {
        line.trim_start_matches("> ").to_string()
    }
}

/// Every version in the changelog, in file order (newest first).
pub fn entries() -> &'static Vec<Entry> {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut entries = Vec::new();
        let mut heading = String::new();
        let mut body: Vec<String> = Vec::new();
        fn flush(entries: &mut Vec<Entry>, heading: &str, body: &mut Vec<String>) {
            if !heading.is_empty() {
                let (version, date) = heading.split_once('·').unwrap_or((heading, ""));
                entries.push(Entry {
                    title: format!("更新日志 · {}", version.trim()),
                    date: date.trim().to_owned(),
                    body: body.join("\n").trim().to_owned(),
                });
            }
            body.clear();
        }
        for line in CHANGELOG.lines() {
            let text = line.trim_start_matches('#').trim();
            let is_version_header = line.starts_with('#')
                && (text.starts_with("当前开发版") || text.chars().next().is_some_and(|c| c.is_ascii_digit()));
            if is_version_header {
                flush(&mut entries, &heading, &mut body);
                heading = text.to_owned();
            } else if !heading.is_empty() {
                body.push(plain(line));
            }
        }
        flush(&mut entries, &heading, &mut body);
        entries
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_version_heading_becomes_one_entry() {
        let entries = entries();
        assert!(entries.len() > 5, "expected several versions, got {}", entries.len());
        assert!(
            entries.iter().all(|entry| entry.title.starts_with("更新日志 · ")),
            "entries must be titled consistently"
        );
        assert!(
            entries.iter().any(|entry| entry.title.contains("1.0")),
            "the current release must appear in the history"
        );
    }

    #[test]
    fn markdown_is_reduced_to_plain_text() {
        assert_eq!(plain("- **bold** and `code`"), "• bold and code");
        assert_eq!(plain("> quoted"), "quoted");
        assert_eq!(plain("### 标题"), "标题");
    }

    #[test]
    fn bodies_are_not_empty_for_real_versions() {
        let with_body = entries().iter().filter(|entry| !entry.body.is_empty()).count();
        assert!(with_body >= 5, "only {with_body} entries carry a body");
    }
}

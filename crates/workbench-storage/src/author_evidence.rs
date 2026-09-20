//! Exact author-credit components, shared by the native discovery projection.
//! Mirrors the UI's author-evidence.ts; never guesses from a title or substring.
use crate::DiscoveryRecord;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

fn whitespace(c: char) -> bool {
    (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}'
}

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .to_lowercase()
        .split(whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Default)]
struct Parts {
    names: HashSet<String>,
    members: HashSet<String>,
}

impl Parts {
    fn add(&mut self, part: &str, nested: bool) {
        let part = part.trim_matches(whitespace);
        if part.is_empty() || part.contains("..") || part.contains('…') {
            return;
        }
        self.names.insert(part.to_owned());
        if nested {
            self.members.insert(part.to_owned());
        }
    }

    fn add_list(&mut self, value: &str, nested: bool) {
        let chars: Vec<char> = value.chars().collect();
        let mut part = String::new();
        for (index, c) in chars.iter().copied().enumerate() {
            let delimiter = matches!(c, '、' | ',' | ';')
                || (matches!(c, '&' | '×' | '/')
                    && index > 0
                    && index + 1 < chars.len()
                    && whitespace(chars[index - 1])
                    && whitespace(chars[index + 1]));
            if delimiter {
                self.add(&part, nested);
                part.clear();
            } else {
                part.push(c);
            }
        }
        self.add(&part, nested);
    }

    fn collect(&mut self, text: &str, nested: bool) {
        self.add(text, nested);
        let mut part = String::new();
        let mut group = String::new();
        let mut depth = 0;
        for c in text.chars() {
            if c == '(' {
                if depth == 0 {
                    self.add_list(&part, nested);
                    part.clear();
                    group.clear();
                } else {
                    group.push(c);
                }
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    self.collect(&group, true);
                } else {
                    group.push(c);
                }
            } else if depth > 0 {
                group.push(c);
            } else {
                part.push(c);
            }
        }
        self.add_list(&part, nested);
    }
}

fn name_parts(value: &str) -> Parts {
    let value = normalize(value);
    let mut parts = Parts::default();
    parts.add(&value, false);
    let paired: String = value
        .chars()
        .map(|c| match c {
            '[' | '【' | '「' | '『' => '(',
            ']' | '】' | '」' | '』' => ')',
            _ => c,
        })
        .collect();
    let mut depth = 0i32;
    for c in paired.chars() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return parts;
            }
        }
    }
    if depth == 0 {
        parts.collect(&paired, false);
    }
    parts
}

fn name_matches(query: &str, credit: &str) -> bool {
    let expected = normalize(query);
    if expected.is_empty() {
        return false;
    }
    let candidate = name_parts(credit);
    candidate.names.contains(&expected)
        || name_parts(query)
            .members
            .iter()
            .any(|member| candidate.names.contains(member))
}

/// Query association and the historical author_verified flag are not authorship.
pub fn discovery_record_matches_author(record: &DiscoveryRecord) -> bool {
    record.matched_authors.iter().any(|query| {
        record
            .work
            .authors
            .iter()
            .any(|credit| name_matches(query, credit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiscoveryWork, Source};

    #[test]
    fn native_author_credit_matches_ui_components_without_title_guessing() {
        for (query, credit, expected) in [
            ("Writer", "Ｃｉｒｃｌｅ（ＷＲＩＴＥＲ）", true),
            ("Writer", "Circle [Writer, Coauthor]", true),
            ("Coauthor", "Writer & Coauthor", true),
            ("Coauthor", "Writer/Coauthor", false),
            ("Writer", "Circle (Writer", false),
            ("Writer", "Circle (Writer...)", false),
            ("Writer", "Writer Two", false),
            ("Circle (Writer)", "Writer", true),
            ("Circle (Writer)", "Circle", false),
            ("が", "か\u{3099}", true),
            ("が", "か", false),
            ("ος", "ΟΣ", true),
            ("Author Name", " Author\u{3000} Name ", true),
            ("", "Writer", false),
        ] {
            assert_eq!(name_matches(query, credit), expected, "{query} / {credit}");
        }
    }

    #[test]
    fn cold_classification_uses_credits_not_legacy_flags_or_keyword_titles() {
        let mut record = DiscoveryRecord {
            work: DiscoveryWork {
                source: Source::Jm,
                work_id: "123456".into(),
                title: "Writer in a keyword-only title".into(),
                authors: vec!["Other Writer".into()],
                description: None,
                tags: vec!["Writer".into()],
                favorite: None,
                chapter_count: None,
                page_count: None,
                source_updated_at: None,
                cover_available: false,
            },
            matched_authors: vec!["Writer".into()],
            author_verified: true,
            observed_at: 1,
            scan_id: "a".repeat(64),
        };
        assert!(!discovery_record_matches_author(&record));
        record.work.authors = vec!["Circle (Writer)".into()];
        record.author_verified = false;
        assert!(discovery_record_matches_author(&record));
        record.work.authors.clear();
        assert!(!discovery_record_matches_author(&record));
    }
}

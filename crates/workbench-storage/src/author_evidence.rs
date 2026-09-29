//! Exact author-credit components, shared by the native discovery projection.
//! Mirrors the UI's author-evidence.ts; never guesses from a title or substring.
use crate::DiscoveryRecord;
use std::collections::{BTreeSet, HashMap, HashSet};
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

pub(crate) fn normalized_author_credit(value: &str) -> String {
    normalize(value)
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

pub fn author_credit_matches(query: &str, credit: &str) -> bool {
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

/// An indexed form of the exact component rules above. Query provenance never
/// constrains authorship: a work found through another author can still belong
/// to any currently followed, explicitly credited author.
#[derive(Clone, Debug, Default)]
pub struct AuthorCreditIndex {
    components: HashMap<String, BTreeSet<String>>,
    exact: HashMap<String, BTreeSet<String>>,
}

impl AuthorCreditIndex {
    pub fn insert(&mut self, author: &str, aliases: &[String], exact_credits: &[String]) {
        for name in std::iter::once(author).chain(aliases.iter().map(String::as_str)) {
            let expected = normalize(name);
            if !expected.is_empty() {
                self.components
                    .entry(expected)
                    .or_default()
                    .insert(author.to_owned());
                for member in name_parts(name).members {
                    self.components
                        .entry(member)
                        .or_default()
                        .insert(author.to_owned());
                }
            }
        }
        for credit in exact_credits {
            self.exact
                .entry(normalize(credit))
                .or_default()
                .insert(author.to_owned());
        }
    }

    pub fn matching_authors(&self, credits: &[String]) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        for credit in credits {
            if let Some(authors) = self.exact.get(&normalize(credit)) {
                result.extend(authors.iter().cloned());
            }
            for component in name_parts(credit).names {
                if let Some(authors) = self.components.get(&component) {
                    result.extend(authors.iter().cloned());
                }
            }
        }
        result
    }
}

/// Query association and the historical author_verified flag are not authorship.
pub fn discovery_record_matches_author(record: &DiscoveryRecord) -> bool {
    record.matched_authors.iter().any(|query| {
        record
            .work
            .authors
            .iter()
            .any(|credit| author_credit_matches(query, credit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiscoveryWork, Source};

    #[test]
    fn indexed_followed_credits_equal_existing_exact_component_rules() {
        let authors = [
            "Writer",
            "Circle (Writer)",
            "Coauthor",
            "が",
            "P",
            "Author Name",
        ];
        let mut index = AuthorCreditIndex::default();
        for author in authors {
            index.insert(author, &[], &[]);
        }
        for credit in [
            "Ｃｉｒｃｌｅ（ＷＲＩＴＥＲ）",
            "Circle [Writer, Coauthor]",
            "Writer & Coauthor",
            "Writer/Coauthor",
            "Circle (Writer",
            "Circle (Writer...)",
            "Writer Two",
            "Writer",
            "Circle",
            "か\u{3099}",
            "か",
            "Author\u{3000} Name",
            "P",
            "Painter",
        ] {
            let expected: BTreeSet<_> = authors
                .iter()
                .filter(|author| author_credit_matches(author, credit))
                .map(|author| (*author).to_owned())
                .collect();
            assert_eq!(
                index.matching_authors(&[credit.to_owned()]),
                expected,
                "{credit}"
            );
        }
        index.insert("Verified", &["Alias".into()], &["Combined Credit".into()]);
        assert!(index
            .matching_authors(&["Circle (Alias)".into()])
            .contains("Verified"));
        assert!(index
            .matching_authors(&["Combined Credit".into()])
            .contains("Verified"));
        assert!(!index
            .matching_authors(&["Combined Credit Extra".into()])
            .contains("Verified"));
    }

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
            assert_eq!(
                author_credit_matches(query, credit),
                expected,
                "{query} / {credit}"
            );
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
            metadata_detail_at: None,
            scan_id: "a".repeat(64),
            first_discovered_run_id: None,
        };
        assert!(!discovery_record_matches_author(&record));
        record.work.authors = vec!["Circle (Writer)".into()];
        record.author_verified = false;
        assert!(discovery_record_matches_author(&record));
        record.work.authors.clear();
        assert!(!discovery_record_matches_author(&record));
    }
}

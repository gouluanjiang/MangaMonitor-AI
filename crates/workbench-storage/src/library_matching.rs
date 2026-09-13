//! Deterministic identity proposals. No fuzzy title is positive ownership evidence.
use crate::{LibraryItem, LibraryItemState, LibraryReference};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryMatchWork {
    #[serde(flatten)]
    pub reference: LibraryReference,
    pub title: String,
    pub authors: Vec<String>,
    pub page_count: Option<u64>,
}

pub fn matching_text(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .filter(|v| !v.is_whitespace())
        .collect()
}

fn real_author(value: &str) -> bool {
    !value.is_empty()
        && ![
            "unknown",
            "未知",
            "佚名",
            "无名",
            "作者未知",
            "多人",
            "多人合集",
        ]
        .contains(&value)
}

/// Preserve edition, language and volume suffixes; only known creator/event prefixes go.
pub fn matching_title(value: &str, authors: &[String], candidate: bool) -> String {
    let mut value = matching_text(value);
    for suffix in [".zip", ".cbz"] {
        if let Some(stem) = value.strip_suffix(suffix) {
            value = stem.to_owned();
            break;
        }
    }
    loop {
        if let Some(rest) = value.strip_prefix("(c").and_then(|v| v.split_once(')')) {
            if (2..=3).contains(&rest.0.len()) && rest.0.bytes().all(|v| v.is_ascii_digit()) {
                value = rest.1.to_owned();
                continue;
            }
        }
        let Some((prefix, rest)) = value.strip_prefix('[').and_then(|v| v.split_once(']')) else {
            break;
        };
        let source_prefix = prefix
            .strip_prefix("jm")
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|v| v.is_ascii_digit()))
            || prefix
                .strip_prefix("pica")
                .is_some_and(|id| id.len() == 24 && id.bytes().all(|v| v.is_ascii_hexdigit()));
        let creator = authors
            .iter()
            .map(|a| matching_text(a))
            .any(|a| real_author(&a) && (prefix == a || prefix.contains(&format!("({a})"))));
        if source_prefix || creator || candidate {
            value = rest.to_owned();
        } else {
            break;
        }
    }
    value
}

pub fn library_title_candidate(item: &LibraryItem, work: &LibraryMatchWork) -> bool {
    let title = matching_title(&item.title, &item.authors, true);
    !title.is_empty() && title == matching_title(&work.title, &work.authors, true)
}

pub fn strong_library_match(item: &LibraryItem, work: &LibraryMatchWork) -> bool {
    let title = matching_title(&item.title, &item.authors, false);
    let authors: Vec<_> = item
        .authors
        .iter()
        .map(|a| matching_text(a))
        .filter(|a| real_author(a))
        .collect();
    item.state == LibraryItemState::Indexed
        && item.error_code.is_none()
        && work
            .page_count
            .is_some_and(|n| n > 0 && Some(n) == item.page_count)
        && title.chars().count() >= 8
        && title == matching_title(&work.title, &work.authors, false)
        && work
            .authors
            .iter()
            .map(|a| matching_text(a))
            .any(|a| real_author(&a) && authors.contains(&a))
        && !item.references().any(|reference| {
            reference.source == work.reference.source && reference != &work.reference
        })
}

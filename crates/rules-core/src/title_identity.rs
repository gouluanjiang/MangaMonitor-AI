//! M1 conservative representation, not a work matcher or coverage parser.
//! Structure evidence remains in core_text; no guessed series/translation split.
use crate::conservative_title;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Manga,
    Cg,
    Artbook,
    Novel,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    pub kind: String,
    pub lexeme: String,
    /// UTF-8 byte offsets into core_text, never offsets into raw.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TitleIdentity {
    pub schema_version: u32,
    pub raw: Option<String>,
    pub normalized: String,
    /// All identity-bearing text remains here, including structural tokens.
    pub core_text: String,
    pub metadata: Vec<String>,
    pub structure: Vec<Token>,
    /// Only supplied confirmed metadata; no implicit manga default.
    pub content_type: Option<ContentType>,
    pub issues: Vec<String>,
}

fn token(kind: &str, text: &str, start: usize, end: usize) -> Token {
    Token {
        kind: kind.into(),
        lexeme: text[start..end].into(),
        start,
        end,
    }
}

fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for c in text.chars() {
        match c {
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '【' => stack.push('】'),
            ')' | ']' | '】' if stack.pop() != Some(c) => return false,
            _ => {}
        }
    }
    stack.is_empty()
}

/// Extract lexical evidence only. Numeric units and collection membership remain
/// unknown; e.g. a digit in 95式 is not silently made an episode number.
pub fn parse_title(raw: Option<&str>, content_type: Option<ContentType>) -> TitleIdentity {
    let normalized = conservative_title(raw.unwrap_or_default());
    let mut core = normalized.clone();
    let mut metadata = Vec::new();
    // Closed whole annotations at boundaries only. Unknown groups stay intact.
    const FLAGS: &[&str] = &[
        "[chinese]",
        "[中文]",
        "[中国翻译]",
        "[中國翻譯]",
        "[中国翻訳]",
        "[中文翻译]",
        "[中文翻譯]",
        "[dl版]",
        "[digital]",
    ];
    loop {
        let mut removed = false;
        for flag in FLAGS {
            if let Some(rest) = core.strip_prefix(flag) {
                metadata.push((*flag).into());
                core = rest.trim().into();
                removed = true;
                break;
            }
            if let Some(rest) = core.strip_suffix(flag) {
                metadata.push((*flag).into());
                core = rest.trim().into();
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }
    let mut structure = Vec::new();
    let bytes = core.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        structure.push(token("number_unclassified", &core, start, i));
        let tail = &core[i..];
        let trimmed = tail.trim_start();
        if let Some(sep) = trimmed.chars().next().filter(|c| "-–—~〜～".contains(*c)) {
            let right = trimmed[sep.len_utf8()..].trim_start();
            let n = right.bytes().take_while(u8::is_ascii_digit).count();
            if n > 0 {
                let end = core.len() - right.len() + n;
                structure.push(token("range_unclassified", &core, start, end));
            }
        }
    }
    // These tokens are risk/structure evidence, not removal rules or aliases.
    for (kind, terms) in [
        ("volume", &["卷", "巻", "volume", "vol."][..]),
        ("episode_chapter", &["第", "话", "話", "episode", "chapter"]),
        ("part", &["part"]),
        ("front_back", &["前篇", "前編", "后篇", "後篇", "後編"]),
        ("upper_lower", &["上", "下"]),
        (
            "extra_bonus",
            &[
                "extra",
                "bonus",
                "after-story",
                "after story",
                "おまけ",
                "番外",
                "特典",
                "後日談",
                "外傳",
                "外伝",
            ],
        ),
        (
            "collection",
            &[
                "総集編",
                "总集篇",
                "總集編",
                "總集篇",
                "合集",
                "collection",
                "オムニバス",
            ],
        ),
        ("cg", &["cg", "cg图集", "cg圖集", "cg集"]),
        ("artbook", &["artbook", "画集", "畫集", "イラスト集"]),
        ("novel", &["novel", "小说", "小說", "小説"]),
        (
            "settings",
            &["settings", "setting book", "设定集", "設定集", "設定資料"],
        ),
    ] {
        for term in terms {
            for (start, _) in core.match_indices(term) {
                let end = start + term.len();
                // ASCII words must not match parts of normal title words.
                if term.is_ascii()
                    && (core[..start]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                        || core[end..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic()))
                {
                    continue;
                }
                structure.push(token(kind, &core, start, end));
            }
        }
    }
    structure.sort_by(|a, b| (a.start, a.end, &a.kind).cmp(&(b.start, b.end, &b.kind)));
    let mut issues = Vec::new();
    if raw.is_none() || core.is_empty() {
        issues.push("MISSING_TITLE".into());
    }
    if !balanced(&normalized) {
        issues.push("MALFORMED_BRACKETS".into());
    }
    if core.contains(['[', ']', '(', ')', '【', '】']) {
        issues.push("UNINTERPRETED_ANNOTATION".into());
    }
    if core.contains(['|', '丨']) {
        issues.push("BILINGUAL_OR_SEPARATOR_UNRESOLVED".into());
    }
    if core.chars().filter(|c| c.is_alphanumeric()).count() < 4 || core.starts_with("[artist]") {
        issues.push("LOW_INFORMATION".into());
    }
    if content_type.is_none() {
        issues.push("UNKNOWN_CONTENT_TYPE".into());
    }
    for t in &structure {
        let indicated = match t.kind.as_str() {
            "cg" => Some(ContentType::Cg),
            "artbook" => Some(ContentType::Artbook),
            "novel" => Some(ContentType::Novel),
            "settings" => Some(ContentType::Settings),
            _ => None,
        };
        if indicated.is_some() && content_type.is_some() && indicated != content_type {
            issues.push("CONTENT_TYPE_CONFLICT".into());
        }
        if t.kind == "collection" || t.kind == "range_unclassified" {
            issues.push("MEMBERSHIP_OR_RANGE_UNIT_UNKNOWN".into());
        }
    }
    issues.sort();
    issues.dedup();
    TitleIdentity {
        schema_version: 1,
        raw: raw.map(str::to_owned),
        normalized,
        core_text: core,
        metadata,
        structure,
        content_type,
        issues,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Comparison {
    ExactRepresentation,
    DifferentRepresentation,
    NeedsReview,
}

/// Neither equality nor inequality is a SAME/NOT_SAME decision. No author-only
/// inference, no fuzzy score, no version ranking, no coverage authorization.
pub fn compare(a: &TitleIdentity, b: &TitleIdentity) -> Comparison {
    if !a.issues.is_empty() || !b.issues.is_empty() {
        return Comparison::NeedsReview;
    }
    if a.core_text == b.core_text && a.content_type == b.content_type {
        Comparison::ExactRepresentation
    } else {
        Comparison::DifferentRepresentation
    }
}

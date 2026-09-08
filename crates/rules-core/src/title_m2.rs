//! Versioned, boundary/grammar based identity parser. No IO or fuzzy matching.
//! None means unspecified, never false. Evidence offsets refer to normalized UTF-8.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use unicode_casefold::{Locale, UnicodeCaseFold, Variant};
use unicode_normalization::{char::canonical_combining_class, UnicodeNormalization};

pub const RULE_VERSION: &str = "matcher-m2-v2";

// Preserve the allowed NFKC normalization and attachment-preserving simple
// casing. Full casefold can expand a letter into a different spelling (for
// example, ß into ss), which is not sufficient title-identity evidence.
// Keep this stricter key local to M2;
// the compatibility/search normalizer is not an identity authority.
fn norm(s: &str) -> String {
    s.nfkc()
        .map(|c| {
            let folded = c
                .case_fold_with(Variant::Simple, Locale::NonTurkic)
                .next()
                .expect("simple casefold maps one scalar");
            // Simple folding can still turn an attached combining mark into a
            // standalone letter (U+0345 -> U+03B9). That changes spelling too.
            if canonical_combining_class(c) == canonical_combining_class(folded) {
                folded
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub rule: String,
    pub field: String,
    pub span: Option<[usize; 2]>,
    pub text: String,
    pub reference: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Numbering {
    pub first: String,
    pub last: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fields {
    pub series_number: Option<Numbering>,
    pub volume: Option<Numbering>,
    pub episode: Option<Numbering>,
    pub chapter: Option<Numbering>,
    pub part: Option<Numbering>,
    pub front_back: Option<String>,
    pub upper_lower: Option<String>,
    pub extra: Option<String>,
    pub collection: Option<String>,
    pub content_type: Option<String>,
    pub subtitle: Option<String>,
    pub fandom: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub rule_version: String,
    pub raw: Option<String>,
    pub normalized: String,
    pub core: String,
    pub fields: Fields,
    pub evidence: Vec<Evidence>,
    pub issues: BTreeSet<String>,
}
impl Identity {
    fn ev(&mut self, rule: &str, field: &str, start: usize, end: usize, reference: &str) {
        self.evidence.push(Evidence {
            rule: rule.into(),
            field: field.into(),
            span: Some([start, end]),
            text: self.normalized[start..end].into(),
            reference: reference.into(),
        });
    }
}
fn closing(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '【' => Some('】'),
        _ => None,
    }
}
fn groups(s: &str) -> Option<Vec<(usize, usize)>> {
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if let Some(end) = closing(c) {
            if stack.is_empty() {
                start = i;
            }
            stack.push(end);
        } else if ")]】".contains(c) {
            if stack.pop() != Some(c) {
                return None;
            }
            if stack.is_empty() {
                out.push((start, i + c.len_utf8()));
            }
        }
    }
    if stack.is_empty() {
        Some(out)
    } else {
        None
    }
}
fn inside(s: &str) -> &str {
    &s[s.chars().next().unwrap().len_utf8()..s.len() - s.chars().next_back().unwrap().len_utf8()]
}
fn trim(s: &str, lo: &mut usize, hi: &mut usize) {
    *lo += s[*lo..*hi].len() - s[*lo..*hi].trim_start().len();
    *hi = *lo + s[*lo..*hi].trim_end().len();
}
fn event(s: &str) -> bool {
    s.strip_prefix('c')
        .is_some_and(|n| (2..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
        || s.strip_prefix("comic1☆")
            .is_some_and(|n| (1..=2).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
}
// Each entry is an exact reviewed annotation, not a substring permission.
fn translator(s: &str) -> Option<&'static str> {
    match s {
        "空氣系☆漢化" => Some("phase3b:jm:302458"),
        "路人漢化" => Some("phase3b:jm:1244034"),
        "天魔的黑兔個人漢化" => Some("phase3b:jm:1444812"),
        "不可視漢化" => Some("phase3b:jm:234503"),
        "不夠色漢化組" => Some("phase3b:jm:235554"),
        "灰羽社漢化" => Some("phase3b:jm:148465"),
        _ => None,
    }
}
fn creator(s: &str, author: Option<&str>) -> Option<&'static str> {
    let author = norm(author?);
    if s == author {
        return Some("CONFIRMED_WHOLE_AUTHOR_TOKEN");
    }
    let (circle, rest) = s.split_once('(')?;
    if rest.strip_suffix(')')?.trim() != author {
        return None;
    }
    match (circle.trim(), author.as_str()) {
        ("3104丁目", "3104")
        | ("ろいやるびっち", "haruhisky")
        | ("honeyroad", "bee導師")
        | ("hello girls!", "10驛")
        | ("一億萬軒茶屋", "2-g")
        | ("ちまた", "2-g") => Some("PHASE3B_CLOSED_CREATOR_PAIRS"),
        _ => None,
    }
}
fn fandom(s: &str) -> bool {
    [
        "オリジナル",
        "原神",
        "アズールレーン",
        "ブルーアーカイブ",
        "勝利の女神:nikke",
        "ゼンレスゾーンゼロ",
        "fate/stay night",
        "fate/grand order",
        "涼宮ハルヒの憂鬱",
        "くーねるまるた",
        "みょーちゃん先生はかく語りき",
        "アイドルマスター シャイニーカラーズ",
    ]
    .contains(&s)
}
pub fn type_token(s: &str) -> Option<&'static str> {
    match s {
        "manga" | "漫画" | "漫畫" => Some("manga"),
        "cg" | "cg集" | "cg set" => Some("cg"),
        "artbook" | "画集" | "畫集" | "イラスト集" => Some("artbook"),
        "novel" | "小说" | "小說" | "小説" => Some("novel"),
        "settings" | "setting book" | "設定集" | "设定集" => Some("settings"),
        _ => None,
    }
}
fn flag(s: &str) -> bool {
    [
        "chinese",
        "中文",
        "中国翻译",
        "中國翻譯",
        "中国翻訳",
        "中文翻译",
        "中文翻譯",
        "dl版",
        "digital",
        "無修正",
        "无修正",
    ]
    .contains(&s)
}
fn numbering(s: &str) -> Option<Numbering> {
    let nums: Vec<_> = s
        .split(['-', '–', '—', '~', '〜', '～'])
        .map(str::trim)
        .collect();
    if !(1..=2).contains(&nums.len())
        || nums
            .iter()
            .any(|n| n.is_empty() || n.len() > 3 || !n.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let first: u32 = nums[0].parse().ok()?;
    if first == 0 {
        return None;
    }
    if nums.len() == 2 && nums[1].parse::<u32>().ok()? < first {
        return None;
    }
    Some(Numbering {
        first: nums[0].into(),
        last: nums.get(1).map(|s| (*s).into()),
    })
}
fn word_boundary(s: &str, start: usize) -> bool {
    start == 0
        || s[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
}
// Return (start, field, value). Only complete suffix grammars; no arbitrary lexical hits.
fn structural_suffix(s: &str) -> Option<(usize, &'static str, String)> {
    for (suffix, field, value) in [
        ("前篇", "front_back", "front"),
        ("前編", "front_back", "front"),
        ("后篇", "front_back", "back"),
        ("後篇", "front_back", "back"),
        ("後編", "front_back", "back"),
        ("上篇", "upper_lower", "upper"),
        ("上編", "upper_lower", "upper"),
        ("下篇", "upper_lower", "lower"),
        ("下編", "upper_lower", "lower"),
        ("上", "upper_lower", "upper"),
        ("下", "upper_lower", "lower"),
        ("extra", "extra", "extra"),
        ("bonus", "extra", "bonus"),
        ("after-story", "extra", "after-story"),
        ("after story", "extra", "after-story"),
        ("おまけ", "extra", "おまけ"),
        ("总集篇", "collection", "collection"),
        ("總集篇", "collection", "collection"),
        ("總集編", "collection", "collection"),
        ("総集編", "collection", "collection"),
        ("collection", "collection", "collection"),
    ] {
        if let Some(before) = s.strip_suffix(suffix) {
            if word_boundary(s, before.len()) {
                return Some((before.len(), field, value.into()));
            }
        }
    }
    for (suffix, field) in [
        ("卷", "volume"),
        ("巻", "volume"),
        ("话", "episode"),
        ("話", "episode"),
    ] {
        if let Some(before) = s.strip_suffix(suffix) {
            if let Some((prefix, n)) = before.rsplit_once('第') {
                if numbering(n).is_some() {
                    return Some((prefix.len(), field, n.into()));
                }
            }
        }
    }
    for (prefix, field) in [
        ("volume ", "volume"),
        ("vol. ", "volume"),
        ("episode ", "episode"),
        ("chapter ", "chapter"),
        ("part ", "part"),
    ] {
        if let Some(i) = s.rfind(prefix) {
            let n = &s[i + prefix.len()..];
            if word_boundary(s, i) && numbering(n).is_some() {
                return Some((i, field, n.into()));
            }
        }
    }
    // Series number: whitespace-delimited suffix, or one of three observed stems.
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit()
            && numbering(&s[i..]).is_some()
            && (word_boundary(s, i)
                || [
                    "くーねるすまた",
                    "はるこす",
                    "配達バニーガールとサービスえっち",
                ]
                .contains(&&s[..i]))
        {
            return Some((i, "series_number", s[i..].into()));
        }
    }
    None
}

pub fn parse(
    raw: Option<&str>,
    author: Option<&str>,
    supplied_fandom: Option<&str>,
    supplied_type: Option<&str>,
) -> Identity {
    let normalized = norm(raw.unwrap_or_default());
    let mut p = Identity {
        rule_version: RULE_VERSION.into(),
        raw: raw.map(str::to_owned),
        core: normalized.clone(),
        normalized: normalized.clone(),
        fields: Fields::default(),
        evidence: vec![],
        issues: BTreeSet::new(),
    };
    for (field, value) in [("fandom", supplied_fandom), ("content_type", supplied_type)] {
        if let Some(v) = value {
            let v = norm(v);
            if field == "fandom" {
                p.fields.fandom = Some(v.clone());
            } else {
                p.fields.content_type = type_token(&v).map(str::to_owned);
                if p.fields.content_type.is_none() {
                    p.issues.insert("UNKNOWN_CONTENT_TYPE".into());
                }
            }
            p.evidence.push(Evidence {
                rule: "EXPLICIT_INPUT_FIELD".into(),
                field: field.into(),
                span: None,
                text: v,
                reference: format!("input.{field}"),
            });
        }
    }
    let Some(gs) = groups(&normalized) else {
        p.issues.insert("MALFORMED_BRACKETS".into());
        return p;
    };
    let (mut lo, mut hi) = (0, normalized.len());
    trim(&normalized, &mut lo, &mut hi);
    // Only current edges are eligible. An unknown group blocks further peeling on its side.
    loop {
        let mut changed = false;
        for &(a, b) in &gs {
            if a < lo || b > hi || (a != lo && b != hi) {
                continue;
            }
            let g = &normalized[a..b];
            let value = inside(g).trim();
            let leading = a == lo;
            let square = g.starts_with(['[', '【']);
            let rule = if flag(value) && square {
                Some(("EDGE_VERSION_METADATA", "metadata", "phase3b:jm:302458"))
            } else if leading && g.starts_with('(') && event(value) {
                Some((
                    "LEADING_EVENT_GRAMMAR",
                    "metadata",
                    "phase3b:jm:149633;adversarial:event",
                ))
            } else if leading && square && creator(value, author).is_some() {
                Some((
                    "LEADING_CONFIRMED_CREATOR",
                    "metadata",
                    creator(value, author).unwrap(),
                ))
            } else if leading && square && translator(value).is_some() {
                Some((
                    "LEADING_CLOSED_TRANSLATOR",
                    "metadata",
                    translator(value).unwrap(),
                ))
            } else if !leading && g.starts_with('(') && fandom(value) {
                Some(("TRAILING_CLOSED_FANDOM", "fandom", "phase3b:review-export"))
            } else if square && type_token(value).is_some() {
                Some((
                    "EDGE_EXPLICIT_CONTENT_TYPE",
                    "content_type",
                    "adversarial:content-types",
                ))
            } else {
                None
            };
            if let Some((rule, field, reference)) = rule {
                if field == "fandom" {
                    if p.fields.fandom.as_deref().is_some_and(|v| v != value) {
                        p.issues.insert("FANDOM_CONFLICT".into());
                    }
                    p.fields.fandom = Some(value.into());
                } else if field == "content_type" {
                    let ty = type_token(value).unwrap();
                    if p.fields.content_type.as_deref().is_some_and(|v| v != ty) {
                        p.issues.insert("CONTENT_TYPE_CONFLICT".into());
                    }
                    p.fields.content_type = Some(ty.into());
                }
                p.ev(rule, field, a, b, reference);
                if leading {
                    lo = b;
                } else {
                    hi = a;
                }
                trim(&normalized, &mut lo, &mut hi);
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    // Named subtitle delimiters are preserved as a separate identity field.
    if let Some(before) = normalized[lo..hi].strip_suffix('―') {
        if let Some(pos) = before.rfind(" ―") {
            let start = lo + pos;
            p.fields.subtitle = Some(before[pos + " ―".len()..].into());
            p.ev(
                "TRAILING_PAIRED_SUBTITLE",
                "subtitle",
                start,
                hi,
                "adversarial:subtitle",
            );
            hi = start;
            trim(&normalized, &mut lo, &mut hi);
        }
    }
    loop {
        let text = &normalized[lo..hi];
        let group = gs.iter().find(|&&(a, b)| a >= lo && b == hi).copied();
        let parsed = if let Some((a, b)) = group {
            let value = inside(&normalized[a..b]).trim();
            structural_suffix(value)
                .filter(|(i, _, _)| *i == 0)
                .map(|(_, f, v)| (a, f, v))
        } else {
            structural_suffix(text).map(|(i, f, v)| (lo + i, f, v))
        };
        let Some((start, field, value)) = parsed else {
            break;
        };
        let duplicate = match field {
            "series_number" => p
                .fields
                .series_number
                .replace(numbering(&value).unwrap())
                .is_some(),
            "volume" => p
                .fields
                .volume
                .replace(numbering(&value).unwrap())
                .is_some(),
            "episode" => p
                .fields
                .episode
                .replace(numbering(&value).unwrap())
                .is_some(),
            "chapter" => p
                .fields
                .chapter
                .replace(numbering(&value).unwrap())
                .is_some(),
            "part" => p.fields.part.replace(numbering(&value).unwrap()).is_some(),
            "front_back" => p.fields.front_back.replace(value).is_some(),
            "upper_lower" => p.fields.upper_lower.replace(value).is_some(),
            "extra" => p.fields.extra.replace(value).is_some(),
            "collection" => p.fields.collection.replace(value).is_some(),
            _ => unreachable!(),
        };
        if duplicate {
            p.issues.insert("DUPLICATE_STRUCTURAL_FIELD".into());
        }
        p.ev(
            "COMPLETE_SUFFIX_GRAMMAR",
            field,
            start,
            hi,
            "matcher-m2-v2:grammar-and-collision-tests",
        );
        hi = start;
        trim(&normalized, &mut lo, &mut hi);
    }
    p.core = normalized[lo..hi].into();
    if p.core.contains(['[', ']', '(', ')', '【', '】']) {
        p.issues.insert("UNINTERPRETED_ANNOTATION".into());
    }
    if p.core.contains(['|', '丨']) {
        p.issues.insert("BILINGUAL_UNRESOLVED".into());
    }
    if p.core.chars().filter(|c| c.is_alphanumeric()).count() < 4
        || !p.core.chars().any(char::is_alphabetic)
        || p.core.starts_with("[artist]")
        || author.is_some_and(|a| norm(a) == p.core)
    {
        p.issues.insert("LOW_INFORMATION_TITLE".into());
    }
    if raw.is_none() || p.core.is_empty() {
        p.issues.insert("MISSING_PRIMARY".into());
    }
    // A typed marker stranded inside the body is never silently treated as plain text.
    // Veto only: these coarse checks cannot create a field or authorize a match.
    if [
        "extra",
        "bonus",
        "総集編",
        "總集編",
        "总集篇",
        "おまけ",
        "後日談",
        "cg图集",
        "cg圖集",
        "イラスト集",
    ]
    .iter()
    .any(|s| p.core.contains(s))
    {
        p.issues.insert("UNPARSED_IDENTITY_MARKER".into());
    }
    p
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Relation {
    Exact,
    StructuralConflict,
    DifferentCore,
    Insufficient,
}

/// Veto reordered attachments when the same structural fields are present.
/// Missing/different fields are checked separately by compare; true alone is
/// neither a title witness nor proof of the same work.
pub fn structural_attachment_compatible(a: &Identity, b: &Identity) -> bool {
    fn order(identity: &Identity) -> Vec<&str> {
        identity
            .evidence
            .iter()
            .filter(|e| e.rule == "COMPLETE_SUFFIX_GRAMMAR")
            .map(|e| e.field.as_str())
            .collect()
    }
    let a_order = order(a);
    let b_order = order(b);
    a_order == b_order
        || a_order.iter().collect::<BTreeSet<_>>() != b_order.iter().collect::<BTreeSet<_>>()
}

pub fn compare(a: &Identity, b: &Identity) -> Relation {
    if a.rule_version != RULE_VERSION
        || b.rule_version != RULE_VERSION
        || !a.issues.is_empty()
        || !b.issues.is_empty()
        || a.fields.content_type.is_none()
        || b.fields.content_type.is_none()
    {
        return Relation::Insufficient;
    }
    if a.core != b.core {
        return Relation::DifferentCore;
    }
    // Both types are explicit and issue-free here. Different known content
    // types already separate these candidates, even if suffix attachment would
    // otherwise be ambiguous. Missing/conflicting types still fail above.
    if a.fields.content_type != b.fields.content_type {
        return Relation::StructuralConflict;
    }
    // Peeling suffixes into an unordered field set loses attachment evidence:
    // "title 2 Extra" may be an extra of installment 2, whereas "title Extra 2"
    // may be installment 2 of the extras. Preserve the relative grammar order
    // for every structural field, including numbered parts/chapters, without
    // rejecting identical titles or reviewed token aliases/bracket wrappers.
    if !structural_attachment_compatible(a, b) {
        return Relation::Insufficient;
    }
    if a.fields == b.fields {
        return Relation::Exact;
    }
    // Unspecified qualifier/type is not proof that a record lacks that content.
    if a.fields.fandom.is_some() != b.fields.fandom.is_some()
        || a.fields.subtitle.is_some() != b.fields.subtitle.is_some()
    {
        return Relation::Insufficient;
    }
    Relation::StructuralConflict
}

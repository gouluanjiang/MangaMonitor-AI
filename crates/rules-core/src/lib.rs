//! Pure predicates only. This crate has no network, filesystem, download or deletion operations.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use state_model::{Coverage, Version};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;

/// M1 audit-only title representation; deliberately not used by cloud-monitor.
pub mod title_identity;
pub mod title_m2;

#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub source: String,
    #[serde(flatten)]
    pub version: Version,
    pub size: Option<u64>,
}

// The handoff fixtures only rank candidates with explicit quality attributes.
// UNKNOWN is not coerced into a score. This API refuses ambiguous ranking.
pub fn select(candidates: &[Candidate]) -> Result<String, &'static str> {
    let mut remaining: Vec<&Candidate> = candidates.iter().collect();
    if remaining.is_empty() {
        return Err("EMPTY_CANDIDATES");
    }
    for field in [
        |v: &Version| v.chinese,
        |v: &Version| v.uncensored,
        |v: &Version| v.color,
    ] {
        if remaining.len() == 1 {
            return Ok(remaining[0].id.clone());
        }
        if remaining.iter().any(|c| field(&c.version).is_none()) {
            return Err("REVIEW_UNKNOWN");
        }
        let best = remaining.iter().any(|c| field(&c.version) == Some(true));
        remaining.retain(|c| field(&c.version) == Some(best));
    }
    if remaining.iter().any(|c| c.version.translation == "human") {
        remaining.retain(|c| c.version.translation == "human");
    }
    // Compare size only when every tied candidate supplies a comparable size.
    if remaining.iter().all(|c| c.size.is_some()) {
        let size = remaining.iter().filter_map(|c| c.size).max();
        remaining.retain(|c| c.size == size);
    }
    remaining.sort_by_key(|c| (c.source != "jm", &c.source, &c.id));
    Ok(remaining[0].id.clone())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Upgrade {
    Upgrade,
    NoUpgrade,
    ReviewUnknown,
    AddContentKeepOld,
}
pub fn upgrade(local: &Version, candidate: &Version) -> Upgrade {
    use crate::Upgrade::*;
    if local.sample == Some(true) && candidate.sample == Some(false) {
        return match (local.chinese, candidate.chinese) {
            (Some(true), Some(false)) => AddContentKeepOld,
            (Some(_), Some(_)) => Upgrade,
            _ => ReviewUnknown,
        };
    }
    match (local.chinese, candidate.chinese) {
        (Some(false), Some(true)) => return Upgrade,
        (Some(true), Some(false)) | (Some(false), Some(false)) => return NoUpgrade,
        (None, _) | (_, None) => return ReviewUnknown,
        _ => {}
    }
    for (old, new) in [
        (local.uncensored, candidate.uncensored),
        (local.color, candidate.color),
    ] {
        match (old, new) {
            (None, _) | (_, None) => return ReviewUnknown,
            (Some(false), Some(true)) => return Upgrade,
            (Some(true), Some(false)) => return NoUpgrade,
            _ => {}
        }
    }
    NoUpgrade
}

fn has_episode(owned: &Coverage, n: i64) -> bool {
    owned.episodes.contains(&n) || owned.ranges.iter().any(|[a, b]| *a <= n && n <= *b)
}
pub fn covers(owned: &Coverage, target: &Value) -> bool {
    if let Some(n) = target.get("episode").and_then(Value::as_i64) {
        return has_episode(owned, n);
    }
    if let Some(r) = target.get("range").and_then(Value::as_array) {
        if r.len() != 2 {
            return false;
        }
        return match (r[0].as_i64(), r[1].as_i64()) {
            (Some(a), Some(b)) if a <= b => (a..=b).all(|n| has_episode(owned, n)),
            _ => false,
        };
    }
    if let Some(p) = target.get("part").and_then(Value::as_str) {
        return owned.parts.iter().any(|s| s == p);
    }
    if let Some(parts) = target.get("parts").and_then(Value::as_array) {
        return !parts.is_empty()
            && parts.iter().all(|p| {
                p.as_str()
                    .is_some_and(|p| owned.parts.iter().any(|s| s == p))
            });
    }
    if let Some(p) = target.get("extra").and_then(Value::as_str) {
        return owned.extras.iter().any(|s| s == p);
    }
    false
}

pub fn task_done(target: &Value, local: &Value, completion_revision: u64) -> bool {
    if target["revision"].as_u64() != Some(completion_revision) {
        return false;
    }
    for field in ["chinese", "uncensored", "color"] {
        if !target[field].is_null() && local[field] != target[field] {
            return false;
        }
    }
    if let Some(content) = target.get("coverage") {
        let Ok(owned) = serde_json::from_value::<Coverage>(local["coverage"].clone()) else {
            return false;
        };
        if !covers(&owned, content) {
            return false;
        }
    }
    true
}

pub fn catalog_action(c: &Value) -> &'static str {
    if c["source_error"] == true {
        "DO_NOT_COUNT_UNAVAILABLE"
    } else if c["pending"] == true || c["serial"] == true {
        "REFRESH_BY_ID"
    } else if c["old_id"] == true && c["fingerprint_changed"] == true {
        "FETCH_DETAILS_REANALYZE"
    } else if c["old_id"] == true {
        "SKIP_HEAVY_ANALYSIS"
    } else {
        "NEW_RECORD"
    }
}

// Coverage-only test predicate. SAME, version quality, authorization and takeover mode
// are independent prerequisites. This function does not authorize or execute deletion.
pub fn old_coverage_preserved(old: &Coverage, new: &Coverage, complete: bool) -> bool {
    complete
        && !old.collection_membership_unknown
        && (!old.episodes.is_empty()
            || !old.ranges.is_empty()
            || !old.parts.is_empty()
            || !old.extras.is_empty())
        && old.episodes.iter().all(|&n| has_episode(new, n))
        && old
            .ranges
            .iter()
            .all(|[a, b]| a <= b && (*a..=*b).all(|n| has_episode(new, n)))
        && old.parts.iter().all(|p| new.parts.contains(p))
        && old.extras.iter().all(|p| new.extras.contains(p))
}

// Compatibility key for the supplied fixtures, NOT sufficient evidence to merge works.
// Structure/range markers must be checked independently before identity matching.
pub fn normalize_title(s: &str) -> String {
    const REMOVE: &str = "-_・·:：!！?？~〜～,，.。/／\\|｜「」『』【】[]()（）<>＜＞♥♡☆★＊*";
    s.nfkc()
        .case_fold()
        .filter(|c| !c.is_whitespace() && !REMOVE.contains(*c))
        .collect()
}

/// Production identity key: retain punctuation, ranges, digits and part markers.
/// The fixture compatibility normalizer above deliberately is NOT used here.
pub fn conservative_title(s: &str) -> String {
    s.nfkc()
        .case_fold()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

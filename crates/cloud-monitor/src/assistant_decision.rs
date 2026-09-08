//! Deterministic staging logic for assistant-confirmed review decisions.
//!
//! This module performs no filesystem, network, Git, source-site, download, or
//! deletion I/O. It validates one active review, mutates an in-memory copy of
//! `decisions`, and previews the accepted deterministic matcher result.

use crate::{
    matcher_m2,
    monitor::{Decisions, Mapping, State},
};
use rules_core::title_m2;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn decision_document(decisions: &Decisions) -> Value {
    let mut value = serde_json::to_value(decisions).expect("decisions serialize");
    value["schema_version"] = json!(3);
    value
}

fn pair(mapping: &Mapping) -> (String, String) {
    (mapping.source_key.clone(), mapping.work_id.clone())
}

fn validate_existing_decisions(decisions: &Decisions) -> Result<(), String> {
    let mut positive = BTreeSet::new();
    let mut negative = BTreeSet::new();
    let mut positive_source = std::collections::BTreeMap::<String, String>::new();
    for mapping in &decisions.positive_mappings {
        let p = pair(mapping);
        if !positive.insert(p.clone()) {
            return Err("DUPLICATE_ASSISTANT_POSITIVE_MAPPING".into());
        }
        if let Some(existing) = positive_source.insert(p.0.clone(), p.1.clone()) {
            if existing != p.1 {
                return Err("CONFLICTING_ASSISTANT_POSITIVE_MAPPINGS".into());
            }
        }
    }
    for mapping in &decisions.negative_mappings {
        let p = pair(mapping);
        if !negative.insert(p.clone()) {
            return Err("DUPLICATE_ASSISTANT_NEGATIVE_MAPPING".into());
        }
    }
    if positive.iter().any(|p| negative.contains(p)) {
        return Err("CONTRADICTORY_ASSISTANT_MAPPING".into());
    }
    let ignored_sources: BTreeSet<_> = decisions.ignored_source_records.iter().collect();
    if ignored_sources.len() != decisions.ignored_source_records.len() {
        return Err("DUPLICATE_ASSISTANT_IGNORED_SOURCE".into());
    }
    let ignored_works: BTreeSet<_> = decisions.ignored_works.iter().collect();
    if ignored_works.len() != decisions.ignored_works.len() {
        return Err("DUPLICATE_ASSISTANT_IGNORED_WORK".into());
    }
    Ok(())
}

fn inventory_has_work(state: &State, work_id: &str) -> bool {
    state.inventory["works"]
        .as_array()
        .is_some_and(|works| works.iter().any(|work| work["work_id"] == work_id))
}

fn validate_review<'a>(
    state: &'a State,
    review_id: &str,
) -> Result<&'a crate::monitor::Review, String> {
    let review = state
        .review
        .get(review_id)
        .ok_or("ASSISTANT_REVIEW_NOT_FOUND")?;
    if review.status != "REVIEW_REQUIRED" {
        return Err("ASSISTANT_REVIEW_NOT_ACTIVE".into());
    }
    let entry = state
        .catalog
        .get(&review.source_key)
        .ok_or("ASSISTANT_REVIEW_SOURCE_MISSING")?;
    if review.title != entry.record.raw_title || review.author != entry.record.author {
        return Err("ASSISTANT_REVIEW_SOURCE_EVIDENCE_STALE".into());
    }
    if !review.matcher_version.is_empty() && review.matcher_version != title_m2::RULE_VERSION {
        return Err("ASSISTANT_REVIEW_MATCHER_VERSION_STALE".into());
    }
    if let Some(source_key) = review.provenance["analysis"]["source_key"].as_str() {
        if source_key != review.source_key.as_str() {
            return Err("ASSISTANT_REVIEW_PROVENANCE_SOURCE_MISMATCH".into());
        }
    }
    if let Some(fingerprint) = review.provenance["analysis"]["detail_fingerprint"].as_str() {
        if fingerprint != entry.detail_fingerprint.as_str() {
            return Err("ASSISTANT_REVIEW_FINGERPRINT_STALE".into());
        }
    }
    Ok(review)
}

fn require_candidate(
    state: &State,
    review: &crate::monitor::Review,
    work_id: Option<&str>,
) -> Result<String, String> {
    let work_id = work_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("ASSISTANT_DECISION_WORK_ID_REQUIRED")?;
    if !review.candidates.iter().any(|candidate| candidate == work_id) {
        return Err("ASSISTANT_DECISION_WORK_NOT_IN_REVIEW_CANDIDATES".into());
    }
    if !inventory_has_work(state, work_id) {
        return Err("ASSISTANT_DECISION_CANDIDATE_WORK_MISSING".into());
    }
    Ok(work_id.to_owned())
}

fn sort_decisions(decisions: &mut Decisions) {
    decisions.positive_mappings.sort_by_key(pair);
    decisions.negative_mappings.sort_by_key(pair);
    decisions.ignored_source_records.sort();
    decisions.ignored_works.sort();
}

/// Plan one bounded review decision and return:
/// `(proposed decisions.json, decision audit, deterministic matcher preview)`.
pub fn plan(
    state: &State,
    review_id: &str,
    decision: &str,
    work_id: Option<&str>,
) -> Result<(Value, Value, Value), String> {
    if !matches!(decision, "same" | "not-same" | "ignore") {
        return Err("INVALID_ASSISTANT_REVIEW_DECISION".into());
    }
    validate_existing_decisions(&state.decisions)?;
    let review = validate_review(state, review_id)?;
    let source_key = review.source_key.clone();
    let mut proposed = state.clone();
    let mut outcome = "ADDED";
    let selected_work_id = match decision {
        "same" | "not-same" => Some(require_candidate(state, review, work_id)?),
        "ignore" => {
            if work_id.is_some_and(|value| !value.trim().is_empty()) {
                return Err("ASSISTANT_IGNORE_DOES_NOT_ACCEPT_WORK_ID".into());
            }
            None
        }
        _ => unreachable!(),
    };

    match decision {
        "same" => {
            if proposed.decisions.ignored_source_records.contains(&source_key) {
                return Err("ASSISTANT_DECISION_SOURCE_ALREADY_IGNORED".into());
            }
            let work_id = selected_work_id.as_ref().unwrap();
            if proposed.decisions.negative_mappings.iter().any(|mapping| {
                mapping.source_key == source_key && mapping.work_id == work_id.as_str()
            }) {
                return Err("ASSISTANT_SAME_CONTRADICTS_NOT_SAME".into());
            }
            if proposed.decisions.positive_mappings.iter().any(|mapping| {
                mapping.source_key == source_key && mapping.work_id != work_id.as_str()
            }) {
                return Err("ASSISTANT_SAME_ALREADY_POINTS_ELSEWHERE".into());
            }
            if proposed.decisions.positive_mappings.iter().any(|mapping| {
                mapping.source_key == source_key && mapping.work_id == work_id.as_str()
            }) {
                outcome = "NOOP_ALREADY_PRESENT";
            } else {
                proposed.decisions.positive_mappings.push(Mapping {
                    source_key: source_key.clone(),
                    work_id: work_id.clone(),
                });
            }
        }
        "not-same" => {
            if proposed.decisions.ignored_source_records.contains(&source_key) {
                return Err("ASSISTANT_DECISION_SOURCE_ALREADY_IGNORED".into());
            }
            let work_id = selected_work_id.as_ref().unwrap();
            if proposed.decisions.positive_mappings.iter().any(|mapping| {
                mapping.source_key == source_key && mapping.work_id == work_id.as_str()
            }) {
                return Err("ASSISTANT_NOT_SAME_CONTRADICTS_SAME".into());
            }
            if proposed.decisions.negative_mappings.iter().any(|mapping| {
                mapping.source_key == source_key && mapping.work_id == work_id.as_str()
            }) {
                outcome = "NOOP_ALREADY_PRESENT";
            } else {
                proposed.decisions.negative_mappings.push(Mapping {
                    source_key: source_key.clone(),
                    work_id: work_id.clone(),
                });
            }
        }
        "ignore" => {
            if proposed.decisions.ignored_source_records.contains(&source_key) {
                outcome = "NOOP_ALREADY_PRESENT";
            } else {
                proposed
                    .decisions
                    .ignored_source_records
                    .push(source_key.clone());
            }
        }
        _ => unreachable!(),
    }

    sort_decisions(&mut proposed.decisions);
    validate_existing_decisions(&proposed.decisions)?;
    let record = proposed
        .catalog
        .get(&source_key)
        .ok_or("ASSISTANT_REVIEW_SOURCE_MISSING")?
        .record
        .clone();
    let preview = matcher_m2::decide(&proposed, &record, &[]);
    let preview_value =
        serde_json::to_value(&preview).map_err(|_| "ASSISTANT_DECISION_PREVIEW_SERIALIZE")?;
    let audit = json!({
        "schema_version": 1,
        "review_id": review_id,
        "source_key": source_key,
        "decision": decision,
        "work_id": selected_work_id,
        "outcome": outcome,
        "matcher_version": title_m2::RULE_VERSION,
        "input_analysis_context": state.context(),
        "proposed_analysis_context": proposed.context(),
        "preview_disposition": preview.disposition,
        "preview_reason": preview.reason,
        "preview_work_id": preview.work_id,
    });
    Ok((decision_document(&proposed.decisions), audit, preview_value))
}

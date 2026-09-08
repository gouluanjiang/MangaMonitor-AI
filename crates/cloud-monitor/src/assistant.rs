//! Read-only, deterministic views for the optional ChatGPT assistant layer.
//!
//! This module derives bounded JSON views from an already loaded production
//! state.  It performs no network access and no filesystem mutation.

use crate::monitor::State;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const ASSISTANT_VIEW_SCHEMA_VERSION: u64 = 1;
pub const MAX_REVIEW_BATCH: usize = 100;

pub fn scan_summary(state: &State) -> Value {
    let review_required_count = state
        .review
        .values()
        .filter(|review| review.status == "REVIEW_REQUIRED")
        .count();
    json!({
        "schema_version": ASSISTANT_VIEW_SCHEMA_VERSION,
        "view": "scan_summary",
        "scan_id": state.scan.scan_id,
        "scan_status": if state.scan.complete { "COMPLETE" } else { "PARTIAL" },
        "started_at": state.scan.started_at,
        "requested_mode": state.scan.requested_mode,
        "selected_author_count": state.scan.selected_authors.len(),
        "review_required_count": review_required_count,
        "pending_task_count": state.pending.len(),
        "latest_event_count": state.scan.events.len(),
        "source_failures": state.scan.direct_failures,
        "progress_boundary_counts": boundary_counts(state),
    })
}

fn boundary_counts(state: &State) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for cursor in state.scan.progress.values() {
        *counts.entry(cursor.boundary.clone()).or_default() += 1;
    }
    counts
}

pub fn review_backlog_summary(state: &State) -> Value {
    let mut reason_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_seen = Vec::new();
    let mut last_seen = Vec::new();
    let mut total = 0usize;

    for review in state
        .review
        .values()
        .filter(|review| review.status == "REVIEW_REQUIRED")
    {
        total += 1;
        *reason_counts.entry(review.reason.clone()).or_default() += 1;
        if let Some(entry) = state.catalog.get(&review.source_key) {
            first_seen.push(entry.record.first_seen.as_str());
            last_seen.push(entry.record.last_seen.as_str());
        }
    }

    first_seen.sort_unstable();
    last_seen.sort_unstable();
    json!({
        "schema_version": ASSISTANT_VIEW_SCHEMA_VERSION,
        "view": "review_backlog_summary",
        "total": total,
        "reason_counts": reason_counts,
        "oldest_first_seen": first_seen.first().copied(),
        "newest_last_seen": last_seen.last().copied(),
        "max_batch_size": MAX_REVIEW_BATCH,
    })
}

pub fn review_batch(state: &State, offset: usize, limit: usize) -> Result<Value, String> {
    if limit == 0 || limit > MAX_REVIEW_BATCH {
        return Err("INVALID_ASSISTANT_REVIEW_BATCH_LIMIT".into());
    }

    let inventory_works = state.inventory["works"]
        .as_array()
        .ok_or("INVALID_ASSISTANT_INVENTORY_WORKS")?;
    let mut reviews: Vec<_> = state
        .review
        .values()
        .filter(|review| review.status == "REVIEW_REQUIRED")
        .collect();
    reviews.sort_by(|a, b| {
        let a_seen = state
            .catalog
            .get(&a.source_key)
            .map(|entry| entry.record.first_seen.as_str())
            .unwrap_or("");
        let b_seen = state
            .catalog
            .get(&b.source_key)
            .map(|entry| entry.record.first_seen.as_str())
            .unwrap_or("");
        a_seen
            .cmp(b_seen)
            .then_with(|| a.review_id.cmp(&b.review_id))
    });

    let total = reviews.len();
    let selected = reviews.into_iter().skip(offset).take(limit);
    let mut items = Vec::new();

    for review in selected {
        let entry = state
            .catalog
            .get(&review.source_key)
            .ok_or_else(|| format!("MISSING_ASSISTANT_CATALOG_ENTRY:{}", review.source_key))?;
        let candidate_ids: BTreeSet<_> = review.candidates.iter().cloned().collect();
        let mut candidate_works: Vec<Value> = inventory_works
            .iter()
            .filter(|work| {
                work["work_id"]
                    .as_str()
                    .map(|id| candidate_ids.contains(id))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        candidate_works.sort_by(|a, b| {
            a["work_id"]
                .as_str()
                .unwrap_or("")
                .cmp(b["work_id"].as_str().unwrap_or(""))
        });
        let found_ids: BTreeSet<_> = candidate_works
            .iter()
            .filter_map(|work| work["work_id"].as_str().map(str::to_owned))
            .collect();
        let missing_candidate_work_ids: Vec<_> = candidate_ids
            .difference(&found_ids)
            .cloned()
            .collect();

        items.push(json!({
            "review_id": review.review_id,
            "source_key": review.source_key,
            "source": entry.record.source,
            "source_work_id": entry.record.source_work_id,
            "first_seen": entry.record.first_seen,
            "last_seen": entry.record.last_seen,
            "last_checked": entry.record.last_checked,
            "raw_title": entry.record.raw_title,
            "raw_authors": entry.record.author,
            "metadata": entry.record.metadata,
            "processing_result": entry.record.processing_result,
            "review_reason": review.reason,
            "author_evidence": entry.author_evidence,
            "matcher_version": review.matcher_version,
            "identity_evidence": review.identity_evidence,
            "provenance": review.provenance,
            "candidate_work_ids": review.candidates,
            "candidate_works": candidate_works,
            "missing_candidate_work_ids": missing_candidate_work_ids,
        }));
    }

    let returned = items.len();
    let next_offset = if offset.saturating_add(returned) < total {
        Some(offset.saturating_add(returned))
    } else {
        None
    };
    Ok(json!({
        "schema_version": ASSISTANT_VIEW_SCHEMA_VERSION,
        "view": "review_batch",
        "total": total,
        "offset": offset,
        "limit": limit,
        "returned": returned,
        "next_offset": next_offset,
        "items": items,
    }))
}

pub fn pending_task_summary(state: &State) -> Value {
    let tasks: Vec<Value> = state
        .pending
        .values()
        .map(|task| {
            json!({
                "task_id": task.task_id,
                "work_id": task.work_id,
                "task_revision": task.task_revision,
                "status": task.status,
                "action": task.action,
                "first_seen": task.first_seen,
                "target": task.target,
                "old_local_item_ids": task.old_local_item_ids,
            })
        })
        .collect();
    json!({
        "schema_version": ASSISTANT_VIEW_SCHEMA_VERSION,
        "view": "pending_task_summary",
        "count": tasks.len(),
        "tasks": tasks,
    })
}

pub fn collection_summary(state: &State) -> Result<Value, String> {
    let works = state.inventory["works"]
        .as_array()
        .ok_or("INVALID_ASSISTANT_INVENTORY_WORKS")?;
    let mut owned_work_ids = 0usize;
    let mut local_item_count = 0usize;
    let mut authors = BTreeSet::new();

    for work in works {
        if work["owned"].as_bool().unwrap_or(false) {
            owned_work_ids += 1;
            local_item_count += work["local_item_ids"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            if let Some(values) = work["authors_confirmed"].as_array() {
                for author in values.iter().filter_map(Value::as_str) {
                    authors.insert(author.to_owned());
                }
            }
        }
    }

    Ok(json!({
        "schema_version": ASSISTANT_VIEW_SCHEMA_VERSION,
        "view": "collection_summary",
        "total_work_ids": works.len(),
        "owned_work_ids": owned_work_ids,
        "local_item_count": local_item_count,
        "authors_with_owned_works": authors.len(),
    }))
}

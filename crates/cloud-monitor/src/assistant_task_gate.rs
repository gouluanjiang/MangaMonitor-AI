//! Assistant-owned semantic recommendation/approval gates for deterministic
//! pending tasks. This module performs no filesystem, network, Git, source,
//! download, replacement, or deletion I/O.

use crate::monitor::{hash, State, Task};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const GATE_SCHEMA_VERSION: u64 = 1;
pub const MAX_GATE_VIEW: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateRecord {
    pub task_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    #[serde(default)]
    pub assistant_recommended: bool,
    #[serde(default)]
    pub user_approved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateLedger {
    pub schema_version: u64,
    #[serde(default)]
    pub records: Vec<GateRecord>,
}

impl Default for GateLedger {
    fn default() -> Self {
        Self {
            schema_version: GATE_SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

fn binding(record: &GateRecord) -> (String, u64, String) {
    (
        record.task_id.clone(),
        record.task_revision,
        record.target_hash.clone(),
    )
}

pub fn validate_ledger(ledger: &GateLedger) -> Result<(), String> {
    if ledger.schema_version != GATE_SCHEMA_VERSION {
        return Err("INVALID_ASSISTANT_TASK_GATE_SCHEMA_VERSION".into());
    }
    let mut seen = BTreeSet::new();
    for record in &ledger.records {
        if record.task_id.trim().is_empty()
            || record.task_revision == 0
            || record.target_hash.trim().is_empty()
        {
            return Err("INVALID_ASSISTANT_TASK_GATE_RECORD".into());
        }
        if !seen.insert(binding(record)) {
            return Err("DUPLICATE_ASSISTANT_TASK_GATE_BINDING".into());
        }
    }
    Ok(())
}

pub fn parse_ledger(value: Value) -> Result<GateLedger, String> {
    let ledger: GateLedger =
        serde_json::from_value(value).map_err(|_| "INVALID_ASSISTANT_TASK_GATE_LEDGER")?;
    validate_ledger(&ledger)?;
    Ok(ledger)
}

pub fn target_hash(task: &Task) -> String {
    hash(&task.target)
}

fn find_task<'a>(state: &'a State, task_id: &str) -> Result<&'a Task, String> {
    let mut matches = state
        .pending
        .values()
        .filter(|task| task.task_id == task_id);
    let task = matches.next().ok_or("ASSISTANT_TASK_NOT_FOUND")?;
    if matches.next().is_some() {
        return Err("AMBIGUOUS_ASSISTANT_TASK_ID".into());
    }
    Ok(task)
}

fn current_record<'a>(ledger: &'a GateLedger, task: &Task) -> Option<&'a GateRecord> {
    let hash = target_hash(task);
    ledger.records.iter().find(|record| {
        record.task_id == task.task_id
            && record.task_revision == task.task_revision
            && record.target_hash == hash
    })
}

fn allowed_action(task: &Task) -> bool {
    matches!(task.action.as_str(), "download" | "upgrade")
}

pub fn task_view(state: &State, ledger: &GateLedger, task_id: &str) -> Result<Value, String> {
    validate_ledger(ledger)?;
    let task = find_task(state, task_id)?;
    let current_hash = target_hash(task);
    let record = current_record(ledger, task);
    let recommended = record.is_some_and(|gate| gate.assistant_recommended);
    let approved = record.is_some_and(|gate| gate.user_approved);
    let authorized = task.status == "pending" && allowed_action(task) && approved;
    let stale_bindings = ledger
        .records
        .iter()
        .filter(|gate| gate.task_id == task.task_id)
        .filter(|gate| {
            gate.task_revision != task.task_revision || gate.target_hash != current_hash
        })
        .count();
    Ok(json!({
        "schema_version": GATE_SCHEMA_VERSION,
        "view": "task_gate",
        "task_id": task.task_id,
        "work_id": task.work_id,
        "task_revision": task.task_revision,
        "target_hash": current_hash,
        "action": task.action,
        "status": task.status,
        "assistant_recommended": recommended,
        "user_approved": approved,
        "download_authorized": authorized,
        "stale_binding_count": stale_bindings,
    }))
}

pub fn batch_view(
    state: &State,
    ledger: &GateLedger,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    validate_ledger(ledger)?;
    if limit == 0 || limit > MAX_GATE_VIEW {
        return Err("INVALID_ASSISTANT_TASK_GATE_VIEW_LIMIT".into());
    }
    let mut task_ids: Vec<_> = state.pending.values().map(|task| task.task_id.clone()).collect();
    task_ids.sort();
    if task_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("AMBIGUOUS_ASSISTANT_TASK_ID".into());
    }
    let total = task_ids.len();
    let items: Result<Vec<_>, _> = task_ids
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|task_id| task_view(state, ledger, &task_id))
        .collect();
    let items = items?;
    let returned = items.len();
    let next_offset = if offset.saturating_add(returned) < total {
        Some(offset.saturating_add(returned))
    } else {
        None
    };
    Ok(json!({
        "schema_version": GATE_SCHEMA_VERSION,
        "view": "task_gate_batch",
        "total": total,
        "offset": offset,
        "limit": limit,
        "returned": returned,
        "next_offset": next_offset,
        "items": items,
    }))
}

pub fn plan(
    state: &State,
    ledger: &GateLedger,
    operation: &str,
    task_id: &str,
    expected_revision: u64,
    expected_target_hash: &str,
) -> Result<(GateLedger, Value, Value), String> {
    if !matches!(
        operation,
        "recommend" | "clear-recommendation" | "approve" | "revoke"
    ) {
        return Err("INVALID_ASSISTANT_TASK_GATE_OPERATION".into());
    }
    validate_ledger(ledger)?;
    let task = find_task(state, task_id)?;
    let current_hash = target_hash(task);
    if expected_revision != task.task_revision {
        return Err("ASSISTANT_TASK_REVISION_STALE".into());
    }
    if expected_target_hash != current_hash {
        return Err("ASSISTANT_TASK_TARGET_HASH_STALE".into());
    }
    if matches!(operation, "recommend" | "approve") {
        if task.status != "pending" {
            return Err("ASSISTANT_TASK_NOT_PENDING".into());
        }
        if !allowed_action(task) {
            return Err("ASSISTANT_TASK_ACTION_NOT_EXECUTABLE".into());
        }
    }

    let mut proposed = ledger.clone();
    let exact_index = proposed.records.iter().position(|record| {
        record.task_id == task.task_id
            && record.task_revision == task.task_revision
            && record.target_hash == current_hash
    });
    let before = exact_index
        .map(|index| proposed.records[index].clone())
        .unwrap_or(GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: current_hash.clone(),
            assistant_recommended: false,
            user_approved: false,
        });
    let mut after = before.clone();
    match operation {
        "recommend" => after.assistant_recommended = true,
        "clear-recommendation" => after.assistant_recommended = false,
        "approve" => after.user_approved = true,
        "revoke" => after.user_approved = false,
        _ => unreachable!(),
    }
    let changed = before.assistant_recommended != after.assistant_recommended
        || before.user_approved != after.user_approved;

    match exact_index {
        Some(index) if !after.assistant_recommended && !after.user_approved => {
            proposed.records.remove(index);
        }
        Some(index) => proposed.records[index] = after.clone(),
        None if after.assistant_recommended || after.user_approved => proposed.records.push(after.clone()),
        None => {}
    }
    proposed.records.sort_by_key(binding);
    validate_ledger(&proposed)?;
    let preview = task_view(state, &proposed, task_id)?;
    let audit = json!({
        "schema_version": GATE_SCHEMA_VERSION,
        "operation": operation,
        "task_id": task.task_id,
        "work_id": task.work_id,
        "task_revision": task.task_revision,
        "target_hash": current_hash,
        "outcome": if changed { "UPDATED" } else { "NOOP_ALREADY_IN_STATE" },
        "before": {
            "assistant_recommended": before.assistant_recommended,
            "user_approved": before.user_approved,
        },
        "after": {
            "assistant_recommended": after.assistant_recommended,
            "user_approved": after.user_approved,
        },
        "download_authorized_after": preview["download_authorized"],
    });
    Ok((proposed, audit, preview))
}

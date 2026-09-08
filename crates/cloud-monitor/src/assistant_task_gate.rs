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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InventoryAuthority {
    Unsatisfied,
    Satisfied,
    Ambiguous,
}

/// Suppress only the V1 add-only download crash window: once authoritative
/// inventory already owns the exact work and the exact source mapping belongs
/// uniquely to that work, an old still-pending approval must no longer emit a
/// second download command. Upgrade/replacement semantics remain frozen and are
/// intentionally not inferred here.
fn inventory_authority(state: &State, task: &Task) -> InventoryAuthority {
    if task.action != "download" {
        return InventoryAuthority::Unsatisfied;
    }
    let Some((source, source_work_id)) = task.target.source_key.split_once(':') else {
        return InventoryAuthority::Unsatisfied;
    };
    if !matches!(source, "jm" | "pica") || source_work_id.trim().is_empty() {
        // Preserve the executor's existing source-key validation path. This
        // guard is only about already-owned exact source mappings.
        return InventoryAuthority::Unsatisfied;
    }
    let Some(works) = state.inventory.get("works").and_then(Value::as_array) else {
        return InventoryAuthority::Ambiguous;
    };

    let mut target_work_seen = 0usize;
    let mut source_mapping_owners = 0usize;
    let mut target_owned = false;
    let mut target_has_exact_mapping = false;

    for work in works {
        let Some(work_id) = work.get("work_id").and_then(Value::as_str) else {
            return InventoryAuthority::Ambiguous;
        };
        let Some(source_mappings) = work.get("source_mappings").and_then(Value::as_object) else {
            return InventoryAuthority::Ambiguous;
        };
        let Some(ids) = source_mappings.get(source).and_then(Value::as_array) else {
            return InventoryAuthority::Ambiguous;
        };
        let exact_mapping_count = ids
            .iter()
            .filter(|id| id.as_str() == Some(source_work_id))
            .count();
        if exact_mapping_count > 1 {
            return InventoryAuthority::Ambiguous;
        }
        if exact_mapping_count == 1 {
            source_mapping_owners += 1;
        }
        if work_id == task.work_id {
            target_work_seen += 1;
            target_owned = work.get("owned").and_then(Value::as_bool) == Some(true);
            target_has_exact_mapping = exact_mapping_count == 1;
        }
    }

    if target_work_seen > 1 || source_mapping_owners > 1 {
        return InventoryAuthority::Ambiguous;
    }
    if source_mapping_owners == 1 {
        if target_work_seen == 1 && target_owned && target_has_exact_mapping {
            InventoryAuthority::Satisfied
        } else {
            // The source identity is already claimed, but not by the exact
            // owned work this task targets. Never schedule another download
            // while authoritative inventory is contradictory.
            InventoryAuthority::Ambiguous
        }
    } else {
        InventoryAuthority::Unsatisfied
    }
}

pub fn task_view(state: &State, ledger: &GateLedger, task_id: &str) -> Result<Value, String> {
    validate_ledger(ledger)?;
    let task = find_task(state, task_id)?;
    let current_hash = target_hash(task);
    let record = current_record(ledger, task);
    let recommended = record.is_some_and(|gate| gate.assistant_recommended);
    let approved = record.is_some_and(|gate| gate.user_approved);
    let inventory_authority = inventory_authority(state, task);
    let inventory_satisfied = inventory_authority == InventoryAuthority::Satisfied;
    let inventory_authority_valid = inventory_authority != InventoryAuthority::Ambiguous;
    let authorized = task.status == "pending"
        && allowed_action(task)
        && approved
        && inventory_authority == InventoryAuthority::Unsatisfied;
    let stale_bindings = ledger
        .records
        .iter()
        .filter(|gate| gate.task_id == task.task_id)
        .filter(|gate| {
            gate.task_revision != task.task_revision || gate.target_hash != current_hash
        })
        .count();
    let execution_block_reason = match inventory_authority {
        InventoryAuthority::Satisfied => Some("INVENTORY_ALREADY_SATISFIED"),
        InventoryAuthority::Ambiguous => Some("INVENTORY_AUTHORITY_AMBIGUOUS"),
        InventoryAuthority::Unsatisfied => None,
    };
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
        "inventory_satisfied": inventory_satisfied,
        "inventory_authority_valid": inventory_authority_valid,
        "execution_block_reason": execution_block_reason,
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
        match inventory_authority(state, task) {
            InventoryAuthority::Satisfied => {
                return Err("ASSISTANT_TASK_INVENTORY_ALREADY_SATISFIED".into())
            }
            InventoryAuthority::Ambiguous => {
                return Err("ASSISTANT_TASK_INVENTORY_AUTHORITY_AMBIGUOUS".into())
            }
            InventoryAuthority::Unsatisfied => {}
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

//! Deterministic handoff between the assistant approval layer and a future local
//! Windows executor. This module does not perform network, download, filesystem,
//! inventory, replacement, or deletion operations.

use crate::{
    assistant_task_gate::{self, GateLedger},
    monitor::{hash, State, Target, Task},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const EXECUTOR_SCHEMA_VERSION: u64 = 1;
pub const MAX_EXECUTOR_COMMANDS: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutorCommand {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub action: String,
    pub intent: String,
    pub target: Target,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionEvidence {
    pub downloader_reported_full_completion: bool,
    pub artifact_manifest_hash: String,
    pub file_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorReceipt {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub outcome: String,
    #[serde(default)]
    pub completed_at: String,
    #[serde(default)]
    pub completion_evidence: Option<CompletionEvidence>,
    #[serde(default)]
    pub error_code: Option<String>,
}

fn split_source_key(source_key: &str) -> Result<(String, String), String> {
    let (source, source_work_id) = source_key
        .split_once(':')
        .ok_or("INVALID_EXECUTOR_SOURCE_KEY")?;
    if !matches!(source, "jm" | "pica") || source_work_id.trim().is_empty() {
        return Err("INVALID_EXECUTOR_SOURCE_KEY".into());
    }
    Ok((source.to_owned(), source_work_id.to_owned()))
}

fn task_by_id<'a>(state: &'a State, task_id: &str) -> Result<&'a Task, String> {
    let mut matches = state
        .pending
        .values()
        .filter(|task| task.task_id == task_id);
    let task = matches.next().ok_or("EXECUTOR_TASK_NOT_FOUND")?;
    if matches.next().is_some() {
        return Err("AMBIGUOUS_EXECUTOR_TASK_ID".into());
    }
    Ok(task)
}

fn command_id(task_id: &str, task_revision: u64, target_hash: &str) -> String {
    let digest = hash(&(task_id, task_revision, target_hash));
    format!("EXEC_{}", &digest[..20])
}

fn command_for_task(
    state: &State,
    ledger: &GateLedger,
    task: &Task,
) -> Result<Option<ExecutorCommand>, String> {
    let gate = assistant_task_gate::task_view(state, ledger, &task.task_id)?;
    if gate["download_authorized"] != true {
        return Ok(None);
    }

    // The 2026-09-08 thaw authorizes only genuinely additive new-work
    // downloads. Keep broader historical task modelling in the gate layer, but
    // do not advertise upgrade/replacement candidates as runnable executor
    // commands while that authority remains frozen.
    if task.action != "download" || !task.old_local_item_ids.is_empty() {
        return Ok(None);
    }

    let target_hash = assistant_task_gate::target_hash(task);
    let (source, source_work_id) = split_source_key(&task.target.source_key)?;
    Ok(Some(ExecutorCommand {
        schema_version: EXECUTOR_SCHEMA_VERSION,
        command_id: command_id(&task.task_id, task.task_revision, &target_hash),
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash,
        source,
        source_work_id,
        action: task.action.clone(),
        // The first executable handoff is intentionally staging-only. Promotion,
        // replacement and deletion remain separate verified state transitions.
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target: task.target.clone(),
    }))
}

pub fn queue_view(
    state: &State,
    ledger: &GateLedger,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    assistant_task_gate::validate_ledger(ledger)?;
    if limit == 0 || limit > MAX_EXECUTOR_COMMANDS {
        return Err("INVALID_EXECUTOR_QUEUE_LIMIT".into());
    }

    let mut tasks: Vec<_> = state.pending.values().collect();
    tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    if tasks.windows(2).any(|pair| pair[0].task_id == pair[1].task_id) {
        return Err("AMBIGUOUS_EXECUTOR_TASK_ID".into());
    }

    let commands: Result<Vec<_>, _> = tasks
        .into_iter()
        .map(|task| command_for_task(state, ledger, task))
        .collect();
    let commands: Vec<_> = commands?.into_iter().flatten().collect();
    let total_authorized = commands.len();
    let items: Vec<_> = commands.into_iter().skip(offset).take(limit).collect();
    let returned = items.len();
    let next_offset = if offset.saturating_add(returned) < total_authorized {
        Some(offset.saturating_add(returned))
    } else {
        None
    };

    Ok(json!({
        "schema_version": EXECUTOR_SCHEMA_VERSION,
        "view": "executor_queue",
        "state_context": state.context(),
        "total_authorized": total_authorized,
        "offset": offset,
        "limit": limit,
        "returned": returned,
        "next_offset": next_offset,
        "commands": items,
        "physical_delete_authorized": false,
    }))
}

fn validate_receipt_shape(receipt: &ExecutorReceipt) -> Result<(), String> {
    if receipt.schema_version != EXECUTOR_SCHEMA_VERSION {
        return Err("INVALID_EXECUTOR_RECEIPT_SCHEMA_VERSION".into());
    }
    if receipt.task_id.trim().is_empty()
        || receipt.work_id.trim().is_empty()
        || receipt.task_revision == 0
        || receipt.target_hash.trim().is_empty()
        || receipt.command_id
            != command_id(&receipt.task_id, receipt.task_revision, &receipt.target_hash)
    {
        return Err("INVALID_EXECUTOR_RECEIPT_BINDING".into());
    }
    if !matches!(receipt.outcome.as_str(), "SUCCEEDED" | "FAILED" | "CANCELLED") {
        return Err("INVALID_EXECUTOR_RECEIPT_OUTCOME".into());
    }
    if receipt.outcome == "SUCCEEDED" {
        let evidence = receipt
            .completion_evidence
            .as_ref()
            .ok_or("EXECUTOR_SUCCESS_MISSING_COMPLETION_EVIDENCE")?;
        if !evidence.downloader_reported_full_completion
            || evidence.artifact_manifest_hash.trim().is_empty()
            || evidence.file_count == 0
        {
            return Err("EXECUTOR_SUCCESS_INCOMPLETE_COMPLETION_EVIDENCE".into());
        }
    }
    Ok(())
}

/// Validate a local receipt against the current task/gate generation.
///
/// A current successful receipt only becomes *ready for inventory verification*.
/// It never marks a task complete and never authorizes replacement/deletion.
pub fn receipt_view(
    state: &State,
    ledger: &GateLedger,
    receipt: &ExecutorReceipt,
) -> Result<Value, String> {
    assistant_task_gate::validate_ledger(ledger)?;
    validate_receipt_shape(receipt)?;

    let task = task_by_id(state, &receipt.task_id)?;
    let current_hash = assistant_task_gate::target_hash(task);
    let binding_current = task.work_id == receipt.work_id
        && task.task_revision == receipt.task_revision
        && current_hash == receipt.target_hash;
    let current_gate = assistant_task_gate::task_view(state, ledger, &task.task_id)?;
    let approval_current = current_gate["download_authorized"] == true;
    let success = receipt.outcome == "SUCCEEDED";
    let ready_for_inventory_verification = success && binding_current && approval_current;

    Ok(json!({
        "schema_version": EXECUTOR_SCHEMA_VERSION,
        "view": "executor_receipt",
        "command_id": receipt.command_id,
        "task_id": receipt.task_id,
        "work_id": receipt.work_id,
        "receipt_task_revision": receipt.task_revision,
        "current_task_revision": task.task_revision,
        "receipt_target_hash": receipt.target_hash,
        "current_target_hash": current_hash,
        "outcome": receipt.outcome,
        "binding_current": binding_current,
        "approval_current": approval_current,
        "ready_for_inventory_verification": ready_for_inventory_verification,
        "task_completion_authorized": false,
        "replacement_authorized": false,
        "physical_delete_authorized": false,
    }))
}

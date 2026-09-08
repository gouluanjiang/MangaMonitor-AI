//! Current-generation authorization barrier for A6.8 source preflight.
//!
//! This module performs no network or filesystem mutation. It revalidates the
//! exact A5/A6 task approval generation immediately before a future caller may
//! perform pinned source *metadata* enumeration. Its output is diagnostic and
//! explicitly non-reusable; a network runner must call `authorize` in-process
//! against the current state/ledger rather than trust a saved JSON result.

use crate::{
    assistant_task_gate::{self, GateLedger},
    executor_handoff::ExecutorCommand,
    local_executor::{self, LocalExecutionPlan},
    monitor::{hash, State},
    source_bridge_request::{self, SourceBridgeRequest},
    source_preflight::SOURCE_PREFLIGHT_SCHEMA_VERSION,
};
use serde::Serialize;

pub const SOURCE_PREFLIGHT_AUTHORIZATION_SCHEMA_VERSION: u64 = 1;

/// Diagnostic result of an immediate current-state check.
///
/// Intentionally `Serialize`-only: saved JSON is suitable for audit output but
/// cannot be deserialized back into this typed authorization object. A future
/// live runner must call `authorize` again in-process.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SourcePreflightAuthorization {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub source_preflight_schema_version: u64,
    /// Diagnostic hash over the current deterministic context plus the exact
    /// current task generation. It is evidence of what was checked, not a permit.
    pub current_state_binding_hash: String,
    pub gate_ledger_hash: String,
    pub current_generation_verified: bool,
    pub current_user_approval_verified: bool,
    pub source_metadata_read_authorized: bool,
    /// This object is not a transferable capability. A future network runner
    /// must call `authorize` again against the state/ledger it is about to use.
    pub reusable_permit: bool,
    pub image_download_authorized: bool,
    pub staging_write_authorized: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn current_task<'a>(state: &'a State, task_id: &str) -> Result<&'a crate::monitor::Task, String> {
    let mut matches = state
        .pending
        .values()
        .filter(|task| task.task_id == task_id);
    let task = matches.next().ok_or("SOURCE_PREFLIGHT_TASK_NOT_FOUND")?;
    if matches.next().is_some() {
        return Err("AMBIGUOUS_SOURCE_PREFLIGHT_TASK_ID".into());
    }
    Ok(task)
}

/// Revalidate the complete current generation before any source metadata read.
///
/// Successful return authorizes only a live, in-process preflight metadata
/// enumeration for the currently thawed V1 add-only path. Upgrade/replacement
/// tasks remain frozen: the exact current task must be a new-work `download`
/// with no old local item binding. This does not authorize image bytes, staging
/// writes, task state, promotion, replacement, or deletion, and the returned
/// object is not reusable.
pub fn authorize(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
) -> Result<SourcePreflightAuthorization, String> {
    assistant_task_gate::validate_ledger(ledger)?;

    let regenerated_plan = local_executor::plan(command)?;
    if &regenerated_plan != plan {
        return Err("SOURCE_PREFLIGHT_PLAN_COMMAND_MISMATCH".into());
    }
    source_bridge_request::validate(plan, request)?;

    let task = current_task(state, &command.task_id)?;
    let current_target_hash = assistant_task_gate::target_hash(task);
    if task.work_id != command.work_id
        || task.task_revision != command.task_revision
        || current_target_hash != command.target_hash
        || task.action != command.action
    {
        return Err("SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT".into());
    }

    // The 2026-09-08 thaw decision is intentionally narrow: only genuinely
    // additive new-work downloads may enter the live source chain. Keeping this
    // check in the immediate preflight authorization barrier means an `upgrade`
    // or any task already bound to local items fails before the first source
    // metadata request, not merely before a later media write.
    if task.action != "download" || command.action != "download" {
        return Err("V1_ADD_ONLY_LIVE_EXECUTION_REQUIRES_DOWNLOAD_ACTION".into());
    }
    if !task.old_local_item_ids.is_empty() {
        return Err("V1_ADD_ONLY_LIVE_EXECUTION_REQUIRES_NEW_WORK".into());
    }

    let gate = assistant_task_gate::task_view(state, ledger, &task.task_id)?;
    if gate["download_authorized"] != true || gate["user_approved"] != true {
        return Err("SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED".into());
    }

    if request.command_id != command.command_id
        || request.task_id != command.task_id
        || request.work_id != command.work_id
        || request.task_revision != command.task_revision
        || request.target_hash != command.target_hash
        || request.source != command.source
        || request.source_work_id != command.source_work_id
    {
        return Err("SOURCE_PREFLIGHT_REQUEST_COMMAND_MISMATCH".into());
    }

    let current_state_binding_hash = hash(&(
        state.context(),
        task.task_id.as_str(),
        task.work_id.as_str(),
        task.task_revision,
        current_target_hash.as_str(),
        task.action.as_str(),
        task.status.as_str(),
    ));

    Ok(SourcePreflightAuthorization {
        schema_version: SOURCE_PREFLIGHT_AUTHORIZATION_SCHEMA_VERSION,
        command_id: command.command_id.clone(),
        task_id: command.task_id.clone(),
        work_id: command.work_id.clone(),
        task_revision: command.task_revision,
        target_hash: command.target_hash.clone(),
        source: command.source.clone(),
        source_work_id: command.source_work_id.clone(),
        source_preflight_schema_version: SOURCE_PREFLIGHT_SCHEMA_VERSION,
        current_state_binding_hash,
        gate_ledger_hash: hash(ledger),
        current_generation_verified: true,
        current_user_approval_verified: true,
        source_metadata_read_authorized: true,
        reusable_permit: false,
        image_download_authorized: false,
        staging_write_authorized: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

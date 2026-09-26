//! V1.2/V1.3 local/Windows-only execution orchestration.
//!
//! This module composes the already-audited A5/A6 boundaries into one local
//! execution path. It is not a GitHub Actions runner and grants no monitor-state,
//! inventory, task-completion, promotion, replacement, or deletion authority.

use crate::{
    assistant_task_gate::GateLedger,
    executor_handoff::{self, ExecutorCommand, ExecutorReceipt},
    filesystem_verifier::FilesystemVerification,
    image_download_authorization,
    isolated_staging_execution::IsolatedStagingExecutionContext,
    live_media_descriptors, live_media_fetch, live_source_preflight,
    local_executor::{self, LocalExecutionPlan},
    monitor::State,
    source_bridge_request::{self, SourceBridgeRequest},
    source_completion::SourceCompletionProof,
    source_preflight_authorization, verified_execution_receipt,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const LOCAL_EXECUTION_ORCHESTRATOR_SCHEMA_VERSION: u64 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalExecutionReport {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub staging_subdir: String,
    pub preflight_hash: String,
    pub staging_execution_completed: bool,
    pub receipt: ExecutorReceipt,
    /// V1.3 keeps the exact A6.5 manifest-bearing completion proof so a later
    /// local import can re-verify staging without trusting a summary receipt.
    pub source_completion: SourceCompletionProof,
    /// Exact A6.4 filesystem verification observed at download completion.
    /// V1.3 re-runs the verifier and requires byte-for-byte proof equality.
    pub filesystem_verification: FilesystemVerification,
    pub receipt_view: Value,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

/// Rebuild the deterministic local plan/source request and prove that the exact
/// command is still the currently user-approved generation. This performs no
/// network or filesystem mutation.
pub fn prepare_current(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
) -> Result<(LocalExecutionPlan, SourceBridgeRequest), String> {
    let plan = local_executor::plan(command)?;
    let request = source_bridge_request::build(&plan)?;
    let authorization =
        source_preflight_authorization::authorize(state, ledger, command, &plan, &request)?;
    if !authorization.current_generation_verified
        || !authorization.current_user_approval_verified
        || !authorization.source_metadata_read_authorized
        || authorization.reusable_permit
        || authorization.image_download_authorized
        || authorization.staging_write_authorized
        || authorization.inventory_mutation_authorized
        || authorization.task_completion_authorized
        || authorization.promotion_authorized
        || authorization.replacement_authorized
        || authorization.physical_delete_authorized
    {
        return Err("LOCAL_ORCHESTRATOR_UNSAFE_PREPARATION_AUTHORIZATION".into());
    }
    Ok((plan, request))
}

fn reauthorize_image<Reload>(
    reload: &mut Reload,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    live: &live_source_preflight::LiveSourcePreflightResult,
) -> Result<image_download_authorization::ImageDownloadAuthorization, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    let (state, ledger) = reload()?;
    image_download_authorization::authorize(&state, &ledger, command, plan, request, live)
}

fn finalize(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    result: &crate::isolated_staging_execution::IsolatedStagingExecutionResult,
    completed_at: &str,
) -> Result<LocalExecutionReport, String> {
    let receipt = verified_execution_receipt::build(command, plan, result, completed_at)?;
    let receipt_view = executor_handoff::receipt_view(state, ledger, &receipt)?;
    if receipt_view["ready_for_inventory_verification"] != true
        || receipt_view["task_completion_authorized"] != false
        || receipt_view["replacement_authorized"] != false
        || receipt_view["physical_delete_authorized"] != false
    {
        return Err("LOCAL_ORCHESTRATOR_RECEIPT_NOT_CURRENT".into());
    }

    Ok(LocalExecutionReport {
        schema_version: LOCAL_EXECUTION_ORCHESTRATOR_SCHEMA_VERSION,
        command_id: command.command_id.clone(),
        task_id: command.task_id.clone(),
        work_id: command.work_id.clone(),
        task_revision: command.task_revision,
        target_hash: command.target_hash.clone(),
        source: command.source.clone(),
        source_work_id: command.source_work_id.clone(),
        staging_subdir: plan.staging_subdir.clone(),
        preflight_hash: result.preflight_hash.clone(),
        staging_execution_completed: true,
        receipt,
        source_completion: result.source_completion.clone(),
        filesystem_verification: result.filesystem_verification.clone(),
        receipt_view,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    })
}

/// Execute one exact currently approved command using the already accepted A6
/// live-source and command-owned staging chain.
///
/// `reload` must return the caller's current monitor-state/gate snapshot. It is
/// invoked after source preflight and repeatedly by the A6.13/A6.12 layers before
/// source requests, media fetches, file writes, and final acceptance. A future
/// local agent may back this callback with a continuously refreshed checkout;
/// the standalone CLI reloads the supplied local state directory on every call.
///
/// Pica credentials are borrowed only in memory for Pica API metadata calls.
/// A6.14 media GETs deliberately have no credential parameter.
///
/// When `completed_at` is absent, the timestamp is generated only after the
/// staging execution and final current-state reload succeed, so an invocation
/// start time can never masquerade as completion time.
pub async fn execute_live<Reload>(
    initial_state: &State,
    initial_ledger: &GateLedger,
    command: &ExecutorCommand,
    staging_root: &Path,
    pica_token: Option<&str>,
    completed_at: Option<&str>,
    mut reload: Reload,
) -> Result<LocalExecutionReport, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    if let Some(value) = completed_at {
        if value.trim() != value || value.is_empty() || DateTime::parse_from_rfc3339(value).is_err()
        {
            return Err("LOCAL_ORCHESTRATOR_INVALID_COMPLETED_AT".into());
        }
    }

    let (plan, request) = prepare_current(initial_state, initial_ledger, command)?;

    let live = live_source_preflight::run_live(
        initial_state,
        initial_ledger,
        command,
        &plan,
        &request,
        pica_token,
        &mut reload,
    )
    .await?;

    let authorization = reauthorize_image(&mut reload, command, &plan, &request, &live)?;
    let descriptors = live_media_descriptors::run_live(
        &authorization,
        &live.evidence,
        &live.proof,
        pica_token,
        || reauthorize_image(&mut reload, command, &plan, &request, &live),
    )
    .await?;

    let context = IsolatedStagingExecutionContext {
        staging_root,
        plan: &plan,
        authorization: &authorization,
        evidence: &live.evidence,
        preflight: &live.proof,
        descriptors: &descriptors,
    };
    let execution = live_media_fetch::execute_live(context, || {
        reauthorize_image(&mut reload, command, &plan, &request, &live)
    })
    .await?;

    let (final_state, final_ledger) = reload()?;
    let completed_at = completed_at
        .map(str::to_owned)
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    finalize(
        &final_state,
        &final_ledger,
        command,
        &plan,
        &execution,
        &completed_at,
    )
}

/// Explicit desktop continuation; callbacks persist hashes and recheck authority.
// Keep the legacy CLI arguments intact while adding explicit checkpoint hooks.
#[allow(clippy::too_many_arguments)]
pub async fn execute_live_resumable<Reload, Progress>(
    initial_state: &State,
    initial_ledger: &GateLedger,
    command: &ExecutorCommand,
    staging_root: &Path,
    pica_token: Option<&str>,
    completed_at: Option<&str>,
    reload: Reload,
    resume: Option<&crate::isolated_staging_execution::StagingCheckpoint>,
    progress: Progress,
) -> Result<LocalExecutionReport, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
    Progress: FnMut(&crate::isolated_staging_execution::StagingCheckpoint) -> Result<(), String>,
{
    execute_live_resumable_with_output(
        initial_state,
        initial_ledger,
        command,
        staging_root,
        pica_token,
        completed_at,
        reload,
        resume,
        false,
        progress,
    )
    .await
}

/// The desktop caller binds this policy to the confirmed task and reloads it at
/// every authorization point. The legacy entry point always retains WEBP.
#[allow(clippy::too_many_arguments)]
pub async fn execute_live_resumable_with_output<Reload, Progress>(
    initial_state: &State,
    initial_ledger: &GateLedger,
    command: &ExecutorCommand,
    staging_root: &Path,
    pica_token: Option<&str>,
    completed_at: Option<&str>,
    mut reload: Reload,
    resume: Option<&crate::isolated_staging_execution::StagingCheckpoint>,
    jpeg_output: bool,
    progress: Progress,
) -> Result<LocalExecutionReport, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
    Progress: FnMut(&crate::isolated_staging_execution::StagingCheckpoint) -> Result<(), String>,
{
    if std::env::var("GITHUB_ACTIONS").ok().as_deref() == Some("true") {
        return Err("LOCAL_EXECUTOR_GITHUB_ACTIONS_FORBIDDEN".into());
    }
    if let Some(value) = completed_at {
        if value.trim() != value || value.is_empty() || DateTime::parse_from_rfc3339(value).is_err()
        {
            return Err("LOCAL_ORCHESTRATOR_INVALID_COMPLETED_AT".into());
        }
    }

    let (plan, request) = prepare_current(initial_state, initial_ledger, command)?;

    let live = live_source_preflight::run_for_download(
        initial_state,
        initial_ledger,
        command,
        &plan,
        &request,
        pica_token,
        &mut reload,
    )
    .await?;

    let authorization = reauthorize_image(&mut reload, command, &plan, &request, &live)?;
    let mut descriptors = live_media_descriptors::run_for_download(
        &authorization,
        &live.evidence,
        &live.proof,
        pica_token,
        || reauthorize_image(&mut reload, command, &plan, &request, &live),
    )
    .await?;

    if jpeg_output {
        crate::source_media_descriptors::jm_jpeg_output(&mut descriptors)?;
    }
    let context = IsolatedStagingExecutionContext {
        staging_root,
        plan: &plan,
        authorization: &authorization,
        evidence: &live.evidence,
        preflight: &live.proof,
        descriptors: &descriptors,
    };
    let execution = live_media_fetch::execute_live_resumable(
        context,
        resume,
        || reauthorize_image(&mut reload, command, &plan, &request, &live),
        progress,
    )
    .await?;

    let (final_state, final_ledger) = reload()?;
    let completed_at = completed_at
        .map(str::to_owned)
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    finalize(
        &final_state,
        &final_ledger,
        command,
        &plan,
        &execution,
        &completed_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant_task_gate::{target_hash, GateRecord},
        monitor::{hash, Decisions, Scan, Target, Task},
    };
    use serde_json::json;
    use state_model::Version;
    use std::collections::BTreeMap;

    fn fixture() -> (State, GateLedger, ExecutorCommand) {
        let task = Task {
            task_id: "TASK_V1_2".into(),
            work_id: "WORK_V1_2".into(),
            first_seen: "fixed".into(),
            task_revision: 3,
            target: Target {
                source_key: "jm:123456".into(),
                author: "Writer".into(),
                title: "V1.2".into(),
                version: Version::default(),
                coverage: Value::Null,
            },
            action: "download".into(),
            status: "pending".into(),
            old_local_item_ids: Vec::new(),
            binding_authority_hash: String::new(),
        };
        let state = State {
            authors: json!({"authors":[]}),
            inventory: json!({"works":[]}),
            catalog: BTreeMap::new(),
            pending: BTreeMap::from([(task.work_id.clone(), task.clone())]),
            review: BTreeMap::new(),
            cleanup_review: json!([]),
            decisions: Decisions::default(),
            scan: Scan::default(),
        };
        let target_hash = target_hash(&task);
        let ledger = GateLedger {
            schema_version: 1,
            records: vec![GateRecord {
                task_id: task.task_id.clone(),
                task_revision: task.task_revision,
                target_hash: target_hash.clone(),
                assistant_recommended: true,
                user_approved: true,
            }],
        };
        let digest = hash(&(
            task.task_id.as_str(),
            task.task_revision,
            target_hash.as_str(),
        ));
        let command = ExecutorCommand {
            schema_version: 1,
            command_id: format!("EXEC_{}", &digest[..20]),
            task_id: task.task_id,
            work_id: task.work_id,
            task_revision: task.task_revision,
            target_hash,
            source: "jm".into(),
            source_work_id: "123456".into(),
            action: task.action,
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            target: task.target,
        };
        (state, ledger, command)
    }

    #[test]
    fn preparation_is_current_approval_bound_and_staging_only() {
        let (state, ledger, command) = fixture();
        let (plan, request) = prepare_current(&state, &ledger, &command).unwrap();
        assert_eq!(
            plan.staging_subdir,
            format!("commands/{}", command.command_id)
        );
        assert!(!plan.execution_supported);
        assert!(!plan.promotion_authorized);
        assert!(!plan.replacement_authorized);
        assert!(!plan.physical_delete_authorized);
        assert!(!request.network_execution_enabled);
        assert!(!request.staging_write_enabled);
        assert!(!request.inventory_mutation_authorized);
        assert!(!request.task_completion_authorized);
        assert!(!request.promotion_authorized);
        assert!(!request.replacement_authorized);
        assert!(!request.physical_delete_authorized);
    }

    #[test]
    fn revoked_or_revised_task_fails_before_any_live_layer() {
        let (state, mut ledger, command) = fixture();
        ledger.records[0].user_approved = false;
        assert_eq!(
            prepare_current(&state, &ledger, &command).unwrap_err(),
            "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
        );

        let (mut state, ledger, command) = fixture();
        state.pending.get_mut("WORK_V1_2").unwrap().task_revision += 1;
        assert_eq!(
            prepare_current(&state, &ledger, &command).unwrap_err(),
            "SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT"
        );
    }

    #[tokio::test]
    async fn invalid_completion_timestamp_fails_before_reload_or_network() {
        let (state, ledger, command) = fixture();
        let root = std::env::temp_dir();
        let error = execute_live(
            &state,
            &ledger,
            &command,
            &root,
            None,
            Some("not-a-time"),
            || -> Result<(State, GateLedger), String> {
                panic!("invalid timestamp must fail before reload");
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "LOCAL_ORCHESTRATOR_INVALID_COMPLETED_AT");
    }
}

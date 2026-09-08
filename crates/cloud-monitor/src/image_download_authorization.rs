//! A6.10 in-process image-download authorization bound to an exact A6.9 result.
//!
//! This module still downloads and writes nothing. It is the first contract that
//! may positively authorize image-byte download and command-owned staging writes,
//! but only when the exact typed A6.9 live preflight result is still current.

use crate::{
    assistant_task_gate::GateLedger,
    executor_handoff::ExecutorCommand,
    live_source_preflight::{LiveSourcePreflightResult, LIVE_SOURCE_PREFLIGHT_SCHEMA_VERSION},
    local_executor::LocalExecutionPlan,
    monitor::State,
    source_bridge_request::SourceBridgeRequest,
    source_preflight,
    source_preflight_authorization,
};
use serde::Serialize;

pub const IMAGE_DOWNLOAD_AUTHORIZATION_SCHEMA_VERSION: u64 = 1;
const WRITE_SCOPE: &str = "COMMAND_OWNED_STAGING_ONLY";

/// Non-transferable diagnostic result of an immediate image-download gate.
///
/// This type is intentionally Serialize-only. A future downloader must obtain
/// it in-process by calling `authorize` against the current state/gate and the
/// typed A6.9 result; saved JSON cannot be deserialized back into this type.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ImageDownloadAuthorization {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub preflight_hash: String,
    pub expected_chapter_count: u64,
    pub expected_content_units: u64,
    pub staging_subdir: String,
    pub write_scope: String,
    pub current_state_binding_hash: String,
    pub current_gate_ledger_hash: String,
    pub live_preflight_generation_verified: bool,
    pub image_download_authorized: bool,
    pub staging_write_authorized: bool,
    pub reusable_permit: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn validate_live_result(
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    live: &LiveSourcePreflightResult,
) -> Result<(), String> {
    if live.schema_version != LIVE_SOURCE_PREFLIGHT_SCHEMA_VERSION
        || !live.authorization_stable
        || !live.source_metadata_read_completed
        || live.pre_authorization_state_hash != live.post_authorization_state_hash
        || live.pre_authorization_gate_hash != live.post_authorization_gate_hash
    {
        return Err("INVALID_LIVE_SOURCE_PREFLIGHT_RESULT".into());
    }
    if live.command_id != request.command_id
        || live.task_id != request.task_id
        || live.work_id != request.work_id
        || live.task_revision != request.task_revision
        || live.target_hash != request.target_hash
        || live.source != request.source
        || live.source_work_id != request.source_work_id
    {
        return Err("IMAGE_DOWNLOAD_PREFLIGHT_BINDING_MISMATCH".into());
    }
    if live.image_download_authorized
        || live.staging_write_authorized
        || live.inventory_mutation_authorized
        || live.task_completion_authorized
        || live.promotion_authorized
        || live.replacement_authorized
        || live.physical_delete_authorized
    {
        return Err("UNSAFE_LIVE_SOURCE_PREFLIGHT_RESULT".into());
    }

    let recomputed = source_preflight::validate(plan, request, &live.evidence)?;
    if recomputed != live.proof
        || !recomputed.source_scope_verified
        || recomputed.image_download_authorized
        || recomputed.staging_write_authorized
        || recomputed.inventory_mutation_authorized
        || recomputed.task_completion_authorized
        || recomputed.promotion_authorized
        || recomputed.replacement_authorized
        || recomputed.physical_delete_authorized
    {
        return Err("IMAGE_DOWNLOAD_PREFLIGHT_PROOF_MISMATCH".into());
    }
    Ok(())
}

/// Authorize only image-byte download plus writes below the exact command-owned
/// staging subdirectory. All monitor-state and promotion capabilities remain
/// closed, and the returned object is deliberately non-reusable.
pub fn authorize(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    live: &LiveSourcePreflightResult,
) -> Result<ImageDownloadAuthorization, String> {
    validate_live_result(plan, request, live)?;

    let current = source_preflight_authorization::authorize(state, ledger, command, plan, request)?;
    if current.current_state_binding_hash != live.post_authorization_state_hash
        || current.gate_ledger_hash != live.post_authorization_gate_hash
    {
        return Err("IMAGE_DOWNLOAD_PREFLIGHT_GENERATION_STALE".into());
    }

    Ok(ImageDownloadAuthorization {
        schema_version: IMAGE_DOWNLOAD_AUTHORIZATION_SCHEMA_VERSION,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        preflight_hash: live.proof.preflight_hash.clone(),
        expected_chapter_count: live.proof.expected_chapter_count,
        expected_content_units: live.proof.expected_content_units,
        staging_subdir: plan.staging_subdir.clone(),
        write_scope: WRITE_SCOPE.into(),
        current_state_binding_hash: current.current_state_binding_hash,
        current_gate_ledger_hash: current.gate_ledger_hash,
        live_preflight_generation_verified: true,
        image_download_authorized: true,
        staging_write_authorized: true,
        reusable_permit: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant_task_gate::{target_hash, GateRecord},
        live_source_preflight::LiveSourcePreflightResult,
        monitor::{hash, Decisions, Scan, Target, Task},
        source_bridge_request,
        source_preflight::{PreflightChapter, SourcePreflightEvidence},
    };
    use serde_json::{json, Value};
    use state_model::Version;
    use std::collections::BTreeMap;

    fn setup() -> (
        State,
        GateLedger,
        ExecutorCommand,
        LocalExecutionPlan,
        SourceBridgeRequest,
        LiveSourcePreflightResult,
    ) {
        let task = Task {
            task_id: "TASK_A6_10".into(),
            work_id: "WORK_A6_10".into(),
            first_seen: "fixed".into(),
            task_revision: 1,
            target: Target {
                source_key: "jm:123456".into(),
                author: "Writer".into(),
                title: "A6.10".into(),
                version: Version::default(),
                coverage: Value::Null,
            },
            action: "download".into(),
            status: "pending".into(),
            old_local_item_ids: Vec::new(),
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
        let ledger = GateLedger {
            schema_version: 1,
            records: vec![GateRecord {
                task_id: task.task_id.clone(),
                task_revision: task.task_revision,
                target_hash: target_hash(&task),
                assistant_recommended: true,
                user_approved: true,
            }],
        };
        let target_hash = target_hash(&task);
        let digest = hash(&(task.task_id.as_str(), task.task_revision, target_hash.as_str()));
        let command = ExecutorCommand {
            schema_version: 1,
            command_id: format!("EXEC_{}", &digest[..20]),
            task_id: task.task_id.clone(),
            work_id: task.work_id.clone(),
            task_revision: task.task_revision,
            target_hash,
            source: "jm".into(),
            source_work_id: "123456".into(),
            action: "download".into(),
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            target: task.target,
        };
        let plan = crate::local_executor::plan(&command).unwrap();
        let request = source_bridge_request::build(&plan).unwrap();
        let evidence = SourcePreflightEvidence {
            schema_version: source_preflight::SOURCE_PREFLIGHT_SCHEMA_VERSION,
            command_id: request.command_id.clone(),
            task_id: request.task_id.clone(),
            work_id: request.work_id.clone(),
            task_revision: request.task_revision,
            target_hash: request.target_hash.clone(),
            source: request.source.clone(),
            source_work_id: request.source_work_id.clone(),
            upstream_commit: request.upstream_commit.clone(),
            completion_contract_version: request.completion_contract_version,
            scope: request.scope.clone(),
            source_enumeration_complete: true,
            chapter_pagination: None,
            expected_chapter_count: 1,
            chapters: vec![PreflightChapter {
                chapter_id: "123456".into(),
                chapter_order: 1,
                expected_images: 3,
                image_pagination: None,
            }],
            image_bytes_downloaded: false,
            staging_written: false,
        };
        let proof = source_preflight::validate(&plan, &request, &evidence).unwrap();
        let current = source_preflight_authorization::authorize(
            &state, &ledger, &command, &plan, &request,
        )
        .unwrap();
        let live = LiveSourcePreflightResult {
            schema_version: LIVE_SOURCE_PREFLIGHT_SCHEMA_VERSION,
            command_id: request.command_id.clone(),
            task_id: request.task_id.clone(),
            work_id: request.work_id.clone(),
            task_revision: request.task_revision,
            target_hash: request.target_hash.clone(),
            source: request.source.clone(),
            source_work_id: request.source_work_id.clone(),
            authorization_stable: true,
            pre_authorization_state_hash: current.current_state_binding_hash.clone(),
            post_authorization_state_hash: current.current_state_binding_hash,
            pre_authorization_gate_hash: current.gate_ledger_hash.clone(),
            post_authorization_gate_hash: current.gate_ledger_hash,
            source_metadata_read_completed: true,
            evidence,
            proof,
            image_download_authorized: false,
            staging_write_authorized: false,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        (state, ledger, command, plan, request, live)
    }

    #[test]
    fn exact_current_live_preflight_authorizes_only_command_staging_download() {
        let (state, ledger, command, plan, request, live) = setup();
        let auth = authorize(&state, &ledger, &command, &plan, &request, &live).unwrap();
        assert!(auth.live_preflight_generation_verified);
        assert!(auth.image_download_authorized);
        assert!(auth.staging_write_authorized);
        assert!(!auth.reusable_permit);
        assert_eq!(auth.preflight_hash, live.proof.preflight_hash);
        assert_eq!(auth.staging_subdir, format!("commands/{}", command.command_id));
        assert_eq!(auth.write_scope, "COMMAND_OWNED_STAGING_ONLY");
        assert!(!auth.inventory_mutation_authorized);
        assert!(!auth.task_completion_authorized);
        assert!(!auth.promotion_authorized);
        assert!(!auth.replacement_authorized);
        assert!(!auth.physical_delete_authorized);
    }

    #[test]
    fn current_gate_change_after_preflight_invalidates_image_download() {
        let (state, mut ledger, command, plan, request, live) = setup();
        ledger.records[0].assistant_recommended = false;
        assert_eq!(
            authorize(&state, &ledger, &command, &plan, &request, &live).unwrap_err(),
            "IMAGE_DOWNLOAD_PREFLIGHT_GENERATION_STALE"
        );
    }

    #[test]
    fn revoked_approval_after_preflight_invalidates_image_download() {
        let (state, mut ledger, command, plan, request, live) = setup();
        ledger.records[0].user_approved = false;
        assert_eq!(
            authorize(&state, &ledger, &command, &plan, &request, &live).unwrap_err(),
            "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
        );
    }

    #[test]
    fn forged_preflight_proof_or_unsafe_live_capability_fails_closed() {
        let (state, ledger, command, plan, request, mut live) = setup();
        live.proof.preflight_hash = "0".repeat(64);
        assert_eq!(
            authorize(&state, &ledger, &command, &plan, &request, &live).unwrap_err(),
            "IMAGE_DOWNLOAD_PREFLIGHT_PROOF_MISMATCH"
        );

        let (state, ledger, command, plan, request, mut live) = setup();
        live.staging_write_authorized = true;
        assert_eq!(
            authorize(&state, &ledger, &command, &plan, &request, &live).unwrap_err(),
            "UNSAFE_LIVE_SOURCE_PREFLIGHT_RESULT"
        );
    }

    #[test]
    fn changed_task_generation_after_preflight_fails_closed() {
        let (mut state, ledger, command, plan, request, live) = setup();
        state.pending.get_mut("WORK_A6_10").unwrap().task_revision = 2;
        assert_eq!(
            authorize(&state, &ledger, &command, &plan, &request, &live).unwrap_err(),
            "SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT"
        );
    }
}

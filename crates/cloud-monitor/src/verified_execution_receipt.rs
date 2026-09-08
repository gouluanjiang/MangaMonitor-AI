//! A6.15 verified execution-to-receipt bridge.
//!
//! This module converts an already-completed A6.12 staging execution into the
//! existing A6.1 `ExecutorReceipt` shape only after re-validating the immutable
//! command/plan/source/manifest/filesystem proof chain. It does not mutate
//! monitor state, inventory, tasks, library files, replacements, or deletions.

use crate::{
    executor_handoff::{ExecutorCommand, ExecutorReceipt, EXECUTOR_SCHEMA_VERSION},
    filesystem_verifier::FILESYSTEM_VERIFIER_SCHEMA_VERSION,
    isolated_staging_execution::{
        IsolatedStagingExecutionResult, ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
    },
    local_executor::{self, LocalExecutionPlan},
    source_completion::{
        JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT, SOURCE_COMPLETION_SCHEMA_VERSION,
    },
    staging_manifest,
};
use chrono::DateTime;

pub const VERIFIED_EXECUTION_RECEIPT_SCHEMA_VERSION: u64 = 1;

fn no_downstream_authority(result: &IsolatedStagingExecutionResult) -> bool {
    !result.inventory_mutation_authorized
        && !result.task_completion_authorized
        && !result.promotion_authorized
        && !result.replacement_authorized
        && !result.physical_delete_authorized
        && !result.source_completion.inventory_mutation_authorized
        && !result.source_completion.task_completion_authorized
        && !result.source_completion.promotion_authorized
        && !result.source_completion.replacement_authorized
        && !result.source_completion.physical_delete_authorized
        && !result.filesystem_verification.inventory_mutation_authorized
        && !result.filesystem_verification.task_completion_authorized
        && !result.filesystem_verification.promotion_authorized
        && !result.filesystem_verification.replacement_authorized
        && !result.filesystem_verification.physical_delete_authorized
}

fn exact_result_binding(
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    result: &IsolatedStagingExecutionResult,
) -> Result<(), String> {
    let derived_plan = local_executor::plan(command)?;
    if &derived_plan != plan {
        return Err("VERIFIED_RECEIPT_PLAN_COMMAND_MISMATCH".into());
    }
    if result.schema_version != ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION
        || result.command_id != plan.command_id
        || result.task_id != plan.task_id
        || result.work_id != plan.work_id
        || result.task_revision != plan.task_revision
        || result.target_hash != plan.target_hash
        || result.source_work_id != plan.source_work_id
        || result.source != command.source
    {
        return Err("VERIFIED_RECEIPT_EXECUTION_BINDING_MISMATCH".into());
    }
    if !result.staging_execution_completed {
        return Err("VERIFIED_RECEIPT_STAGING_NOT_COMPLETED".into());
    }
    Ok(())
}

fn validate_source_proof(result: &IsolatedStagingExecutionResult) -> Result<(), String> {
    let completion = &result.source_completion;
    if completion.schema_version != SOURCE_COMPLETION_SCHEMA_VERSION
        || !completion.source_contract_verified
        || completion.source != result.source
    {
        return Err("VERIFIED_RECEIPT_SOURCE_COMPLETION_INVALID".into());
    }
    let expected_upstream = match result.source.as_str() {
        "jm" => JM_UPSTREAM_COMMIT,
        "pica" => PICA_UPSTREAM_COMMIT,
        _ => return Err("VERIFIED_RECEIPT_SOURCE_COMPLETION_INVALID".into()),
    };
    if completion.upstream_commit != expected_upstream {
        return Err("VERIFIED_RECEIPT_UPSTREAM_COMMIT_MISMATCH".into());
    }
    // `execution_supported=false` remains the historical A6.5 capability bit;
    // it must not be repurposed into downstream authority by this bridge.
    if completion.execution_supported {
        return Err("VERIFIED_RECEIPT_UNSAFE_SOURCE_CAPABILITY".into());
    }
    Ok(())
}

fn validate_proof_chain(
    plan: &LocalExecutionPlan,
    result: &IsolatedStagingExecutionResult,
) -> Result<crate::staging_manifest::ValidatedStagingManifest, String> {
    validate_source_proof(result)?;

    let validated = staging_manifest::validate(plan, &result.source_completion.manifest)?;
    let filesystem = &result.filesystem_verification;
    if filesystem.schema_version != FILESYSTEM_VERIFIER_SCHEMA_VERSION
        || !filesystem.filesystem_verified
        || filesystem.command_id != validated.command_id
        || filesystem.task_id != validated.task_id
        || filesystem.work_id != validated.work_id
        || filesystem.task_revision != validated.task_revision
        || filesystem.target_hash != validated.target_hash
        || filesystem.manifest_hash != validated.manifest_hash
        || filesystem.file_count != validated.file_count
        || filesystem.total_bytes != validated.total_bytes
    {
        return Err("VERIFIED_RECEIPT_FILESYSTEM_PROOF_MISMATCH".into());
    }
    if !no_downstream_authority(result) {
        return Err("VERIFIED_RECEIPT_UNSAFE_DOWNSTREAM_AUTHORITY".into());
    }
    Ok(validated)
}

fn validate_completed_at(value: &str) -> Result<(), String> {
    if value.trim() != value || value.is_empty() {
        return Err("VERIFIED_RECEIPT_INVALID_COMPLETED_AT".into());
    }
    DateTime::parse_from_rfc3339(value).map_err(|_| "VERIFIED_RECEIPT_INVALID_COMPLETED_AT")?;
    Ok(())
}

/// Build the existing A6.1 success receipt from a fully verified A6.12 result.
///
/// This does not authorize inventory mutation or task completion. The returned
/// receipt must still be passed through A6.1 `receipt_view`, which checks the
/// *current* pending task and *current* user-approval generation before exposing
/// `ready_for_inventory_verification=true`.
pub fn build(
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    result: &IsolatedStagingExecutionResult,
    completed_at: &str,
) -> Result<ExecutorReceipt, String> {
    exact_result_binding(command, plan, result)?;
    let validated = validate_proof_chain(plan, result)?;
    validate_completed_at(completed_at)?;

    Ok(ExecutorReceipt {
        schema_version: EXECUTOR_SCHEMA_VERSION,
        command_id: validated.command_id,
        task_id: validated.task_id,
        work_id: validated.work_id,
        task_revision: validated.task_revision,
        target_hash: validated.target_hash,
        outcome: "SUCCEEDED".into(),
        completed_at: completed_at.into(),
        completion_evidence: Some(validated.completion_evidence),
        error_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        filesystem_verifier::FilesystemVerification,
        isolated_staging_execution::IsolatedStagingExecutionResult,
        monitor::{hash, Target},
        source_completion::SourceCompletionProof,
        staging_manifest::{StagedArtifact, StagingManifest, STAGING_MANIFEST_SCHEMA_VERSION},
    };
    use serde_json::Value;
    use state_model::Version;

    fn command() -> ExecutorCommand {
        let target = Target {
            source_key: "jm:123456".into(),
            author: "Writer".into(),
            title: "Verified receipt fixture".into(),
            version: Version::default(),
            coverage: Value::Null,
        };
        let target_hash = hash(&target);
        let task_id = "TASK_A6_15".to_string();
        let task_revision = 1;
        let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
        ExecutorCommand {
            schema_version: EXECUTOR_SCHEMA_VERSION,
            command_id: format!("EXEC_{}", &digest[..20]),
            task_id,
            work_id: "WORK_A6_15".into(),
            task_revision,
            target_hash,
            source: "jm".into(),
            source_work_id: "123456".into(),
            action: "download".into(),
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            target,
        }
    }

    fn fixture() -> (ExecutorCommand, LocalExecutionPlan, IsolatedStagingExecutionResult) {
        let command = command();
        let plan = local_executor::plan(&command).unwrap();
        let manifest = StagingManifest {
            schema_version: STAGING_MANIFEST_SCHEMA_VERSION,
            command_id: plan.command_id.clone(),
            task_id: plan.task_id.clone(),
            work_id: plan.work_id.clone(),
            task_revision: plan.task_revision,
            target_hash: plan.target_hash.clone(),
            backend: plan.backend.clone(),
            source_work_id: plan.source_work_id.clone(),
            staging_subdir: plan.staging_subdir.clone(),
            source_enumeration_complete: true,
            all_scheduled_downloads_joined: true,
            downloader_reported_full_completion: true,
            expected_content_units: 1,
            completed_content_units: 1,
            failed_content_units: 0,
            artifacts: vec![StagedArtifact {
                relative_path: "chapters/000001-123456/000001.webp".into(),
                size_bytes: 12,
                sha256: "a".repeat(64),
            }],
        };
        let validated = staging_manifest::validate(&plan, &manifest).unwrap();
        let source_completion = SourceCompletionProof {
            schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
            source: "jm".into(),
            upstream_commit: JM_UPSTREAM_COMMIT.into(),
            source_contract_verified: true,
            execution_supported: false,
            manifest,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        let filesystem_verification = FilesystemVerification {
            schema_version: FILESYSTEM_VERIFIER_SCHEMA_VERSION,
            command_id: validated.command_id.clone(),
            task_id: validated.task_id.clone(),
            work_id: validated.work_id.clone(),
            task_revision: validated.task_revision,
            target_hash: validated.target_hash.clone(),
            manifest_hash: validated.manifest_hash.clone(),
            file_count: validated.file_count,
            total_bytes: validated.total_bytes,
            filesystem_verified: true,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        let result = IsolatedStagingExecutionResult {
            schema_version: ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
            command_id: plan.command_id.clone(),
            task_id: plan.task_id.clone(),
            work_id: plan.work_id.clone(),
            task_revision: plan.task_revision,
            target_hash: plan.target_hash.clone(),
            source: "jm".into(),
            source_work_id: plan.source_work_id.clone(),
            preflight_hash: "b".repeat(64),
            staging_execution_completed: true,
            source_completion,
            filesystem_verification,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        (command, plan, result)
    }

    #[test]
    fn exact_verified_execution_builds_a6_1_success_receipt() {
        let (command, plan, result) = fixture();
        let receipt = build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap();
        assert_eq!(receipt.outcome, "SUCCEEDED");
        assert_eq!(receipt.command_id, command.command_id);
        assert_eq!(receipt.completed_at, "2026-09-07T08:30:00Z");
        let evidence = receipt.completion_evidence.unwrap();
        assert!(evidence.downloader_reported_full_completion);
        assert_eq!(evidence.file_count, 1);
        assert!(!evidence.artifact_manifest_hash.is_empty());
    }

    #[test]
    fn source_or_upstream_drift_fails_closed() {
        let (command, plan, mut result) = fixture();
        result.source_completion.upstream_commit = "0".repeat(40);
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_UPSTREAM_COMMIT_MISMATCH"
        );

        let (_, _, mut result) = fixture();
        result.source_completion.source = "pica".into();
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_SOURCE_COMPLETION_INVALID"
        );
    }

    #[test]
    fn filesystem_manifest_drift_fails_closed() {
        let (command, plan, mut result) = fixture();
        result.filesystem_verification.manifest_hash = "c".repeat(64);
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_FILESYSTEM_PROOF_MISMATCH"
        );
    }

    #[test]
    fn incomplete_staging_cannot_become_success_receipt() {
        let (command, plan, mut result) = fixture();
        result.staging_execution_completed = false;
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_STAGING_NOT_COMPLETED"
        );
    }

    #[test]
    fn downstream_authority_injection_fails_closed() {
        let (command, plan, mut result) = fixture();
        result.task_completion_authorized = true;
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_UNSAFE_DOWNSTREAM_AUTHORITY"
        );
    }

    #[test]
    fn command_plan_generation_drift_fails_closed() {
        let (mut command, plan, result) = fixture();
        command.source = "pica".into();
        command.source_work_id = "222222222222222222222222".into();
        command.target.source_key = "pica:222222222222222222222222".into();
        command.target_hash = hash(&command.target);
        let digest = hash(&(
            command.task_id.as_str(),
            command.task_revision,
            command.target_hash.as_str(),
        ));
        command.command_id = format!("EXEC_{}", &digest[..20]);
        assert_eq!(
            build(&command, &plan, &result, "2026-09-07T08:30:00Z").unwrap_err(),
            "VERIFIED_RECEIPT_PLAN_COMMAND_MISMATCH"
        );
    }

    #[test]
    fn completed_at_must_be_rfc3339() {
        let (command, plan, result) = fixture();
        assert_eq!(
            build(&command, &plan, &result, "not-a-time").unwrap_err(),
            "VERIFIED_RECEIPT_INVALID_COMPLETED_AT"
        );
    }
}

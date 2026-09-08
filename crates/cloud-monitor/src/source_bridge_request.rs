//! Disabled source-bridge request contract for A6.6.
//!
//! Converts an accepted local execution plan into the exact requirements a
//! future JM/Pica worker must satisfy. It deliberately grants no network or
//! filesystem execution capability.

use crate::{
    local_executor::{LocalExecutionPlan, LOCAL_EXECUTOR_SCHEMA_VERSION},
    monitor::hash,
    source_completion::{
        JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT, SOURCE_COMPLETION_SCHEMA_VERSION,
    },
};
use serde::{Deserialize, Serialize};

pub const SOURCE_BRIDGE_REQUEST_SCHEMA_VERSION: u64 = 1;
const FULL_SCOPE: &str = "FULL_SOURCE_WORK";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceBridgeRequest {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub upstream_commit: String,
    pub scope: String,
    pub staging_subdir: String,
    pub auth_mode: String,
    pub completion_contract_version: u64,
    pub require_complete_chapter_enumeration: bool,
    pub require_complete_image_enumeration: bool,
    pub require_all_downloads_joined: bool,
    pub require_terminal_completed_state: bool,
    pub require_exact_artifact_hashes: bool,
    pub allow_task_create_return_as_completion: bool,
    pub network_execution_enabled: bool,
    pub staging_write_enabled: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn expected_command_id(plan: &LocalExecutionPlan) -> String {
    let digest = hash(&(
        plan.task_id.as_str(),
        plan.task_revision,
        plan.target_hash.as_str(),
    ));
    format!("EXEC_{}", &digest[..20])
}

fn valid_jm_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_pica_id(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_plan(plan: &LocalExecutionPlan) -> Result<(&'static str, &'static str, &'static str), String> {
    if plan.schema_version != LOCAL_EXECUTOR_SCHEMA_VERSION
        || plan.intent != "DOWNLOAD_TO_STAGING_ONLY"
        || plan.command_id != expected_command_id(plan)
        || plan.task_id.trim().is_empty()
        || plan.work_id.trim().is_empty()
        || plan.task_revision == 0
        || !lower_hex_sha256(&plan.target_hash)
        || plan.staging_subdir != format!("commands/{}", plan.command_id)
        || plan.execution_supported
        || plan.promotion_authorized
        || plan.replacement_authorized
        || plan.physical_delete_authorized
    {
        return Err("INVALID_SOURCE_BRIDGE_PLAN".into());
    }
    match plan.backend.as_str() {
        "JM" if valid_jm_id(&plan.source_work_id) => Ok(("jm", JM_UPSTREAM_COMMIT, "NONE")),
        "PICA" if valid_pica_id(&plan.source_work_id) => {
            Ok(("pica", PICA_UPSTREAM_COMMIT, "PICA_TOKEN_REQUIRED"))
        }
        "JM" | "PICA" => Err("INVALID_SOURCE_BRIDGE_SOURCE_WORK_ID".into()),
        _ => Err("UNSUPPORTED_SOURCE_BRIDGE_BACKEND".into()),
    }
}

pub fn validate(plan: &LocalExecutionPlan, request: &SourceBridgeRequest) -> Result<(), String> {
    let (source, upstream_commit, auth_mode) = validate_plan(plan)?;
    if request.schema_version != SOURCE_BRIDGE_REQUEST_SCHEMA_VERSION
        || request.command_id != plan.command_id
        || request.task_id != plan.task_id
        || request.work_id != plan.work_id
        || request.task_revision != plan.task_revision
        || request.target_hash != plan.target_hash
        || request.source != source
        || request.source_work_id != plan.source_work_id
        || request.upstream_commit != upstream_commit
        || request.scope != FULL_SCOPE
        || request.staging_subdir != plan.staging_subdir
        || request.auth_mode != auth_mode
        || request.completion_contract_version != SOURCE_COMPLETION_SCHEMA_VERSION
    {
        return Err("SOURCE_BRIDGE_REQUEST_BINDING_MISMATCH".into());
    }
    if !request.require_complete_chapter_enumeration
        || !request.require_complete_image_enumeration
        || !request.require_all_downloads_joined
        || !request.require_terminal_completed_state
        || !request.require_exact_artifact_hashes
        || request.allow_task_create_return_as_completion
    {
        return Err("UNSAFE_SOURCE_BRIDGE_COMPLETION_REQUIREMENTS".into());
    }
    if request.network_execution_enabled
        || request.staging_write_enabled
        || request.inventory_mutation_authorized
        || request.task_completion_authorized
        || request.promotion_authorized
        || request.replacement_authorized
        || request.physical_delete_authorized
    {
        return Err("UNSAFE_SOURCE_BRIDGE_CAPABILITIES".into());
    }
    Ok(())
}

/// Build a pinned, execution-disabled request for a future source worker.
pub fn build(plan: &LocalExecutionPlan) -> Result<SourceBridgeRequest, String> {
    let (source, upstream_commit, auth_mode) = validate_plan(plan)?;
    let request = SourceBridgeRequest {
        schema_version: SOURCE_BRIDGE_REQUEST_SCHEMA_VERSION,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        source: source.into(),
        source_work_id: plan.source_work_id.clone(),
        upstream_commit: upstream_commit.into(),
        scope: FULL_SCOPE.into(),
        staging_subdir: plan.staging_subdir.clone(),
        auth_mode: auth_mode.into(),
        completion_contract_version: SOURCE_COMPLETION_SCHEMA_VERSION,
        require_complete_chapter_enumeration: true,
        require_complete_image_enumeration: true,
        require_all_downloads_joined: true,
        require_terminal_completed_state: true,
        require_exact_artifact_hashes: true,
        allow_task_create_return_as_completion: false,
        network_execution_enabled: false,
        staging_write_enabled: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    validate(plan, &request)?;
    Ok(request)
}

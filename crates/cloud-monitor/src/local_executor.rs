//! Safe local-executor planning skeleton for A6.2.
//!
//! This module deliberately performs no network, download, archive write,
//! promotion, replacement, or deletion. It only validates an A6.1 executor
//! command and derives an isolated staging plan.

use crate::{
    executor_handoff::{ExecutorCommand, EXECUTOR_SCHEMA_VERSION},
    monitor::hash,
};
use serde::{Deserialize, Serialize};

pub const LOCAL_EXECUTOR_SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalExecutionPlan {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub backend: String,
    pub source_work_id: String,
    pub intent: String,
    pub staging_subdir: String,
    pub execution_supported: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn expected_command_id(command: &ExecutorCommand) -> String {
    let digest = hash(&(
        command.task_id.as_str(),
        command.task_revision,
        command.target_hash.as_str(),
    ));
    format!("EXEC_{}", &digest[..20])
}

/// Validate an A6.1 command and derive a staging-only local plan.
///
/// `execution_supported=false` is intentional in A6.2 skeleton: a later phase
/// must provide a trusted JM/Pica completion bridge before any real download can
/// be executed through this interface.
pub fn plan(command: &ExecutorCommand) -> Result<LocalExecutionPlan, String> {
    if command.schema_version != EXECUTOR_SCHEMA_VERSION {
        return Err("INVALID_LOCAL_EXECUTOR_COMMAND_SCHEMA".into());
    }
    if command.intent != "DOWNLOAD_TO_STAGING_ONLY" {
        return Err("UNSAFE_LOCAL_EXECUTOR_INTENT".into());
    }
    if !matches!(command.action.as_str(), "download" | "upgrade") {
        return Err("UNSUPPORTED_LOCAL_EXECUTOR_ACTION".into());
    }
    if command.task_id.trim().is_empty()
        || command.work_id.trim().is_empty()
        || command.task_revision == 0
        || command.target_hash.trim().is_empty()
        || !safe_component(&command.command_id)
        || command.command_id != expected_command_id(command)
    {
        return Err("INVALID_LOCAL_EXECUTOR_BINDING".into());
    }
    if hash(&command.target) != command.target_hash {
        return Err("LOCAL_EXECUTOR_TARGET_HASH_MISMATCH".into());
    }
    let backend = match command.source.as_str() {
        "jm" => "JM",
        "pica" => "PICA",
        _ => return Err("UNSUPPORTED_LOCAL_EXECUTOR_SOURCE".into()),
    };
    if command.source_work_id.trim().is_empty()
        || command.target.source_key != format!("{}:{}", command.source, command.source_work_id)
    {
        return Err("LOCAL_EXECUTOR_SOURCE_BINDING_MISMATCH".into());
    }

    Ok(LocalExecutionPlan {
        schema_version: LOCAL_EXECUTOR_SCHEMA_VERSION,
        command_id: command.command_id.clone(),
        task_id: command.task_id.clone(),
        work_id: command.work_id.clone(),
        task_revision: command.task_revision,
        target_hash: command.target_hash.clone(),
        backend: backend.into(),
        source_work_id: command.source_work_id.clone(),
        intent: command.intent.clone(),
        staging_subdir: format!("commands/{}", command.command_id),
        execution_supported: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

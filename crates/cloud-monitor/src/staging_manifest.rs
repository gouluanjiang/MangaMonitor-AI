//! Source-independent staging artifact/completion protocol for A6.3.
//!
//! This module validates metadata only. It does not touch the filesystem, call
//! JM/Pica, promote staging content, mutate inventory, complete tasks, replace
//! old versions, or delete anything.

use crate::{
    executor_handoff::CompletionEvidence,
    local_executor::{LocalExecutionPlan, LOCAL_EXECUTOR_SCHEMA_VERSION},
    monitor::hash,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const STAGING_MANIFEST_SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedArtifact {
    /// Portable path relative to the command-owned staging root. `/` is the only
    /// allowed separator; absolute, parent, empty, or Windows-unsafe segments
    /// are rejected.
    pub relative_path: String,
    pub size_bytes: u64,
    /// Lower-case SHA-256 hex of the exact staged file bytes.
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagingManifest {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub backend: String,
    pub source_work_id: String,
    pub staging_subdir: String,

    /// Source-specific bridge asserts it enumerated the complete requested
    /// source scope. This is the fail-closed barrier for incomplete pagination.
    pub source_enumeration_complete: bool,
    /// Source-specific bridge asserts all scheduled download work has actually
    /// joined/completed, rather than merely being spawned.
    pub all_scheduled_downloads_joined: bool,
    pub downloader_reported_full_completion: bool,
    pub expected_content_units: u64,
    pub completed_content_units: u64,
    pub failed_content_units: u64,

    /// Must be strictly sorted by `relative_path` for one canonical manifest
    /// serialization/hash and must contain no duplicate/colliding paths.
    pub artifacts: Vec<StagedArtifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatedStagingManifest {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub manifest_hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub completion_evidence: CompletionEvidence,
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

fn safe_command_id(value: &str) -> bool {
    value.starts_with("EXEC_")
        && value.len() > "EXEC_".len()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn reserved_windows_segment(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or_default().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        || stem
            .strip_prefix("LPT")
            .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

fn portable_relative_file_path(path: &str) -> bool {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.chars().any(|c| c.is_control())
    {
        return false;
    }
    path.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !reserved_windows_segment(segment)
            && !segment
                .chars()
                .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
            && !segment.ends_with(' ')
            && !segment.ends_with('.')
    })
}

fn expected_plan_command_id(plan: &LocalExecutionPlan) -> String {
    let digest = hash(&(
        plan.task_id.as_str(),
        plan.task_revision,
        plan.target_hash.as_str(),
    ));
    format!("EXEC_{}", &digest[..20])
}

fn validate_plan_binding(plan: &LocalExecutionPlan, manifest: &StagingManifest) -> Result<(), String> {
    if plan.schema_version != LOCAL_EXECUTOR_SCHEMA_VERSION {
        return Err("INVALID_STAGING_PLAN_SCHEMA".into());
    }
    if plan.intent != "DOWNLOAD_TO_STAGING_ONLY"
        || !safe_command_id(&plan.command_id)
        || plan.command_id != expected_plan_command_id(plan)
        || plan.task_id.trim().is_empty()
        || plan.work_id.trim().is_empty()
        || plan.task_revision == 0
        || !lower_hex_sha256(&plan.target_hash)
        || !matches!(plan.backend.as_str(), "JM" | "PICA")
        || plan.source_work_id.trim().is_empty()
        || plan.staging_subdir != format!("commands/{}", plan.command_id)
    {
        return Err("INVALID_STAGING_PLAN_BINDING".into());
    }
    if plan.execution_supported
        || plan.promotion_authorized
        || plan.replacement_authorized
        || plan.physical_delete_authorized
    {
        return Err("UNSAFE_STAGING_PLAN_CAPABILITIES".into());
    }
    if manifest.command_id != plan.command_id
        || manifest.task_id != plan.task_id
        || manifest.work_id != plan.work_id
        || manifest.task_revision != plan.task_revision
        || manifest.target_hash != plan.target_hash
        || manifest.backend != plan.backend
        || manifest.source_work_id != plan.source_work_id
        || manifest.staging_subdir != plan.staging_subdir
    {
        return Err("STAGING_MANIFEST_BINDING_MISMATCH".into());
    }
    Ok(())
}

pub fn validate(
    plan: &LocalExecutionPlan,
    manifest: &StagingManifest,
) -> Result<ValidatedStagingManifest, String> {
    if manifest.schema_version != STAGING_MANIFEST_SCHEMA_VERSION {
        return Err("INVALID_STAGING_MANIFEST_SCHEMA".into());
    }
    validate_plan_binding(plan, manifest)?;

    if !manifest.source_enumeration_complete {
        return Err("STAGING_SOURCE_ENUMERATION_INCOMPLETE".into());
    }
    if !manifest.all_scheduled_downloads_joined {
        return Err("STAGING_DOWNLOADS_NOT_JOINED".into());
    }
    if !manifest.downloader_reported_full_completion {
        return Err("STAGING_DOWNLOADER_NOT_COMPLETE".into());
    }
    if manifest.expected_content_units == 0
        || manifest.completed_content_units != manifest.expected_content_units
        || manifest.failed_content_units != 0
    {
        return Err("STAGING_CONTENT_UNITS_INCOMPLETE".into());
    }
    if manifest.artifacts.is_empty() {
        return Err("STAGING_MANIFEST_EMPTY".into());
    }

    let mut previous: Option<&str> = None;
    let mut casefolded = BTreeSet::new();
    let mut total_bytes = 0u64;
    for artifact in &manifest.artifacts {
        if !portable_relative_file_path(&artifact.relative_path) {
            return Err("INVALID_STAGING_ARTIFACT_PATH".into());
        }
        if artifact.size_bytes == 0 || !lower_hex_sha256(&artifact.sha256) {
            return Err("INVALID_STAGING_ARTIFACT_METADATA".into());
        }
        if previous.is_some_and(|value| value >= artifact.relative_path.as_str()) {
            return Err("STAGING_ARTIFACTS_NOT_STRICTLY_SORTED".into());
        }
        previous = Some(&artifact.relative_path);
        if !casefolded.insert(artifact.relative_path.to_lowercase()) {
            return Err("STAGING_ARTIFACT_PATH_COLLISION".into());
        }
        total_bytes = total_bytes
            .checked_add(artifact.size_bytes)
            .ok_or("STAGING_TOTAL_SIZE_OVERFLOW")?;
    }

    let manifest_hash = hash(manifest);
    let file_count = u64::try_from(manifest.artifacts.len())
        .map_err(|_| "STAGING_FILE_COUNT_OVERFLOW")?;
    let completion_evidence = CompletionEvidence {
        downloader_reported_full_completion: true,
        artifact_manifest_hash: manifest_hash.clone(),
        file_count,
    };

    Ok(ValidatedStagingManifest {
        schema_version: STAGING_MANIFEST_SCHEMA_VERSION,
        command_id: manifest.command_id.clone(),
        task_id: manifest.task_id.clone(),
        work_id: manifest.work_id.clone(),
        task_revision: manifest.task_revision,
        target_hash: manifest.target_hash.clone(),
        manifest_hash,
        file_count,
        total_bytes,
        completion_evidence,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

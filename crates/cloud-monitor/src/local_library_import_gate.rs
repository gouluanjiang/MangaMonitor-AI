//! Public V1.3 capability boundary for local library import.
//!
//! The implementation module is crate-private. External callers can only reach
//! this gate, which limits V1.3 to add-only `download` tasks and permits the
//! mutating operation only on Windows. Upgrade/replacement remains an A7 concern.
//! Mutating materialization also requires an independently versioned, repository-
//! tracked local policy that is default-closed and intentionally separate from
//! cloud `production_enabled`.

use crate::{
    assistant_task_gate::GateLedger,
    executor_handoff::ExecutorCommand,
    local_execution_orchestrator::LocalExecutionReport,
    monitor::State,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub use crate::local_library_import::{
    LocalLibraryImportPlan, LocalLibraryImportReceipt, LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
    LOCAL_LIBRARY_SIDECAR,
};

pub const LOCAL_MATERIALIZATION_POLICY_FILE: &str = "local-materialization-policy.json";
pub const LOCAL_MATERIALIZATION_POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalMaterializationPolicyFile {
    schema_version: u32,
    materialization_enabled: bool,
    command_id: Option<String>,
    task_id: Option<String>,
    task_revision: Option<u64>,
    target_hash: Option<String>,
    manifest_hash: Option<String>,
}

/// Opaque runtime authority for one exact mutating local-library materialization.
///
/// Callers cannot construct this value directly. It is loaded from the fixed
/// repository-level policy file adjacent to the supplied state directory. When
/// enabled it is bound to the exact command/task revision/target hash and accepted
/// staging manifest that were separately approved for materialization.
#[derive(Debug)]
pub struct LocalMaterializationAuthority {
    materialization_enabled: bool,
    command_id: Option<String>,
    task_id: Option<String>,
    task_revision: Option<u64>,
    target_hash: Option<String>,
    manifest_hash: Option<String>,
}

fn parse_materialization_policy(bytes: &[u8]) -> Result<LocalMaterializationAuthority, String> {
    let policy: LocalMaterializationPolicyFile =
        serde_json::from_slice(bytes).map_err(|_| "LOCAL_MATERIALIZATION_POLICY_JSON")?;
    if policy.schema_version != LOCAL_MATERIALIZATION_POLICY_SCHEMA_VERSION {
        return Err("LOCAL_MATERIALIZATION_POLICY_SCHEMA".into());
    }

    let binding_present = policy.command_id.is_some()
        || policy.task_id.is_some()
        || policy.task_revision.is_some()
        || policy.target_hash.is_some()
        || policy.manifest_hash.is_some();
    if !policy.materialization_enabled && binding_present {
        return Err("LOCAL_MATERIALIZATION_DISABLED_BINDING_PRESENT".into());
    }
    if policy.materialization_enabled {
        for value in [
            policy.command_id.as_deref(),
            policy.task_id.as_deref(),
            policy.target_hash.as_deref(),
            policy.manifest_hash.as_deref(),
        ] {
            if value.is_none_or(|value| value.trim().is_empty()) {
                return Err("LOCAL_MATERIALIZATION_BINDING_REQUIRED".into());
            }
        }
        if policy.task_revision.is_none() {
            return Err("LOCAL_MATERIALIZATION_BINDING_REQUIRED".into());
        }
    }

    Ok(LocalMaterializationAuthority {
        materialization_enabled: policy.materialization_enabled,
        command_id: policy.command_id,
        task_id: policy.task_id,
        task_revision: policy.task_revision,
        target_hash: policy.target_hash,
        manifest_hash: policy.manifest_hash,
    })
}

fn materialization_policy_path(state_dir: &Path) -> Result<PathBuf, String> {
    let root = state_dir
        .parent()
        .ok_or("LOCAL_MATERIALIZATION_POLICY_ROOT_REQUIRED")?;
    Ok(root.join(LOCAL_MATERIALIZATION_POLICY_FILE))
}

/// Load the repository-tracked local materialization authority associated with
/// `state_dir`. The policy path is fixed to the state directory's parent so a
/// mutating caller cannot substitute an arbitrary CLI policy path.
pub fn load_materialization_authority(
    state_dir: &Path,
) -> Result<LocalMaterializationAuthority, String> {
    let path = materialization_policy_path(state_dir)?;
    let bytes = fs::read(path).map_err(|_| "LOCAL_MATERIALIZATION_POLICY_READ")?;
    parse_materialization_policy(&bytes)
}

fn require_materialization_values(
    authority: &LocalMaterializationAuthority,
    command_id: &str,
    task_id: &str,
    task_revision: u64,
    target_hash: &str,
    manifest_hash: &str,
) -> Result<(), String> {
    if !authority.materialization_enabled {
        return Err("LOCAL_MATERIALIZATION_DISABLED".into());
    }
    if authority.command_id.as_deref() != Some(command_id)
        || authority.task_id.as_deref() != Some(task_id)
        || authority.task_revision != Some(task_revision)
        || authority.target_hash.as_deref() != Some(target_hash)
        || authority.manifest_hash.as_deref() != Some(manifest_hash)
    {
        return Err("LOCAL_MATERIALIZATION_BINDING_MISMATCH".into());
    }
    Ok(())
}

fn require_materialization_authority(
    authority: &LocalMaterializationAuthority,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
) -> Result<(), String> {
    require_materialization_values(
        authority,
        &command.command_id,
        &command.task_id,
        command.task_revision,
        &command.target_hash,
        &report.filesystem_verification.manifest_hash,
    )
}

fn require_add_only_values(
    command_action: &str,
    current_task_action: &str,
    old_local_item_ids: &[String],
) -> Result<(), String> {
    if command_action != "download"
        || current_task_action != "download"
        || !old_local_item_ids.is_empty()
    {
        return Err("LOCAL_LIBRARY_V1_3_DOWNLOAD_ONLY".into());
    }
    Ok(())
}

fn require_add_only_download(state: &State, command: &ExecutorCommand) -> Result<(), String> {
    let current_task = state
        .pending
        .get(&command.work_id)
        .ok_or("LOCAL_LIBRARY_V1_3_CURRENT_TASK_REQUIRED")?;
    require_add_only_values(
        &command.action,
        &current_task.action,
        &current_task.old_local_item_ids,
    )
}

/// Read-only V1.3 import planning. This can run on CI/non-Windows platforms but
/// still rejects upgrade/replacement tasks before consulting the filesystem.
pub fn plan(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    staging_root: &Path,
    library_root: &Path,
) -> Result<LocalLibraryImportPlan, String> {
    require_add_only_download(state, command)?;
    crate::local_library_import::plan(
        state,
        ledger,
        command,
        report,
        staging_root,
        library_root,
    )
}

/// Mutating V1.3 import. Only Windows is exposed for product use, and the
/// independent local materialization policy must be enabled for this exact
/// command/task revision/target hash and accepted staging manifest first. The
/// internal implementation atomically reserves a fresh destination directory,
/// uses create-new writes only, preserves partial output, and rechecks current
/// task / approval before each artifact and immediately before the completion
/// sidecar.
pub fn execute_add_only<Reload>(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    staging_root: &Path,
    library_root: &Path,
    authority: &LocalMaterializationAuthority,
    confirmation_command_id: &str,
    reload: Reload,
) -> Result<LocalLibraryImportReceipt, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    require_add_only_download(state, command)?;
    require_materialization_authority(authority, command, report)?;
    if !cfg!(windows) {
        return Err("LOCAL_LIBRARY_V1_3_WINDOWS_ONLY".into());
    }
    crate::local_library_import::execute_add_only(
        state,
        ledger,
        command,
        report,
        staging_root,
        library_root,
        confirmation_command_id,
        reload,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_3_capability_rejects_upgrade_and_replacement_inputs() {
        let none: Vec<String> = Vec::new();
        assert!(require_add_only_values("download", "download", &none).is_ok());
        assert_eq!(
            require_add_only_values("upgrade", "upgrade", &["LOCAL_1".into()]).unwrap_err(),
            "LOCAL_LIBRARY_V1_3_DOWNLOAD_ONLY"
        );
        assert_eq!(
            require_add_only_values("download", "download", &["LOCAL_1".into()]).unwrap_err(),
            "LOCAL_LIBRARY_V1_3_DOWNLOAD_ONLY"
        );
        assert_eq!(
            require_add_only_values("download", "upgrade", &none).unwrap_err(),
            "LOCAL_LIBRARY_V1_3_DOWNLOAD_ONLY"
        );
    }

    #[test]
    fn materialization_policy_is_default_closed_strict_and_exactly_bound() {
        let disabled = parse_materialization_policy(
            br#"{"schema_version":1,"materialization_enabled":false,"command_id":null,"task_id":null,"task_revision":null,"target_hash":null,"manifest_hash":null}"#,
        )
        .expect("valid disabled policy");
        assert_eq!(
            require_materialization_values(
                &disabled,
                "EXEC_1",
                "TASK_1",
                3,
                "HASH_1",
                "MANIFEST_1",
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_DISABLED"
        );

        let enabled = parse_materialization_policy(
            br#"{"schema_version":1,"materialization_enabled":true,"command_id":"EXEC_1","task_id":"TASK_1","task_revision":3,"target_hash":"HASH_1","manifest_hash":"MANIFEST_1"}"#,
        )
        .expect("valid enabled policy");
        assert!(require_materialization_values(
            &enabled,
            "EXEC_1",
            "TASK_1",
            3,
            "HASH_1",
            "MANIFEST_1",
        )
        .is_ok());
        assert_eq!(
            require_materialization_values(
                &enabled,
                "EXEC_2",
                "TASK_1",
                3,
                "HASH_1",
                "MANIFEST_1",
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_BINDING_MISMATCH"
        );
        assert_eq!(
            require_materialization_values(
                &enabled,
                "EXEC_1",
                "TASK_1",
                3,
                "HASH_1",
                "MANIFEST_2",
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_BINDING_MISMATCH"
        );

        assert_eq!(
            parse_materialization_policy(
                br#"{"schema_version":2,"materialization_enabled":false,"command_id":null,"task_id":null,"task_revision":null,"target_hash":null,"manifest_hash":null}"#,
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_POLICY_SCHEMA"
        );
        assert_eq!(
            parse_materialization_policy(
                br#"{"schema_version":1,"materialization_enabled":true,"command_id":"EXEC_1","task_id":"TASK_1","task_revision":3,"target_hash":"HASH_1","manifest_hash":null}"#,
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_BINDING_REQUIRED"
        );
        assert_eq!(
            parse_materialization_policy(
                br#"{"schema_version":1,"materialization_enabled":false,"command_id":"EXEC_1","task_id":null,"task_revision":null,"target_hash":null,"manifest_hash":null}"#,
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_DISABLED_BINDING_PRESENT"
        );
        assert_eq!(
            parse_materialization_policy(
                br#"{"schema_version":1,"materialization_enabled":false,"command_id":null,"task_id":null,"task_revision":null,"target_hash":null,"manifest_hash":null,"extra":1}"#,
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_POLICY_JSON"
        );
    }

    #[test]
    fn repository_materialization_policy_remains_disabled() {
        let state_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("monitor-state");
        let authority = load_materialization_authority(&state_dir)
            .expect("repository materialization policy parses");
        assert_eq!(
            require_materialization_values(
                &authority,
                "EXEC_1",
                "TASK_1",
                3,
                "HASH_1",
                "MANIFEST_1",
            )
            .unwrap_err(),
            "LOCAL_MATERIALIZATION_DISABLED"
        );
    }
}

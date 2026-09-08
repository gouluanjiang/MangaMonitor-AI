//! V1.8 exact local task completion after verified V1.7 inventory apply.
//!
//! This is the narrow terminal V1 add-only state mutation. It never downloads,
//! imports, mutates inventory, replaces/deletes content, publishes cloud state,
//! or enables production. The only durable write is `pending.json`, changing one
//! exact current `download` task revision from `pending` to `completed` after a
//! fresh real-library rescan and an independent reconstruction of the exact V1.7
//! inventory post-state.

use crate::{
    local_inventory_apply::{InventoryApplyReceipt, INVENTORY_APPLY_RECEIPT_SCHEMA_VERSION},
    local_inventory_apply_authorization::{self, InventoryApplyAuthorization},
    local_inventory_rescan::{self, LocalInventoryRescanReport},
    local_inventory_update_candidate::InventoryUpdateCandidate,
    local_library_import_gate::LocalLibraryImportReceipt,
    monitor::{hash, State},
    persistence,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const TASK_COMPLETION_RECEIPT_SCHEMA_VERSION: u64 = 1;
const PENDING_SCHEMA_VERSION: u64 = 4;
const WRITE_TARGET: &str = "pending.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCompletionReceipt {
    pub schema_version: u64,
    pub completion_id: String,
    pub evidence_hash: String,
    pub outcome: String,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub binding_authority_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub authorization_id: String,
    pub authorization_hash: String,
    pub apply_receipt_hash: String,
    pub inventory_post_hash: String,
    pub inventory_post_count: u64,
    pub inventory_observation_hash: String,
    pub pending_pre_hash: String,
    pub pending_post_hash: String,
    pub filesystem_verified: bool,
    pub inventory_state_verified: bool,
    pub task_completion_authorized: bool,
    pub task_completion_performed: bool,
    pub task_completion_verified: bool,
    pub inventory_mutation_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

fn pending_document(state: &State) -> Value {
    json!({
        "schema_version": PENDING_SCHEMA_VERSION,
        "tasks": state.pending.values().collect::<Vec<_>>(),
    })
}

fn validate_task_binding(state: &State, candidate: &InventoryUpdateCandidate) -> Result<String, String> {
    let task = state
        .pending
        .get(&candidate.work_id)
        .ok_or("TASK_V1_8_CURRENT_TASK_REQUIRED")?;
    if task.task_id != candidate.task_id
        || task.task_revision != candidate.task_revision
        || task.action != "download"
        || !matches!(task.status.as_str(), "pending" | "completed")
        || !task.old_local_item_ids.is_empty()
        || hash(&task.target) != candidate.target_hash
        || task.target.source_key != format!("{}:{}", candidate.source, candidate.source_work_id)
        || task.binding_authority_hash.trim().is_empty()
    {
        return Err("TASK_V1_8_CURRENT_TASK_BINDING_MISMATCH".into());
    }
    let entry = state
        .catalog
        .get(&task.target.source_key)
        .ok_or("TASK_V1_8_CURRENT_SOURCE_ENTRY_REQUIRED")?;
    if entry.work_id.as_deref() != Some(candidate.work_id.as_str())
        || entry.identity_evidence["binding_authority_hash"].as_str()
            != Some(task.binding_authority_hash.as_str())
    {
        return Err("TASK_V1_8_A03_AUTHORITY_BINDING_MISMATCH".into());
    }
    Ok(task.binding_authority_hash.clone())
}

fn validate_apply_receipt(
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    receipt: &InventoryApplyReceipt,
) -> Result<(), String> {
    let expected_performed = match receipt.outcome.as_str() {
        "APPLIED_EXACT_POST" => true,
        "ALREADY_APPLIED_EXACT_POST" => false,
        _ => return Err("TASK_V1_8_APPLY_RECEIPT_OUTCOME_INVALID".into()),
    };
    if receipt.schema_version != INVENTORY_APPLY_RECEIPT_SCHEMA_VERSION
        || receipt.authorization_id != authorization.authorization_id
        || receipt.authorization_hash != authorization.authorization_hash
        || receipt.candidate_id != candidate.candidate_id
        || receipt.candidate_hash != candidate.candidate_hash
        || receipt.command_id != candidate.command_id
        || receipt.task_id != candidate.task_id
        || receipt.work_id != candidate.work_id
        || receipt.task_revision != candidate.task_revision
        || receipt.target_hash != candidate.target_hash
        || receipt.source != candidate.source
        || receipt.source_work_id != candidate.source_work_id
        || receipt.inventory_pre_hash != authorization.inventory_snapshot_hash
        || receipt.inventory_pre_count != authorization.current_total_work_ids
        || receipt.inventory_post_count != authorization.proposed_total_work_ids
        || !receipt.inventory_mutation_authorized
        || receipt.inventory_mutation_performed != expected_performed
        || !receipt.inventory_state_verified
        || receipt.task_completion_authorized
        || receipt.promotion_authorized
        || receipt.replacement_authorized
        || receipt.physical_delete_authorized
        || receipt.production_enablement_authorized
    {
        return Err("TASK_V1_8_APPLY_RECEIPT_BINDING_INVALID".into());
    }
    Ok(())
}

fn verify_exact_inventory_post(
    state: &State,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<String, String> {
    let binding_authority_hash = validate_task_binding(state, candidate)?;
    let works = state.inventory["works"]
        .as_array()
        .ok_or("TASK_V1_8_INVENTORY_WORKS_INVALID")?;
    if state.inventory["schema_version"].as_u64() != Some(authorization.inventory_schema_version)
        || state.inventory["rules_version"].as_str()
            != Some(authorization.inventory_rules_version.as_str())
        || state.inventory["total_work_ids"].as_u64() != Some(authorization.proposed_total_work_ids)
        || u64::try_from(works.len()).ok() != Some(authorization.proposed_total_work_ids)
        || works.last() != Some(&candidate.proposed_work)
    {
        return Err("TASK_V1_8_INVENTORY_NOT_EXACT_V1_7_POST".into());
    }

    let mut pre = state.inventory.clone();
    pre["works"]
        .as_array_mut()
        .ok_or("TASK_V1_8_INVENTORY_WORKS_INVALID")?
        .pop();
    pre["total_work_ids"] = json!(authorization.current_total_work_ids);
    if hash(&pre) != authorization.inventory_snapshot_hash {
        return Err("TASK_V1_8_INVENTORY_PRE_RECONSTRUCTION_MISMATCH".into());
    }

    // Reconstruct the exact pre-V1.7 authorization state. V1.8 may be retried
    // after the task status was already written as completed, so status alone is
    // normalized back to pending for this read-only V1.6 reauthorization.
    let mut reconstructed = state.clone();
    reconstructed.inventory = pre.clone();
    reconstructed
        .pending
        .get_mut(&candidate.work_id)
        .ok_or("TASK_V1_8_CURRENT_TASK_REQUIRED")?
        .status = "pending".into();
    let fresh = local_inventory_apply_authorization::authorize(&reconstructed, report, candidate)
        .map_err(|_| "TASK_V1_8_V1_6_REAUTHORIZATION_FAILED".to_string())?;
    if &fresh != authorization {
        return Err("TASK_V1_8_V1_6_AUTHORIZATION_NOT_EXACT".into());
    }

    let mut expected_post = pre;
    expected_post["works"]
        .as_array_mut()
        .ok_or("TASK_V1_8_INVENTORY_WORKS_INVALID")?
        .push(candidate.proposed_work.clone());
    expected_post["total_work_ids"] = json!(authorization.proposed_total_work_ids);
    if expected_post != state.inventory {
        return Err("TASK_V1_8_INVENTORY_POST_REBUILD_MISMATCH".into());
    }
    if binding_authority_hash.trim().is_empty() {
        return Err("TASK_V1_8_A03_AUTHORITY_REQUIRED".into());
    }
    Ok(hash(&expected_post))
}

fn validate_evidence_chain(
    state: &State,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    apply_receipt: &InventoryApplyReceipt,
) -> Result<(String, String), String> {
    validate_apply_receipt(candidate, authorization, apply_receipt)?;
    let inventory_post_hash = verify_exact_inventory_post(state, report, candidate, authorization)?;
    if apply_receipt.inventory_post_hash != inventory_post_hash
        || candidate.rescan_id != report.rescan_id
        || candidate.inventory_observation_hash != report.inventory_observation_hash
        || candidate.command_id != report.command_id
        || candidate.task_id != report.task_id
        || candidate.work_id != report.work_id
        || candidate.task_revision != report.task_revision
        || candidate.target_hash != report.target_hash
        || candidate.source != report.source
        || candidate.source_work_id != report.source_work_id
        || !report.sidecar_verified
        || !report.manifest_verified
        || !report.filesystem_verified
        || !report.inventory_observation_complete
        || report.inventory_mutation_authorized
        || report.task_completion_authorized
        || report.promotion_authorized
        || report.replacement_authorized
        || report.physical_delete_authorized
        || report.production_enablement_authorized
    {
        return Err("TASK_V1_8_EVIDENCE_CHAIN_MISMATCH".into());
    }
    let authority = state.pending[&candidate.work_id].binding_authority_hash.clone();
    Ok((inventory_post_hash, authority))
}

fn completion_identity(
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    apply_receipt: &InventoryApplyReceipt,
    binding_authority_hash: &str,
) -> (String, String) {
    let evidence_hash = hash(&(
        report.inventory_observation_hash.as_str(),
        candidate.candidate_hash.as_str(),
        authorization.authorization_hash.as_str(),
        hash(apply_receipt),
        binding_authority_hash,
        candidate.task_id.as_str(),
        candidate.task_revision,
        candidate.target_hash.as_str(),
    ));
    let completion_id = format!("TASK_COMPLETION_{}", &evidence_hash[..20]);
    (completion_id, evidence_hash)
}

fn temp_path(state_dir: &Path, completion_id: &str) -> PathBuf {
    state_dir.join(format!(".pending.v1-8-{completion_id}.tmp"))
}

fn open_rw(path: &Path, error: &'static str) -> Result<std::fs::File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| error.into())
}

fn prepare_synced_replacement(path: &Path, expected: &[u8]) -> Result<(), String> {
    if path.exists() {
        let existing = fs::read(path).map_err(|_| "TASK_V1_8_TEMP_READ_FAILED")?;
        if existing != expected {
            return Err("TASK_V1_8_TEMP_CONFLICT".into());
        }
        open_rw(path, "TASK_V1_8_TEMP_OPEN_FAILED")?
            .sync_all()
            .map_err(|_| "TASK_V1_8_TEMP_SYNC_FAILED")?;
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "TASK_V1_8_TEMP_CREATE_FAILED")?;
    file.write_all(expected)
        .map_err(|_| "TASK_V1_8_TEMP_WRITE_FAILED")?;
    file.sync_all()
        .map_err(|_| "TASK_V1_8_TEMP_SYNC_FAILED".to_string())
}

#[cfg(windows)]
fn atomic_replace_file(target: &Path, replacement: &Path) -> Result<(), String> {
    use std::{os::windows::ffi::OsStrExt, ptr};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
    }

    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement_wide: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let ok = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            replacement_wide.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err("TASK_V1_8_ATOMIC_REPLACE_FAILED".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(target: &Path, replacement: &Path) -> Result<(), String> {
    fs::rename(replacement, target).map_err(|_| "TASK_V1_8_ATOMIC_REPLACE_FAILED".into())
}

fn sync_target(path: &Path) -> Result<(), String> {
    open_rw(path, "TASK_V1_8_TARGET_OPEN_FAILED")?
        .sync_all()
        .map_err(|_| "TASK_V1_8_TARGET_SYNC_FAILED".into())
}

fn reread_pending(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "TASK_V1_8_PENDING_REREAD_FAILED")?)
        .map_err(|_| "TASK_V1_8_PENDING_REREAD_JSON_INVALID".into())
}

#[expect(clippy::too_many_arguments, reason = "V1.8 receipt construction keeps every audited evidence binding explicit so completion authority is never inferred")]
fn make_receipt(
    outcome: &str,
    performed: bool,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    apply_receipt: &InventoryApplyReceipt,
    binding_authority_hash: &str,
    inventory_post_hash: &str,
    pending_pre_hash: &str,
    pending_post_hash: &str,
) -> TaskCompletionReceipt {
    let (completion_id, evidence_hash) = completion_identity(
        report,
        candidate,
        authorization,
        apply_receipt,
        binding_authority_hash,
    );
    TaskCompletionReceipt {
        schema_version: TASK_COMPLETION_RECEIPT_SCHEMA_VERSION,
        completion_id,
        evidence_hash,
        outcome: outcome.into(),
        command_id: candidate.command_id.clone(),
        task_id: candidate.task_id.clone(),
        work_id: candidate.work_id.clone(),
        task_revision: candidate.task_revision,
        target_hash: candidate.target_hash.clone(),
        binding_authority_hash: binding_authority_hash.into(),
        source: candidate.source.clone(),
        source_work_id: candidate.source_work_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        candidate_hash: candidate.candidate_hash.clone(),
        authorization_id: authorization.authorization_id.clone(),
        authorization_hash: authorization.authorization_hash.clone(),
        apply_receipt_hash: hash(apply_receipt),
        inventory_post_hash: inventory_post_hash.into(),
        inventory_post_count: authorization.proposed_total_work_ids,
        inventory_observation_hash: report.inventory_observation_hash.clone(),
        pending_pre_hash: pending_pre_hash.into(),
        pending_post_hash: pending_post_hash.into(),
        filesystem_verified: true,
        inventory_state_verified: true,
        task_completion_authorized: true,
        task_completion_performed: performed,
        task_completion_verified: true,
        inventory_mutation_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    }
}

/// Complete one exact V1 add-only task after re-verifying the real imported tree
/// and the exact V1.7 inventory post-state.
pub fn complete(
    state_dir: &Path,
    library_root: &Path,
    import_receipt: &LocalLibraryImportReceipt,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    apply_receipt: &InventoryApplyReceipt,
) -> Result<TaskCompletionReceipt, String> {
    let report = local_inventory_rescan::rescan(library_root, import_receipt)?;
    let current = persistence::load(state_dir)?;
    let (inventory_post_hash, binding_authority_hash) =
        validate_evidence_chain(&current, &report, candidate, authorization, apply_receipt)?;

    let current_task = current
        .pending
        .get(&candidate.work_id)
        .ok_or("TASK_V1_8_CURRENT_TASK_REQUIRED")?;
    let current_doc = pending_document(&current);
    let mut hypothetical_pre = current.clone();
    hypothetical_pre
        .pending
        .get_mut(&candidate.work_id)
        .ok_or("TASK_V1_8_CURRENT_TASK_REQUIRED")?
        .status = "pending".into();
    let pending_pre_hash = hash(&pending_document(&hypothetical_pre));

    if current_task.status == "completed" {
        let second_report = local_inventory_rescan::rescan(library_root, import_receipt)?;
        if second_report != report {
            return Err("TASK_V1_8_RETRY_FILESYSTEM_OBSERVATION_CHANGED".into());
        }
        let reloaded = persistence::load(state_dir)?;
        let (retry_inventory_hash, retry_authority) = validate_evidence_chain(
            &reloaded,
            &second_report,
            candidate,
            authorization,
            apply_receipt,
        )?;
        if retry_inventory_hash != inventory_post_hash
            || retry_authority != binding_authority_hash
            || reloaded.pending[&candidate.work_id].status != "completed"
        {
            return Err("TASK_V1_8_RETRY_STATE_CHANGED".into());
        }
        let pending_post_hash = hash(&pending_document(&reloaded));
        return Ok(make_receipt(
            "ALREADY_COMPLETED_EXACT_REVISION",
            false,
            &second_report,
            candidate,
            authorization,
            apply_receipt,
            &binding_authority_hash,
            &inventory_post_hash,
            &pending_pre_hash,
            &pending_post_hash,
        ));
    }
    if current_task.status != "pending" {
        return Err("TASK_V1_8_TASK_NOT_PENDING_OR_COMPLETED".into());
    }

    let mut proposed = current.clone();
    proposed
        .pending
        .get_mut(&candidate.work_id)
        .ok_or("TASK_V1_8_CURRENT_TASK_REQUIRED")?
        .status = "completed".into();
    let proposed_doc = pending_document(&proposed);
    let pending_post_hash = hash(&proposed_doc);
    let bytes = serde_json::to_vec_pretty(&proposed_doc)
        .map_err(|_| "TASK_V1_8_PENDING_SERIALIZE_FAILED")?;
    let (completion_id, _) = completion_identity(
        &report,
        candidate,
        authorization,
        apply_receipt,
        &binding_authority_hash,
    );
    let replacement = temp_path(state_dir, &completion_id);
    prepare_synced_replacement(&replacement, &bytes)?;

    // Close both state and filesystem race windows immediately before the only
    // authorized write. No inventory or media path is mutated here.
    let second_report = local_inventory_rescan::rescan(library_root, import_receipt)?;
    if second_report != report {
        return Err("TASK_V1_8_PRE_WRITE_FILESYSTEM_OBSERVATION_CHANGED".into());
    }
    let pre_write = persistence::load(state_dir)?;
    if hash(&pending_document(&pre_write)) != hash(&current_doc) {
        return Err("TASK_V1_8_PRE_WRITE_PENDING_CHANGED".into());
    }
    let (pre_write_inventory_hash, pre_write_authority) = validate_evidence_chain(
        &pre_write,
        &second_report,
        candidate,
        authorization,
        apply_receipt,
    )?;
    if pre_write_inventory_hash != inventory_post_hash
        || pre_write_authority != binding_authority_hash
        || pre_write.pending[&candidate.work_id].status != "pending"
    {
        return Err("TASK_V1_8_PRE_WRITE_STATE_CHANGED".into());
    }

    let target = state_dir.join(WRITE_TARGET);
    atomic_replace_file(&target, &replacement)?;
    sync_target(&target)?;
    let reread = reread_pending(&target)?;
    if reread != proposed_doc || hash(&reread) != pending_post_hash {
        return Err("TASK_V1_8_POST_WRITE_PENDING_MISMATCH".into());
    }
    let reloaded = persistence::load(state_dir)?;
    if hash(&reloaded.inventory) != inventory_post_hash
        || reloaded.pending[&candidate.work_id].task_id != candidate.task_id
        || reloaded.pending[&candidate.work_id].task_revision != candidate.task_revision
        || reloaded.pending[&candidate.work_id].status != "completed"
        || reloaded.pending[&candidate.work_id].binding_authority_hash != binding_authority_hash
        || pending_document(&reloaded) != proposed_doc
    {
        return Err("TASK_V1_8_POST_WRITE_STATE_RELOAD_MISMATCH".into());
    }

    Ok(make_receipt(
        "COMPLETED_EXACT_REVISION",
        true,
        &second_report,
        candidate,
        authorization,
        apply_receipt,
        &binding_authority_hash,
        &inventory_post_hash,
        &pending_pre_hash,
        &pending_post_hash,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::{Decisions, Entry, Scan, Target, Task};
    use serde_json::json;
    use state_model::{Record, Version};
    use std::{collections::BTreeMap, time::{SystemTime, UNIX_EPOCH}};

    fn task(status: &str) -> Task {
        Task {
            task_id: "TASK_fixture".into(),
            work_id: "WORK_fixture".into(),
            first_seen: "fixed".into(),
            task_revision: 3,
            target: Target {
                source_key: "jm:123".into(),
                author: "Writer".into(),
                title: "Work 7".into(),
                version: Version::default(),
                coverage: Value::Null,
            },
            action: "download".into(),
            status: status.into(),
            old_local_item_ids: vec![],
            binding_authority_hash: "authority-fixture".into(),
        }
    }

    fn state(status: &str) -> State {
        let task = task(status);
        let record = Record {
            source: "jm".into(),
            source_work_id: "123".into(),
            author: vec!["Writer".into()],
            raw_title: "Work 7".into(),
            metadata: json!({}),
            fingerprint: "f".into(),
            first_seen: "fixed".into(),
            last_seen: "fixed".into(),
            last_checked: "fixed".into(),
            processing_result: "PENDING".into(),
        };
        State {
            authors: json!({"authors":[]}),
            inventory: json!({"schema_version":8,"rules_version":"r","total_work_ids":0,"works":[]}),
            catalog: BTreeMap::from([("jm:123".into(), Entry {
                record,
                author_evidence: Default::default(),
                search_fingerprint: "f".into(),
                detail_fingerprint: "d".into(),
                analysis_context: "ctx".into(),
                work_id: Some("WORK_fixture".into()),
                analysis_count: 1,
                unavailable_streak: 0,
                active: true,
                last_unavailable_check: None,
                search_queries: Default::default(),
                matcher_version: String::new(),
                identity_evidence: json!({"binding_authority_hash":"authority-fixture"}),
                identity_provenance: Value::Null,
            })]),
            pending: BTreeMap::from([("WORK_fixture".into(), task)]),
            review: BTreeMap::new(),
            cleanup_review: json!([]),
            decisions: Decisions::default(),
            scan: Scan::default(),
        }
    }

    #[test]
    fn pending_document_preserves_exact_completed_task_for_audit() {
        let pending = pending_document(&state("pending"));
        let completed = pending_document(&state("completed"));
        assert_eq!(pending["schema_version"], PENDING_SCHEMA_VERSION);
        assert_eq!(pending["tasks"][0]["status"], "pending");
        assert_eq!(completed["tasks"][0]["status"], "completed");
        assert_ne!(hash(&pending), hash(&completed));
        assert_eq!(completed["tasks"][0]["task_revision"], 3);
        assert_eq!(completed["tasks"][0]["binding_authority_hash"], "authority-fixture");
    }

    #[test]
    fn task_binding_requires_a03_authority_and_exact_revision() {
        let mut candidate = InventoryUpdateCandidate {
            schema_version: 1,
            candidate_id: "INVENTORY_CANDIDATE_x".into(),
            candidate_hash: "0".repeat(64),
            operation: "ADD_NEW_WORK_ONLY".into(),
            inventory_schema_version: 8,
            inventory_rules_version: "r".into(),
            inventory_snapshot_hash: "1".repeat(64),
            state_context_hash: "2".repeat(64),
            matcher_version: "m".into(),
            current_total_work_ids: 0,
            proposed_total_work_ids: 1,
            rescan_id: "RESCAN_x".into(),
            inventory_observation_hash: "3".repeat(64),
            command_id: "EXEC_x".into(),
            task_id: "TASK_fixture".into(),
            work_id: "WORK_fixture".into(),
            task_revision: 3,
            target_hash: hash(&task("pending").target),
            source: "jm".into(),
            source_work_id: "123".into(),
            proposed_local_item_id: "LOCAL_x".into(),
            source_identity_hash: "4".repeat(64),
            proposed_work: json!({}),
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        };
        assert_eq!(validate_task_binding(&state("pending"), &candidate).unwrap(), "authority-fixture");
        candidate.task_revision = 4;
        assert_eq!(
            validate_task_binding(&state("pending"), &candidate).unwrap_err(),
            "TASK_V1_8_CURRENT_TASK_BINDING_MISMATCH"
        );
    }

    #[test]
    fn atomic_pending_replace_round_trip() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("mangamonitor-v18-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("pending.json");
        let replacement = dir.join("replacement.tmp");
        fs::write(&target, b"pre").unwrap();
        fs::write(&replacement, b"post").unwrap();
        open_rw(&replacement, "TEST_REPLACEMENT_OPEN").unwrap().sync_all().unwrap();
        atomic_replace_file(&target, &replacement).unwrap();
        sync_target(&target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"post");
        assert!(!replacement.exists());
        let _ = fs::remove_dir_all(dir);
    }
}

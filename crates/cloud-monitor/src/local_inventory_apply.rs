//! V1.7 crash-safe local add-only inventory apply.
//!
//! This gate performs exactly one authorized mutation of `inventory_index.json`:
//! append the V1.5 proposed work and increment `total_work_ids` by one. It never
//! completes tasks, publishes state, enables production/materialization, or
//! authorizes replacement/overwrite/delete.

use crate::{
    local_inventory_apply_authorization::{self, InventoryApplyAuthorization},
    local_inventory_rescan::LocalInventoryRescanReport,
    local_inventory_update_candidate::InventoryUpdateCandidate,
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

pub const INVENTORY_APPLY_RECEIPT_SCHEMA_VERSION: u64 = 1;
const AUTH_OPERATION: &str = "AUTHORIZE_ADD_NEW_WORK_APPLY";
const AUTH_WRITE_TARGET: &str = "inventory_index.json";
const AUTH_TRANSITION: &str = "APPEND_ONE_WORK_AND_INCREMENT_TOTAL_ONLY";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryApplyReceipt {
    pub schema_version: u64,
    pub outcome: String,
    pub authorization_id: String,
    pub authorization_hash: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub inventory_pre_hash: String,
    pub inventory_post_hash: String,
    pub inventory_pre_count: u64,
    pub inventory_post_count: u64,
    pub inventory_mutation_authorized: bool,
    pub inventory_mutation_performed: bool,
    pub inventory_state_verified: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

fn validate_binding(
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<(), String> {
    if authorization.schema_version
        != local_inventory_apply_authorization::INVENTORY_APPLY_AUTHORIZATION_SCHEMA_VERSION
        || authorization.operation != AUTH_OPERATION
        || authorization.authorized_write_target != AUTH_WRITE_TARGET
        || authorization.authorized_transition != AUTH_TRANSITION
        || authorization.candidate_id != candidate.candidate_id
        || authorization.candidate_hash != candidate.candidate_hash
        || authorization.inventory_schema_version != candidate.inventory_schema_version
        || authorization.inventory_rules_version != candidate.inventory_rules_version
        || authorization.inventory_snapshot_hash != candidate.inventory_snapshot_hash
        || authorization.state_context_hash != candidate.state_context_hash
        || authorization.matcher_version != candidate.matcher_version
        || authorization.current_total_work_ids != candidate.current_total_work_ids
        || authorization.proposed_total_work_ids != candidate.proposed_total_work_ids
        || authorization.rescan_id != candidate.rescan_id
        || authorization.inventory_observation_hash != candidate.inventory_observation_hash
        || authorization.command_id != candidate.command_id
        || authorization.task_id != candidate.task_id
        || authorization.work_id != candidate.work_id
        || authorization.task_revision != candidate.task_revision
        || authorization.target_hash != candidate.target_hash
        || authorization.source != candidate.source
        || authorization.source_work_id != candidate.source_work_id
        || authorization.proposed_local_item_id != candidate.proposed_local_item_id
        || authorization.source_identity_hash != candidate.source_identity_hash
        || authorization.proposed_work_hash != hash(&candidate.proposed_work)
        || !authorization.inventory_mutation_authorized
        || authorization.task_completion_authorized
        || authorization.promotion_authorized
        || authorization.replacement_authorized
        || authorization.physical_delete_authorized
        || authorization.production_enablement_authorized
    {
        return Err("INVENTORY_V1_7_AUTHORIZATION_BINDING_INVALID".into());
    }
    Ok(())
}

fn expected_post_inventory(
    pre: &Value,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<Value, String> {
    validate_binding(candidate, authorization)?;
    if hash(pre) != authorization.inventory_snapshot_hash
        || pre["schema_version"].as_u64() != Some(authorization.inventory_schema_version)
        || pre["rules_version"].as_str()
            != Some(authorization.inventory_rules_version.as_str())
        || pre["total_work_ids"].as_u64() != Some(authorization.current_total_work_ids)
    {
        return Err("INVENTORY_V1_7_PRE_STATE_MISMATCH".into());
    }
    let works = pre["works"]
        .as_array()
        .ok_or("INVENTORY_V1_7_PRE_WORKS_INVALID")?;
    if u64::try_from(works.len()).ok() != Some(authorization.current_total_work_ids) {
        return Err("INVENTORY_V1_7_PRE_COUNT_MISMATCH".into());
    }
    if works.iter().any(|work| {
        work["work_id"].as_str() == Some(candidate.work_id.as_str())
            || work["local_item_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|id| id.as_str() == Some(candidate.proposed_local_item_id.as_str()))
            || work["source_mappings"][candidate.source.as_str()]
                .as_array()
                .into_iter()
                .flatten()
                .any(|id| id.as_str() == Some(candidate.source_work_id.as_str()))
    }) {
        return Err("INVENTORY_V1_7_PRE_IDENTITY_ALREADY_PRESENT".into());
    }

    let mut post = pre.clone();
    let post_works = post["works"]
        .as_array_mut()
        .ok_or("INVENTORY_V1_7_POST_WORKS_INVALID")?;
    post_works.push(candidate.proposed_work.clone());
    post["total_work_ids"] = json!(authorization.proposed_total_work_ids);
    if u64::try_from(post["works"].as_array().unwrap().len()).ok()
        != Some(authorization.proposed_total_work_ids)
    {
        return Err("INVENTORY_V1_7_POST_COUNT_MISMATCH".into());
    }
    Ok(post)
}

fn reconstruct_pre_from_exact_post(
    current: &Value,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<Value, String> {
    validate_binding(candidate, authorization)?;
    if current["schema_version"].as_u64() != Some(authorization.inventory_schema_version)
        || current["rules_version"].as_str()
            != Some(authorization.inventory_rules_version.as_str())
        || current["total_work_ids"].as_u64() != Some(authorization.proposed_total_work_ids)
    {
        return Err("INVENTORY_V1_7_POST_ENVELOPE_MISMATCH".into());
    }
    let works = current["works"]
        .as_array()
        .ok_or("INVENTORY_V1_7_POST_WORKS_INVALID")?;
    if u64::try_from(works.len()).ok() != Some(authorization.proposed_total_work_ids)
        || works.last() != Some(&candidate.proposed_work)
    {
        return Err("INVENTORY_V1_7_POST_APPEND_MISMATCH".into());
    }

    let mut pre = current.clone();
    pre["works"]
        .as_array_mut()
        .ok_or("INVENTORY_V1_7_POST_WORKS_INVALID")?
        .pop();
    pre["total_work_ids"] = json!(authorization.current_total_work_ids);
    if hash(&pre) != authorization.inventory_snapshot_hash {
        return Err("INVENTORY_V1_7_POST_REVERSE_HASH_MISMATCH".into());
    }
    let rebuilt = expected_post_inventory(&pre, candidate, authorization)?;
    if &rebuilt != current {
        return Err("INVENTORY_V1_7_POST_REBUILD_MISMATCH".into());
    }
    Ok(pre)
}

fn reread_inventory(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "INVENTORY_V1_7_REREAD_FAILED")?)
        .map_err(|_| "INVENTORY_V1_7_REREAD_JSON_INVALID".into())
}

fn temp_path(target: &Path, authorization: &InventoryApplyAuthorization) -> Result<PathBuf, String> {
    let parent = target
        .parent()
        .ok_or("INVENTORY_V1_7_TARGET_PARENT_REQUIRED")?;
    Ok(parent.join(format!(
        ".inventory_index.v1-7-{}.tmp",
        authorization.authorization_id
    )))
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
        let existing = fs::read(path).map_err(|_| "INVENTORY_V1_7_TEMP_READ_FAILED")?;
        if existing != expected {
            return Err("INVENTORY_V1_7_TEMP_CONFLICT".into());
        }
        open_rw(path, "INVENTORY_V1_7_TEMP_OPEN_FAILED")?
            .sync_all()
            .map_err(|_| "INVENTORY_V1_7_TEMP_SYNC_FAILED")?;
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "INVENTORY_V1_7_TEMP_CREATE_FAILED")?;
    file.write_all(expected)
        .map_err(|_| "INVENTORY_V1_7_TEMP_WRITE_FAILED")?;
    file.sync_all()
        .map_err(|_| "INVENTORY_V1_7_TEMP_SYNC_FAILED".to_string())
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
        return Err("INVENTORY_V1_7_ATOMIC_REPLACE_FAILED".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(target: &Path, replacement: &Path) -> Result<(), String> {
    fs::rename(replacement, target).map_err(|_| "INVENTORY_V1_7_ATOMIC_REPLACE_FAILED".into())
}

fn sync_target(path: &Path) -> Result<(), String> {
    open_rw(path, "INVENTORY_V1_7_TARGET_OPEN_FAILED")?
        .sync_all()
        .map_err(|_| "INVENTORY_V1_7_TARGET_SYNC_FAILED".into())
}

fn receipt(
    outcome: &str,
    mutation_performed: bool,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
    post: &Value,
) -> InventoryApplyReceipt {
    InventoryApplyReceipt {
        schema_version: INVENTORY_APPLY_RECEIPT_SCHEMA_VERSION,
        outcome: outcome.into(),
        authorization_id: authorization.authorization_id.clone(),
        authorization_hash: authorization.authorization_hash.clone(),
        candidate_id: candidate.candidate_id.clone(),
        candidate_hash: candidate.candidate_hash.clone(),
        command_id: candidate.command_id.clone(),
        task_id: candidate.task_id.clone(),
        work_id: candidate.work_id.clone(),
        task_revision: candidate.task_revision,
        target_hash: candidate.target_hash.clone(),
        source: candidate.source.clone(),
        source_work_id: candidate.source_work_id.clone(),
        inventory_pre_hash: authorization.inventory_snapshot_hash.clone(),
        inventory_post_hash: hash(post),
        inventory_pre_count: authorization.current_total_work_ids,
        inventory_post_count: authorization.proposed_total_work_ids,
        inventory_mutation_authorized: true,
        inventory_mutation_performed: mutation_performed,
        inventory_state_verified: true,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    }
}

fn reauthorize(
    state: &State,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<(), String> {
    let fresh = local_inventory_apply_authorization::authorize(state, report, candidate)?;
    if &fresh != authorization {
        return Err("INVENTORY_V1_7_AUTHORIZATION_NOT_CURRENT".into());
    }
    Ok(())
}

pub fn apply(
    state_dir: &Path,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
    authorization: &InventoryApplyAuthorization,
) -> Result<InventoryApplyReceipt, String> {
    validate_binding(candidate, authorization)?;
    let target = state_dir.join(AUTH_WRITE_TARGET);
    let current_state = persistence::load(state_dir)?;
    let current_hash = hash(&current_state.inventory);

    if current_hash == authorization.inventory_snapshot_hash {
        reauthorize(&current_state, report, candidate, authorization)?;
        let post = expected_post_inventory(&current_state.inventory, candidate, authorization)?;
        let bytes = serde_json::to_vec_pretty(&post)
            .map_err(|_| "INVENTORY_V1_7_POST_SERIALIZE_FAILED")?;
        let replacement = temp_path(&target, authorization)?;
        prepare_synced_replacement(&replacement, &bytes)?;

        let pre_write_state = persistence::load(state_dir)?;
        if hash(&pre_write_state.inventory) != authorization.inventory_snapshot_hash {
            return Err("INVENTORY_V1_7_PRE_WRITE_INVENTORY_CHANGED".into());
        }
        reauthorize(&pre_write_state, report, candidate, authorization)?;

        atomic_replace_file(&target, &replacement)?;
        sync_target(&target)?;
        let reread = reread_inventory(&target)?;
        if reread != post
            || hash(&reread) != hash(&post)
            || reread["total_work_ids"].as_u64() != Some(authorization.proposed_total_work_ids)
            || u64::try_from(reread["works"].as_array().map_or(0, Vec::len)).ok()
                != Some(authorization.proposed_total_work_ids)
        {
            return Err("INVENTORY_V1_7_POST_WRITE_VERIFICATION_FAILED".into());
        }
        let reloaded = persistence::load(state_dir)?;
        if reloaded.inventory != post {
            return Err("INVENTORY_V1_7_POST_STATE_RELOAD_MISMATCH".into());
        }
        return Ok(receipt(
            "APPLIED_EXACT_POST",
            true,
            candidate,
            authorization,
            &post,
        ));
    }

    let pre = reconstruct_pre_from_exact_post(&current_state.inventory, candidate, authorization)
        .map_err(|_| "INVENTORY_V1_7_STATE_NEITHER_EXACT_PRE_NOR_POST".to_string())?;
    let mut reconstructed_state = current_state.clone();
    reconstructed_state.inventory = pre.clone();
    reauthorize(&reconstructed_state, report, candidate, authorization)?;
    let expected_post = expected_post_inventory(&pre, candidate, authorization)?;
    if current_state.inventory != expected_post {
        return Err("INVENTORY_V1_7_STATE_NEITHER_EXACT_PRE_NOR_POST".into());
    }
    let reread = reread_inventory(&target)?;
    if reread != expected_post {
        return Err("INVENTORY_V1_7_RETRY_REREAD_MISMATCH".into());
    }
    Ok(receipt(
        "ALREADY_APPLIED_EXACT_POST",
        false,
        candidate,
        authorization,
        &expected_post,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn candidate(pre: &Value, proposed_work: Value) -> InventoryUpdateCandidate {
        let source_identity_hash = hash(&json!({"identity":"fixture"}));
        let candidate_hash = hash(&json!({"candidate":"fixture"}));
        InventoryUpdateCandidate {
            schema_version: 1,
            candidate_id: format!("INVENTORY_CANDIDATE_{}", &candidate_hash[..20]),
            candidate_hash,
            operation: "ADD_NEW_WORK_ONLY".into(),
            inventory_schema_version: 8,
            inventory_rules_version: "fixture-rules-v1".into(),
            inventory_snapshot_hash: hash(pre),
            state_context_hash: hash(&"fixture-context"),
            matcher_version: rules_core::title_m2::RULE_VERSION.into(),
            current_total_work_ids: 1,
            proposed_total_work_ids: 2,
            rescan_id: "RESCAN_fixture".into(),
            inventory_observation_hash: hash(&"fixture-observation"),
            command_id: "EXEC_fixture".into(),
            task_id: "TASK_fixture".into(),
            work_id: "WORK_NEW".into(),
            task_revision: 1,
            target_hash: hash(&"fixture-target"),
            source: "jm".into(),
            source_work_id: "123".into(),
            proposed_local_item_id: "LOCAL_NEW".into(),
            source_identity_hash,
            proposed_work,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        }
    }

    fn authorization(candidate: &InventoryUpdateCandidate) -> InventoryApplyAuthorization {
        InventoryApplyAuthorization {
            schema_version: 1,
            authorization_id: "INVENTORY_APPLY_AUTH_fixture".into(),
            authorization_hash: hash(&"fixture-auth"),
            operation: AUTH_OPERATION.into(),
            authorized_write_target: AUTH_WRITE_TARGET.into(),
            authorized_transition: AUTH_TRANSITION.into(),
            candidate_id: candidate.candidate_id.clone(),
            candidate_hash: candidate.candidate_hash.clone(),
            inventory_schema_version: candidate.inventory_schema_version,
            inventory_rules_version: candidate.inventory_rules_version.clone(),
            inventory_snapshot_hash: candidate.inventory_snapshot_hash.clone(),
            state_context_hash: candidate.state_context_hash.clone(),
            matcher_version: candidate.matcher_version.clone(),
            current_total_work_ids: candidate.current_total_work_ids,
            proposed_total_work_ids: candidate.proposed_total_work_ids,
            rescan_id: candidate.rescan_id.clone(),
            inventory_observation_hash: candidate.inventory_observation_hash.clone(),
            command_id: candidate.command_id.clone(),
            task_id: candidate.task_id.clone(),
            work_id: candidate.work_id.clone(),
            task_revision: candidate.task_revision,
            target_hash: candidate.target_hash.clone(),
            source: candidate.source.clone(),
            source_work_id: candidate.source_work_id.clone(),
            proposed_local_item_id: candidate.proposed_local_item_id.clone(),
            source_identity_hash: candidate.source_identity_hash.clone(),
            proposed_work_hash: hash(&candidate.proposed_work),
            inventory_mutation_authorized: true,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        }
    }

    fn fixture() -> (
        Value,
        InventoryUpdateCandidate,
        InventoryApplyAuthorization,
    ) {
        let pre = json!({
            "schema_version":8,
            "rules_version":"fixture-rules-v1",
            "total_work_ids":1,
            "works":[{
                "work_id":"WORK_OLD",
                "owned":true,
                "local_item_ids":["LOCAL_OLD"],
                "authors_confirmed":["fixture"],
                "title_candidates":[],
                "versions":[],
                "source_mappings":{"jm":[],"pica":[]}
            }]
        });
        let proposed = json!({
            "work_id":"WORK_NEW",
            "owned":true,
            "local_item_ids":["LOCAL_NEW"],
            "authors_confirmed":["fixture"],
            "title_candidates":[{"primary":"Fixture","normalized_key":"fixture","fandom_or_source":null}],
            "versions":[{"local_item_id":"LOCAL_NEW"}],
            "source_mappings":{"jm":["123"],"pica":[]}
        });
        let candidate = candidate(&pre, proposed);
        let authorization = authorization(&candidate);
        (pre, candidate, authorization)
    }

    #[test]
    fn exact_post_reverses_to_unique_pre_and_rebuilds() {
        let (pre, candidate, authorization) = fixture();
        let post = expected_post_inventory(&pre, &candidate, &authorization).unwrap();
        let reversed =
            reconstruct_pre_from_exact_post(&post, &candidate, &authorization).unwrap();
        assert_eq!(reversed, pre);
        assert_eq!(
            expected_post_inventory(&reversed, &candidate, &authorization).unwrap(),
            post
        );
    }

    #[test]
    fn post_with_extra_change_fails_closed() {
        let (pre, candidate, authorization) = fixture();
        let mut post = expected_post_inventory(&pre, &candidate, &authorization).unwrap();
        post["works"][0]["owned"] = json!(false);
        assert!(reconstruct_pre_from_exact_post(&post, &candidate, &authorization).is_err());
    }

    #[test]
    fn post_must_be_append_not_insert_or_duplicate() {
        let (pre, candidate, authorization) = fixture();
        let mut post = expected_post_inventory(&pre, &candidate, &authorization).unwrap();
        post["works"].as_array_mut().unwrap().swap(0, 1);
        assert!(reconstruct_pre_from_exact_post(&post, &candidate, &authorization).is_err());
    }

    #[test]
    fn atomic_replace_round_trip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let unique = format!("mangamonitor-v17-{}-{stamp}", std::process::id());
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("inventory_index.json");
        let replacement = dir.join("replacement.tmp");
        fs::write(&target, b"pre").unwrap();
        fs::write(&replacement, b"post").unwrap();
        open_rw(&replacement, "TEST_REPLACEMENT_OPEN")
            .unwrap()
            .sync_all()
            .unwrap();
        atomic_replace_file(&target, &replacement).unwrap();
        sync_target(&target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"post");
        assert!(!replacement.exists());
        let _ = fs::remove_dir_all(dir);
    }
}

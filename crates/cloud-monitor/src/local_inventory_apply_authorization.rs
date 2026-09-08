//! V1.6 authorization-only gate for one add-only inventory apply.
//!
//! This module never writes state or the filesystem. It independently binds a
//! V1.5 candidate to the current durable state, the exact V1.4 rescan evidence,
//! and a fresh current-matcher PROVEN_NEW result. A successful artifact only
//! authorizes a later gate to append one new work to inventory_index.json and
//! increment total_work_ids by one. Task completion and every destructive or
//! production authority remain closed.

use crate::{
    local_inventory_rescan::LocalInventoryRescanReport,
    local_inventory_update_candidate::{
        self, InventoryUpdateCandidate, INVENTORY_UPDATE_CANDIDATE_SCHEMA_VERSION,
    },
    matcher_m2::{self, Outcome, ScopeCertificate},
    monitor::{hash, State},
};
use rules_core::title_m2;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const INVENTORY_APPLY_AUTHORIZATION_SCHEMA_VERSION: u64 = 1;
const OPERATION: &str = "AUTHORIZE_ADD_NEW_WORK_APPLY";
const WRITE_TARGET: &str = "inventory_index.json";
const TRANSITION: &str = "APPEND_ONE_WORK_AND_INCREMENT_TOTAL_ONLY";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InventoryApplyAuthorization {
    pub schema_version: u64,
    pub authorization_id: String,
    pub authorization_hash: String,
    pub operation: String,
    pub authorized_write_target: String,
    pub authorized_transition: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub inventory_schema_version: u64,
    pub inventory_rules_version: String,
    pub inventory_snapshot_hash: String,
    pub state_context_hash: String,
    pub matcher_version: String,
    pub current_total_work_ids: u64,
    pub proposed_total_work_ids: u64,
    pub rescan_id: String,
    pub inventory_observation_hash: String,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub proposed_local_item_id: String,
    pub source_identity_hash: String,
    pub proposed_work_hash: String,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

fn candidate_hash(candidate: &InventoryUpdateCandidate) -> String {
    hash(&json!({
        "schema_version": candidate.schema_version,
        "operation": candidate.operation.as_str(),
        "inventory_schema_version": candidate.inventory_schema_version,
        "inventory_rules_version": candidate.inventory_rules_version.as_str(),
        "inventory_snapshot_hash": candidate.inventory_snapshot_hash.as_str(),
        "state_context_hash": candidate.state_context_hash.as_str(),
        "matcher_version": candidate.matcher_version.as_str(),
        "current_total_work_ids": candidate.current_total_work_ids,
        "proposed_total_work_ids": candidate.proposed_total_work_ids,
        "rescan_id": candidate.rescan_id.as_str(),
        "inventory_observation_hash": candidate.inventory_observation_hash.as_str(),
        "command_id": candidate.command_id.as_str(),
        "task_id": candidate.task_id.as_str(),
        "work_id": candidate.work_id.as_str(),
        "task_revision": candidate.task_revision,
        "target_hash": candidate.target_hash.as_str(),
        "source": candidate.source.as_str(),
        "source_work_id": candidate.source_work_id.as_str(),
        "proposed_local_item_id": candidate.proposed_local_item_id.as_str(),
        "source_identity_hash": candidate.source_identity_hash.as_str(),
        "proposed_work": &candidate.proposed_work,
    }))
}

fn validate_candidate_envelope(candidate: &InventoryUpdateCandidate) -> Result<(), String> {
    if candidate.schema_version != INVENTORY_UPDATE_CANDIDATE_SCHEMA_VERSION
        || candidate.operation != "ADD_NEW_WORK_ONLY"
        || !matches!(candidate.source.as_str(), "jm" | "pica")
        || candidate.candidate_id.trim().is_empty()
        || candidate.candidate_hash.len() != 64
        || candidate.inventory_snapshot_hash.len() != 64
        || candidate.state_context_hash.len() != 64
        || candidate.source_identity_hash.len() != 64
        || candidate.task_revision == 0
        || candidate.current_total_work_ids.checked_add(1)
            != Some(candidate.proposed_total_work_ids)
        || candidate.inventory_mutation_authorized
        || candidate.task_completion_authorized
        || candidate.promotion_authorized
        || candidate.replacement_authorized
        || candidate.physical_delete_authorized
        || candidate.production_enablement_authorized
    {
        return Err("INVENTORY_V1_6_CANDIDATE_NOT_SAFE".into());
    }
    let expected_hash = candidate_hash(candidate);
    if candidate.candidate_hash != expected_hash
        || candidate.candidate_id != format!("INVENTORY_CANDIDATE_{}", &expected_hash[..20])
    {
        return Err("INVENTORY_V1_6_CANDIDATE_HASH_MISMATCH".into());
    }
    Ok(())
}

fn validate_current_inventory(state: &State, candidate: &InventoryUpdateCandidate) -> Result<(), String> {
    if hash(&state.inventory) != candidate.inventory_snapshot_hash
        || state.context() != candidate.state_context_hash
        || state.inventory["schema_version"].as_u64() != Some(candidate.inventory_schema_version)
        || state.inventory["rules_version"].as_str()
            != Some(candidate.inventory_rules_version.as_str())
        || state.inventory["total_work_ids"].as_u64() != Some(candidate.current_total_work_ids)
    {
        return Err("INVENTORY_V1_6_CURRENT_SNAPSHOT_CHANGED".into());
    }
    let works = state.inventory["works"]
        .as_array()
        .ok_or("INVENTORY_V1_6_CURRENT_WORKS_INVALID")?;
    if u64::try_from(works.len()).ok() != Some(candidate.current_total_work_ids) {
        return Err("INVENTORY_V1_6_CURRENT_WORK_COUNT_MISMATCH".into());
    }
    for work in works {
        if work["work_id"].as_str() == Some(candidate.work_id.as_str()) {
            return Err("INVENTORY_V1_6_WORK_ALREADY_PRESENT".into());
        }
        if work["local_item_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|id| id.as_str() == Some(candidate.proposed_local_item_id.as_str()))
        {
            return Err("INVENTORY_V1_6_LOCAL_ITEM_ALREADY_PRESENT".into());
        }
        if work["source_mappings"][candidate.source.as_str()]
            .as_array()
            .into_iter()
            .flatten()
            .any(|id| id.as_str() == Some(candidate.source_work_id.as_str()))
        {
            return Err("INVENTORY_V1_6_SOURCE_ALREADY_MAPPED".into());
        }
    }
    Ok(())
}

fn validate_proposed_work(candidate: &InventoryUpdateCandidate) -> Result<(), String> {
    let work = &candidate.proposed_work;
    let local_ids = work["local_item_ids"]
        .as_array()
        .ok_or("INVENTORY_V1_6_PROPOSED_LOCAL_IDS_INVALID")?;
    let versions = work["versions"]
        .as_array()
        .ok_or("INVENTORY_V1_6_PROPOSED_VERSIONS_INVALID")?;
    if work["work_id"].as_str() != Some(candidate.work_id.as_str())
        || work["owned"].as_bool() != Some(true)
        || local_ids.len() != 1
        || local_ids[0].as_str() != Some(candidate.proposed_local_item_id.as_str())
        || versions.len() != 1
        || versions[0]["local_item_id"].as_str()
            != Some(candidate.proposed_local_item_id.as_str())
        || !work["authors_confirmed"]
            .as_array()
            .is_some_and(|authors| authors.len() == 1 && authors[0].as_str().is_some_and(|v| !v.trim().is_empty()))
        || !work["title_candidates"]
            .as_array()
            .is_some_and(|titles| titles.len() == 1 && titles[0]["primary"].as_str().is_some_and(|v| !v.trim().is_empty()))
    {
        return Err("INVENTORY_V1_6_PROPOSED_WORK_INVALID".into());
    }
    let jm = work["source_mappings"]["jm"]
        .as_array()
        .ok_or("INVENTORY_V1_6_PROPOSED_SOURCE_MAPPING_INVALID")?;
    let pica = work["source_mappings"]["pica"]
        .as_array()
        .ok_or("INVENTORY_V1_6_PROPOSED_SOURCE_MAPPING_INVALID")?;
    let source_ok = match candidate.source.as_str() {
        "jm" => jm.len() == 1 && jm[0].as_str() == Some(candidate.source_work_id.as_str()) && pica.is_empty(),
        "pica" => pica.len() == 1 && pica[0].as_str() == Some(candidate.source_work_id.as_str()) && jm.is_empty(),
        _ => false,
    };
    if !source_ok {
        return Err("INVENTORY_V1_6_PROPOSED_SOURCE_MAPPING_INVALID".into());
    }
    Ok(())
}

fn validate_report_binding(
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
) -> Result<(), String> {
    if report.rescan_id != candidate.rescan_id
        || report.inventory_observation_hash != candidate.inventory_observation_hash
        || report.command_id != candidate.command_id
        || report.task_id != candidate.task_id
        || report.work_id != candidate.work_id
        || report.task_revision != candidate.task_revision
        || report.target_hash != candidate.target_hash
        || report.source != candidate.source
        || report.source_work_id != candidate.source_work_id
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
        return Err("INVENTORY_V1_6_RESCAN_BINDING_MISMATCH".into());
    }
    Ok(())
}

fn independently_revalidate_matcher(
    state: &State,
    candidate: &InventoryUpdateCandidate,
) -> Result<(), String> {
    let task = state
        .pending
        .get(&candidate.work_id)
        .ok_or("INVENTORY_V1_6_CURRENT_TASK_REQUIRED")?;
    if task.task_id != candidate.task_id
        || task.task_revision != candidate.task_revision
        || task.action != "download"
        || task.status != "pending"
        || !task.old_local_item_ids.is_empty()
        || hash(&task.target) != candidate.target_hash
        || task.target.source_key != format!("{}:{}", candidate.source, candidate.source_work_id)
        || !task.target.coverage.is_null()
    {
        return Err("INVENTORY_V1_6_CURRENT_TASK_CHANGED".into());
    }
    let entry = state
        .catalog
        .get(&task.target.source_key)
        .ok_or("INVENTORY_V1_6_CURRENT_SOURCE_ENTRY_REQUIRED")?;
    if !entry.active
        || entry.record.processing_result != "PENDING"
        || entry.work_id.as_deref() != Some(candidate.work_id.as_str())
        || entry.matcher_version != title_m2::RULE_VERSION
        || entry.analysis_context != state.context()
        || entry.identity_provenance["analysis_context"].as_str()
            != Some(entry.analysis_context.as_str())
        || entry.record.source != candidate.source
        || entry.record.source_work_id != candidate.source_work_id
    {
        return Err("INVENTORY_V1_6_CURRENT_IDENTITY_CONTEXT_CHANGED".into());
    }
    let stored: Outcome = serde_json::from_value(entry.identity_evidence.clone())
        .map_err(|_| "INVENTORY_V1_6_STORED_IDENTITY_INVALID")?;
    let certificates: Vec<ScopeCertificate> = serde_json::from_value(
        stored.content_type_evidence["scope_certificates"].clone(),
    )
    .map_err(|_| "INVENTORY_V1_6_SCOPE_CERTIFICATES_REQUIRED")?;
    if certificates.is_empty()
        || certificates.iter().any(|certificate| {
            certificate.author != task.target.author
                || certificate.inventory_hash != candidate.inventory_snapshot_hash
                || certificate.reference.trim().is_empty()
        })
    {
        return Err("INVENTORY_V1_6_SCOPE_CERTIFICATES_STALE".into());
    }
    let rerun = matcher_m2::decide(state, &entry.record, &certificates);
    if rerun.disposition != "PROVEN_NEW"
        || rerun.reason != "COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT"
        || rerun.uniqueness != "PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT"
        || rerun.work_id.as_deref() != Some(candidate.work_id.as_str())
        || rerun.author_evidence.canonical_author.as_deref() != Some(task.target.author.as_str())
        || rerun.source_identity.rule_version != title_m2::RULE_VERSION
        || rerun.source_identity.raw.as_deref() != Some(task.target.title.as_str())
        || !rerun.source_identity.issues.is_empty()
        || hash(&rerun.source_identity) != candidate.source_identity_hash
        || hash(&rerun) != hash(&stored)
    {
        return Err("INVENTORY_V1_6_FRESH_MATCHER_NOT_PROVEN_NEW".into());
    }
    Ok(())
}

/// Authorize exactly one future add-only inventory mutation without executing it.
pub fn authorize(
    state: &State,
    report: &LocalInventoryRescanReport,
    candidate: &InventoryUpdateCandidate,
) -> Result<InventoryApplyAuthorization, String> {
    validate_candidate_envelope(candidate)?;
    validate_report_binding(report, candidate)?;
    validate_current_inventory(state, candidate)?;
    validate_proposed_work(candidate)?;
    independently_revalidate_matcher(state, candidate)?;

    // Rebuild through the complete V1.5 contract as an additional consistency
    // check after the independent current-state and matcher validation above.
    let rebuilt = local_inventory_update_candidate::build(state, report)?;
    if &rebuilt != candidate {
        return Err("INVENTORY_V1_6_CANDIDATE_REBUILD_MISMATCH".into());
    }

    let proposed_work_hash = hash(&candidate.proposed_work);
    let authorization_hash = hash(&json!({
        "schema_version": INVENTORY_APPLY_AUTHORIZATION_SCHEMA_VERSION,
        "operation": OPERATION,
        "authorized_write_target": WRITE_TARGET,
        "authorized_transition": TRANSITION,
        "candidate_id": candidate.candidate_id.as_str(),
        "candidate_hash": candidate.candidate_hash.as_str(),
        "inventory_schema_version": candidate.inventory_schema_version,
        "inventory_rules_version": candidate.inventory_rules_version.as_str(),
        "inventory_snapshot_hash": candidate.inventory_snapshot_hash.as_str(),
        "state_context_hash": candidate.state_context_hash.as_str(),
        "matcher_version": candidate.matcher_version.as_str(),
        "current_total_work_ids": candidate.current_total_work_ids,
        "proposed_total_work_ids": candidate.proposed_total_work_ids,
        "rescan_id": candidate.rescan_id.as_str(),
        "inventory_observation_hash": candidate.inventory_observation_hash.as_str(),
        "command_id": candidate.command_id.as_str(),
        "task_id": candidate.task_id.as_str(),
        "work_id": candidate.work_id.as_str(),
        "task_revision": candidate.task_revision,
        "target_hash": candidate.target_hash.as_str(),
        "source": candidate.source.as_str(),
        "source_work_id": candidate.source_work_id.as_str(),
        "proposed_local_item_id": candidate.proposed_local_item_id.as_str(),
        "source_identity_hash": candidate.source_identity_hash.as_str(),
        "proposed_work_hash": proposed_work_hash.as_str(),
        "inventory_mutation_authorized": true,
        "task_completion_authorized": false,
        "promotion_authorized": false,
        "replacement_authorized": false,
        "physical_delete_authorized": false,
        "production_enablement_authorized": false,
    }));
    let authorization_id = format!("INVENTORY_APPLY_AUTH_{}", &authorization_hash[..20]);

    Ok(InventoryApplyAuthorization {
        schema_version: INVENTORY_APPLY_AUTHORIZATION_SCHEMA_VERSION,
        authorization_id,
        authorization_hash,
        operation: OPERATION.into(),
        authorized_write_target: WRITE_TARGET.into(),
        authorized_transition: TRANSITION.into(),
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
        proposed_work_hash,
        inventory_mutation_authorized: true,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::{Decisions, Entry, Scan, Target, Task};
    use serde_json::{json, Value};
    use state_model::{Record, Version};
    use std::collections::BTreeMap;

    fn existing_work() -> Value {
        json!({
            "work_id": "WORK_EXISTING",
            "local_item_ids": ["LOCAL_ITEM_EXISTING"],
            "authors_confirmed": ["fixture-author"],
            "title_candidates": [{
                "primary": "Fixture Work 6",
                "normalized_key": rules_core::normalize_title("Fixture Work 6"),
                "fandom_or_source": null
            }],
            "owned": true,
            "versions": [{
                "local_item_id": "LOCAL_ITEM_EXISTING",
                "language": {"chinese":"confirmed","evidence":"fixture"},
                "version": {
                    "censorship":"unknown","censorship_evidence":"fixture",
                    "color":"unknown","color_evidence":"fixture",
                    "translation_type":"unknown","translation_type_evidence":"fixture",
                    "sample_or_preview": null,"sample_evidence":"fixture",
                    "ignored_quality_source_flags": []
                },
                "content": {
                    "type":"manga","collection":false,"complete_edition":false,
                    "coverage_ranges":[],"explicit_member_numbers":[],"part_markers":[],
                    "complete_count":null,"extra_content":[]
                }
            }],
            "source_mappings": {"jm":[],"pica":[]}
        })
    }

    fn fixture() -> (State, LocalInventoryRescanReport, InventoryUpdateCandidate) {
        let source = "jm";
        let source_work_id = "123456";
        let source_key = format!("{source}:{source_work_id}");
        let work_id = format!("WORK_SRC_{}", &hash(&source_key)[..20]);
        let task_id = format!("TASK_{}", &hash(&work_id)[..20]);
        let target = Target {
            source_key: source_key.clone(),
            author: "fixture-author".into(),
            title: "Fixture Work 7".into(),
            version: Version {
                chinese: Some(true),
                uncensored: None,
                color: Some(true),
                translation: "human".into(),
                sample: None,
            },
            coverage: Value::Null,
        };
        let target_hash = hash(&target);
        let command_id = format!(
            "EXEC_{}",
            &hash(&(task_id.as_str(), 1u64, target_hash.as_str()))[..20]
        );
        let inventory = json!({
            "schema_version": 8,
            "rules_version": "fixture-rules-v1",
            "total_work_ids": 1,
            "works": [existing_work()]
        });
        let mut state = State {
            authors: json!({"authors":[{"name":"fixture-author","enabled":true}]}),
            inventory,
            catalog: BTreeMap::new(),
            pending: BTreeMap::new(),
            review: BTreeMap::new(),
            cleanup_review: json!([]),
            decisions: Decisions::default(),
            scan: Scan::default(),
        };
        state.pending.insert(
            work_id.clone(),
            Task {
                task_id: task_id.clone(),
                work_id: work_id.clone(),
                first_seen: "2026-09-07T00:00:00Z".into(),
                task_revision: 1,
                target: target.clone(),
                action: "download".into(),
                status: "pending".into(),
                old_local_item_ids: vec![],
            },
        );
        let record = Record {
            source: source.into(),
            source_work_id: source_work_id.into(),
            author: vec![target.author.clone()],
            raw_title: target.title.clone(),
            metadata: json!({"content_type":"manga"}),
            fingerprint: "fixture-search".into(),
            first_seen: "2026-09-07T00:00:00Z".into(),
            last_seen: "2026-09-07T00:00:00Z".into(),
            last_checked: "2026-09-07T00:00:00Z".into(),
            processing_result: "PENDING".into(),
        };
        let certificate = ScopeCertificate {
            author: target.author.clone(),
            inventory_hash: hash(&state.inventory),
            reference: "v1.6-fixture-complete-author-scope".into(),
        };
        let outcome = matcher_m2::decide(&state, &record, std::slice::from_ref(&certificate));
        assert_eq!(outcome.disposition, "PROVEN_NEW");
        let context = state.context();
        state.catalog.insert(
            source_key.clone(),
            Entry {
                record,
                author_evidence: outcome.author_evidence.clone(),
                search_fingerprint: "fixture-search".into(),
                detail_fingerprint: "fixture-detail".into(),
                analysis_context: context.clone(),
                work_id: Some(work_id.clone()),
                analysis_count: 1,
                unavailable_streak: 0,
                active: true,
                last_unavailable_check: None,
                search_queries: Default::default(),
                matcher_version: title_m2::RULE_VERSION.into(),
                identity_evidence: serde_json::to_value(&outcome).unwrap(),
                identity_provenance: json!({"analysis_context":context}),
            },
        );
        let library_relative_dir = format!("mangamonitor-{command_id}");
        let sidecar_sha256 = "a".repeat(64);
        let manifest_hash = "b".repeat(64);
        let observation_hash = hash(&(
            work_id.as_str(),
            target_hash.as_str(),
            source,
            source_work_id,
            library_relative_dir.as_str(),
            sidecar_sha256.as_str(),
            manifest_hash.as_str(),
            1u64,
            123u64,
        ));
        let report = LocalInventoryRescanReport {
            schema_version: crate::local_inventory_rescan::LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION,
            rescan_id: format!("RESCAN_{}", &observation_hash[..20]),
            command_id,
            task_id,
            work_id,
            task_revision: 1,
            target_hash,
            source: source.into(),
            source_work_id: source_work_id.into(),
            library_relative_dir,
            sidecar_sha256,
            manifest_hash,
            file_count: 1,
            total_bytes: 123,
            sidecar_verified: true,
            manifest_verified: true,
            filesystem_verified: true,
            inventory_observation_complete: true,
            inventory_observation_hash: observation_hash,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        };
        let candidate = local_inventory_update_candidate::build(&state, &report).expect("v1.5 candidate");
        (state, report, candidate)
    }

    #[test]
    fn exact_current_evidence_authorizes_only_add_one_inventory_mutation() {
        let (state, report, candidate) = fixture();
        let before = hash(&state.inventory);
        let first = authorize(&state, &report, &candidate).expect("authorization");
        let second = authorize(&state, &report, &candidate).expect("deterministic replay");
        assert_eq!(first, second);
        assert_eq!(hash(&state.inventory), before);
        assert_eq!(first.authorized_write_target, WRITE_TARGET);
        assert_eq!(first.authorized_transition, TRANSITION);
        assert!(first.inventory_mutation_authorized);
        assert!(!first.task_completion_authorized);
        assert!(!first.promotion_authorized);
        assert!(!first.replacement_authorized);
        assert!(!first.physical_delete_authorized);
        assert!(!first.production_enablement_authorized);
    }

    #[test]
    fn tampered_candidate_or_changed_inventory_fails_closed() {
        let (state, report, mut candidate) = fixture();
        candidate.proposed_work["owned"] = json!(false);
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_CANDIDATE_HASH_MISMATCH"
        );

        let (mut state, report, candidate) = fixture();
        state.inventory["rules_version"] = json!("changed");
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_CURRENT_SNAPSHOT_CHANGED"
        );
    }

    #[test]
    fn changed_task_or_rescan_binding_fails_closed() {
        let (mut state, report, candidate) = fixture();
        state.pending.get_mut(&candidate.work_id).unwrap().status = "done".into();
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_CURRENT_TASK_CHANGED"
        );

        let (state, mut report, candidate) = fixture();
        report.task_revision = 2;
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_RESCAN_BINDING_MISMATCH"
        );
    }

    #[test]
    fn stale_scope_certificate_or_matcher_evidence_fails_closed() {
        let (mut state, report, candidate) = fixture();
        let key = format!("{}:{}", candidate.source, candidate.source_work_id);
        let entry = state.catalog.get_mut(&key).unwrap();
        let mut stored: Outcome = serde_json::from_value(entry.identity_evidence.clone()).unwrap();
        stored.content_type_evidence["scope_certificates"][0]["inventory_hash"] = json!("0".repeat(64));
        entry.identity_evidence = serde_json::to_value(stored).unwrap();
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_SCOPE_CERTIFICATES_STALE"
        );

        let (mut state, report, candidate) = fixture();
        let key = format!("{}:{}", candidate.source, candidate.source_work_id);
        let entry = state.catalog.get_mut(&key).unwrap();
        let mut stored: Outcome = serde_json::from_value(entry.identity_evidence.clone()).unwrap();
        stored.disposition = "AUTO_EXISTING".into();
        entry.identity_evidence = serde_json::to_value(stored).unwrap();
        assert_eq!(
            authorize(&state, &report, &candidate).unwrap_err(),
            "INVENTORY_V1_6_FRESH_MATCHER_NOT_PROVEN_NEW"
        );
    }
}

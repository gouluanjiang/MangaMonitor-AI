//! V1.5 read-only add-only inventory update candidate builder.
//!
//! This module consumes the current durable state plus a verified V1.4 rescan
//! report and emits a deterministic proposal for one brand-new inventory work.
//! It never writes inventory/state, completes tasks, replaces/deletes content,
//! touches the local filesystem, calls JM/Pica, or enables production.

use crate::{
    local_inventory_rescan::{
        LocalInventoryRescanReport, LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION,
    },
    matcher_m2::Outcome,
    monitor::{hash, State},
};
use rules_core::title_m2;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const INVENTORY_UPDATE_CANDIDATE_SCHEMA_VERSION: u64 = 1;
const SUPPORTED_INVENTORY_SCHEMA_VERSION: u64 = 8;
const OPERATION: &str = "ADD_NEW_WORK_ONLY";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InventoryUpdateCandidate {
    pub schema_version: u64,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub operation: String,
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
    /// Complete proposed work object using the existing inventory field layout.
    /// V1.5 never applies this object to the durable inventory.
    pub proposed_work: Value,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_rescan(report: &LocalInventoryRescanReport) -> Result<(), String> {
    if report.schema_version != LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION {
        return Err("INVENTORY_V1_5_RESCAN_SCHEMA_MISMATCH".into());
    }
    if !matches!(report.source.as_str(), "jm" | "pica")
        || report.command_id.trim().is_empty()
        || report.task_id.trim().is_empty()
        || report.work_id.trim().is_empty()
        || report.task_revision == 0
        || !lower_sha256(&report.target_hash)
        || report.source_work_id.trim().is_empty()
        || report.library_relative_dir != format!("mangamonitor-{}", report.command_id)
        || !lower_sha256(&report.sidecar_sha256)
        || !lower_sha256(&report.manifest_hash)
        || report.file_count == 0
        || report.total_bytes == 0
        || !report.sidecar_verified
        || !report.manifest_verified
        || !report.filesystem_verified
        || !report.inventory_observation_complete
        || !lower_sha256(&report.inventory_observation_hash)
        || report.inventory_mutation_authorized
        || report.task_completion_authorized
        || report.promotion_authorized
        || report.replacement_authorized
        || report.physical_delete_authorized
        || report.production_enablement_authorized
    {
        return Err("INVENTORY_V1_5_RESCAN_NOT_SAFE".into());
    }
    let observation_hash = hash(&(
        report.work_id.as_str(),
        report.target_hash.as_str(),
        report.source.as_str(),
        report.source_work_id.as_str(),
        report.library_relative_dir.as_str(),
        report.sidecar_sha256.as_str(),
        report.manifest_hash.as_str(),
        report.file_count,
        report.total_bytes,
    ));
    if report.inventory_observation_hash != observation_hash
        || report.rescan_id != format!("RESCAN_{}", &observation_hash[..20])
    {
        return Err("INVENTORY_V1_5_RESCAN_HASH_MISMATCH".into());
    }
    Ok(())
}

struct InventoryFacts {
    schema_version: u64,
    rules_version: String,
    total_work_ids: u64,
    snapshot_hash: String,
    local_item_ids: BTreeSet<String>,
}

fn validate_inventory(
    state: &State,
    report: &LocalInventoryRescanReport,
) -> Result<InventoryFacts, String> {
    let schema_version = state.inventory["schema_version"]
        .as_u64()
        .ok_or("INVENTORY_V1_5_INVENTORY_SCHEMA_INVALID")?;
    if schema_version != SUPPORTED_INVENTORY_SCHEMA_VERSION {
        return Err("INVENTORY_V1_5_INVENTORY_SCHEMA_UNSUPPORTED".into());
    }
    let rules_version = state.inventory["rules_version"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or("INVENTORY_V1_5_RULES_VERSION_REQUIRED")?
        .to_string();
    let works = state.inventory["works"]
        .as_array()
        .ok_or("INVENTORY_V1_5_WORKS_ARRAY_REQUIRED")?;
    let total_work_ids = state.inventory["total_work_ids"]
        .as_u64()
        .ok_or("INVENTORY_V1_5_TOTAL_WORK_IDS_INVALID")?;
    let actual_total =
        u64::try_from(works.len()).map_err(|_| "INVENTORY_V1_5_WORK_COUNT_OVERFLOW")?;
    if total_work_ids != actual_total {
        return Err("INVENTORY_V1_5_TOTAL_WORK_IDS_MISMATCH".into());
    }

    let mut work_ids = BTreeSet::new();
    let mut local_item_ids = BTreeSet::new();
    let mut mapped_source_ids = BTreeSet::new();
    for work in works {
        let work_id = work["work_id"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or("INVENTORY_V1_5_EXISTING_WORK_INVALID")?;
        if !work_ids.insert(work_id.to_string()) {
            return Err("INVENTORY_V1_5_DUPLICATE_WORK_ID".into());
        }
        if work_id == report.work_id {
            return Err("INVENTORY_V1_5_WORK_ALREADY_PRESENT".into());
        }
        if !work["owned"].is_boolean()
            || !work["local_item_ids"].is_array()
            || !work["versions"].is_array()
            || !work["source_mappings"].is_object()
        {
            return Err("INVENTORY_V1_5_EXISTING_WORK_INVALID".into());
        }
        for value in work["local_item_ids"].as_array().into_iter().flatten() {
            let id = value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or("INVENTORY_V1_5_EXISTING_LOCAL_ITEM_ID_INVALID")?;
            if !local_item_ids.insert(id.to_string()) {
                return Err("INVENTORY_V1_5_DUPLICATE_LOCAL_ITEM_ID".into());
            }
        }
        for source in ["jm", "pica"] {
            let ids = work["source_mappings"][source]
                .as_array()
                .ok_or("INVENTORY_V1_5_SOURCE_MAPPINGS_INVALID")?;
            for value in ids {
                let source_id = value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or("INVENTORY_V1_5_SOURCE_MAPPING_ID_INVALID")?;
                if !mapped_source_ids.insert((source.to_string(), source_id.to_string())) {
                    return Err("INVENTORY_V1_5_DUPLICATE_SOURCE_MAPPING".into());
                }
                if source == report.source && source_id == report.source_work_id {
                    return Err("INVENTORY_V1_5_SOURCE_ALREADY_MAPPED".into());
                }
            }
        }
    }

    Ok(InventoryFacts {
        schema_version,
        rules_version,
        total_work_ids,
        snapshot_hash: hash(&state.inventory),
        local_item_ids,
    })
}

fn language_value(value: Option<bool>) -> (&'static str, &'static str) {
    match value {
        Some(true) => ("confirmed", "SOURCE_TARGET_EXPLICIT_CHINESE"),
        Some(false) => ("not_chinese", "SOURCE_TARGET_EXPLICIT_NOT_CHINESE"),
        None => ("unknown", "INSUFFICIENT_EVIDENCE"),
    }
}

fn censorship_value(value: Option<bool>) -> (&'static str, &'static str) {
    match value {
        Some(true) => ("uncensored", "SOURCE_TARGET_EXPLICIT_UNCENSORED"),
        Some(false) => ("censored", "SOURCE_TARGET_EXPLICIT_CENSORED"),
        None => ("unknown", "NO_CENSORSHIP_EVIDENCE"),
    }
}

fn color_value(value: Option<bool>) -> (&'static str, &'static str) {
    match value {
        Some(true) => ("color", "SOURCE_TARGET_EXPLICIT_COLOR"),
        Some(false) => ("monochrome", "SOURCE_TARGET_EXPLICIT_MONOCHROME"),
        None => ("unknown", "NO_COLOR_EVIDENCE"),
    }
}

fn version_value(
    version: &state_model::Version,
    local_item_id: &str,
    content_type: &str,
) -> Result<Value, String> {
    if !matches!(version.translation.as_str(), "human" | "ai" | "unknown") {
        return Err("INVENTORY_V1_5_TRANSLATION_VALUE_UNSUPPORTED".into());
    }
    let (chinese, chinese_evidence) = language_value(version.chinese);
    let (censorship, censorship_evidence) = censorship_value(version.uncensored);
    let (color, color_evidence) = color_value(version.color);
    let translation_evidence = match version.translation.as_str() {
        "human" => "SOURCE_TARGET_EXPLICIT_HUMAN_TRANSLATION",
        "ai" => "SOURCE_TARGET_EXPLICIT_AI_TRANSLATION",
        _ => "NO_TRANSLATION_METHOD_EVIDENCE",
    };
    let (sample, sample_evidence) = match version.sample {
        Some(true) => (json!(true), "SOURCE_TARGET_EXPLICIT_SAMPLE_OR_PREVIEW"),
        Some(false) => (json!(false), "SOURCE_TARGET_EXPLICIT_NON_SAMPLE"),
        // Existing readers use `as_bool()`, so null preserves UNKNOWN rather than
        // coercing missing sample evidence into a false quality claim.
        None => (Value::Null, "INSUFFICIENT_EVIDENCE"),
    };
    Ok(json!({
        "local_item_id": local_item_id,
        "language": {
            "chinese": chinese,
            "evidence": chinese_evidence,
        },
        "version": {
            "censorship": censorship,
            "censorship_evidence": censorship_evidence,
            "color": color,
            "color_evidence": color_evidence,
            "translation_type": version.translation.as_str(),
            "translation_type_evidence": translation_evidence,
            "sample_or_preview": sample,
            "sample_evidence": sample_evidence,
            "ignored_quality_source_flags": [],
        },
        "content": {
            "type": content_type,
            "collection": false,
            "complete_edition": false,
            "coverage_ranges": [],
            "explicit_member_numbers": [],
            "part_markers": [],
            "complete_count": null,
            "extra_content": [],
        }
    }))
}

/// Build one deterministic add-only inventory proposal from the current state.
///
/// The returned object is evidence only. Every mutating/downstream authority is
/// hard-coded false; a later independently audited gate must revalidate current
/// state and explicitly authorize any durable change.
pub fn build(
    state: &State,
    report: &LocalInventoryRescanReport,
) -> Result<InventoryUpdateCandidate, String> {
    validate_rescan(report)?;
    let inventory = validate_inventory(state, report)?;

    let task = state
        .pending
        .get(&report.work_id)
        .ok_or("INVENTORY_V1_5_CURRENT_TASK_REQUIRED")?;
    let expected_task_id = format!("TASK_{}", &hash(&report.work_id)[..20]);
    let expected_command_id = format!(
        "EXEC_{}",
        &hash(&(
            task.task_id.as_str(),
            task.task_revision,
            report.target_hash.as_str(),
        ))[..20]
    );
    if task.task_id != report.task_id
        || task.task_id != expected_task_id
        || task.task_revision != report.task_revision
        || task.action != "download"
        || task.status != "pending"
        || !task.old_local_item_ids.is_empty()
        || hash(&task.target) != report.target_hash
        || task.target.source_key != format!("{}:{}", report.source, report.source_work_id)
        || report.command_id != expected_command_id
    {
        return Err("INVENTORY_V1_5_CURRENT_TASK_BINDING_MISMATCH".into());
    }
    if task.target.author.trim().is_empty() || task.target.title.trim().is_empty() {
        return Err("INVENTORY_V1_5_TARGET_IDENTITY_INCOMPLETE".into());
    }
    // Coverage application remains a later audited stage. V1.5 only handles the
    // exact no-coverage new-work tasks produced by the current bind_work path.
    if !task.target.coverage.is_null() {
        return Err("INVENTORY_V1_5_COVERAGE_NOT_ENABLED".into());
    }

    let source_key = task.target.source_key.as_str();
    let entry = state
        .catalog
        .get(source_key)
        .ok_or("INVENTORY_V1_5_CURRENT_SOURCE_ENTRY_REQUIRED")?;
    let state_context_hash = state.context();
    if !entry.active
        || entry.record.processing_result != "PENDING"
        || entry.work_id.as_deref() != Some(report.work_id.as_str())
        || entry.matcher_version != title_m2::RULE_VERSION
        || entry.analysis_context != state_context_hash
        || entry.identity_provenance["analysis_context"].as_str()
            != Some(entry.analysis_context.as_str())
        || entry.record.source != report.source
        || entry.record.source_work_id != report.source_work_id
    {
        return Err("INVENTORY_V1_5_IDENTITY_CONTEXT_STALE".into());
    }

    let outcome: Outcome = serde_json::from_value(entry.identity_evidence.clone())
        .map_err(|_| "INVENTORY_V1_5_IDENTITY_EVIDENCE_INVALID")?;
    let expected_new_work_id = format!("WORK_SRC_{}", &hash(&source_key)[..20]);
    if outcome.source_key != source_key
        || outcome.disposition != "PROVEN_NEW"
        || outcome.reason != "COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT"
        || outcome.uniqueness != "PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT"
        || outcome.work_id.as_deref() != Some(report.work_id.as_str())
        || report.work_id != expected_new_work_id
        || outcome.author_evidence.canonical_author.as_deref()
            != Some(task.target.author.as_str())
        || outcome.source_identity.rule_version != title_m2::RULE_VERSION
        || outcome.source_identity.raw.as_deref() != Some(task.target.title.as_str())
        || !outcome.source_identity.issues.is_empty()
    {
        return Err("INVENTORY_V1_5_NOT_CURRENT_PROVEN_NEW".into());
    }
    // V1.5 deliberately excludes collection/extra semantics. The current
    // PROVEN_NEW rule is an explicit disjoint installment proof, and this extra
    // check prevents a future matcher change from silently widening the stage.
    if outcome.source_identity.fields.collection.is_some()
        || outcome.source_identity.fields.extra.is_some()
    {
        return Err("INVENTORY_V1_5_COLLECTION_OR_EXTRA_NOT_ENABLED".into());
    }
    let content_type = outcome
        .source_identity
        .fields
        .content_type
        .as_deref()
        .ok_or("INVENTORY_V1_5_EXPLICIT_CONTENT_TYPE_REQUIRED")?;
    if !matches!(content_type, "manga" | "cg" | "artbook" | "novel" | "settings") {
        return Err("INVENTORY_V1_5_CONTENT_TYPE_UNSUPPORTED".into());
    }

    let local_item_seed = hash(&(
        inventory.snapshot_hash.as_str(),
        report.rescan_id.as_str(),
        report.inventory_observation_hash.as_str(),
        report.work_id.as_str(),
        report.target_hash.as_str(),
        report.manifest_hash.as_str(),
    ));
    let proposed_local_item_id = format!("LOCAL_ITEM_V15_{}", &local_item_seed[..20]);
    if inventory.local_item_ids.contains(&proposed_local_item_id) {
        return Err("INVENTORY_V1_5_LOCAL_ITEM_ID_COLLISION".into());
    }

    let version = version_value(
        &task.target.version,
        &proposed_local_item_id,
        content_type,
    )?;
    let source_mappings = match report.source.as_str() {
        "jm" => json!({"jm":[report.source_work_id.clone()],"pica":[]}),
        "pica" => json!({"jm":[],"pica":[report.source_work_id.clone()]}),
        _ => return Err("INVENTORY_V1_5_SOURCE_UNSUPPORTED".into()),
    };
    let proposed_work = json!({
        "work_id": report.work_id.clone(),
        "local_item_ids": [proposed_local_item_id.clone()],
        "authors_confirmed": [task.target.author.clone()],
        "title_candidates": [{
            "primary": task.target.title.clone(),
            "normalized_key": rules_core::normalize_title(&task.target.title),
            "fandom_or_source": outcome.source_identity.fields.fandom.clone(),
        }],
        "owned": true,
        "versions": [version],
        "source_mappings": source_mappings,
    });

    let proposed_total_work_ids = inventory
        .total_work_ids
        .checked_add(1)
        .ok_or("INVENTORY_V1_5_PROPOSED_COUNT_OVERFLOW")?;
    let source_identity_hash = hash(&outcome.source_identity);
    let candidate_hash = hash(&json!({
        "schema_version": INVENTORY_UPDATE_CANDIDATE_SCHEMA_VERSION,
        "operation": OPERATION,
        "inventory_schema_version": inventory.schema_version,
        "inventory_rules_version": inventory.rules_version,
        "inventory_snapshot_hash": inventory.snapshot_hash,
        "state_context_hash": state_context_hash,
        "matcher_version": title_m2::RULE_VERSION,
        "current_total_work_ids": inventory.total_work_ids,
        "proposed_total_work_ids": proposed_total_work_ids,
        "rescan_id": report.rescan_id,
        "inventory_observation_hash": report.inventory_observation_hash,
        "command_id": report.command_id,
        "task_id": report.task_id,
        "work_id": report.work_id,
        "task_revision": report.task_revision,
        "target_hash": report.target_hash,
        "source": report.source,
        "source_work_id": report.source_work_id,
        "proposed_local_item_id": proposed_local_item_id,
        "source_identity_hash": source_identity_hash,
        "proposed_work": proposed_work,
    }));
    let candidate_id = format!("INVENTORY_CANDIDATE_{}", &candidate_hash[..20]);

    Ok(InventoryUpdateCandidate {
        schema_version: INVENTORY_UPDATE_CANDIDATE_SCHEMA_VERSION,
        candidate_id,
        candidate_hash,
        operation: OPERATION.into(),
        inventory_schema_version: inventory.schema_version,
        inventory_rules_version: inventory.rules_version,
        inventory_snapshot_hash: inventory.snapshot_hash,
        state_context_hash,
        matcher_version: title_m2::RULE_VERSION.into(),
        current_total_work_ids: inventory.total_work_ids,
        proposed_total_work_ids,
        rescan_id: report.rescan_id.clone(),
        inventory_observation_hash: report.inventory_observation_hash.clone(),
        command_id: report.command_id.clone(),
        task_id: report.task_id.clone(),
        work_id: report.work_id.clone(),
        task_revision: report.task_revision,
        target_hash: report.target_hash.clone(),
        source: report.source.clone(),
        source_work_id: report.source_work_id.clone(),
        proposed_local_item_id,
        source_identity_hash,
        proposed_work,
        inventory_mutation_authorized: false,
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
    use crate::monitor::{AuthorEvidence, Decisions, Entry, Scan, Target, Task};
    use serde_json::json;
    use state_model::{Record, Version};
    use std::collections::BTreeMap;

    fn make_state() -> (State, LocalInventoryRescanReport) {
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
            "schema_version": SUPPORTED_INVENTORY_SCHEMA_VERSION,
            "rules_version": "fixture-rules-v1",
            "total_work_ids": 0,
            "works": []
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
        let identity = title_m2::parse(
            Some(&target.title),
            Some(&target.author),
            None,
            Some("manga"),
        );
        assert!(identity.issues.is_empty());
        let outcome = Outcome {
            source_key: source_key.clone(),
            disposition: "PROVEN_NEW".into(),
            reason: "COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT".into(),
            work_id: Some(work_id.clone()),
            author_evidence: AuthorEvidence {
                canonical_author: Some(target.author.clone()),
                rule: "DIRECT_EXACT".into(),
                matched_tokens: vec![target.author.clone()],
            },
            source_identity: identity,
            content_type_evidence: json!({"explicit":true}),
            candidate_evidence: vec![],
            matching_work_ids: vec![],
            scope_work_ids: vec![],
            uniqueness: "PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT".into(),
            downstream_result: "PENDING".into(),
        };
        let context = state.context();
        state.catalog.insert(
            source_key.clone(),
            Entry {
                record: Record {
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
                },
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
            schema_version: LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION,
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
        (state, report)
    }

    fn refresh_context(state: &mut State, source_key: &str) {
        let context = state.context();
        let entry = state.catalog.get_mut(source_key).unwrap();
        entry.analysis_context = context.clone();
        entry.identity_provenance = json!({"analysis_context":context});
    }

    #[test]
    fn exact_proven_new_builds_deterministic_candidate_without_mutation() {
        let (state, report) = make_state();
        let before = hash(&state.inventory);
        let first = build(&state, &report).expect("candidate");
        let second = build(&state, &report).expect("candidate replay");
        assert_eq!(first, second);
        assert_eq!(hash(&state.inventory), before);
        assert_eq!(first.operation, OPERATION);
        assert_eq!(first.current_total_work_ids, 0);
        assert_eq!(first.proposed_total_work_ids, 1);
        assert_eq!(first.proposed_work["work_id"], report.work_id);
        assert_eq!(first.proposed_work["owned"], true);
        assert_eq!(
            first.proposed_work["source_mappings"]["jm"][0].as_str(),
            Some(report.source_work_id.as_str())
        );
        assert!(first.proposed_work["source_mappings"]["pica"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(first.proposed_work["versions"][0]["version"]["sample_or_preview"].is_null());
        assert!(!first.inventory_mutation_authorized);
        assert!(!first.task_completion_authorized);
        assert!(!first.promotion_authorized);
        assert!(!first.replacement_authorized);
        assert!(!first.physical_delete_authorized);
        assert!(!first.production_enablement_authorized);
    }

    #[test]
    fn existing_work_or_existing_source_mapping_fails_closed() {
        let (mut state, report) = make_state();
        state.inventory["works"] = json!([{
            "work_id": report.work_id,
            "local_item_ids": [],
            "authors_confirmed": [],
            "title_candidates": [],
            "owned": false,
            "versions": [],
            "source_mappings": {"jm":[],"pica":[]}
        }]);
        state.inventory["total_work_ids"] = json!(1);
        refresh_context(
            &mut state,
            &format!("{}:{}", report.source, report.source_work_id),
        );
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_WORK_ALREADY_PRESENT"
        );

        let (mut state, report) = make_state();
        state.inventory["works"] = json!([{
            "work_id": "WORK_other",
            "local_item_ids": ["LOCAL_ITEM_other"],
            "authors_confirmed": ["fixture-author"],
            "title_candidates": [],
            "owned": true,
            "versions": [],
            "source_mappings": {"jm":[report.source_work_id.clone()],"pica":[]}
        }]);
        state.inventory["total_work_ids"] = json!(1);
        refresh_context(
            &mut state,
            &format!("{}:{}", report.source, report.source_work_id),
        );
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_SOURCE_ALREADY_MAPPED"
        );
    }

    #[test]
    fn stale_or_upgrade_task_fails_closed() {
        let (mut state, report) = make_state();
        state.pending.get_mut(&report.work_id).unwrap().action = "upgrade".into();
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_CURRENT_TASK_BINDING_MISMATCH"
        );

        let (mut state, report) = make_state();
        state
            .pending
            .get_mut(&report.work_id)
            .unwrap()
            .old_local_item_ids
            .push("LOCAL_OLD".into());
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_CURRENT_TASK_BINDING_MISMATCH"
        );
    }

    #[test]
    fn unsafe_or_tampered_rescan_fails_closed() {
        let (state, mut report) = make_state();
        report.inventory_mutation_authorized = true;
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_RESCAN_NOT_SAFE"
        );

        let (state, mut report) = make_state();
        report.inventory_observation_hash = "c".repeat(64);
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_RESCAN_HASH_MISMATCH"
        );
    }

    #[test]
    fn stale_identity_context_or_non_proven_new_fails_closed() {
        let (mut state, report) = make_state();
        state
            .catalog
            .get_mut(&format!("{}:{}", report.source, report.source_work_id))
            .unwrap()
            .analysis_context = "stale".into();
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_IDENTITY_CONTEXT_STALE"
        );

        let (mut state, report) = make_state();
        let source_key = format!("{}:{}", report.source, report.source_work_id);
        let entry = state.catalog.get_mut(&source_key).unwrap();
        let mut outcome: Outcome = serde_json::from_value(entry.identity_evidence.clone()).unwrap();
        outcome.disposition = "AUTO_EXISTING".into();
        entry.identity_evidence = serde_json::to_value(outcome).unwrap();
        assert_eq!(
            build(&state, &report).unwrap_err(),
            "INVENTORY_V1_5_NOT_CURRENT_PROVEN_NEW"
        );
    }
}

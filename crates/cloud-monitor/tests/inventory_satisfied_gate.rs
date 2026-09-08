use cloud_monitor::{
    assistant_task_gate::{plan, target_hash, task_view, GateLedger, GateRecord},
    executor_handoff::queue_view,
    monitor::{Decisions, Scan, State, Target, Task},
};
use serde_json::{json, Value};
use state_model::Version;
use std::collections::BTreeMap;

fn task() -> Task {
    Task {
        task_id: "TASK_1".into(),
        work_id: "WORK_1".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target: Target {
            source_key: "jm:123".into(),
            author: "Writer".into(),
            title: "Work 1".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: Vec::new(),
    }
}

fn state_with_inventory(inventory: Value) -> State {
    let task = task();
    State {
        authors: json!({"authors":[]}),
        inventory,
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(task.work_id.clone(), task)]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn approved(state: &State) -> GateLedger {
    let task = &state.pending["WORK_1"];
    GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: target_hash(task),
            assistant_recommended: true,
            user_approved: true,
        }],
    }
}

fn owned_work(work_id: &str, jm_ids: Value) -> Value {
    json!({
        "work_id": work_id,
        "owned": true,
        "local_item_ids": [format!("LOCAL_{work_id}")],
        "authors_confirmed": ["Writer"],
        "title_candidates": [{"primary":"Work 1","normalized_key":"work 1","fandom_or_source":null}],
        "versions": [],
        "source_mappings": {"jm": jm_ids, "pica": []}
    })
}

#[test]
fn exact_owned_source_mapping_suppresses_existing_approval_and_executor_queue() {
    let state = state_with_inventory(json!({
        "works": [owned_work("WORK_1", json!(["123"]))]
    }));
    let ledger = approved(&state);

    let view = task_view(&state, &ledger, "TASK_1").unwrap();
    assert_eq!(view["user_approved"], true);
    assert_eq!(view["inventory_satisfied"], true);
    assert_eq!(view["inventory_authority_valid"], true);
    assert_eq!(view["execution_block_reason"], "INVENTORY_ALREADY_SATISFIED");
    assert_eq!(view["download_authorized"], false);

    let queue = queue_view(&state, &ledger, 0, 50).unwrap();
    assert_eq!(queue["total_authorized"], 0);
    assert!(queue["commands"].as_array().unwrap().is_empty());

    let hash = target_hash(&state.pending["WORK_1"]);
    assert_eq!(
        plan(&state, &ledger, "recommend", "TASK_1", 1, &hash).unwrap_err(),
        "ASSISTANT_TASK_INVENTORY_ALREADY_SATISFIED"
    );
    assert_eq!(
        plan(&state, &ledger, "approve", "TASK_1", 1, &hash).unwrap_err(),
        "ASSISTANT_TASK_INVENTORY_ALREADY_SATISFIED"
    );

    let (revoked, _, preview) =
        plan(&state, &ledger, "revoke", "TASK_1", 1, &hash).unwrap();
    assert!(!revoked.records[0].user_approved);
    assert_eq!(preview["download_authorized"], false);
}

#[test]
fn ambiguous_source_mapping_fails_closed_before_executor_command() {
    let state = state_with_inventory(json!({
        "works": [
            owned_work("WORK_1", json!(["123"])),
            owned_work("WORK_OTHER", json!(["123"]))
        ]
    }));
    let ledger = approved(&state);

    let view = task_view(&state, &ledger, "TASK_1").unwrap();
    assert_eq!(view["inventory_satisfied"], false);
    assert_eq!(view["inventory_authority_valid"], false);
    assert_eq!(view["execution_block_reason"], "INVENTORY_AUTHORITY_AMBIGUOUS");
    assert_eq!(view["download_authorized"], false);
    assert_eq!(queue_view(&state, &ledger, 0, 50).unwrap()["total_authorized"], 0);

    let hash = target_hash(&state.pending["WORK_1"]);
    assert_eq!(
        plan(&state, &ledger, "approve", "TASK_1", 1, &hash).unwrap_err(),
        "ASSISTANT_TASK_INVENTORY_AUTHORITY_AMBIGUOUS"
    );
}

#[test]
fn owned_work_without_exact_source_mapping_is_not_falsely_satisfied() {
    let state = state_with_inventory(json!({
        "works": [owned_work("WORK_1", json!([]))]
    }));
    let ledger = approved(&state);

    let view = task_view(&state, &ledger, "TASK_1").unwrap();
    assert_eq!(view["inventory_satisfied"], false);
    assert_eq!(view["inventory_authority_valid"], true);
    assert!(view["execution_block_reason"].is_null());
    assert_eq!(view["download_authorized"], true);
    assert_eq!(queue_view(&state, &ledger, 0, 50).unwrap()["total_authorized"], 1);
}

#[test]
fn mapping_on_another_work_does_not_count_as_satisfied_and_is_ambiguous_authority() {
    let state = state_with_inventory(json!({
        "works": [
            owned_work("WORK_1", json!([])),
            owned_work("WORK_OTHER", json!(["123"]))
        ]
    }));
    let ledger = approved(&state);

    let view = task_view(&state, &ledger, "TASK_1").unwrap();
    assert_eq!(view["inventory_satisfied"], false);
    assert_eq!(view["inventory_authority_valid"], true);
    assert_eq!(view["download_authorized"], true);
}

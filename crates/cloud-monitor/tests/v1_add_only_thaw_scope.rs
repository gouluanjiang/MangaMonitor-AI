use cloud_monitor::{
    assistant_task_gate::{target_hash, GateLedger, GateRecord},
    executor_handoff::{self, ExecutorCommand},
    local_execution_orchestrator,
    monitor::{hash, Decisions, Scan, State, Target, Task},
};
use serde_json::{json, Value};
use state_model::Version;
use std::collections::BTreeMap;

fn fixture(action: &str, old_local_item_ids: Vec<String>) -> (State, GateLedger, ExecutorCommand) {
    let task = Task {
        task_id: "TASK_THAW_SCOPE".into(),
        work_id: "WORK_THAW_SCOPE".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target: Target {
            source_key: "jm:123456".into(),
            author: "Writer".into(),
            title: "Thaw Scope".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: action.into(),
        status: "pending".into(),
        old_local_item_ids,
    };
    let target_hash = target_hash(&task);
    let ledger = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: target_hash.clone(),
            assistant_recommended: true,
            user_approved: true,
        }],
    };
    let digest = hash(&(
        task.task_id.as_str(),
        task.task_revision,
        target_hash.as_str(),
    ));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: task.action.clone(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target: task.target.clone(),
    };
    let state = State {
        authors: json!({"authors":[]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(task.work_id.clone(), task)]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    (state, ledger, command)
}

#[test]
fn thawed_live_execution_accepts_only_add_new_download_tasks() {
    let (state, ledger, command) = fixture("download", vec![]);
    local_execution_orchestrator::prepare_current(&state, &ledger, &command).unwrap();
    let queue = executor_handoff::queue_view(&state, &ledger, 0, 10).unwrap();
    assert_eq!(queue["total_authorized"], 1);
    assert_eq!(queue["commands"][0]["action"], "download");
}

#[test]
fn thawed_live_execution_rejects_upgrade_before_source_access() {
    let (state, ledger, command) = fixture("upgrade", vec!["LOCAL_OLD".into()]);
    assert_eq!(
        local_execution_orchestrator::prepare_current(&state, &ledger, &command).unwrap_err(),
        "V1_ADD_ONLY_LIVE_EXECUTION_REQUIRES_DOWNLOAD_ACTION"
    );
    let queue = executor_handoff::queue_view(&state, &ledger, 0, 10).unwrap();
    assert_eq!(queue["total_authorized"], 0);
}

#[test]
fn thawed_live_execution_rejects_download_task_bound_to_existing_local_items() {
    let (state, ledger, command) = fixture("download", vec!["LOCAL_OLD".into()]);
    assert_eq!(
        local_execution_orchestrator::prepare_current(&state, &ledger, &command).unwrap_err(),
        "V1_ADD_ONLY_LIVE_EXECUTION_REQUIRES_NEW_WORK"
    );
    let queue = executor_handoff::queue_view(&state, &ledger, 0, 10).unwrap();
    assert_eq!(queue["total_authorized"], 0);
}

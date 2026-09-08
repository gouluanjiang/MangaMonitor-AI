use cloud_monitor::{
    assistant_task_gate::{target_hash, GateLedger, GateRecord},
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Decisions, Scan, State, Target, Task},
    source_bridge_request,
    source_preflight_authorization,
};
use serde_json::{json, Value};
use state_model::Version;
use std::collections::BTreeMap;

fn task() -> Task {
    Task {
        task_id: "TASK_A6_8".into(),
        work_id: "WORK_A6_8".into(),
        first_seen: "fixed".into(),
        task_revision: 3,
        target: Target {
            source_key: "jm:123456".into(),
            author: "Writer".into(),
            title: "A6.8".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: Vec::new(),
        binding_authority_hash: String::new(),
    }
}

fn state() -> State {
    let task = task();
    State {
        authors: json!({"authors":[]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(task.work_id.clone(), task)]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn approved(state: &State) -> GateLedger {
    let task = &state.pending["WORK_A6_8"];
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

fn command_for(task: &Task) -> ExecutorCommand {
    let target_hash = target_hash(task);
    let digest = hash(&(task.task_id.as_str(), task.task_revision, target_hash.as_str()));
    ExecutorCommand {
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
    }
}

fn setup() -> (
    State,
    GateLedger,
    ExecutorCommand,
    local_executor::LocalExecutionPlan,
    source_bridge_request::SourceBridgeRequest,
) {
    let state = state();
    let ledger = approved(&state);
    let command = command_for(&state.pending["WORK_A6_8"]);
    let plan = local_executor::plan(&command).unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    (state, ledger, command, plan, request)
}

#[test]
fn current_exact_approval_allows_metadata_preflight_only() {
    let (state, ledger, command, plan, request) = setup();
    let auth = source_preflight_authorization::authorize(
        &state, &ledger, &command, &plan, &request,
    )
    .unwrap();

    assert!(auth.current_generation_verified);
    assert!(auth.current_user_approval_verified);
    assert!(auth.source_metadata_read_authorized);
    assert!(!auth.reusable_permit);
    assert!(!auth.image_download_authorized);
    assert!(!auth.staging_write_authorized);
    assert!(!auth.inventory_mutation_authorized);
    assert!(!auth.task_completion_authorized);
    assert!(!auth.promotion_authorized);
    assert!(!auth.replacement_authorized);
    assert!(!auth.physical_delete_authorized);
    assert!(!auth.current_state_binding_hash.is_empty());
    assert!(!auth.gate_ledger_hash.is_empty());
}

#[test]
fn revoked_or_stale_approval_fails_closed() {
    let (state, mut ledger, command, plan, request) = setup();
    ledger.records[0].user_approved = false;
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
    );

    let (state, mut ledger, command, plan, request) = setup();
    ledger.records[0].task_revision = 2;
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
    );
}

#[test]
fn changed_revision_or_target_invalidates_old_command() {
    let (mut revised, ledger, command, plan, request) = setup();
    revised.pending.get_mut("WORK_A6_8").unwrap().task_revision = 4;
    assert_eq!(
        source_preflight_authorization::authorize(&revised, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT"
    );

    let (mut changed, ledger, command, plan, request) = setup();
    changed.pending.get_mut("WORK_A6_8").unwrap().target.title = "changed".into();
    assert_eq!(
        source_preflight_authorization::authorize(&changed, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT"
    );
}

#[test]
fn forged_plan_or_request_cannot_reuse_current_approval() {
    let (state, ledger, command, mut plan, request) = setup();
    plan.staging_subdir = "commands/forged".into();
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_PLAN_COMMAND_MISMATCH"
    );

    let (state, ledger, command, plan, mut request) = setup();
    request.source_work_id = "654321".into();
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_BRIDGE_REQUEST_BINDING_MISMATCH"
    );
}

#[test]
fn duplicate_current_task_id_fails_closed() {
    let (mut state, ledger, command, plan, request) = setup();
    let mut duplicate = state.pending["WORK_A6_8"].clone();
    duplicate.work_id = "WORK_A6_8_DUP".into();
    state.pending.insert(duplicate.work_id.clone(), duplicate);
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "AMBIGUOUS_SOURCE_PREFLIGHT_TASK_ID"
    );
}

#[test]
fn removed_or_nonpending_task_cannot_replay_old_artifacts() {
    let (mut removed, ledger, command, plan, request) = setup();
    removed.pending.clear();
    assert_eq!(
        source_preflight_authorization::authorize(&removed, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_TASK_NOT_FOUND"
    );

    let (mut state, ledger, command, plan, request) = setup();
    state.pending.get_mut("WORK_A6_8").unwrap().status = "completed".into();
    assert_eq!(
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
            .unwrap_err(),
        "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
    );
}

use cloud_monitor::{
    assistant_task_gate::{target_hash, GateLedger, GateRecord},
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Decisions, Scan, State, Target, Task},
    persistence,
    source_bridge_request,
};
use serde_json::{json, Value};
use state_model::Version;
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

fn unique(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mangamonitor-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn fixture() -> (
    State,
    GateLedger,
    ExecutorCommand,
    local_executor::LocalExecutionPlan,
    source_bridge_request::SourceBridgeRequest,
) {
    let task = Task {
        task_id: "TASK_A6_9_CLI".into(),
        work_id: "WORK_A6_9_CLI".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target: Target {
            source_key: "jm:123456".into(),
            author: "Writer".into(),
            title: "A6.9 CLI".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: Vec::new(),
        binding_authority_hash: String::new(),
    };
    let state = State {
        authors: json!({"authors":[]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(task.work_id.clone(), task.clone())]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    let ledger = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: target_hash(&task),
            assistant_recommended: true,
            user_approved: false,
        }],
    };
    let target_hash = target_hash(&task);
    let digest = hash(&(task.task_id.as_str(), task.task_revision, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target: task.target,
    };
    let plan = local_executor::plan(&command).unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    (state, ledger, command, plan, request)
}

#[test]
fn live_cli_rejects_revoked_approval_before_any_network_attempt() {
    let dir = unique("a6-9-live-preauth");
    let (state, ledger, command, plan, request) = fixture();
    persistence::save(&dir, &state).unwrap();

    let gate_path = dir.join("assistant-task-gates.json");
    let command_path = dir.join("command.json");
    let plan_path = dir.join("plan.json");
    let request_path = dir.join("request.json");
    fs::write(&gate_path, serde_json::to_vec_pretty(&ledger).unwrap()).unwrap();
    fs::write(&command_path, serde_json::to_vec_pretty(&command).unwrap()).unwrap();
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    fs::write(&request_path, serde_json::to_vec_pretty(&request).unwrap()).unwrap();

    let checkpoint_before = fs::read(dir.join("checkpoint.json")).unwrap();
    let gate_before = fs::read(&gate_path).unwrap();
    let command_before = fs::read(&command_path).unwrap();
    let plan_before = fs::read(&plan_path).unwrap();
    let request_before = fs::read(&request_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-live-source-preflight"))
        .arg("--state")
        .arg(&dir)
        .arg("--gates")
        .arg(&gate_path)
        .arg("--command")
        .arg(&command_path)
        .arg("--plan")
        .arg(&plan_path)
        .arg("--request")
        .arg(&request_path)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"));
    assert!(!stderr.contains("CONNECT_ERROR"));
    assert!(!stderr.contains("TIMEOUT"));

    assert_eq!(fs::read(dir.join("checkpoint.json")).unwrap(), checkpoint_before);
    assert_eq!(fs::read(&gate_path).unwrap(), gate_before);
    assert_eq!(fs::read(&command_path).unwrap(), command_before);
    assert_eq!(fs::read(&plan_path).unwrap(), plan_before);
    assert_eq!(fs::read(&request_path).unwrap(), request_before);

    let _ = fs::remove_dir_all(dir);
}

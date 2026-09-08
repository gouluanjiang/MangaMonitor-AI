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

fn state() -> State {
    let task = Task {
        task_id: "TASK_A6_8_CLI".into(),
        work_id: "WORK_A6_8_CLI".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target: Target {
            source_key: "pica:0123456789abcdef01234567".into(),
            author: "Writer".into(),
            title: "A6.8 CLI".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: vec![],
        binding_authority_hash: String::new(),
    };
    State {
        authors: json!({"schema_version":3,"authors":[]}),
        inventory: json!({"schema_version":3,"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(task.work_id.clone(), task)]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn command(task: &Task) -> ExecutorCommand {
    let target_hash = target_hash(task);
    let digest = hash(&(task.task_id.as_str(), task.task_revision, target_hash.as_str()));
    ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash,
        source: "pica".into(),
        source_work_id: "0123456789abcdef01234567".into(),
        action: task.action.clone(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target: task.target.clone(),
    }
}

fn run_authorize(
    dir: &std::path::Path,
    gate_path: &std::path::Path,
    command_path: &std::path::Path,
    plan_path: &std::path::Path,
    request_path: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_assistant-source-preflight-authorize"))
        .arg("--state")
        .arg(dir)
        .arg("--gates")
        .arg(gate_path)
        .arg("--command")
        .arg(command_path)
        .arg("--plan")
        .arg(plan_path)
        .arg("--request")
        .arg(request_path)
        .output()
        .unwrap()
}

#[test]
fn authorization_cli_rechecks_atomic_checkpoint_without_mutation() {
    let dir = unique("a6-8-auth-cli");
    let state = state();
    persistence::save(&dir, &state).unwrap();
    let task = &state.pending["WORK_A6_8_CLI"];
    let gates = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: target_hash(task),
            assistant_recommended: true,
            user_approved: true,
        }],
    };
    let command = command(task);
    let plan = local_executor::plan(&command).unwrap();
    let request = source_bridge_request::build(&plan).unwrap();

    let checkpoint_path = dir.join("checkpoint.json");
    let gate_path = dir.join("assistant-task-gates.json");
    let command_path = dir.join("command.json");
    let plan_path = dir.join("plan.json");
    let request_path = dir.join("request.json");
    fs::write(&gate_path, serde_json::to_vec_pretty(&gates).unwrap()).unwrap();
    fs::write(&command_path, serde_json::to_vec_pretty(&command).unwrap()).unwrap();
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    fs::write(&request_path, serde_json::to_vec_pretty(&request).unwrap()).unwrap();

    let checkpoint_bytes = fs::read(&checkpoint_path).unwrap();
    let before_state: BTreeMap<_, _> = persistence::FILES
        .iter()
        .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
        .collect();
    let gate_bytes = fs::read(&gate_path).unwrap();
    let command_bytes = fs::read(&command_path).unwrap();
    let plan_bytes = fs::read(&plan_path).unwrap();
    let request_bytes = fs::read(&request_path).unwrap();

    let output = run_authorize(&dir, &gate_path, &command_path, &plan_path, &request_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["source_metadata_read_authorized"], true);
    assert_eq!(value["reusable_permit"], false);
    assert_eq!(value["image_download_authorized"], false);
    assert_eq!(value["staging_write_authorized"], false);
    assert_eq!(value["physical_delete_authorized"], false);

    assert_eq!(fs::read(&checkpoint_path).unwrap(), checkpoint_bytes);
    for (name, bytes) in before_state {
        assert_eq!(fs::read(dir.join(name)).unwrap(), bytes);
    }
    assert_eq!(fs::read(&gate_path).unwrap(), gate_bytes);
    assert_eq!(fs::read(&command_path).unwrap(), command_bytes);
    assert_eq!(fs::read(&plan_path).unwrap(), plan_bytes);
    assert_eq!(fs::read(&request_path).unwrap(), request_bytes);

    let mut revoked = gates.clone();
    revoked.records[0].user_approved = false;
    fs::write(&gate_path, serde_json::to_vec_pretty(&revoked).unwrap()).unwrap();
    let output = run_authorize(&dir, &gate_path, &command_path, &plan_path, &request_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"));

    // The security-sensitive CLI never falls back to the eight export files.
    // Without the authoritative checkpoint it fails closed even if those
    // human-facing exports are all still present and internally readable.
    fs::write(&gate_path, serde_json::to_vec_pretty(&gates).unwrap()).unwrap();
    fs::remove_file(&checkpoint_path).unwrap();
    let output = run_authorize(&dir, &gate_path, &command_path, &plan_path, &request_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("READ_checkpoint.json"));

    let _ = fs::remove_dir_all(dir);
}

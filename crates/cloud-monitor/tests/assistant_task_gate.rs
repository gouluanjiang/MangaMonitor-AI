use cloud_monitor::{assistant_task_gate::*, monitor::*, persistence};
use serde_json::{json, Value};
use state_model::Version;
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, process::Command};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> PathBuf {
    root().join("fixtures/matcher-m3/phase3b-old-state")
}

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

fn task(status: &str, action: &str) -> Task {
    Task {
        task_id: "TASK_1".into(),
        work_id: "WORK_1".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target: Target {
            source_key: "jm:123".into(),
            author: "Writer".into(),
            title: "作品标题".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: action.into(),
        status: status.into(),
        old_local_item_ids: Vec::new(),
        binding_authority_hash: String::new(),
    }
}

fn state_with(task: Task) -> State {
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

#[test]
fn recommendation_and_approval_are_independent_and_exact() {
    let state = state_with(task("pending", "download"));
    let hash = target_hash(&state.pending["WORK_1"]);
    let empty = GateLedger::default();

    let (recommended, audit, preview) =
        plan(&state, &empty, "recommend", "TASK_1", 1, &hash).unwrap();
    assert_eq!(audit["outcome"], "UPDATED");
    assert_eq!(preview["assistant_recommended"], true);
    assert_eq!(preview["user_approved"], false);
    assert_eq!(preview["download_authorized"], false);

    let (approved, _, preview) =
        plan(&state, &recommended, "approve", "TASK_1", 1, &hash).unwrap();
    assert_eq!(preview["assistant_recommended"], true);
    assert_eq!(preview["user_approved"], true);
    assert_eq!(preview["download_authorized"], true);

    let (direct, _, preview) =
        plan(&state, &empty, "approve", "TASK_1", 1, &hash).unwrap();
    assert_eq!(preview["assistant_recommended"], false);
    assert_eq!(preview["user_approved"], true);
    assert_eq!(preview["download_authorized"], true);
    assert_eq!(approved.records.len(), 1);
    assert_eq!(direct.records.len(), 1);
}

#[test]
fn stale_binding_never_authorizes_a_revised_task() {
    let original = state_with(task("pending", "download"));
    let old_hash = target_hash(&original.pending["WORK_1"]);
    let ledger = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: "TASK_1".into(),
            task_revision: 1,
            target_hash: old_hash.clone(),
            assistant_recommended: true,
            user_approved: true,
        }],
    };

    let mut revised = original.clone();
    let revised_task = revised.pending.get_mut("WORK_1").unwrap();
    revised_task.task_revision = 2;
    revised_task.target.title = "更好的候选".into();
    let new_hash = target_hash(revised_task);
    assert_ne!(old_hash, new_hash);
    let view = task_view(&revised, &ledger, "TASK_1").unwrap();
    assert_eq!(view["assistant_recommended"], false);
    assert_eq!(view["user_approved"], false);
    assert_eq!(view["download_authorized"], false);
    assert_eq!(view["stale_binding_count"], 1);
    assert_eq!(
        plan(&revised, &ledger, "approve", "TASK_1", 1, &old_hash).unwrap_err(),
        "ASSISTANT_TASK_REVISION_STALE"
    );
    assert_eq!(
        plan(&revised, &ledger, "approve", "TASK_1", 2, &old_hash).unwrap_err(),
        "ASSISTANT_TASK_TARGET_HASH_STALE"
    );
}

#[test]
fn positive_gates_require_current_executable_pending_task() {
    for status in [
        "inactive",
        "ignored",
        "done",
        "superseded_by_decision",
        "superseded_by_identity_reanalysis",
    ] {
        let state = state_with(task(status, "download"));
        let hash = target_hash(&state.pending["WORK_1"]);
        assert_eq!(
            plan(&state, &GateLedger::default(), "approve", "TASK_1", 1, &hash)
                .unwrap_err(),
            "ASSISTANT_TASK_NOT_PENDING"
        );
    }
    let state = state_with(task("pending", "inspect"));
    let hash = target_hash(&state.pending["WORK_1"]);
    assert_eq!(
        plan(&state, &GateLedger::default(), "recommend", "TASK_1", 1, &hash)
            .unwrap_err(),
        "ASSISTANT_TASK_ACTION_NOT_EXECUTABLE"
    );
    assert_eq!(
        plan(&state, &GateLedger::default(), "approve", "MISSING", 1, &hash).unwrap_err(),
        "ASSISTANT_TASK_NOT_FOUND"
    );
}

#[test]
fn gate_operations_are_idempotent_and_clear_empty_current_record() {
    let state = state_with(task("pending", "upgrade"));
    let hash = target_hash(&state.pending["WORK_1"]);
    let empty = GateLedger::default();
    let (recommended, _, _) =
        plan(&state, &empty, "recommend", "TASK_1", 1, &hash).unwrap();
    let (same, audit, _) =
        plan(&state, &recommended, "recommend", "TASK_1", 1, &hash).unwrap();
    assert_eq!(recommended, same);
    assert_eq!(audit["outcome"], "NOOP_ALREADY_IN_STATE");

    let (approved, _, _) = plan(&state, &same, "approve", "TASK_1", 1, &hash).unwrap();
    let (still_approved, audit, _) =
        plan(&state, &approved, "approve", "TASK_1", 1, &hash).unwrap();
    assert_eq!(approved, still_approved);
    assert_eq!(audit["outcome"], "NOOP_ALREADY_IN_STATE");

    let (not_recommended, _, _) = plan(
        &state,
        &still_approved,
        "clear-recommendation",
        "TASK_1",
        1,
        &hash,
    )
    .unwrap();
    assert!(!not_recommended.records[0].assistant_recommended);
    assert!(not_recommended.records[0].user_approved);
    let (cleared, _, preview) =
        plan(&state, &not_recommended, "revoke", "TASK_1", 1, &hash).unwrap();
    assert!(cleared.records.is_empty());
    assert_eq!(preview["download_authorized"], false);
    let (same_empty, audit, _) =
        plan(&state, &cleared, "revoke", "TASK_1", 1, &hash).unwrap();
    assert_eq!(same_empty, cleared);
    assert_eq!(audit["outcome"], "NOOP_ALREADY_IN_STATE");
}

#[test]
fn malformed_duplicate_ledger_fails_closed_and_historical_bindings_survive() {
    let state = state_with(task("pending", "download"));
    let hash = target_hash(&state.pending["WORK_1"]);
    let record = GateRecord {
        task_id: "TASK_1".into(),
        task_revision: 1,
        target_hash: hash.clone(),
        assistant_recommended: false,
        user_approved: true,
    };
    let duplicate = GateLedger {
        schema_version: 1,
        records: vec![record.clone(), record],
    };
    assert_eq!(
        validate_ledger(&duplicate).unwrap_err(),
        "DUPLICATE_ASSISTANT_TASK_GATE_BINDING"
    );
    let bad = GateLedger {
        schema_version: 2,
        records: Vec::new(),
    };
    assert_eq!(
        validate_ledger(&bad).unwrap_err(),
        "INVALID_ASSISTANT_TASK_GATE_SCHEMA_VERSION"
    );

    let ledger = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: "TASK_1".into(),
            task_revision: 2,
            target_hash: "historical".into(),
            assistant_recommended: true,
            user_approved: false,
        }],
    };
    let (updated, _, _) =
        plan(&state, &ledger, "approve", "TASK_1", 1, &hash).unwrap();
    assert_eq!(updated.records.len(), 2);
    assert!(updated.records.iter().any(|gate| gate.task_revision == 2));
    assert!(updated
        .records
        .iter()
        .any(|gate| gate.task_revision == 1 && gate.user_approved));
}

#[test]
fn bounded_gate_view_is_deterministic() {
    let mut state = state_with(task("pending", "download"));
    for index in 2..=4 {
        let mut task = task("pending", "download");
        task.task_id = format!("TASK_{index}");
        task.work_id = format!("WORK_{index}");
        state.pending.insert(task.work_id.clone(), task);
    }
    let view = batch_view(&state, &GateLedger::default(), 1, 2).unwrap();
    assert_eq!(view["total"], 4);
    assert_eq!(view["returned"], 2);
    assert_eq!(view["next_offset"], 3);
    assert_eq!(view["items"][0]["task_id"], "TASK_2");
    assert_eq!(view["items"][1]["task_id"], "TASK_3");
    assert_eq!(
        batch_view(&state, &GateLedger::default(), 0, 101).unwrap_err(),
        "INVALID_ASSISTANT_TASK_GATE_VIEW_LIMIT"
    );
}

fn copy_state(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for name in persistence::FILES {
        fs::copy(source.join(name), destination.join(name)).unwrap();
    }
}

fn state_bytes(dir: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    persistence::FILES
        .iter()
        .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
        .collect()
}

#[test]
fn gate_cli_is_offline_staged_and_never_mutates_inputs() {
    let state_dir = unique("a5-state");
    copy_state(&fixture(), &state_dir);
    let pending = json!({
        "schema_version": 3,
        "tasks": [{
            "task_id":"TASK_A5",
            "work_id":"WORK_A5",
            "first_seen":"fixed",
            "task_revision":7,
            "target":{
                "source_key":"jm:123",
                "author":"Writer",
                "title":"A5 fixture task",
                "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
                "coverage":null
            },
            "action":"download",
            "status":"pending",
            "old_local_item_ids":[]
        }]
    });
    fs::write(
        state_dir.join("pending.json"),
        serde_json::to_vec_pretty(&pending).unwrap(),
    )
    .unwrap();
    let loaded = persistence::load(&state_dir).unwrap();
    let hash = target_hash(&loaded.pending["WORK_A5"]);
    let before = state_bytes(&state_dir);

    let gate_input = unique("a5-gates.json");
    let gate_bytes = serde_json::to_vec_pretty(&GateLedger::default()).unwrap();
    fs::write(&gate_input, &gate_bytes).unwrap();
    let output = unique("a5-output");
    let run = Command::new(env!("CARGO_BIN_EXE_assistant-task-gate-edit"))
        .current_dir(root())
        .args(["--state"])
        .arg(&state_dir)
        .args(["--gates"])
        .arg(&gate_input)
        .args(["--output"])
        .arg(&output)
        .args([
            "--operation",
            "approve",
            "--task-id",
            "TASK_A5",
            "--task-revision",
            "7",
            "--target-hash",
            &hash,
        ])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let preview: Value = serde_json::from_slice(
        &fs::read(output.join("executor-preview.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(preview["download_authorized"], true);
    assert_eq!(before, state_bytes(&state_dir));
    assert_eq!(gate_bytes, fs::read(&gate_input).unwrap());

    let view = Command::new(env!("CARGO_BIN_EXE_assistant-task-gate-view"))
        .current_dir(root())
        .args(["--state"])
        .arg(&state_dir)
        .args(["--gates"])
        .arg(output.join("assistant-task-gates.json"))
        .args(["--task-id", "TASK_A5"])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(view.status.success(), "{}", String::from_utf8_lossy(&view.stderr));
    let view: Value = serde_json::from_slice(&view.stdout).unwrap();
    assert_eq!(view["user_approved"], true);
    assert_eq!(view["download_authorized"], true);

    let rerun = Command::new(env!("CARGO_BIN_EXE_assistant-task-gate-edit"))
        .current_dir(root())
        .args(["--state"])
        .arg(&state_dir)
        .args(["--gates"])
        .arg(&gate_input)
        .args(["--output"])
        .arg(&output)
        .args([
            "--operation",
            "approve",
            "--task-id",
            "TASK_A5",
            "--task-revision",
            "7",
            "--target-hash",
            &hash,
        ])
        .output()
        .unwrap();
    assert!(!rerun.status.success());
    assert!(String::from_utf8_lossy(&rerun.stderr).contains("ASSISTANT_TASK_GATE_OUTPUT_EXISTS"));
    assert_eq!(before, state_bytes(&state_dir));
    assert_eq!(gate_bytes, fs::read(&gate_input).unwrap());

    fs::remove_dir_all(state_dir).unwrap();
    fs::remove_dir_all(output).unwrap();
    fs::remove_file(gate_input).unwrap();
}

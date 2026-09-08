use cloud_monitor::{
    assistant_task_gate::{target_hash, GateLedger, GateRecord},
    executor_handoff::{queue_view, receipt_view, CompletionEvidence, ExecutorReceipt},
    monitor::*,
    persistence,
};
use serde_json::{json, Value};
use state_model::Version;
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, process::Command};

fn task() -> Task {
    Task {
        task_id: "TASK_1".into(),
        work_id: "WORK_1".into(),
        first_seen: "fixed".into(),
        task_revision: 3,
        target: Target {
            source_key: "jm:123".into(),
            author: "Writer".into(),
            title: "作品标题".into(),
            version: Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: Vec::new(),
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
    let task = &state.pending["WORK_1"];
    GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: target_hash(task),
            assistant_recommended: false,
            user_approved: true,
        }],
    }
}

fn command_binding(state: &State, ledger: &GateLedger) -> (String, String) {
    let queue = queue_view(state, ledger, 0, 50).unwrap();
    (
        queue["commands"][0]["command_id"].as_str().unwrap().to_owned(),
        queue["commands"][0]["target_hash"].as_str().unwrap().to_owned(),
    )
}

fn success_receipt(state: &State, ledger: &GateLedger) -> ExecutorReceipt {
    let task = &state.pending["WORK_1"];
    let (command_id, target_hash) = command_binding(state, ledger);
    ExecutorReceipt {
        schema_version: 1,
        command_id,
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash,
        outcome: "SUCCEEDED".into(),
        completed_at: "fixed".into(),
        completion_evidence: Some(CompletionEvidence {
            downloader_reported_full_completion: true,
            artifact_manifest_hash: "manifest-hash".into(),
            file_count: 10,
        }),
        error_code: None,
    }
}

#[test]
fn queue_requires_exact_user_approval_and_is_staging_only() {
    let state = state();
    let empty = queue_view(&state, &GateLedger::default(), 0, 50).unwrap();
    assert_eq!(empty["total_authorized"], 0);

    let ledger = approved(&state);
    let queue = queue_view(&state, &ledger, 0, 50).unwrap();
    assert_eq!(queue["total_authorized"], 1);
    assert_eq!(queue["commands"][0]["task_id"], "TASK_1");
    assert_eq!(queue["commands"][0]["task_revision"], 3);
    assert_eq!(queue["commands"][0]["source"], "jm");
    assert_eq!(queue["commands"][0]["source_work_id"], "123");
    assert_eq!(queue["commands"][0]["intent"], "DOWNLOAD_TO_STAGING_ONLY");
    assert_eq!(queue["physical_delete_authorized"], false);

    let mut stale = ledger.clone();
    stale.records[0].task_revision = 2;
    let queue = queue_view(&state, &stale, 0, 50).unwrap();
    assert_eq!(queue["total_authorized"], 0);
}

#[test]
fn queue_is_bounded_and_source_keys_fail_closed() {
    let base = state();
    assert_eq!(
        queue_view(&base, &approved(&base), 0, 201).unwrap_err(),
        "INVALID_EXECUTOR_QUEUE_LIMIT"
    );

    let mut bad = state();
    bad.pending.get_mut("WORK_1").unwrap().target.source_key = "other:123".into();
    let ledger = approved(&bad);
    assert_eq!(
        queue_view(&bad, &ledger, 0, 50).unwrap_err(),
        "INVALID_EXECUTOR_SOURCE_KEY"
    );
}

#[test]
fn successful_receipt_only_reaches_inventory_verification_gate() {
    let state = state();
    let ledger = approved(&state);
    let receipt = success_receipt(&state, &ledger);
    let view = receipt_view(&state, &ledger, &receipt).unwrap();
    assert_eq!(view["binding_current"], true);
    assert_eq!(view["approval_current"], true);
    assert_eq!(view["ready_for_inventory_verification"], true);
    assert_eq!(view["task_completion_authorized"], false);
    assert_eq!(view["replacement_authorized"], false);
    assert_eq!(view["physical_delete_authorized"], false);
}

#[test]
fn stale_or_failed_receipts_never_advance() {
    let state = state();
    let ledger = approved(&state);
    let mut receipt = success_receipt(&state, &ledger);

    let mut revised = state.clone();
    revised.pending.get_mut("WORK_1").unwrap().task_revision = 4;
    let view = receipt_view(&revised, &ledger, &receipt).unwrap();
    assert_eq!(view["binding_current"], false);
    assert_eq!(view["ready_for_inventory_verification"], false);

    receipt.outcome = "FAILED".into();
    receipt.completion_evidence = None;
    let view = receipt_view(&state, &ledger, &receipt).unwrap();
    assert_eq!(view["ready_for_inventory_verification"], false);
}

#[test]
fn success_receipt_requires_full_completion_evidence_and_valid_command_binding() {
    let state = state();
    let ledger = approved(&state);
    let mut receipt = success_receipt(&state, &ledger);
    receipt.completion_evidence = None;
    assert_eq!(
        receipt_view(&state, &ledger, &receipt).unwrap_err(),
        "EXECUTOR_SUCCESS_MISSING_COMPLETION_EVIDENCE"
    );

    let mut receipt = success_receipt(&state, &ledger);
    receipt.completion_evidence.as_mut().unwrap().downloader_reported_full_completion = false;
    assert_eq!(
        receipt_view(&state, &ledger, &receipt).unwrap_err(),
        "EXECUTOR_SUCCESS_INCOMPLETE_COMPLETION_EVIDENCE"
    );

    let mut receipt = success_receipt(&state, &ledger);
    receipt.command_id = "EXEC_forged".into();
    assert_eq!(
        receipt_view(&state, &ledger, &receipt).unwrap_err(),
        "INVALID_EXECUTOR_RECEIPT_BINDING"
    );
}

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
fn executor_queue_cli_is_offline_and_does_not_mutate_monitor_state() {
    let state_dir = unique("a6-state");
    copy_state(&fixture(), &state_dir);
    let pending = json!({
        "schema_version": 3,
        "tasks": [{
            "task_id":"TASK_A6",
            "work_id":"WORK_A6",
            "first_seen":"fixed",
            "task_revision":9,
            "target":{
                "source_key":"pica:abcdef",
                "author":"Writer",
                "title":"A6 fixture task",
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
    let task = &loaded.pending["WORK_A6"];
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
    let gate_path = unique("a6-gates.json");
    fs::write(&gate_path, serde_json::to_vec_pretty(&gates).unwrap()).unwrap();
    let before = state_bytes(&state_dir);

    let run = Command::new(env!("CARGO_BIN_EXE_assistant-executor-queue"))
        .current_dir(root())
        .args(["--state"])
        .arg(&state_dir)
        .args(["--gates"])
        .arg(&gate_path)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let queue: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(queue["commands"][0]["source"], "pica");
    assert_eq!(queue["commands"][0]["source_work_id"], "abcdef");
    assert_eq!(before, state_bytes(&state_dir));

    fs::remove_dir_all(state_dir).unwrap();
    fs::remove_file(gate_path).unwrap();
}

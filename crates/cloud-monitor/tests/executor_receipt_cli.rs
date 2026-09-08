use cloud_monitor::{
    assistant_task_gate::{target_hash, GateLedger, GateRecord},
    executor_handoff::{CompletionEvidence, ExecutorReceipt},
    persistence,
};
use serde_json::{json, Value};
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
fn receipt_cli_validates_current_success_offline_without_state_mutation() {
    let state_dir = unique("a6-receipt-state");
    copy_state(&fixture(), &state_dir);
    let pending = json!({
        "schema_version": 3,
        "tasks": [{
            "task_id":"TASK_A6_RECEIPT",
            "work_id":"WORK_A6_RECEIPT",
            "first_seen":"fixed",
            "task_revision":4,
            "target":{
                "source_key":"jm:654321",
                "author":"Writer",
                "title":"A6 receipt fixture task",
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
    let state = persistence::load(&state_dir).unwrap();
    let task = &state.pending["WORK_A6_RECEIPT"];
    let hash = target_hash(task);
    let gates = GateLedger {
        schema_version: 1,
        records: vec![GateRecord {
            task_id: task.task_id.clone(),
            task_revision: task.task_revision,
            target_hash: hash.clone(),
            assistant_recommended: true,
            user_approved: true,
        }],
    };
    let gate_path = unique("a6-receipt-gates.json");
    fs::write(&gate_path, serde_json::to_vec_pretty(&gates).unwrap()).unwrap();

    let queue = Command::new(env!("CARGO_BIN_EXE_assistant-executor-queue"))
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
    assert!(queue.status.success(), "{}", String::from_utf8_lossy(&queue.stderr));
    let queue: Value = serde_json::from_slice(&queue.stdout).unwrap();
    let command_id = queue["commands"][0]["command_id"].as_str().unwrap().to_owned();

    let receipt = ExecutorReceipt {
        schema_version: 1,
        command_id,
        task_id: task.task_id.clone(),
        work_id: task.work_id.clone(),
        task_revision: task.task_revision,
        target_hash: hash,
        outcome: "SUCCEEDED".into(),
        completed_at: "fixed".into(),
        completion_evidence: Some(CompletionEvidence {
            downloader_reported_full_completion: true,
            artifact_manifest_hash: "manifest-hash".into(),
            file_count: 12,
        }),
        error_code: None,
    };
    let receipt_path = unique("a6-receipt.json");
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    let before = state_bytes(&state_dir);

    let check = Command::new(env!("CARGO_BIN_EXE_assistant-executor-receipt-check"))
        .current_dir(root())
        .args(["--state"])
        .arg(&state_dir)
        .args(["--gates"])
        .arg(&gate_path)
        .args(["--receipt"])
        .arg(&receipt_path)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let view: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(view["ready_for_inventory_verification"], true);
    assert_eq!(view["task_completion_authorized"], false);
    assert_eq!(view["replacement_authorized"], false);
    assert_eq!(view["physical_delete_authorized"], false);
    assert_eq!(before, state_bytes(&state_dir));

    fs::remove_dir_all(state_dir).unwrap();
    fs::remove_file(gate_path).unwrap();
    fs::remove_file(receipt_path).unwrap();
}

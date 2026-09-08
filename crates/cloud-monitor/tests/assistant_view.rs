use cloud_monitor::{assistant, persistence};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, process::Command};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> std::path::PathBuf {
    root().join("fixtures/matcher-m3/phase3b-old-state")
}

fn bytes(dir: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    persistence::FILES
        .iter()
        .map(|name| (*name, std::fs::read(dir.join(name)).unwrap()))
        .collect()
}

#[test]
fn assistant_views_are_bounded_deterministic_and_read_only() {
    let input = fixture();
    let before = bytes(&input);
    let state = persistence::load(&input).unwrap();

    let scan_a = assistant::scan_summary(&state);
    let scan_b = assistant::scan_summary(&state);
    assert_eq!(scan_a, scan_b);

    let summary = assistant::review_backlog_summary(&state);
    let total = summary["total"].as_u64().unwrap() as usize;
    assert!(total > 0);

    let batch = assistant::review_batch(&state, 0, 7).unwrap();
    assert_eq!(batch["returned"], 7);
    assert_eq!(batch["limit"], 7);
    assert_eq!(batch["total"].as_u64().unwrap() as usize, total);
    for item in batch["items"].as_array().unwrap() {
        let candidate_ids = item["candidate_work_ids"].as_array().unwrap();
        let candidate_works = item["candidate_works"].as_array().unwrap();
        assert!(candidate_works.len() <= candidate_ids.len());
        for work in candidate_works {
            let id = work["work_id"].as_str().unwrap();
            assert!(candidate_ids.iter().any(|candidate| candidate == id));
        }
    }

    let pending = assistant::pending_task_summary(&state);
    assert!(pending["tasks"].is_array());
    let collection = assistant::collection_summary(&state).unwrap();
    assert!(collection["total_work_ids"].as_u64().unwrap() > 0);

    assert_eq!(before, bytes(&input));
}

#[test]
fn assistant_review_batch_rejects_unbounded_limits() {
    let state = persistence::load(&fixture()).unwrap();
    assert_eq!(
        assistant::review_batch(&state, 0, 0).unwrap_err(),
        "INVALID_ASSISTANT_REVIEW_BATCH_LIMIT"
    );
    assert_eq!(
        assistant::review_batch(&state, 0, assistant::MAX_REVIEW_BATCH + 1).unwrap_err(),
        "INVALID_ASSISTANT_REVIEW_BATCH_LIMIT"
    );
}

#[test]
fn assistant_cli_reads_state_without_modifying_it() {
    let input = fixture();
    let before = bytes(&input);
    let output = Command::new(env!("CARGO_BIN_EXE_assistant-view"))
        .current_dir(root())
        .args(["--state"])
        .arg(&input)
        .args(["--view", "review-batch", "--limit", "3"])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["view"], "review_batch");
    assert_eq!(parsed["returned"], 3);
    assert_eq!(before, bytes(&input));
}

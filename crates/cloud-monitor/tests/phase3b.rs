use cloud_monitor::{monitor::*, persistence::*};
use serde_json::{json, Value};
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, path::Path, process::Command};

fn setup(root: &Path, count: usize) -> Vec<String> {
    let names: Vec<_> = (0..count).map(|index| format!("Writer{index}")).collect();
    let state = State {
        authors: json!({"authors":names.iter().map(|name|json!({"name":name,"enabled":true})).collect::<Vec<_>>()}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    save(&root.join("seed"), &state).unwrap();
    write_json(&root.join("authors.json"), &names).unwrap();
    names
}

fn observation(name: &str, source: &str) -> Value {
    let id = if source == "jm" {
        format!("{}1", &hash(&name)[..6])
    } else {
        hash(&(name, source))[..24].to_owned()
    };
    let record = Record::new(
        source,
        id,
        vec![name.into()],
        format!("Long title for {name}"),
        json!({}),
    );
    let page = SearchPage {
        page: 1,
        reported_total: Some(1),
        reported_pages: Some(1),
        reported_limit: Some(20),
        response_fields: vec![],
        record_fields: vec![],
        records: vec![record.clone()],
        redirect_to_detail: false,
    };
    json!({"source":source,"author":name,"page":page,"details":{key(&record):record},"error":null})
}

fn invoke(
    root: &Path,
    input: &str,
    output: &str,
    tape: &str,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_phase3b"))
        .args(["--state"])
        .arg(root.join(input))
        .args(["--output"])
        .arg(root.join(output))
        .args(["--authors"])
        .arg(root.join("authors.json"))
        .args(["--replay"])
        .arg(root.join(tape))
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn phase3b_soft_batch_monthly_and_commit_manifest_are_deterministic() {
    let root = std::env::temp_dir().join(format!("manga-phase3b-batch-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root, 5);
    let observations: Vec<_> = names[2..4]
        .iter()
        .flat_map(|name| [observation(name, "jm"), observation(name, "pica")])
        .collect();
    write_json(
        &root.join("tape.json"),
        &json!({"observations":observations}),
    )
    .unwrap();
    let output = invoke(
        &root,
        "seed",
        "result",
        "tape.json",
        &[
            "--mode",
            "monthly",
            "--batch-size",
            "2",
            "--batch-index",
            "1",
            "--expected-base-sha",
            "abc123",
            "--actual-base-sha",
            "abc123",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.join("result/scan-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["selected_authors"], json!(["Writer2", "Writer3"]));
    assert_eq!(report["requested_mode"], "monthly");
    assert_eq!(report["effective_requested_mode"], "incremental");
    assert_eq!(report["batch_count"], 3);
    assert_eq!(report["complete"], true);
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("result/state-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["base_commit"], "abc123");
    assert_eq!(manifest["complete"], true);
}

#[test]
fn phase3b_complete_scan_replay_is_idempotent() {
    let root = std::env::temp_dir().join(format!("manga-phase3b-idempotent-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root, 2);
    let observations: Vec<_> = names
        .iter()
        .flat_map(|name| [observation(name, "jm"), observation(name, "pica")])
        .collect();
    write_json(
        &root.join("complete.json"),
        &json!({"observations": observations}),
    )
    .unwrap();

    let first = invoke(
        &root,
        "seed",
        "first",
        "complete.json",
        &["--mode", "full", "--batch-size", "2"],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(load_checkpoint(&root.join("first")).unwrap().scan.complete);

    let replay = invoke(
        &root,
        "first",
        "second",
        "complete.json",
        &[
            "--mode",
            "full",
            "--batch-size",
            "2",
            "--assert-idempotent",
        ],
    );
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.join("second/scan-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["business_state_unchanged"], true);
    assert_eq!(report["new_events"], 0);
    assert_eq!(report["reanalyzed"], 0);
}

#[test]
fn phase3b_base_commit_mismatch_fails_before_state_load_or_requests() {
    let root = std::env::temp_dir().join(format!("manga-phase3b-base-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    setup(&root, 5);
    write_json(&root.join("tape.json"), &json!({"observations":[]})).unwrap();
    let output = invoke(
        &root,
        "seed",
        "result",
        "tape.json",
        &[
            "--mode",
            "monthly",
            "--expected-base-sha",
            "old",
            "--actual-base-sha",
            "new",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("BASE_COMMIT_MISMATCH"));
    assert!(!root.join("result/checkpoint.json").exists());
}

#[test]
fn phase3b_partial_source_failure_checkpoint_resumes_without_unavailable_inference() {
    let root = std::env::temp_dir().join(format!("manga-phase3b-recovery-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root, 2);
    let first = json!({"observations":[
        {"source":"jm","author":names[0],"page":null,"details":{},"error":"HTTP_503"},
        observation(&names[0], "pica"),
        observation(&names[1], "jm"),
        observation(&names[1], "pica")
    ]});
    write_json(&root.join("failure.json"), &first).unwrap();
    let failed = invoke(
        &root,
        "seed",
        "result",
        "failure.json",
        &["--mode", "full", "--batch-size", "2"],
    );
    assert!(failed.status.success());
    let partial = load_checkpoint(&root.join("result")).unwrap();
    assert!(!partial.scan.complete);
    assert_eq!(partial.scan.progress["jm|Writer0"].boundary, "SOURCE_ERROR");
    assert!(partial
        .catalog
        .values()
        .all(|entry| entry.active && entry.unavailable_streak == 0));

    let ordinary_replay = invoke(
        &root,
        "result",
        "replay",
        "failure.json",
        &[
            "--mode",
            "full",
            "--batch-size",
            "2",
            "--assert-idempotent",
        ],
    );
    assert!(!ordinary_replay.status.success());
    assert!(String::from_utf8_lossy(&ordinary_replay.stderr)
        .contains("INCOMPLETE_SCAN_REQUIRES_RESUME"));
    assert!(!root.join("replay/checkpoint.json").exists());
    let still_partial = load_checkpoint(&root.join("result")).unwrap();
    assert!(!still_partial.scan.complete);
    assert_eq!(still_partial.scan.progress["jm|Writer0"].boundary, "SOURCE_ERROR");

    write_json(
        &root.join("recovery.json"),
        &json!({"observations":[observation(&names[0], "jm")]}),
    )
    .unwrap();
    let recovered = invoke(
        &root,
        "seed",
        "result",
        "recovery.json",
        &["--mode", "full", "--batch-size", "2", "--resume"],
    );
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let state = load_checkpoint(&root.join("result")).unwrap();
    assert!(state.scan.complete);
    assert_eq!(state.scan.progress["jm|Writer0"].boundary, "COMPLETE");
    assert!(state
        .catalog
        .values()
        .all(|entry| entry.active && entry.unavailable_streak == 0));
}

#[test]
fn phase3b_soft_batches_keep_one_latest_event_window_for_the_cycle() {
    let root = std::env::temp_dir().join(format!("manga-phase3b-latest-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root, 4);
    for batch in 0..2 {
        let observations: Vec<_> = names[batch * 2..batch * 2 + 2]
            .iter()
            .flat_map(|name| [observation(name, "jm"), observation(name, "pica")])
            .collect();
        write_json(
            &root.join(format!("tape{batch}.json")),
            &json!({"observations":observations}),
        )
        .unwrap();
    }
    let first = invoke(
        &root,
        "seed",
        "first",
        "tape0.json",
        &[
            "--mode",
            "monthly",
            "--batch-size",
            "2",
            "--batch-index",
            "0",
        ],
    );
    assert!(first.status.success());
    let first_events = load_checkpoint(&root.join("first"))
        .unwrap()
        .scan
        .events
        .len();
    assert!(first_events > 0);
    let second = invoke(
        &root,
        "first",
        "second",
        "tape1.json",
        &[
            "--mode",
            "monthly",
            "--batch-size",
            "2",
            "--batch-index",
            "1",
            "--continue-cycle",
        ],
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let final_state = load_checkpoint(&root.join("second")).unwrap();
    assert!(final_state.scan.events.len() > first_events);
    let latest: Value =
        serde_json::from_slice(&std::fs::read(root.join("second/latest.json")).unwrap()).unwrap();
    assert_eq!(
        latest["events"].as_array().unwrap().len(),
        final_state.scan.events.len()
    );
}

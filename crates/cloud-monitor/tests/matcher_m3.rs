use cloud_monitor::{monitor::*, persistence::*};
use serde_json::{json, Value};
use state_model::Record;
use std::{collections::BTreeMap, path::Path, process::Command};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn invoke(input: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_phase3b"))
        .current_dir(root())
        .args(["--state"])
        .arg(input)
        .args(["--output"])
        .arg(output)
        .args([
            "--authors",
            "fixtures/phase3a-authors.json",
            "--mode",
            "full",
            "--replay",
            "fixtures/matcher-m2/phase3b-observations.json",
            "--repair-overlay",
            "fixtures/matcher-m2/inventory-primary-repair.json",
        ])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap()
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn production_path_migrates_215_reviews_without_duplicate_notifications_and_is_idempotent() {
    let root = root();
    let input = root.join("fixtures/matcher-m3/phase3b-old-state");
    let input_before: BTreeMap<_, _> = FILES
        .iter()
        .map(|name| (*name, std::fs::read(input.join(name)).unwrap()))
        .collect();
    let temp = std::env::temp_dir().join(format!(
        "manga-m3-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let first = temp.join("first");
    let second = temp.join("second");

    let run = invoke(&input, &first);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let first_report = read(&first.join("scan-report.json"));
    assert_eq!(first_report["matcher_version"], "matcher-m2-v1");
    assert_eq!(first_report["reanalyzed"], 215);
    assert_eq!(first_report["review"], 215);
    assert_eq!(first_report["new_review_events"], 0);
    assert_eq!(first_report["new_events"], 0);
    assert_eq!(first_report["review_migration"]["total"], 215);
    assert_eq!(first_report["review_migration"]["reclassified"], 187);
    assert_eq!(
        first_report["review_migration"]["classification_unchanged"],
        28
    );
    assert_eq!(
        first_report["review_migration"]["suppressed_new_review_notifications"],
        215
    );
    assert!(read(&first.join("latest.json"))["events"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(first_report["requests"], 0);
    assert_eq!(first_report["image_requests"], 0);
    assert_eq!(
        first_report["review_reasons"]["TITLE_WITNESS_UNKNOWN_SOURCE_CONTENT_TYPE"],
        13
    );
    assert_eq!(
        first_report["review_reasons"]["UNKNOWN_SOURCE_CONTENT_TYPE_NO_IDENTITY_PROOF"],
        107
    );
    let first_state = load_checkpoint(&first).unwrap();
    assert_eq!(
        first_state
            .catalog
            .values()
            .filter(|entry| entry.matcher_version == "matcher-m2-v1")
            .count(),
        215
    );
    assert_eq!(
        first_state
            .catalog
            .values()
            .filter(|entry| entry.identity_evidence["disposition"] == "REVIEW_REQUIRED")
            .count(),
        215
    );
    assert_eq!(
        first_state
            .catalog
            .values()
            .filter(|entry| {
                entry.identity_evidence["reason"] == "TITLE_WITNESS_UNKNOWN_SOURCE_CONTENT_TYPE"
                    && !entry.identity_evidence["matching_work_ids"]
                        .as_array()
                        .unwrap()
                        .is_empty()
            })
            .count(),
        13
    );
    assert_eq!(first_state.scan.inventory_repairs.len(), 1);
    assert_eq!(
        first_state.scan.inventory_repairs[0]["work_id"],
        "WORK_02657"
    );
    assert_eq!(
        first_state.scan.inventory_repairs[0]["local_item_id"],
        "LOCAL_ITEM_2705"
    );
    assert!(first_state.scan.inventory_repairs[0]["analysis_inventory_hash_before"].is_string());
    assert!(first_state.scan.inventory_repairs[0]["analysis_inventory_hash_after"].is_string());

    let run = invoke(&first, &second);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let second_report = read(&second.join("scan-report.json"));
    assert_eq!(second_report["reanalyzed"], 0);
    assert_eq!(second_report["new_events"], 0);
    assert_eq!(second_report["new_review_events"], 0);
    assert_eq!(second_report["review_migration"]["total"], 0);
    assert_eq!(second_report["review_migration"]["reclassified"], 0);
    assert_eq!(
        second_report["review_migration"]["classification_unchanged"],
        0
    );
    assert_eq!(second_report["business_state_unchanged"], true);
    let second_state = load_checkpoint(&second).unwrap();
    assert_eq!(
        hash(&(&first_state.pending, &first_state.review)),
        hash(&(&second_state.pending, &second_state.review))
    );
    for (name, bytes) in input_before {
        assert_eq!(std::fs::read(input.join(name)).unwrap(), bytes);
    }
    assert_eq!(
        read(&root.join("monitor-config.json"))["production_enabled"],
        false
    );
}

#[test]
fn matcher_version_is_part_of_the_production_analysis_context() {
    let state = State {
        authors: json!({"authors":[]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    let legacy = hash(&(&state.decisions, &state.inventory, &state.authors));
    assert_ne!(state.context(), legacy);
    assert_eq!(
        state.context(),
        hash(&(
            rules_core::title_m2::RULE_VERSION,
            &state.decisions,
            &state.inventory,
            &state.authors
        ))
    );
}

#[test]
fn a_real_identity_evidence_change_can_create_a_new_review_event() {
    let mut state = State {
        authors: json!({"authors":[{"name":"Writer","enabled":true}]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    state
        .begin(
            "first",
            "2026-09-06T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    let unknown = Record::new(
        "jm",
        "1".into(),
        vec![],
        "Long title".into(),
        json!({"content_type":"manga"}),
    );
    state.accept(&unknown, &unknown).unwrap();
    assert_eq!(state.scan.events[0]["kind"], "NEW_REVIEW");
    state
        .begin(
            "second",
            "2026-09-07T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    let confirmed = Record::new(
        "jm",
        "1".into(),
        vec!["Writer".into()],
        "Long title".into(),
        json!({"content_type":"manga"}),
    );
    state.accept(&confirmed, &confirmed).unwrap();
    assert_eq!(state.scan.events.len(), 1);
    assert_eq!(state.scan.events[0]["kind"], "NEW_REVIEW");
    assert_eq!(
        state
            .review
            .values()
            .filter(|review| review.status == "REVIEW_REQUIRED")
            .count(),
        1
    );
}

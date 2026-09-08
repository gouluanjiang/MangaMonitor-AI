use cloud_monitor::{monitor::*, persistence::*};
use serde_json::json;
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, fs, process::Command};

fn state() -> State {
    state_with("full", 5)
}

fn state_with(mode: &str, threshold: usize) -> State {
    let mut state = State {
        authors: json!({"schema_version":1,"authors":[{"name":"Writer","enabled":true}]}),
        inventory: json!({"schema_version":8,"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    state
        .begin(
            "audit-one",
            "2026-09-08T00:00:00Z",
            vec!["Writer".into()],
            mode,
            threshold,
        )
        .unwrap();
    state
}

fn record(id: &str, title: &str) -> Record {
    Record::new(
        "jm",
        id.into(),
        vec!["Writer".into()],
        title.into(),
        json!({"content_type":"manga"}),
    )
}

fn page(
    page: u64,
    records: &[&str],
    total: Option<u64>,
    pages: Option<u64>,
    limit: Option<u64>,
) -> SearchPage {
    SearchPage {
        page,
        reported_total: total,
        reported_pages: pages,
        reported_limit: limit,
        response_fields: vec![],
        record_fields: vec![],
        redirect_to_detail: false,
        records: records
            .iter()
            .map(|id| record(id, "Stable title"))
            .collect(),
    }
}

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mangamonitor-{label}-{}", hash(&now())))
}

#[test]
fn source_errors_are_visible_per_scan_but_replay_idempotent_within_scan() {
    let mut state = state();
    state.source_error("jm", "Writer", "FETCH_FAILED");
    assert_eq!(state.scan.events.len(), 1);
    assert!(state.scan.events[0]["event_id"]
        .as_str()
        .unwrap()
        .starts_with("warning:audit-one:"));

    state.source_error("jm", "Writer", "FETCH_FAILED");
    assert_eq!(state.scan.events.len(), 1);

    state
        .begin(
            "audit-two",
            "2026-09-08T01:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    state.source_error("jm", "Writer", "FETCH_FAILED");
    assert_eq!(state.scan.events.len(), 1);
    assert!(state.scan.events[0]["event_id"]
        .as_str()
        .unwrap()
        .starts_with("warning:audit-two:"));
}

#[test]
fn review_csv_neutralizes_formula_leading_values() {
    let mut state = state();
    let r = record("formula", "=HYPERLINK(\"https://example.invalid\")");
    state.accept(&r, &r).unwrap();
    let root = std::env::temp_dir().join(format!("mangamonitor-csv-{}", hash(&now())));
    fs::create_dir_all(&root).unwrap();
    export_reviews(&root, &state).unwrap();
    let csv = fs::read_to_string(root.join("review-export.csv")).unwrap();
    assert!(csv.contains("\"'=HYPERLINK(\"\"https://example.invalid\"\")\""));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unchanged_pending_task_survives_unrelated_context_migration() {
    let mut state = state();
    state.decisions.positive_mappings.push(Mapping {
        source_key: "jm:stable".into(),
        work_id: "WORK_ONE".into(),
    });
    let r = record("stable", "Stable title");
    state.accept(&r, &r).unwrap();
    let before = state.pending["WORK_ONE"].clone();

    state
        .decisions
        .ignored_source_records
        .push("jm:unrelated".into());
    state
        .begin(
            "audit-two",
            "2026-09-08T01:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    state.accept(&r, &r).unwrap();

    let after = &state.pending["WORK_ONE"];
    assert_eq!(after.status, "pending");
    assert_eq!(after.task_revision, before.task_revision);
    assert_eq!(hash(&after.target), hash(&before.target));
}

#[test]
fn contradictory_pagination_is_incomplete_and_never_full() {
    let mut first = state();
    assert!(first.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(21), Some(1), Some(20)),
    ));
    assert!(first.page_boundary(
        "jm",
        "Writer",
        &page(3, &["three"], Some(21), Some(3), Some(20)),
    ));
    assert_eq!(
        first.scan.progress["jm|Writer"].boundary,
        "INCOMPLETE_PAGINATION"
    );
    assert!(first.scan.last_full.is_empty());

    let mut duplicate = state();
    assert!(!duplicate.page_boundary("jm", "Writer", &page(1, &["same"], Some(2), None, None),));
    assert!(duplicate.page_boundary("jm", "Writer", &page(2, &["same"], Some(2), None, None),));
    assert_eq!(
        duplicate.scan.progress["jm|Writer"].boundary,
        "INCOMPLETE_PAGINATION"
    );
    assert!(duplicate.scan.last_full.is_empty());

    let mut terminal = state();
    assert!(terminal.page_boundary(
        "jm",
        "Writer",
        &page(1, &["terminal"], Some(1), Some(1), Some(20)),
    ));
    assert!(terminal.scan.last_full.contains_key("jm|Writer"));

    let mut empty = state();
    assert!(empty.page_boundary("jm", "Writer", &page(1, &[], Some(0), Some(0), Some(20)),));
    assert!(empty.scan.last_full.contains_key("jm|Writer"));

    let mut redirect = state();
    let mut detail = page(1, &["redirect"], Some(1), Some(1), Some(20));
    detail.redirect_to_detail = true;
    assert!(redirect.page_boundary("jm", "Writer", &detail));
    assert!(redirect.scan.last_full.contains_key("jm|Writer"));
}

#[test]
fn a04_invalid_page_does_not_advance_cursor_and_resume_retries_it() {
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(3), Some(1)),
    ));
    let after_page_one = state.scan.progress["jm|Writer"].clone();

    // The page count contradicts total/limit. Its record must not be consumed.
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(2), Some(1)),
    ));
    assert_eq!(
        state.scan.progress["jm|Writer"].next_page,
        after_page_one.next_page
    );
    assert_eq!(
        state.scan.progress["jm|Writer"].observed_ids,
        after_page_one.observed_ids
    );
    assert_eq!(
        state.scan.progress["jm|Writer"].historical_streak,
        after_page_one.historical_streak
    );
    assert!(state.scan.last_full.is_empty());

    // Resume starts at the same expected page and can now complete normally.
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(3), Some(1)),
    ));
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(3, &["three"], Some(3), Some(3), Some(1)),
    ));
    assert_eq!(state.scan.progress["jm|Writer"].boundary, "COMPLETE");
    assert!(state.scan.last_full.contains_key("jm|Writer"));
}

#[test]
fn a04_malformed_page_cannot_trigger_historical_early_stop() {
    let mut state = state_with("incremental", 1);
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["new"], Some(4), Some(4), Some(1)),
    ));
    state.scan.historical_ids.insert("jm:old".to_string());

    // This page would reach the historical threshold, but its metadata is
    // contradictory and must therefore fail closed before early-stop logic.
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["old"], Some(4), Some(2), Some(1)),
    ));
    let cursor = &state.scan.progress["jm|Writer"];
    assert_eq!(cursor.boundary, "INCOMPLETE_PAGINATION");
    assert_eq!(cursor.next_page, 2);
    assert_eq!(cursor.historical_streak, 0);
    assert!(!cursor.observed_ids.contains("jm:old"));
    assert!(!state
        .scan
        .events
        .iter()
        .any(|event| { event["reason"] == json!("EARLY_STOP_HEURISTIC") }));
}

#[test]
fn a04_terminal_page_with_short_observed_count_stays_incomplete() {
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(2), Some(2)),
    ));
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(2), Some(2)),
    ));
    let cursor = &state.scan.progress["jm|Writer"];
    assert_eq!(cursor.boundary, "INCOMPLETE_PAGINATION");
    assert_eq!(cursor.next_page, 2);
    assert_eq!(cursor.observed_ids.len(), 1);
    assert!(state.scan.last_full.is_empty());
}

#[test]
fn a04_partial_duplicate_overlap_fails_closed_without_deduping() {
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(2), Some(2)),
    ));
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["one", "two"], Some(3), Some(2), Some(2)),
    ));
    let cursor = &state.scan.progress["jm|Writer"];
    assert_eq!(cursor.boundary, "INCOMPLETE_PAGINATION");
    assert_eq!(cursor.next_page, 2);
    assert_eq!(cursor.observed_ids.len(), 1);
    assert!(state.scan.last_full.is_empty());
}

#[test]
fn resume_refuses_to_replace_changed_authority_and_keeps_checkpoint() {
    let root = temp_root("resume-authority");
    let input = root.join("input");
    let output = root.join("output");
    let authors = root.join("authors.json");
    fs::create_dir_all(&root).unwrap();
    let original = state();
    save(&input, &original).unwrap();
    save(&output, &original).unwrap();
    let checkpoint_hash = hash(&load_checkpoint(&output).unwrap());
    write_json(&output.join("state-manifest.json"), &json!({
        "schema_version":1,"base_commit":"fixture-base","state_hash":checkpoint_hash,
        "scan_id":original.scan.scan_id,"complete":false,"strategy_complete":false,
        "requested_mode":"full","effective_requested_mode":"full",
        "batch_index":0,"batch_count":1,"batch_size":1,"selected_authors":["Writer"],
        "all_authors":["Writer"],"recovery":null,
        "matcher_version":rules_core::title_m2::RULE_VERSION,"analysis_context":original.context()
    })).unwrap();
    let mut changed = original.clone();
    changed.decisions.positive_mappings.push(Mapping {
        source_key: "jm:changed".into(),
        work_id: "WORK_CHANGED".into(),
    });
    save(&input, &changed).unwrap();
    write_json(&authors, &json!(["Writer"])).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_phase3b"))
        .args(["--state"])
        .arg(&input)
        .args(["--output"])
        .arg(&output)
        .args(["--authors"])
        .arg(&authors)
        .args([
            "--resume",
            "--mode",
            "full",
            "--batch-size",
            "1",
            "--threshold",
            "5",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("RESUME_AUTHORITY_MISMATCH"));
    assert_eq!(hash(&load_checkpoint(&output).unwrap()), checkpoint_hash);
    assert!(!output.join("scan-report.json").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn state_loader_rejects_unknown_schema_authority_and_duplicate_review_ids() {
    let root = temp_root("schema");
    fs::create_dir_all(&root).unwrap();
    let state = state();
    save(&root, &state).unwrap();

    let mut future = serde_json::from_slice::<serde_json::Value>(
        &fs::read(root.join("decisions.json")).unwrap(),
    )
    .unwrap();
    future["schema_version"] = json!(99);
    write_json(&root.join("decisions.json"), &future).unwrap();
    assert!(load(&root)
        .unwrap_err()
        .contains("UNSUPPORTED_SCHEMA_decisions.json_99"));

    save(&root, &state).unwrap();
    let mut authority = serde_json::from_slice::<serde_json::Value>(
        &fs::read(root.join("decisions.json")).unwrap(),
    )
    .unwrap();
    authority["new_authority_field"] = json!(true);
    write_json(&root.join("decisions.json"), &authority).unwrap();
    assert_eq!(load(&root).unwrap_err(), "UNSUPPORTED_DECISIONS_FIELD");

    save(&root, &state).unwrap();
    let mut review =
        serde_json::from_slice::<serde_json::Value>(&fs::read(root.join("review.json")).unwrap())
            .unwrap();
    review["match_review"] = json!([{
        "review_id":"DUP",
        "source_key":"jm:one",
        "reason":"TEST",
        "author":["Writer"],
        "title":"one",
        "candidates":[],
        "status":"REVIEW_REQUIRED"
    }, {
        "review_id":"DUP",
        "source_key":"jm:two",
        "reason":"TEST",
        "author":["Writer"],
        "title":"two",
        "candidates":[],
        "status":"REVIEW_REQUIRED"
    }]);
    write_json(&root.join("review.json"), &review).unwrap();
    assert_eq!(load(&root).unwrap_err(), "DUPLICATE_REVIEW_ID");
    let _ = fs::remove_dir_all(root);
}

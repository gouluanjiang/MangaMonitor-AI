use cloud_monitor::{monitor::*, persistence::export_reviews};
use serde_json::json;
use state_model::Record;
use std::{collections::BTreeMap, fs};

fn state() -> State {
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
        .begin("audit-one", "2026-09-08T00:00:00Z", vec!["Writer".into()], "full", 5)
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
        .begin("audit-two", "2026-09-08T01:00:00Z", vec!["Writer".into()], "full", 5)
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

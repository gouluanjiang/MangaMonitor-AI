use cloud_monitor::{matcher_m2, monitor::*, persistence};
use serde_json::{json, Value};
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, fs, path::Path};

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
        .begin(
            "a04-final",
            "2026-09-08T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    state
}

fn record(id: &str) -> Record {
    Record::new(
        "jm",
        id.into(),
        vec!["Writer".into()],
        "Stable title".into(),
        json!({"content_type":"manga"}),
    )
}

fn page(
    number: u64,
    records: &[&str],
    total: Option<u64>,
    pages: Option<u64>,
    limit: Option<u64>,
) -> SearchPage {
    SearchPage {
        page: number,
        reported_total: total,
        reported_pages: pages,
        reported_limit: limit,
        response_fields: vec![],
        record_fields: vec![],
        redirect_to_detail: false,
        records: records.iter().map(|id| record(id)).collect(),
    }
}

#[test]
fn ordinary_empty_page_is_rejected_before_cursor_commit_and_retry_completes() {
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(3), Some(1))
    ));
    let committed = state.scan.progress["jm|Writer"].clone();
    assert!(committed.pagination_contract.is_some());

    assert!(state.page_boundary("jm", "Writer", &page(2, &[], Some(3), Some(3), Some(1))));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.historical_streak, committed.historical_streak);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    assert_eq!(rejected.boundary, "INCOMPLETE_PAGINATION");
    assert!(state.scan.last_full.is_empty());

    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(3), Some(1))
    ));
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(3, &["three"], Some(3), Some(3), Some(1))
    ));
    assert_eq!(state.scan.progress["jm|Writer"].boundary, "COMPLETE");
    assert!(state.scan.last_full.contains_key("jm|Writer"));
}

#[test]
fn pagination_contract_cannot_shrink_mid_enumeration() {
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(3), Some(1))
    ));
    let committed = state.scan.progress["jm|Writer"].clone();

    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(2), Some(2), Some(1))
    ));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    assert!(state.scan.last_full.is_empty());

    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(3), Some(1))
    ));
    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(3, &["three"], Some(3), Some(3), Some(1))
    ));
    assert_eq!(state.scan.progress["jm|Writer"].boundary, "COMPLETE");
}

#[test]
fn durable_checkpoint_preserves_contract_and_rejects_shrink_after_reload() {
    let root =
        std::env::temp_dir().join(format!("mangamonitor-a04-contract-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let mut state = state();
    assert!(!state.page_boundary(
        "jm",
        "Writer",
        &page(1, &["one"], Some(3), Some(3), Some(1))
    ));
    persistence::save(&root, &state).unwrap();

    let mut resumed = persistence::load_checkpoint(&root).unwrap();
    let committed = resumed.scan.progress["jm|Writer"].clone();
    assert_eq!(
        committed.pagination_contract,
        Some(PaginationContract {
            reported_total: Some(3),
            reported_pages: Some(3),
            reported_limit: Some(1),
        })
    );
    assert!(resumed.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(2), Some(2), Some(1))
    ));
    let rejected = &resumed.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn legacy_mid_cycle_cursor_without_contract_fails_closed() {
    let mut state = state();
    let mut legacy = Cursor {
        next_page: 2,
        boundary: "CHECKPOINT".into(),
        mode: "full".into(),
        ..Cursor::default()
    };
    legacy.observed_ids.insert("jm:one".into());
    state
        .scan
        .progress
        .insert("jm|Writer".into(), legacy.clone());

    assert!(state.page_boundary(
        "jm",
        "Writer",
        &page(2, &["two"], Some(3), Some(3), Some(1))
    ));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, 2);
    assert_eq!(rejected.observed_ids, legacy.observed_ids);
    assert!(rejected.pagination_contract.is_none());
    assert_eq!(rejected.boundary, "INCOMPLETE_PAGINATION");
}

#[test]
fn public_seed_is_already_at_audited_repair_after_state_and_repair_is_noop() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = persistence::load(&repo.join("monitor-state")).unwrap();
    let repair: Value = serde_json::from_slice(
        &fs::read(repo.join("fixtures/matcher-m2/inventory-primary-repair.json")).unwrap(),
    )
    .unwrap();
    let before_hash = hash(&state.inventory);
    assert_eq!(
        Some(before_hash.as_str()),
        repair["analysis_inventory_hash_after"].as_str()
    );

    let work = state.inventory["works"]
        .as_array()
        .unwrap()
        .iter()
        .find(|work| work["work_id"] == "WORK_02657")
        .unwrap();
    assert_eq!(
        work["title_candidates"],
        json!([{
            "primary":"竿役募集してる推しの爆乳エロ配信者が妹になりました",
            "normalized_key":null,
            "fandom_or_source":"オリジナル"
        }])
    );

    let mut repaired = state.inventory.clone();
    matcher_m2::repair_primary(&mut repaired, &repair).unwrap();
    assert_eq!(hash(&repaired), before_hash);
    assert_eq!(repaired, state.inventory);
}

#[test]
fn public_decisions_seed_matches_current_persistence_save_schema() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state_dir = repo.join("monitor-state");
    let state = persistence::load(&state_dir).unwrap();
    let public: Value = serde_json::from_slice(&fs::read(state_dir.join("decisions.json")).unwrap()).unwrap();
    assert_eq!(public["schema_version"], 3);
    for field in ["positive_mappings", "negative_mappings", "ignored_source_records", "ignored_works"] {
        assert_eq!(public[field], json!([]));
    }

    let output = std::env::temp_dir().join(format!(
        "mangamonitor-public-decisions-roundtrip-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&output);
    persistence::save(&output, &state).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(output.join("decisions.json")).unwrap()).unwrap();
    assert_eq!(saved, public);
    assert_eq!(saved["schema_version"], 3);
    for field in ["positive_mappings", "negative_mappings", "ignored_source_records", "ignored_works"] {
        assert_eq!(saved[field], public[field]);
    }
    let _ = fs::remove_dir_all(output);
}

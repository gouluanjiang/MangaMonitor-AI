use cloud_monitor::{
    assistant_author, assistant_decision, assistant_publication, assistant_task_gate,
    monitor::{AuthorEvidence, Decisions, Entry, Review, Scan, State, Target, Task},
    persistence,
};
use rules_core::title_m2;
use serde::Serialize;
use serde_json::{json, Value};
use state_model::{Record, Version};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

fn unique(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mangamonitor-publication-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn write_json(path: &Path, value: &impl Serialize) {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    fs::write(path, bytes).unwrap();
}

fn work(id: &str, title: &str) -> Value {
    json!({
        "work_id": id,
        "owned": true,
        "authors_confirmed": ["Writer"],
        "local_item_ids": [format!("LOCAL_{id}")],
        "title_candidates": [{"primary":title,"fandom_or_source":null}],
        "versions": [{"local_item_id":format!("LOCAL_{id}"),"content":{"type":"manga"}}],
        "source_mappings": {"jm":[],"pica":[]}
    })
}

fn sample_state() -> State {
    let mut record = Record::new(
        "jm",
        "123".into(),
        vec!["Writer".into()],
        "作品标题".into(),
        json!({"content_type":"manga"}),
    );
    record.first_seen = "fixed".into();
    record.last_seen = "fixed".into();
    record.last_checked = "fixed".into();

    let mut catalog = BTreeMap::new();
    catalog.insert(
        "jm:123".into(),
        Entry {
            record: record.clone(),
            author_evidence: AuthorEvidence::default(),
            search_fingerprint: "search".into(),
            detail_fingerprint: "detail".into(),
            analysis_context: "old-context".into(),
            work_id: None,
            analysis_count: 1,
            unavailable_streak: 0,
            active: true,
            last_unavailable_check: None,
            search_queries: Default::default(),
            matcher_version: title_m2::RULE_VERSION.into(),
            identity_evidence: Value::Null,
            identity_provenance: Value::Null,
        },
    );
    let review = Review {
        review_id: "R1".into(),
        source_key: "jm:123".into(),
        reason: "TEST_REVIEW".into(),
        author: record.author.clone(),
        title: record.raw_title.clone(),
        candidates: vec!["W1".into()],
        status: "REVIEW_REQUIRED".into(),
        matcher_version: title_m2::RULE_VERSION.into(),
        identity_evidence: Value::Null,
        provenance: json!({"analysis":{"source_key":"jm:123","detail_fingerprint":"detail"}}),
    };
    let target = Target {
        source_key: "jm:123".into(),
        author: "Writer".into(),
        title: "作品标题".into(),
        version: Version::default(),
        coverage: Value::Null,
    };
    let task = Task {
        task_id: "TASK_1".into(),
        work_id: "W1".into(),
        first_seen: "fixed".into(),
        task_revision: 1,
        target,
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: vec![],
    };
    State {
        authors: json!({
            "schema_version": 1,
            "authors":[{"author_id":"AUTHOR_1","name":"Writer","enabled":true}]
        }),
        inventory: json!({"works":[work("W1", "作品标题")]}),
        catalog,
        pending: BTreeMap::from([("W1".into(), task)]),
        review: BTreeMap::from([("R1".into(), review)]),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn save_state(path: &Path, state: &State) {
    fs::create_dir_all(path).unwrap();
    persistence::save(path, state).unwrap();
}

fn stage_author(state: &State, path: &Path) {
    fs::create_dir_all(path).unwrap();
    let (payload, audit) = assistant_author::plan(&state.authors, "disable", "Writer").unwrap();
    write_json(&path.join("authors.json"), &payload);
    write_json(&path.join("author-change.json"), &audit);
}

fn stage_decision(state: &State, path: &Path) {
    fs::create_dir_all(path).unwrap();
    let (payload, audit, preview) =
        assistant_decision::plan(state, "R1", "same", Some("W1")).unwrap();
    write_json(&path.join("decisions.json"), &payload);
    write_json(&path.join("decision-change.json"), &audit);
    write_json(&path.join("reanalyze-preview.json"), &preview);
}

fn stage_gate(state: &State, path: &Path) {
    fs::create_dir_all(path).unwrap();
    let ledger = assistant_task_gate::GateLedger::default();
    let task = state.pending.get("W1").unwrap();
    let target_hash = assistant_task_gate::target_hash(task);
    let (payload, audit, preview) = assistant_task_gate::plan(
        state,
        &ledger,
        "approve",
        "TASK_1",
        1,
        &target_hash,
    )
    .unwrap();
    write_json(&path.join("assistant-task-gates.json"), &payload);
    write_json(&path.join("task-gate-change.json"), &audit);
    write_json(&path.join("executor-preview.json"), &preview);
}

#[test]
fn current_state_replay_authorizes_exactly_one_monitor_state_file() {
    let root = unique("all-kinds");
    let state_dir = root.join("state");
    let state = sample_state();
    save_state(&state_dir, &state);

    let author = root.join("author");
    stage_author(&state, &author);
    let author_check = assistant_publication::check(&state_dir, &author, "author").unwrap();
    assert_eq!(author_check.target_file, "authors.json");
    assert!(author_check.monitor_state_mutation_authorized);
    assert!(!author_check.manga_file_access_authorized);
    assert!(!author_check.download_execution_authorized);
    assert!(!author_check.production_enablement_authorized);

    let decision = root.join("decision");
    stage_decision(&state, &decision);
    let decision_check =
        assistant_publication::check(&state_dir, &decision, "decision").unwrap();
    assert_eq!(decision_check.target_file, "decisions.json");

    let gate = root.join("gate");
    stage_gate(&state, &gate);
    let gate_check = assistant_publication::check(&state_dir, &gate, "task-gate").unwrap();
    assert_eq!(gate_check.target_file, "assistant-task-gates.json");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_review_or_task_generation_fails_closed() {
    let root = unique("stale");
    let state_dir = root.join("state");
    let state = sample_state();
    save_state(&state_dir, &state);

    let decision = root.join("decision");
    stage_decision(&state, &decision);
    let mut changed = state.clone();
    changed.catalog.get_mut("jm:123").unwrap().detail_fingerprint = "new-detail".into();
    save_state(&state_dir, &changed);
    assert!(assistant_publication::check(&state_dir, &decision, "decision").is_err());

    save_state(&state_dir, &state);
    let gate = root.join("gate");
    stage_gate(&state, &gate);
    let mut changed = state.clone();
    changed.pending.get_mut("W1").unwrap().task_revision = 2;
    save_state(&state_dir, &changed);
    assert!(assistant_publication::check(&state_dir, &gate, "task-gate").is_err());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unexpected_staging_files_are_rejected_before_replay() {
    let root = unique("extra-file");
    let state_dir = root.join("state");
    let state = sample_state();
    save_state(&state_dir, &state);
    let stage = root.join("author");
    stage_author(&state, &stage);
    fs::write(stage.join("manga.webp"), b"not allowed").unwrap();
    assert_eq!(
        assistant_publication::check(&state_dir, &stage, "author").unwrap_err(),
        "ASSISTANT_PUBLICATION_STAGING_FILE_SET_MISMATCH"
    );
    fs::remove_dir_all(root).unwrap();
}

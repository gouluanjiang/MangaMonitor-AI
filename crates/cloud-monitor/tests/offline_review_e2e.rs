use cloud_monitor::{monitor::*, persistence};
use serde_json::{json, Value};
use state_model::Record;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn work(id: &str) -> Value {
    json!({
        "work_id": id,
        "owned": false,
        "authors_confirmed": ["Writer"],
        "local_item_ids": [format!("LOCAL_{id}")],
        "title_candidates": [{"primary":"作品标题","fandom_or_source":null}],
        "versions": [{"local_item_id":format!("LOCAL_{id}"),"content":{"type":"manga"}}],
        "source_mappings": {"jm":[],"pica":[]}
    })
}

fn seed_state() -> State {
    State {
        authors: json!({"authors":[{"author_id":"AUTHOR_WRITER","name":"Writer","enabled":true}]}),
        inventory: json!({"schema_version":1,"total_work_ids":2,"works":[work("W1"),work("W2")]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn source_record() -> Record {
    let mut record = Record::new(
        "jm",
        "123".into(),
        vec!["Writer".into()],
        "作品标题".into(),
        json!({"content_type":"manga","finished":true}),
    );
    record.first_seen = "fixture".into();
    record.last_seen = "fixture".into();
    record.last_checked = "fixture".into();
    record
}

fn state_bytes(dir: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    persistence::FILES
        .iter()
        .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
        .collect()
}

fn run_offline(command: &mut Command) -> std::process::Output {
    command
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap()
}

#[test]
fn discovery_review_export_human_decision_and_offline_reanalysis_are_end_to_end() {
    let root = unique("offline-review-e2e");
    let discovered_dir = root.join("discovered-state");
    let initial_bundle = root.join("initial-review-bundle");
    let decision_stage = root.join("decision-stage");
    let resolved_dir = root.join("resolved-state");
    let resolved_bundle = root.join("resolved-review-bundle");
    fs::create_dir_all(&root).unwrap();

    // 1. Discover one source record through the real monitor analysis path. Two exact
    // local candidates deliberately make identity ambiguous, so the safe outcome is review.
    let mut discovered = seed_state();
    discovered
        .begin(
            "offline-demo-discovery",
            "2026-09-08T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    let record = source_record();
    discovered.accept(&record, &record).unwrap();
    discovered.note_search_query(&record, "Writer");

    let active_review = discovered
        .review
        .values()
        .find(|review| review.status == "REVIEW_REQUIRED")
        .cloned()
        .expect("ambiguous discovery must require human review");
    assert_eq!(active_review.source_key, "jm:123");
    assert_eq!(active_review.reason, "AMBIGUOUS_EXISTING_IDENTITY");
    assert_eq!(active_review.candidates, vec!["W1", "W2"]);
    assert_eq!(discovered.catalog["jm:123"].work_id, None);
    assert!(discovered.pending.is_empty());
    persistence::save(&discovered_dir, &discovered).unwrap();

    // 2. Export the same bounded assistant bundle a human/ChatGPT reviewer consumes.
    // The export must remain offline and leave the discovered state byte-for-byte unchanged.
    let before_export = state_bytes(&discovered_dir);
    let export = run_offline(
        Command::new(env!("CARGO_BIN_EXE_assistant-export"))
            .current_dir(repo_root())
            .args(["--state"])
            .arg(&discovered_dir)
            .args(["--output"])
            .arg(&initial_bundle)
            .args(["--batch-size", "25"]),
    );
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(before_export, state_bytes(&discovered_dir));

    let review_summary: Value = serde_json::from_slice(
        &fs::read(initial_bundle.join("review-summary.json")).unwrap(),
    )
    .unwrap();
    let review_batch: Value = serde_json::from_slice(
        &fs::read(initial_bundle.join("review-batch-000000.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(review_summary["total"], 1);
    assert_eq!(review_batch["returned"], 1);
    assert_eq!(review_batch["items"][0]["review_id"], active_review.review_id);
    assert_eq!(
        review_batch["items"][0]["candidate_work_ids"],
        json!(["W1", "W2"])
    );

    // 3. Stage an explicit HUMAN_SAME decision. This CLI is staging-only: the input
    // monitor state must remain unchanged and no decision becomes authoritative yet.
    let decision = run_offline(
        Command::new(env!("CARGO_BIN_EXE_assistant-decision-edit"))
            .current_dir(repo_root())
            .args(["--state"])
            .arg(&discovered_dir)
            .args(["--output"])
            .arg(&decision_stage)
            .args(["--review-id", &active_review.review_id])
            .args(["--decision", "same", "--work-id", "W1"]),
    );
    assert!(
        decision.status.success(),
        "{}",
        String::from_utf8_lossy(&decision.stderr)
    );
    assert_eq!(before_export, state_bytes(&discovered_dir));

    let staged_decisions: Decisions = serde_json::from_slice(
        &fs::read(decision_stage.join("decisions.json")).unwrap(),
    )
    .unwrap();
    let preview: Value = serde_json::from_slice(
        &fs::read(decision_stage.join("reanalyze-preview.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(preview["disposition"], "AUTHORITATIVE_EXISTING");
    assert_eq!(preview["reason"], "HUMAN_SAME");

    // 4. Simulate publication of exactly that decision into a separate state generation,
    // then start the next offline scan generation. begin() detects the changed analysis
    // context and re-runs the real matcher without any source/network request.
    let mut resolved = persistence::load(&discovered_dir).unwrap();
    resolved.decisions = staged_decisions;
    resolved
        .begin(
            "offline-demo-reanalysis",
            "2026-09-08T00:01:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();

    assert_eq!(resolved.catalog["jm:123"].work_id.as_deref(), Some("W1"));
    assert_eq!(
        resolved.catalog["jm:123"].identity_evidence["reason"],
        "HUMAN_SAME"
    );
    assert_eq!(resolved.catalog["jm:123"].record.processing_result, "PENDING");
    assert!(resolved
        .review
        .values()
        .all(|review| review.status != "REVIEW_REQUIRED"));
    let task = resolved.pending.get("W1").expect("human SAME creates one bounded pending task");
    assert_eq!(task.task_revision, 1);
    assert_eq!(task.action, "download");
    assert_eq!(task.status, "pending");
    assert_eq!(task.target.source_key, "jm:123");
    assert!(!resolved.pending.contains_key("W2"));
    persistence::save(&resolved_dir, &resolved).unwrap();

    // 5. Re-export the resolved state: the review queue is empty and the pending task
    // is now visible. The export is still read-only/offline.
    let before_resolved_export = state_bytes(&resolved_dir);
    let export_resolved = run_offline(
        Command::new(env!("CARGO_BIN_EXE_assistant-export"))
            .current_dir(repo_root())
            .args(["--state"])
            .arg(&resolved_dir)
            .args(["--output"])
            .arg(&resolved_bundle)
            .args(["--batch-size", "25"]),
    );
    assert!(
        export_resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&export_resolved.stderr)
    );
    assert_eq!(before_resolved_export, state_bytes(&resolved_dir));
    let resolved_review: Value = serde_json::from_slice(
        &fs::read(resolved_bundle.join("review-summary.json")).unwrap(),
    )
    .unwrap();
    let pending: Value =
        serde_json::from_slice(&fs::read(resolved_bundle.join("pending.json")).unwrap()).unwrap();
    assert_eq!(resolved_review["total"], 0);
    assert_eq!(pending["count"], 1);
    assert_eq!(pending["tasks"][0]["work_id"], "W1");
    assert_eq!(pending["tasks"][0]["status"], "pending");

    // 6. Starting another scan with unchanged decisions is idempotent at the business
    // state level: it must not duplicate the task, reopen review, or bump its revision.
    let business_before = hash(&(
        &resolved.catalog,
        &resolved.review,
        &resolved.pending,
        &resolved.decisions,
    ));
    resolved
        .begin(
            "offline-demo-idempotent-replay",
            "2026-09-08T00:02:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    let business_after = hash(&(
        &resolved.catalog,
        &resolved.review,
        &resolved.pending,
        &resolved.decisions,
    ));
    assert_eq!(business_before, business_after);
    assert_eq!(resolved.pending["W1"].task_revision, 1);
    assert!(resolved.scan.events.is_empty());

    println!(
        "offline review E2E: discovery -> review export -> HUMAN_SAME -> pending task -> idempotent replay PASS"
    );
    fs::remove_dir_all(root).unwrap();
}

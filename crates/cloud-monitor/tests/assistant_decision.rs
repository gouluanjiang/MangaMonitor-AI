use cloud_monitor::{assistant_decision, monitor::*, persistence};
use rules_core::title_m2;
use serde_json::{json, Value};
use state_model::Record;
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

fn record() -> Record {
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
    record
}

fn sample_state() -> State {
    let record = record();
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
        candidates: vec!["W1".into(), "W2".into()],
        status: "REVIEW_REQUIRED".into(),
        matcher_version: title_m2::RULE_VERSION.into(),
        identity_evidence: Value::Null,
        provenance: json!({"analysis":{"source_key":"jm:123","detail_fingerprint":"detail"}}),
    };
    State {
        authors: json!({"authors":[{"author_id":"AUTHOR_1","name":"Writer","enabled":true}]}),
        inventory: json!({"works":[work("W1", "作品标题"), work("W2", "另一个作品")]}),
        catalog,
        pending: BTreeMap::new(),
        review: BTreeMap::from([("R1".into(), review)]),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn decisions(value: Value) -> Decisions {
    serde_json::from_value(value).unwrap()
}

#[test]
fn same_not_same_ignore_and_idempotence_are_bounded() {
    let state = sample_state();
    let (same, same_audit, same_preview) =
        assistant_decision::plan(&state, "R1", "same", Some("W1")).unwrap();
    assert_eq!(same["positive_mappings"].as_array().unwrap().len(), 1);
    assert_eq!(same_preview["disposition"], "AUTHORITATIVE_EXISTING");
    assert_eq!(same_preview["reason"], "HUMAN_SAME");
    assert_eq!(same_audit["outcome"], "ADDED");

    let (not_same, _, not_same_preview) =
        assistant_decision::plan(&state, "R1", "not-same", Some("W1")).unwrap();
    assert_eq!(not_same["negative_mappings"].as_array().unwrap().len(), 1);
    assert_ne!(not_same_preview["disposition"], "AUTO_EXISTING");

    let (ignored, _, ignored_preview) =
        assistant_decision::plan(&state, "R1", "ignore", None).unwrap();
    assert_eq!(ignored["ignored_source_records"], json!(["jm:123"]));
    assert_eq!(ignored_preview["disposition"], "IGNORED");
    assert_eq!(ignored_preview["reason"], "HUMAN_IGNORE_SOURCE");

    for (decision, work_id, first) in [
        ("same", Some("W1"), same),
        ("not-same", Some("W1"), not_same),
        ("ignore", None, ignored),
    ] {
        let mut replay = sample_state();
        replay.decisions = decisions(first.clone());
        let (second, audit, _) =
            assistant_decision::plan(&replay, "R1", decision, work_id).unwrap();
        assert_eq!(first, second);
        assert_eq!(audit["outcome"], "NOOP_ALREADY_PRESENT");
    }
    assert!(state.decisions.positive_mappings.is_empty());
}

#[test]
fn conflicts_non_candidates_and_stale_reviews_fail_closed() {
    let mut state = sample_state();
    state.decisions.negative_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W1".into(),
    });
    assert_eq!(
        assistant_decision::plan(&state, "R1", "same", Some("W1")).unwrap_err(),
        "ASSISTANT_SAME_CONTRADICTS_NOT_SAME"
    );

    let mut state = sample_state();
    state.decisions.positive_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W1".into(),
    });
    assert_eq!(
        assistant_decision::plan(&state, "R1", "not-same", Some("W1")).unwrap_err(),
        "ASSISTANT_NOT_SAME_CONTRADICTS_SAME"
    );

    let mut state = sample_state();
    state.decisions.positive_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W2".into(),
    });
    assert_eq!(
        assistant_decision::plan(&state, "R1", "same", Some("W1")).unwrap_err(),
        "ASSISTANT_SAME_ALREADY_POINTS_ELSEWHERE"
    );

    let state = sample_state();
    assert_eq!(
        assistant_decision::plan(&state, "R1", "same", Some("W9")).unwrap_err(),
        "ASSISTANT_DECISION_WORK_NOT_IN_REVIEW_CANDIDATES"
    );
    assert_eq!(
        assistant_decision::plan(&state, "R1", "ignore", Some("W1")).unwrap_err(),
        "ASSISTANT_IGNORE_DOES_NOT_ACCEPT_WORK_ID"
    );

    let mut state = sample_state();
    state.review.get_mut("R1").unwrap().candidates.push("W3".into());
    assert_eq!(
        assistant_decision::plan(&state, "R1", "same", Some("W3")).unwrap_err(),
        "ASSISTANT_DECISION_CANDIDATE_WORK_MISSING"
    );
    assert_eq!(
        assistant_decision::plan(&state, "missing", "ignore", None).unwrap_err(),
        "ASSISTANT_REVIEW_NOT_FOUND"
    );

    let mut resolved = sample_state();
    resolved.review.get_mut("R1").unwrap().status = "RESOLVED".into();
    assert_eq!(
        assistant_decision::plan(&resolved, "R1", "ignore", None).unwrap_err(),
        "ASSISTANT_REVIEW_NOT_ACTIVE"
    );

    let mut stale = sample_state();
    stale.catalog.get_mut("jm:123").unwrap().detail_fingerprint = "changed".into();
    assert_eq!(
        assistant_decision::plan(&stale, "R1", "ignore", None).unwrap_err(),
        "ASSISTANT_REVIEW_FINGERPRINT_STALE"
    );

    let mut stale_rule = sample_state();
    stale_rule.review.get_mut("R1").unwrap().matcher_version = "old-rule".into();
    assert_eq!(
        assistant_decision::plan(&stale_rule, "R1", "ignore", None).unwrap_err(),
        "ASSISTANT_REVIEW_MATCHER_VERSION_STALE"
    );
}

#[test]
fn unrelated_decisions_are_preserved() {
    let mut state = sample_state();
    state.decisions.positive_mappings.push(Mapping {
        source_key: "pica:other".into(),
        work_id: "W2".into(),
    });
    state.decisions.ignored_source_records.push("jm:old".into());
    state.decisions.ignored_works.push("W_IGNORED".into());
    let (proposed, _, _) =
        assistant_decision::plan(&state, "R1", "not-same", Some("W1")).unwrap();
    assert!(proposed["positive_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mapping| mapping["source_key"] == "pica:other" && mapping["work_id"] == "W2"));
    assert!(proposed["ignored_source_records"]
        .as_array()
        .unwrap()
        .contains(&json!("jm:old")));
    assert!(proposed["ignored_works"]
        .as_array()
        .unwrap()
        .contains(&json!("W_IGNORED")));
}

fn state_bytes(dir: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    persistence::FILES
        .iter()
        .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
        .collect()
}

#[test]
fn assistant_decision_cli_stages_real_fixture_offline_without_mutating_input() {
    let input = fixture();
    let state = persistence::load(&input).unwrap();
    let review = state
        .review
        .values()
        .find(|review| review.status == "REVIEW_REQUIRED")
        .unwrap();
    let review_id = review.review_id.clone();
    let source_key = review.source_key.clone();
    let before = state_bytes(&input);
    let output = unique("assistant-decision-ignore");
    let run = Command::new(env!("CARGO_BIN_EXE_assistant-decision-edit"))
        .current_dir(root())
        .args(["--state"])
        .arg(&input)
        .args(["--output"])
        .arg(&output)
        .args(["--review-id", &review_id, "--decision", "ignore"])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let audit: Value =
        serde_json::from_slice(&fs::read(output.join("decision-change.json")).unwrap()).unwrap();
    let preview: Value =
        serde_json::from_slice(&fs::read(output.join("reanalyze-preview.json")).unwrap()).unwrap();
    let proposed: Value =
        serde_json::from_slice(&fs::read(output.join("decisions.json")).unwrap()).unwrap();
    assert_eq!(audit["source_key"], source_key);
    assert_eq!(preview["disposition"], "IGNORED");
    assert!(proposed["ignored_source_records"]
        .as_array()
        .unwrap()
        .contains(&json!(source_key)));
    assert_eq!(before, state_bytes(&input));

    let rerun = Command::new(env!("CARGO_BIN_EXE_assistant-decision-edit"))
        .current_dir(root())
        .args(["--state"])
        .arg(&input)
        .args(["--output"])
        .arg(&output)
        .args(["--review-id", &review_id, "--decision", "ignore"])
        .output()
        .unwrap();
    assert!(!rerun.status.success());
    assert!(String::from_utf8_lossy(&rerun.stderr).contains("ASSISTANT_DECISION_OUTPUT_EXISTS"));
    assert_eq!(before, state_bytes(&input));

    fs::remove_dir_all(output).unwrap();
}

use cloud_monitor::{monitor::*, persistence::*};
use serde_json::{json, Value};
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, path::Path, process::Command};

fn historical_record(source: &str, author: &str, id: &str) -> Record {
    Record::new(
        source,
        id.into(),
        vec![author.into()],
        format!("Historical work {author} {id}"),
        json!({"content_type":"manga","finished":true}),
    )
}

fn add_historical(state: &mut State, source: &str, author: &str, id: &str) {
    let record = historical_record(source, author, id);
    let context = state.context();
    state.catalog.insert(
        key(&record),
        Entry {
            search_fingerprint: record.fingerprint.clone(),
            detail_fingerprint: record.fingerprint.clone(),
            analysis_context: context,
            record,
            author_evidence: AuthorEvidence::default(),
            work_id: None,
            analysis_count: 1,
            unavailable_streak: 0,
            active: true,
            last_unavailable_check: None,
            search_queries: Default::default(),
            matcher_version: rules_core::title_m2::RULE_VERSION.into(),
            identity_evidence: Value::Null,
            identity_provenance: Value::Null,
        },
    );
}

fn setup(root: &Path) -> Vec<String> {
    let authors = vec!["Writer0".to_string(), "Writer1".to_string()];
    let mut state = State {
        authors: json!({"authors":authors.iter().map(|name|json!({"name":name,"enabled":true})).collect::<Vec<_>>()}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    let recent_full = now();
    for author in &authors {
        for source in ["jm", "pica"] {
            // `monthly` deliberately falls back to a full scan when no recent full
            // coverage exists. Seed a recent full boundary so this regression tests
            // the actual incremental/early-stop path rather than that six-month guard.
            state
                .scan
                .last_full
                .insert(State::cursor_key(source, author), recent_full.clone());
            for index in 1..=5 {
                add_historical(&mut state, source, author, &format!("{source}-{author}-{index}"));
            }
        }
    }
    save(&root.join("seed"), &state).unwrap();
    write_json(&root.join("authors.json"), &authors).unwrap();
    authors
}

fn early_stop_observation(source: &str, author: &str) -> Value {
    let records: Vec<_> = (1..=5)
        .map(|index| historical_record(source, author, &format!("{source}-{author}-{index}")))
        .collect();
    let page = SearchPage {
        page: 1,
        reported_total: Some(6),
        reported_pages: None,
        reported_limit: Some(20),
        response_fields: vec![],
        record_fields: vec![],
        redirect_to_detail: false,
        records,
    };
    json!({
        "source":source,
        "author":author,
        "page":page,
        "details":{},
        "error":null
    })
}

fn invoke(
    root: &Path,
    input: &str,
    output: &str,
    tape: &str,
    batch_index: &str,
    continue_cycle: bool,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_phase3b"));
    command
        .args(["--state"])
        .arg(root.join(input))
        .args(["--output"])
        .arg(root.join(output))
        .args(["--authors"])
        .arg(root.join("authors.json"))
        .args(["--replay"])
        .arg(root.join(tape))
        .args([
            "--mode",
            "monthly",
            "--threshold",
            "5",
            "--batch-size",
            "1",
            "--batch-index",
            batch_index,
        ]);
    if continue_cycle {
        command.arg("--continue-cycle");
    }
    command.output().unwrap()
}

#[test]
fn incremental_early_stop_is_strategy_complete_and_allows_next_batch_without_full_coverage_claim() {
    let root = std::env::temp_dir().join(format!("phase3b-early-stop-flow-{}", now()));
    std::fs::create_dir_all(&root).unwrap();
    let authors = setup(&root);

    for (batch, author) in authors.iter().enumerate() {
        write_json(
            &root.join(format!("tape{batch}.json")),
            &json!({"observations":[
                early_stop_observation("jm", author),
                early_stop_observation("pica", author)
            ]}),
        )
        .unwrap();
    }

    let first = invoke(&root, "seed", "batch0", "tape0.json", "0", false);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_report: Value = serde_json::from_slice(
        &std::fs::read(root.join("batch0/scan-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(first_report["complete"], false);
    assert_eq!(first_report["coverage_complete"], false);
    assert_eq!(first_report["strategy_complete"], true);
    assert_eq!(
        first_report["boundaries"]["jm|Writer0"]["boundary"],
        "EARLY_STOP_HEURISTIC"
    );
    assert_eq!(
        first_report["boundaries"]["pica|Writer0"]["boundary"],
        "EARLY_STOP_HEURISTIC"
    );
    let first_manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("batch0/state-manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(first_manifest["complete"], false);
    assert_eq!(first_manifest["coverage_complete"], false);
    assert_eq!(first_manifest["strategy_complete"], true);

    let second = invoke(&root, "batch0", "batch1", "tape1.json", "1", true);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: Value = serde_json::from_slice(
        &std::fs::read(root.join("batch1/scan-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(second_report["selected_authors"], json!(["Writer1"]));
    assert_eq!(second_report["coverage_complete"], false);
    assert_eq!(second_report["strategy_complete"], true);
    assert_eq!(
        second_report["boundaries"]["jm|Writer1"]["boundary"],
        "EARLY_STOP_HEURISTIC"
    );
    assert_eq!(
        second_report["boundaries"]["pica|Writer1"]["boundary"],
        "EARLY_STOP_HEURISTIC"
    );
}

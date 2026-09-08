use cloud_monitor::{monitor::*, persistence::*};
use serde_json::{json, Value};
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, path::Path, process::Command};

fn setup(root: &Path) -> Vec<String> {
    let names: Vec<_> = (0..5).map(|i| format!("Writer{i}")).collect();
    let s = State {
        authors: json!({"authors":names.iter().map(|n|json!({"name":n,"enabled":true})).collect::<Vec<_>>()}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    save(&root.join("seed"), &s).unwrap();
    write_json(&root.join("authors.json"), &names).unwrap();
    names
}
fn observation(name: &str, index: usize, source: &str, page: u64, total: u64) -> Value {
    let id = if source == "jm" {
        format!("{index}{page}")
    } else {
        format!("{index:012x}{page:012x}")
    };
    let r = Record::new(
        source,
        id,
        vec![name.into()],
        format!("Long title {page}"),
        json!({}),
    );
    let p = SearchPage {
        page,
        reported_total: Some(total),
        reported_pages: Some(total),
        reported_limit: Some(1),
        response_fields: vec![],
        record_fields: vec![],
        records: vec![r.clone()],
        redirect_to_detail: false,
    };
    json!({"source":source,"author":name,"page":p,"details":{key(&r):r},"error":null})
}
fn invoke(root: &Path, input: &str, output: &str, tape: &str, extra: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_phase3a"))
        .args(["--state"])
        .arg(root.join(input))
        .arg("--output")
        .arg(root.join(output))
        .arg("--authors")
        .arg(root.join("authors.json"))
        .arg("--replay")
        .arg(root.join(tape))
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn five_author_cli_replay_is_idempotent_and_input_is_unchanged() {
    let root = std::env::temp_dir().join(format!("manga-cli-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root);
    let before = hash(&load(&root.join("seed")).unwrap());
    let mut observations = vec![];
    for (i, n) in names.iter().enumerate() {
        for src in ["jm", "pica"] {
            observations.push(observation(n, i, src, 1, 1));
        }
    }
    write_json(
        &root.join("tape.json"),
        &json!({"observations":observations}),
    )
    .unwrap();
    invoke(&root, "seed", "first", "tape.json", &[]);
    invoke(
        &root,
        "first",
        "second",
        "tape.json",
        &["--assert-idempotent"],
    );
    let second = load(&root.join("second")).unwrap();
    assert!(second.scan.complete);
    assert!(second.scan.events.is_empty());
    assert_eq!(before, hash(&load(&root.join("seed")).unwrap()));
}
#[test]
fn cli_resume_keeps_start_snapshot_and_completes_remaining_pages() {
    let root = std::env::temp_dir().join(format!("manga-resume-{}", hash(&now())));
    std::fs::create_dir_all(&root).unwrap();
    let names = setup(&root);
    for p in [1, 2] {
        let mut observations = vec![];
        for (i, n) in names.iter().enumerate() {
            for src in ["jm", "pica"] {
                observations.push(observation(n, i, src, p, 2));
            }
        }
        write_json(
            &root.join(format!("page{p}.json")),
            &json!({"observations":observations}),
        )
        .unwrap();
    }
    invoke(&root, "seed", "result", "page1.json", &[]);
    let first = load_checkpoint(&root.join("result")).unwrap();
    assert!(!first.scan.complete);
    assert_eq!(first.scan.progress["jm|Writer0"].next_page, 2);
    invoke(&root, "seed", "result", "page2.json", &["--resume"]);
    let second = load_checkpoint(&root.join("result")).unwrap();
    assert!(second.scan.complete);
    assert_eq!(first.scan.scan_id, second.scan.scan_id);
    assert!(second.scan.historical_ids.is_empty());
    assert_eq!(second.catalog.len(), 20);
}

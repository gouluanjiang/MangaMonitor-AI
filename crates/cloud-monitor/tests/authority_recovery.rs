use cloud_monitor::{authority_recovery, monitor::*, persistence::*, scope_certificates};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use state_model::{Record, SearchPage};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("authority-recovery-{label}-{}", hash(&now())));
    fs::create_dir_all(&root).unwrap();
    root
}

fn setup(root: &Path) {
    let mut state = State {
        authors: json!({"schema_version":1,"authors":[{"name":"Writer0","enabled":true},{"name":"Writer1","enabled":true}]}),
        inventory: json!({"schema_version":8,"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    for author in ["Writer0", "Writer1"] {
        for source in ["jm", "pica"] {
            state
                .scan
                .last_full
                .insert(State::cursor_key(source, author), now());
        }
    }
    save(&root.join("seed"), &state).unwrap();
    write_json(&root.join("empty.json"), &json!({"observations":[]})).unwrap();
}

fn record(author: &str, number: usize) -> Record {
    Record::new(
        "jm",
        format!("{author}-{number}"),
        vec![author.into()],
        format!("Distinct manga {author} {number}"),
        json!({"content_type":"manga","finished":true}),
    )
}

fn observation(author: &str, source: &str, page_number: u64) -> Value {
    let numbers = if page_number == 1 { 1..=5 } else { 6..=6 };
    let records: Vec<_> = if source == "jm" {
        numbers.map(|n| record(author, n)).collect()
    } else {
        vec![]
    };
    let details: BTreeMap<_, _> = records.iter().map(|r| (key(r), r.clone())).collect();
    let page = SearchPage {
        page: page_number,
        reported_total: Some(if source == "jm" { 6 } else { 0 }),
        reported_pages: Some(if source == "jm" { 2 } else { 1 }),
        reported_limit: Some(5),
        response_fields: vec![],
        record_fields: vec![],
        records,
        redirect_to_detail: false,
    };
    json!({"source":source,"author":author,"page":page,"details":details,"error":null})
}

fn tape(root: &Path, name: &str, author: &str, complete: bool) {
    let mut observations = vec![observation(author, "jm", 1), observation(author, "pica", 1)];
    if complete {
        observations.push(observation(author, "jm", 2));
    }
    write_json(&root.join(name), &json!({"observations":observations})).unwrap();
}

fn invoke(root: &Path, input: &str, output: &str, batch: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phase3b"))
        .arg("--state")
        .arg(root.join(input))
        .arg("--output")
        .arg(root.join(output))
        .arg("--authors")
        .arg(root.join(input).join("authors.json"))
        .args([
            "--mode",
            "monthly",
            "--batch-size",
            "1",
            "--batch-index",
            batch,
            "--threshold",
            "5",
            "--expected-base-sha",
            "fixture-current-base",
            "--actual-base-sha",
            "fixture-current-base",
        ])
        .args(extra)
        .current_dir(root)
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn read(root: &Path, path: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

fn hashes(path: &Path) -> BTreeMap<String, String> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                hash(&fs::read(entry.path()).unwrap()),
            )
        })
        .collect()
}

fn partial(root: &Path) {
    setup(root);
    tape(root, "partial.json", "Writer1", false);
    success(invoke(
        root,
        "seed",
        "partial",
        "1",
        &["--replay", "partial.json"],
    ));
    assert_eq!(
        read(root, "partial/state-manifest.json")["strategy_complete"],
        false
    );
    copy(&root.join("partial"), &root.join("current"));
}

fn fixture_task() -> Task {
    Task {
        task_id: "FIXTURE_TASK".into(),
        work_id: "FIXTURE_WORK".into(),
        first_seen: now(),
        task_revision: 7,
        target: Target {
            source_key: "jm:Writer1-1".into(),
            author: "Writer1".into(),
            title: "Fixture work".into(),
            version: state_model::Version::default(),
            coverage: Value::Null,
        },
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: vec![],
        binding_authority_hash: String::new(),
    }
}

fn legacy_checkpoint(root: &Path, directory: &str) {
    let checkpoint = load_checkpoint(&root.join(directory)).unwrap();
    let serialized = serde_json::to_string(&checkpoint).unwrap();
    let legacy = serialized
        .strip_suffix(",\"identity_authority_hash\":\"\"}}")
        .unwrap()
        .to_owned()
        + "}}";
    // Hash compact bytes in original struct field order. Hashing a parsed Value
    // would alphabetize object keys and would not reproduce the historical hash.
    let legacy_hash = format!("{:x}", Sha256::digest(legacy.as_bytes()));
    fs::write(root.join(directory).join("checkpoint.json"), legacy).unwrap();
    let mut manifest = read(root, &format!("{directory}/state-manifest.json"));
    manifest["state_hash"] = json!(legacy_hash);
    write_json(&root.join(directory).join("state-manifest.json"), &manifest).unwrap();
}

fn classify(root: &Path, extra: &[&str]) -> String {
    let mut args = vec!["--resume-preflight"];
    args.extend_from_slice(extra);
    success(invoke(root, "current", "partial", "1", &args))["classification"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn implicit_fresh_partial_is_rejected_for_live_and_replay_before_source_or_repair() {
    let root = root("implicit");
    partial(&root);
    for extra in [vec!["--live-source"], vec!["--replay", "empty.json"]] {
        let output = invoke(&root, "current", "rejected", "0", &extra);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("INCOMPLETE_SCAN_REQUIRES_RESUME_OR_FULL_RECOVERY"));
        assert!(!root.join("rejected/checkpoint.json").exists());
    }
}

#[test]
fn preflight_is_read_only_and_classifies_exact_options_and_corrupt_checkpoint() {
    let root = root("classify");
    partial(&root);
    let before_current = hashes(&root.join("current"));
    let before_checkpoint = hashes(&root.join("partial"));
    assert_eq!(classify(&root, &[]), "RESUMABLE_EXACT");
    assert_eq!(before_current, hashes(&root.join("current")));
    assert_eq!(before_checkpoint, hashes(&root.join("partial")));
    let result = success(invoke(
        &root,
        "current",
        "absent",
        "1",
        &[
            "--resume-preflight",
            "--live-source",
            "--repair-overlay",
            "missing.json",
        ],
    ));
    assert_eq!(result["classification"], "CHECKPOINT_CORRUPT");
    assert!(!root.join("absent").exists());
    let options = authority_recovery::Options {
        requested_mode: "monthly",
        threshold: 4,
        batch_size: 1,
        batch_index: 1,
        all_authors: &["Writer0".into(), "Writer1".into()],
    };
    assert_eq!(
        authority_recovery::preflight(&root.join("current"), &root.join("partial"), &options)
            .classification,
        authority_recovery::Classification::OptionsMismatch
    );
    let checkpoint_file = root.join("partial/checkpoint.json");
    let original = fs::read(&checkpoint_file).unwrap();
    let mut corrupt: Value = serde_json::from_slice(&original).unwrap();
    corrupt["decisions"]["ignored_works"] = json!(["tampered"]);
    write_json(&checkpoint_file, &corrupt).unwrap();
    assert_eq!(classify(&root, &[]), "CHECKPOINT_CORRUPT");
    let mut unknown: Value = serde_json::from_slice(&original).unwrap();
    unknown["scan"]["future_authority"] = json!(true);
    write_json(&checkpoint_file, &unknown).unwrap();
    assert_eq!(classify(&root, &[]), "CHECKPOINT_CORRUPT");
    fs::write(&checkpoint_file, &original).unwrap();
    let manifest_path = root.join("partial/state-manifest.json");
    let original_manifest = fs::read(&manifest_path).unwrap();
    let mut damaged_manifest: Value = serde_json::from_slice(&original_manifest).unwrap();
    damaged_manifest["batch_count"] = json!(3);
    write_json(&manifest_path, &damaged_manifest).unwrap();
    assert_eq!(classify(&root, &[]), "CHECKPOINT_CORRUPT");
    fs::write(&manifest_path, &original_manifest).unwrap();
    fs::write(checkpoint_file, original).unwrap();
    fs::remove_file(root.join("partial/state-manifest.json")).unwrap();
    assert_eq!(classify(&root, &[]), "CHECKPOINT_CORRUPT");
}

#[test]
fn preflight_detects_each_authority_surface_and_rejects_invalid_certificates() {
    for surface in ["decisions", "inventory", "authors", "certificates"] {
        let root = root(surface);
        partial(&root);
        match surface {
            "decisions" => {
                let mut doc = read(&root, "current/decisions.json");
                doc["ignored_source_records"] = json!(["jm:Writer1-1"]);
                write_json(&root.join("current/decisions.json"), &doc).unwrap();
            }
            "inventory" => {
                let mut doc = read(&root, "current/inventory_index.json");
                doc["works"] = json!([{"work_id":"CURRENT_OWNED","owned":true,"authors_confirmed":["Writer1"],"versions":[]}]);
                write_json(&root.join("current/inventory_index.json"), &doc).unwrap();
            }
            "authors" => {
                let mut doc = read(&root, "current/authors.json");
                doc["authors"][0]["enabled"] = json!(false);
                write_json(&root.join("current/authors.json"), &doc).unwrap();
            }
            "certificates" => {
                let state = load(&root.join("current")).unwrap();
                let snapshot = json!({"complete":true,"works":[]});
                let projection = json!([]);
                let evidence = hash(&"independent-fixture");
                let attestation = scope_certificates::CompletenessAttestation {
                    schema_version: 1,
                    snapshot_hash: hash(&snapshot),
                    inventory_hash: hash(&state.inventory),
                    projection_hash: hash(&projection),
                    projection_count: 0,
                    producer: "independent-fixture".into(),
                    producer_evidence_hash: evidence.clone(),
                    attestation_hash: hash(&(
                        1u64,
                        hash(&snapshot),
                        hash(&state.inventory),
                        hash(&projection),
                        0u64,
                        "independent-fixture",
                        evidence,
                    )),
                };
                let certificate = scope_certificates::certify_snapshot(
                    "Writer1",
                    &state.authors,
                    &state.inventory,
                    &snapshot,
                    &attestation,
                    rules_core::title_m2::RULE_VERSION,
                )
                .unwrap();
                let document = scope_certificates::ScopeCertificateDocument {
                    schema_version: 1,
                    certificate_set_hash: scope_certificates::certificate_set_hash(
                        std::slice::from_ref(&certificate),
                    ),
                    certificates: vec![certificate],
                };
                write_json(&root.join("current/scope-certificates.json"), &document).unwrap();
            }
            _ => unreachable!(),
        }
        let before = hashes(&root.join("current"));
        assert_eq!(
            classify(&root, &[]),
            "AUTHORITY_DRIFT_REQUIRES_FULL_RECOVERY",
            "{surface}"
        );
        assert_eq!(before, hashes(&root.join("current")));
        let mut certificates = read(&root, "current/scope-certificates.json");
        certificates["schema_version"] = json!(999);
        write_json(&root.join("current/scope-certificates.json"), &certificates).unwrap();
        assert_eq!(classify(&root, &[]), "CURRENT_STATE_INVALID");
    }
}

#[test]
fn full_recovery_restarts_batch_zero_reanalyzes_and_survives_partial_resume_then_monthly() {
    let root = root("generation");
    partial(&root);
    let prior_full = load_checkpoint(&root.join("partial"))
        .unwrap()
        .scan
        .last_full["jm|Writer1"]
        .clone();
    let original_checkpoint = fs::read(root.join("partial/checkpoint.json")).unwrap();
    let mut decisions = read(&root, "current/decisions.json");
    decisions["ignored_source_records"] = json!(["jm:Writer1-1"]);
    write_json(&root.join("current/decisions.json"), &decisions).unwrap();
    copy(&root.join("current"), &root.join("recovery0"));
    tape(&root, "writer0.json", "Writer0", true);
    success(invoke(
        &root,
        "current",
        "recovery0",
        "0",
        &[
            "--replay",
            "writer0.json",
            "--recover-authority-drift-full",
            "--recovery-from-batch-index",
            "1",
        ],
    ));
    let manifest = read(&root, "recovery0/state-manifest.json");
    assert_eq!(manifest["recovery"]["kind"], "AUTHORITY_DRIFT_FULL");
    assert_eq!(
        manifest["recovery"]["evidence"][0]["state_hash"],
        hash(&load_checkpoint(&root.join("partial")).unwrap())
    );
    assert_eq!(manifest["batch_index"], 0);
    assert_eq!(manifest["effective_requested_mode"], "full");
    let recovered = load_checkpoint(&root.join("recovery0")).unwrap();
    assert_eq!(
        recovered.decisions.ignored_source_records,
        vec!["jm:Writer1-1"]
    );
    assert!(recovered.catalog["jm:Writer1-1"].analysis_count > 1);
    assert_eq!(
        original_checkpoint,
        fs::read(root.join("partial/checkpoint.json")).unwrap()
    );
    copy(&root.join("recovery0"), &root.join("forged"));
    let mut forged = manifest.clone();
    forged["recovery"] = Value::Null;
    write_json(&root.join("forged/state-manifest.json"), &forged).unwrap();
    assert!(!invoke(
        &root,
        "forged",
        "forged-attempt",
        "0",
        &["--replay", "empty.json"]
    )
    .status
    .success());
    success(invoke(
        &root,
        "recovery0",
        "recovery1",
        "1",
        &["--replay", "partial.json", "--continue-cycle"],
    ));
    let partial_recovery = load_checkpoint(&root.join("recovery1")).unwrap();
    assert!(!partial_recovery.scan.complete);
    assert_ne!(
        partial_recovery.scan.progress["jm|Writer1"].boundary,
        "EARLY_STOP_HEURISTIC"
    );
    assert_eq!(partial_recovery.scan.progress["jm|Writer1"].next_page, 2);
    assert_eq!(partial_recovery.scan.last_full["jm|Writer1"], prior_full);
    assert_eq!(
        read(&root, "recovery1/state-manifest.json")["recovery"],
        manifest["recovery"]
    );
    let mut forged_partial = read(&root, "recovery1/state-manifest.json");
    forged_partial["strategy_complete"] = json!(true);
    copy(&root.join("recovery1"), &root.join("forged-partial"));
    write_json(
        &root.join("forged-partial/state-manifest.json"),
        &forged_partial,
    )
    .unwrap();
    assert!(authority_recovery::bound_checkpoint(&root.join("forged-partial")).is_err());
    copy(&root.join("recovery1"), &root.join("resumed"));
    write_json(
        &root.join("page2.json"),
        &json!({"observations":[observation("Writer1","jm",2)]}),
    )
    .unwrap();
    success(invoke(
        &root,
        "recovery1",
        "resumed",
        "1",
        &["--replay", "page2.json", "--resume"],
    ));
    let final_state = load_checkpoint(&root.join("resumed")).unwrap();
    assert!(final_state.scan.complete);
    assert!(final_state.catalog.contains_key("jm:Writer1-6"));
    assert_eq!(
        final_state.scan.last_full["jm|Writer1"],
        partial_recovery.scan.started_at
    );
    assert_eq!(
        read(&root, "resumed/state-manifest.json")["recovery"],
        manifest["recovery"]
    );
    assert_eq!(final_state.pending.len(), recovered.pending.len());
    success(invoke(
        &root,
        "resumed",
        "monthly",
        "0",
        &["--replay", "writer0.json"],
    ));
    let normal = read(&root, "monthly/state-manifest.json");
    assert!(normal["recovery"].is_null());
    assert_eq!(normal["effective_requested_mode"], "incremental");
    assert_eq!(normal["strategy_complete"], true);
}

#[test]
fn exact_resume_preserves_current_completed_task_export_in_the_same_scan() {
    let root = root("current-completion");
    partial(&root);
    let mut checkpoint = load_checkpoint(&root.join("partial")).unwrap();
    let task = fixture_task();
    checkpoint.pending.insert(task.work_id.clone(), task);
    let mut manifest = read(&root, "partial/state-manifest.json");
    manifest["state_hash"] = json!(hash(&checkpoint));
    save(&root.join("partial"), &checkpoint).unwrap();
    write_json(&root.join("partial/state-manifest.json"), &manifest).unwrap();
    copy(&root.join("partial"), &root.join("current"));
    // Model a completed exact task revision that has already been published;
    // public scan progress and identity authority are unchanged by completion.
    let mut pending = read(&root, "current/pending.json");
    pending["tasks"][0]["status"] = json!("completed");
    write_json(&root.join("current/pending.json"), &pending).unwrap();
    assert_eq!(
        load(&root.join("current")).unwrap().context(),
        checkpoint.context()
    );
    assert_eq!(classify(&root, &[]), "RESUMABLE_EXACT");
    let current_before = hashes(&root.join("current"));
    copy(&root.join("partial"), &root.join("resumed-current"));
    success(invoke(
        &root,
        "current",
        "resumed-current",
        "1",
        &["--resume", "--replay", "empty.json"],
    ));
    let resumed = load_checkpoint(&root.join("resumed-current")).unwrap();
    assert_eq!(resumed.pending["FIXTURE_WORK"].status, "completed");
    assert_eq!(resumed.pending["FIXTURE_WORK"].task_revision, 7);
    assert_eq!(
        hash(&resumed.pending),
        hash(&load(&root.join("current")).unwrap().pending)
    );
    assert_eq!(current_before, hashes(&root.join("current")));
    assert_eq!(
        load_checkpoint(&root.join("partial")).unwrap().pending["FIXTURE_WORK"].status,
        "pending"
    );
}

#[test]
fn unpublished_partial_refuses_changed_original_business_exports_without_reviving_task() {
    let root = root("unpublished-completion");
    setup(&root);
    let mut seed = load(&root.join("seed")).unwrap();
    let task = fixture_task();
    seed.pending.insert(task.work_id.clone(), task);
    save(&root.join("seed"), &seed).unwrap();
    tape(&root, "partial.json", "Writer1", false);
    success(invoke(
        &root,
        "seed",
        "staged",
        "1",
        &["--replay", "partial.json"],
    ));
    let before = hashes(&root.join("staged"));
    let mut pending = read(&root, "seed/pending.json");
    pending["tasks"][0]["status"] = json!("completed");
    write_json(&root.join("seed/pending.json"), &pending).unwrap();
    let preflight = success(invoke(
        &root,
        "seed",
        "staged",
        "1",
        &["--resume-preflight"],
    ));
    assert_eq!(preflight["classification"], "CURRENT_STATE_INVALID");
    assert_eq!(preflight["reason"], "RESUME_INPUT_BUSINESS_STATE_MISMATCH");
    let output = invoke(
        &root,
        "seed",
        "staged",
        "1",
        &["--resume", "--replay", "empty.json"],
    );
    assert!(!output.status.success());
    assert_eq!(before, hashes(&root.join("staged")));
    assert_eq!(
        load(&root.join("seed")).unwrap().pending["FIXTURE_WORK"].status,
        "completed"
    );
    let mut manifest = read(&root, "staged/state-manifest.json");
    manifest.as_object_mut().unwrap().remove("input_state_hash");
    write_json(&root.join("staged/state-manifest.json"), &manifest).unwrap();
    let legacy = success(invoke(
        &root,
        "seed",
        "staged",
        "1",
        &["--resume-preflight"],
    ));
    assert_eq!(legacy["reason"], "LEGACY_RESUME_INPUT_BUSINESS_UNBOUND");
}

#[test]
fn tracked_legacy_complete_checkpoint_hash_allows_next_offline_cycle() {
    let root = root("tracked-legacy");
    let tracked = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../monitor-state");
    let before = hashes(&tracked);
    copy(&tracked, &root.join("legacy"));
    let (checkpoint, manifest) =
        authority_recovery::bound_checkpoint(&root.join("legacy")).unwrap();
    assert!(checkpoint.scan.complete && manifest.strategy_complete);
    write_json(&root.join("empty.json"), &json!({"observations":[]})).unwrap();
    success(invoke(
        &root,
        "legacy",
        "next",
        "0",
        &["--replay", "empty.json"],
    ));
    assert_eq!(read(&root, "next/scan-report.json")["requests"], 0);
    assert_eq!(hashes(&tracked), before);
}

#[test]
fn legacy_matcher_epoch_requires_full_reanalysis_and_empty_unbound_epoch_is_rejected() {
    let root = root("rule-epoch");
    partial(&root);
    let mut old = load_checkpoint(&root.join("partial")).unwrap();
    for entry in old.catalog.values_mut() {
        entry.matcher_version = "legacy-matcher-v1".into();
        entry.analysis_context = hash(&"legacy-context");
        entry.work_id = Some("OBSOLETE_AUTOMATIC_BINDING".into());
    }
    let mut manifest = read(&root, "partial/state-manifest.json");
    manifest["state_hash"] = json!(hash(&old));
    manifest.as_object_mut().unwrap().remove("matcher_version");
    manifest.as_object_mut().unwrap().remove("analysis_context");
    save(&root.join("partial"), &old).unwrap();
    write_json(&root.join("partial/state-manifest.json"), &manifest).unwrap();
    legacy_checkpoint(&root, "partial");
    copy(&root.join("partial"), &root.join("current"));
    assert_eq!(
        classify(&root, &[]),
        "AUTHORITY_DRIFT_REQUIRES_FULL_RECOVERY"
    );
    copy(&root.join("current"), &root.join("recovered"));
    success(invoke(
        &root,
        "current",
        "recovered",
        "0",
        &[
            "--replay",
            "empty.json",
            "--recover-authority-drift-full",
            "--recovery-from-batch-index",
            "1",
        ],
    ));
    let recovered = load_checkpoint(&root.join("recovered")).unwrap();
    assert!(recovered.catalog.values().all(|entry| entry.matcher_version
        == rules_core::title_m2::RULE_VERSION
        && entry.analysis_context == recovered.context()));
    assert!(recovered
        .catalog
        .values()
        .all(|entry| entry.work_id.as_deref() != Some("OBSOLETE_AUTOMATIC_BINDING")));
    old.catalog.clear();
    manifest["state_hash"] = json!(hash(&old));
    save(&root.join("partial"), &old).unwrap();
    write_json(&root.join("partial/state-manifest.json"), &manifest).unwrap();
    legacy_checkpoint(&root, "partial");
    copy(&root.join("partial"), &root.join("current"));
    assert_eq!(classify(&root, &[]), "CHECKPOINT_CORRUPT");
}

#[test]
fn recovery_cannot_bypass_exact_resume_options_or_complete_state() {
    let root = root("no-bypass");
    partial(&root);
    copy(&root.join("current"), &root.join("attempt"));
    let output = invoke(
        &root,
        "current",
        "attempt",
        "0",
        &[
            "--replay",
            "empty.json",
            "--recover-authority-drift-full",
            "--recovery-from-batch-index",
            "1",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ResumableExact"));
    tape(&root, "full.json", "Writer1", true);
    success(invoke(
        &root,
        "seed",
        "complete",
        "1",
        &["--replay", "full.json"],
    ));
    copy(&root.join("complete"), &root.join("complete-attempt"));
    assert!(!invoke(
        &root,
        "complete",
        "complete-attempt",
        "0",
        &[
            "--replay",
            "empty.json",
            "--recover-authority-drift-full",
            "--recovery-from-batch-index",
            "1"
        ]
    )
    .status
    .success());
    let mut decisions = read(&root, "current/decisions.json");
    decisions["ignored_works"] = json!(["changed"]);
    write_json(&root.join("current/decisions.json"), &decisions).unwrap();
    let options = authority_recovery::Options {
        requested_mode: "full",
        threshold: 5,
        batch_size: 1,
        batch_index: 1,
        all_authors: &["Writer0".into(), "Writer1".into()],
    };
    assert_eq!(
        authority_recovery::preflight(&root.join("current"), &root.join("partial"), &options)
            .classification,
        authority_recovery::Classification::OptionsMismatch
    );
}

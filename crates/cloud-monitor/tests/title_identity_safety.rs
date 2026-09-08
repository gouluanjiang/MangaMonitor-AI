//! Offline regressions through the production State entry points. No adapter,
//! media execution, or business-state fixture is involved.
use cloud_monitor::{
    matcher_m2::ScopeCertificate,
    monitor::{hash, key, Decisions, Mapping, Scan, State},
    scope_certificates::{self, ScopeCertificateDocument, ScopeCertificateRecord},
};
use rules_core::title_m2;
use serde_json::json;
use state_model::Record;
use std::collections::BTreeMap;

fn state(local_title: &str) -> State {
    State {
        authors: json!({"authors": [{"name": "Writer", "enabled": true}]}),
        inventory: json!({"works": [{
            "work_id": "W1", "owned": false,
            "authors_confirmed": ["Writer"], "local_item_ids": ["L1"],
            "title_candidates": [{"primary": local_title, "fandom_or_source": null}],
            "versions": [{"local_item_id": "L1", "content": {"type": "manga"}}],
            "source_mappings": {"jm": [], "pica": []}
        }]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}

fn record(title: &str) -> Record {
    Record::new(
        "jm",
        "title-safety".into(),
        vec!["Writer".into()],
        title.into(),
        json!({"content_type": "manga"}),
    )
}

fn begin(state: &mut State, scan: &str) {
    state
        .begin(
            scan,
            "2026-09-08T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
}

fn analyze(mut state: State, title: &str) -> State {
    let record = record(title);
    begin(&mut state, "title-safety");
    state.accept(&record, &record).unwrap();
    state
}

fn assert_review(state: &State) {
    let entry = &state.catalog["jm:title-safety"];
    assert_eq!(entry.identity_evidence["disposition"], "REVIEW_REQUIRED");
    assert_eq!(entry.work_id, None);
    assert_eq!(entry.record.processing_result, "REVIEW_REQUIRED");
    assert!(state.pending.values().all(|task| task.status != "pending"));
    assert_eq!(
        state
            .review
            .values()
            .filter(|r| r.status == "REVIEW_REQUIRED")
            .count(),
        1
    );
}

#[test]
fn unsafe_casefold_spellings_do_not_authorize_existing_or_new_work() {
    for (local, source) in [
        ("Maße", "Masse"),
        ("Straße", "Strasse"),
        ("Αθήνα ᾀ", "Αθήνα ἀι"),
        ("İstanbul", "i\u{307}stanbul"),
        ("Nexus \u{345}", "Nexus ι"),
    ] {
        // These were equal under the previous full-fold identity normalizer.
        assert_eq!(
            rules_core::conservative_title(local),
            rules_core::conservative_title(source)
        );
        for (local, source) in [(local, source), (source, local)] {
            let mut s = state(local);
            // Even independent complete scope cannot turn a spelling collision
            // into proof of a new installment.
            s.scan.scope_certificates.push(ScopeCertificate {
                author: "Writer".into(),
                inventory_hash: hash(&s.inventory),
                reference: "offline:independent-complete-scope".into(),
            });
            assert_review(&analyze(s, source));
        }
    }
}

#[test]
fn structural_attachment_is_not_an_unordered_bag_of_fields() {
    for marker in [
        "Extra",
        "Bonus",
        "After Story",
        "Collection",
        "総集編",
        "おまけ",
    ] {
        for number in [1, 2, 31, 999] {
            let left = format!("Cosmic Voyage {number} {marker}");
            let right = format!("Cosmic Voyage {marker} {number}");
            for (local, source) in [(&left, &right), (&right, &left)] {
                let s = analyze(state(local), source);
                assert_review(&s);
                let evidence = &s.catalog["jm:title-safety"].identity_evidence;
                assert_eq!(evidence["reason"], "STRUCTURAL_ATTACHMENT_UNRESOLVED");
                assert_eq!(
                    evidence["candidate_evidence"][0]["title_witness_without_type_authorization"],
                    false
                );
                assert_eq!(
                    evidence["candidate_evidence"][0]["relation"],
                    "Insufficient"
                );
            }
        }
    }
    for (local, source) in [
        (
            "Cosmic Voyage Part 2 Chapter 3",
            "Cosmic Voyage Chapter 3 Part 2",
        ),
        (
            "Cosmic Voyage Volume 2 Extra",
            "Cosmic Voyage Extra Volume 2",
        ),
    ] {
        assert_review(&analyze(state(local), source));
    }
}

#[test]
fn attachment_ambiguity_cannot_prove_a_disjoint_installment() {
    let mut s = state("Cosmic Voyage Part 2 Chapter 3");
    s.scan.scope_certificates.push(ScopeCertificate {
        author: "Writer".into(),
        inventory_hash: hash(&s.inventory),
        reference: "offline:independent-complete-scope".into(),
    });
    assert_review(&analyze(s, "Cosmic Voyage Chapter 4 Part 2"));
}

#[test]
fn ambiguous_attachment_beside_an_exact_candidate_requires_independent_exclusion() {
    let mut s = state("Cosmic Voyage 2 Extra");
    let mut exact_work = s.inventory["works"][0].clone();
    exact_work["work_id"] = json!("W2");
    exact_work["title_candidates"][0]["primary"] = json!("Cosmic Voyage Extra 2");
    s.inventory["works"]
        .as_array_mut()
        .unwrap()
        .push(exact_work);
    let mut s = analyze(s, "Cosmic Voyage Extra 2");
    assert_review(&s);
    assert_eq!(
        s.catalog["jm:title-safety"].identity_evidence["reason"],
        "STRUCTURAL_ATTACHMENT_UNRESOLVED"
    );
    s.decisions.negative_mappings.push(Mapping {
        source_key: "jm:title-safety".into(),
        work_id: "W1".into(),
    });
    begin(&mut s, "independent-not-same");
    assert_eq!(
        s.catalog["jm:title-safety"].identity_evidence["disposition"],
        "AUTO_EXISTING"
    );
    assert_eq!(s.catalog["jm:title-safety"].work_id.as_deref(), Some("W2"));
}

#[test]
fn an_explicit_different_type_candidate_does_not_hide_the_unique_exact_work() {
    for local_type in [json!("cg"), json!(null), json!("unsupported")] {
        let mut s = state("Cosmic Voyage 2 Extra");
        s.inventory["works"][0]["versions"][0]["content"]["type"] = local_type.clone();
        let mut exact_work = state("Cosmic Voyage Extra 2").inventory["works"][0].clone();
        exact_work["work_id"] = json!("W2");
        s.inventory["works"]
            .as_array_mut()
            .unwrap()
            .push(exact_work);
        let s = analyze(s, "Cosmic Voyage Extra 2");
        if local_type == "cg" {
            let entry = &s.catalog["jm:title-safety"];
            assert_eq!(entry.identity_evidence["disposition"], "AUTO_EXISTING");
            assert_eq!(entry.work_id.as_deref(), Some("W2"));
            assert_eq!(
                entry.identity_evidence["candidate_evidence"][0]["relation"],
                "StructuralConflict"
            );
        } else {
            assert_review(&s);
        }
    }
}

#[test]
fn an_excluded_candidate_cannot_pollute_local_issues_or_missing_primary() {
    for titles in [
        json!([{"primary": "Cosmic Voyage 2 Extra Extra", "fandom_or_source": null}]),
        json!([{"primary": null, "fandom_or_source": null}]),
        json!([{"primary": "", "fandom_or_source": null}]),
        json!([]),
    ] {
        let mut s = state("Cosmic Voyage 2 Extra");
        s.inventory["works"][0]["title_candidates"] = titles.clone();
        let mut exact_work = state("Cosmic Voyage Extra 2").inventory["works"][0].clone();
        exact_work["work_id"] = json!("W2");
        s.inventory["works"]
            .as_array_mut()
            .unwrap()
            .push(exact_work);
        let mut s = analyze(s, "Cosmic Voyage Extra 2");
        assert_review(&s);
        s.decisions.negative_mappings.push(Mapping {
            source_key: "jm:title-safety".into(),
            work_id: "W1".into(),
        });
        begin(&mut s, "independent-not-same");
        assert_eq!(
            s.catalog["jm:title-safety"].identity_evidence["disposition"], "AUTO_EXISTING",
            "{titles}"
        );
        assert_eq!(s.catalog["jm:title-safety"].work_id.as_deref(), Some("W2"));
    }
}

#[test]
fn exact_titles_nfkc_simple_casing_and_order_preserving_aliases_still_bind() {
    for (local, source) in [
        ("Maße", "Maße"),
        ("Straße", "STRAẞE"),
        ("Μάιος", "ΜΆΙΟΣ"),
        ("Nexus \u{345}", "Nexus \u{345}"),
        ("ＡＢＣＤ Part ３", "abcd part 3"),
        ("Café Voyage", "Cafe\u{301} Voyage"),
        ("Cosmic Voyage 2 Extra", "Cosmic Voyage 2 Extra"),
        ("Cosmic Voyage Extra 2", "Cosmic Voyage Extra 2"),
        ("Cosmic Voyage 2 Extra", "COSMIC VOYAGE 2 [Extra]"),
        ("Cosmic Voyage 第2卷 Extra", "Cosmic Voyage Volume 2 Extra"),
        ("Cosmic Voyage 2", "[Writer] Cosmic Voyage 2 [DL版]"),
    ] {
        let s = analyze(state(local), source);
        let entry = &s.catalog["jm:title-safety"];
        assert_eq!(
            entry.identity_evidence["disposition"], "AUTO_EXISTING",
            "{local} / {source}"
        );
        assert_eq!(entry.work_id.as_deref(), Some("W1"));
        assert_eq!(s.pending["W1"].status, "pending");
    }
}

#[test]
fn explicit_same_or_source_mapping_still_resolves_the_ambiguous_titles() {
    for (local, source) in [
        ("Maße", "Masse"),
        ("Cosmic Voyage 2 Extra", "Cosmic Voyage Extra 2"),
    ] {
        for human in [true, false] {
            let mut s = state(local);
            if human {
                s.decisions.positive_mappings.push(Mapping {
                    source_key: "jm:title-safety".into(),
                    work_id: "W1".into(),
                });
            } else {
                s.inventory["works"][0]["source_mappings"]["jm"] = json!(["title-safety"]);
            }
            let s = analyze(s, source);
            let entry = &s.catalog["jm:title-safety"];
            assert_eq!(
                entry.identity_evidence["disposition"],
                "AUTHORITATIVE_EXISTING"
            );
            assert_eq!(
                entry.identity_evidence["reason"],
                if human {
                    "HUMAN_SAME"
                } else {
                    "SAME_SITE_SOURCE_ID"
                }
            );
            assert_eq!(entry.work_id.as_deref(), Some("W1"));
        }
    }
}

#[test]
fn prior_automatic_bindings_and_tasks_are_reanalyzed_once_without_source_io() {
    for (local, source) in [
        ("Maße", "Masse"),
        ("Cosmic Voyage 2 Extra", "Cosmic Voyage Extra 2"),
    ] {
        // Seed a real pending task, then represent the legacy v1 assignment and
        // its original context. This avoids copying the old unsafe matcher into
        // the test or treating a cached AUTO_EXISTING result as authority.
        let mut s = analyze(state(source), source);
        s.inventory = state(local).inventory;
        let legacy_context = hash(&("matcher-m2-v1", &s.decisions, &s.inventory, &s.authors));
        assert_ne!(s.context(), legacy_context);
        let r = record(source);
        let entry = s.catalog.get_mut(&key(&r)).unwrap();
        entry.analysis_context = legacy_context;
        entry.matcher_version = "matcher-m2-v1".into();
        entry.identity_evidence["source_identity"]["rule_version"] = json!("matcher-m2-v1");
        let previous_count = entry.analysis_count;
        let previous_revision = s.pending["W1"].task_revision;
        assert_eq!(s.pending["W1"].status, "pending");

        begin(&mut s, "migrate-v2");
        assert_review(&s);
        let entry = &s.catalog[&key(&r)];
        assert_eq!(entry.analysis_count, previous_count + 1);
        assert_eq!(entry.matcher_version, title_m2::RULE_VERSION);
        assert_eq!(
            entry.identity_provenance["trigger"],
            "ANALYSIS_CONTEXT_MIGRATION"
        );
        assert_eq!(s.pending["W1"].status, "superseded_by_identity_reanalysis");
        assert_eq!(s.pending["W1"].task_revision, previous_revision);
        let business_hash = hash(&(&s.catalog, &s.pending, &s.review));

        begin(&mut s, "repeat-v2");
        assert_eq!(hash(&(&s.catalog, &s.pending, &s.review)), business_hash);
        assert!(s.scan.events.is_empty());
        assert!(!s.needs_detail(&r));
    }
}

#[test]
fn v1_scope_certificates_and_parsed_identity_are_stale() {
    let s = state("Cosmic Voyage 2");
    let mut record = ScopeCertificateRecord {
        author: "Writer".into(),
        inventory_hash: hash(&s.inventory),
        complete_inventory_snapshot_hash: hash(&"snapshot"),
        author_scope_projection_hash: hash(&"projection"),
        author_scope_projection_count: 1,
        completeness_attestation_hash: hash(&"attestation"),
        rule_version: "matcher-m2-v1".into(),
        reference_hash: String::new(),
    };
    record.reference_hash = hash(&(
        &record.author,
        &record.inventory_hash,
        &record.complete_inventory_snapshot_hash,
        &record.author_scope_projection_hash,
        record.author_scope_projection_count,
        &record.completeness_attestation_hash,
        &record.rule_version,
    ));
    let document = ScopeCertificateDocument {
        schema_version: scope_certificates::SCHEMA_VERSION,
        certificate_set_hash: scope_certificates::certificate_set_hash(std::slice::from_ref(
            &record,
        )),
        certificates: vec![record],
    };
    assert_eq!(
        scope_certificates::validate_document(document, &s.authors, &s.inventory).unwrap_err(),
        "SCOPE_CERTIFICATE_RULE_VERSION_STALE"
    );
    let current = title_m2::parse(Some("Cosmic Voyage 2"), None, None, Some("manga"));
    let mut old = current.clone();
    old.rule_version = "matcher-m2-v1".into();
    assert_eq!(
        title_m2::compare(&old, &current),
        title_m2::Relation::Insufficient
    );
}

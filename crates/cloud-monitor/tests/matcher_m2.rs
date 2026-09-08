use cloud_monitor::{matcher_m2::*, monitor::*, persistence::*};
use serde_json::{json, Value};
use state_model::Record;
use std::collections::BTreeMap;

#[test]
fn unsupported_explicit_type_is_not_overridden_by_title_annotation() {
    let s = state(vec![work("W1", json!("作品标题"))]);
    for value in [
        json!("unsupported"),
        json!(["manga", null]),
        json!({"kind":"manga"}),
    ] {
        let mut r = record("作品标题 [manga]");
        r.metadata["content_type"] = value;
        assert_eq!(decide(&s, &r, &[]).reason, "CONTENT_TYPE_CONFLICT");
    }
}
#[test]
fn unknown_fandom_can_hide_a_collision_and_prevents_binding() {
    let mut w = work("W1", json!("作品标题"));
    w["title_candidates"][0]["fandom_or_source"] = json!("原神");
    let s = state(vec![w, work("W2", json!("作品标题"))]);
    assert_eq!(
        decide(&s, &record("作品标题 (原神)"), &[]).reason,
        "LOCAL_IDENTITY_COLLISION_UNRESOLVED"
    );
}
#[test]
fn ignore_keeps_existing_pending_task_inactive() {
    let mut w = work("W1", json!("作品标题"));
    w["owned"] = json!(false);
    let mut s = state(vec![w]);
    let mut audit = ReplayAudit::default();
    let r = record("作品标题");
    replay(&mut s, std::slice::from_ref(&r), &mut audit, &[]);
    assert_eq!(s.pending["W1"].status, "pending");
    s.decisions.ignored_source_records.push("jm:123".into());
    replay(&mut s, &[r], &mut audit, &[]);
    assert_eq!(s.pending["W1"].status, "ignored");
}
fn work(id: &str, title: Value) -> Value {
    json!({"work_id":id,"owned":true,"authors_confirmed":["Writer"],"local_item_ids":[format!("LOCAL_{id}")],"title_candidates":[{"primary":title,"fandom_or_source":null}],"versions":[{"local_item_id":format!("LOCAL_{id}"),"content":{"type":"manga"}}],"source_mappings":{"jm":[],"pica":[]}})
}
fn state(works: Vec<Value>) -> State {
    State {
        authors: json!({"authors":[{"name":"Writer","enabled":true}]}),
        inventory: json!({"works":works}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    }
}
fn record(title: &str) -> Record {
    let mut r = Record::new(
        "jm",
        "123".into(),
        vec!["Writer".into()],
        title.into(),
        json!({"content_type":"manga"}),
    );
    r.first_seen = "fixed".into();
    r.last_seen = "fixed".into();
    r.last_checked = "fixed".into();
    r
}
fn cert(s: &State) -> ScopeCertificate {
    ScopeCertificate {
        author: "Writer".into(),
        inventory_hash: hash(&s.inventory),
        reference: "test:independently_confirmed_complete_author_inventory".into(),
    }
}
#[test]
fn unique_existing_has_full_evidence_chain_and_keeps_unknown_version_review() {
    let mut s = state(vec![
        work("W1", json!("作品标题 2")),
        work("W2", json!("另一个作品标题")),
    ]);
    let r = record("作品标题 2");
    let o = decide(&s, &r, &[]);
    assert_eq!(o.disposition, "AUTO_EXISTING");
    assert_eq!(o.work_id.as_deref(), Some("W1"));
    assert_eq!(o.matching_work_ids, ["W1"]);
    assert_eq!(o.author_evidence.rule, "DIRECT_EXACT");
    assert_eq!(o.candidate_evidence.len(), 2);
    assert!(!o.content_type_evidence["traces"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut audit = ReplayAudit::default();
    assert_eq!(replay(&mut s, &[r], &mut audit, &[]), 1);
    assert_eq!(s.catalog["jm:123"].work_id.as_deref(), Some("W1"));
    assert!(s
        .review
        .values()
        .any(|v| v.reason == "UNKNOWN_LOCAL_VERSION"));
    assert!(s.pending.is_empty());
}
#[test]
fn same_author_different_title_is_not_new_proof() {
    let s = state(vec![work("W1", json!("本地作品标题"))]);
    let o = decide(&s, &record("完全不同标题"), &[cert(&s)]);
    assert_eq!(o.disposition, "REVIEW_REQUIRED");
    assert!(o.matching_work_ids.is_empty());
    assert_eq!(o.scope_work_ids, ["W1"]);
}
#[test]
fn complete_pinned_scope_and_disjoint_installment_can_prove_new() {
    let s = state(vec![work("W1", json!("作品标题 2"))]);
    let o = decide(&s, &record("作品标题 3"), &[cert(&s)]);
    assert_eq!(o.disposition, "PROVEN_NEW");
    assert_eq!(o.reason, "COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT");
}
#[test]
fn incomplete_scope_and_empty_scope_never_prove_new() {
    let s = state(vec![work("W1", json!("作品标题 2"))]);
    assert_eq!(
        decide(&s, &record("作品标题 3"), &[]).disposition,
        "REVIEW_REQUIRED"
    );
    let s = state(vec![]);
    assert_eq!(
        decide(&s, &record("作品标题 3"), &[cert(&s)]).disposition,
        "REVIEW_REQUIRED"
    );
}
#[test]
fn stale_scope_certificate_is_rejected() {
    let mut s = state(vec![work("W1", json!("作品标题 2"))]);
    let c = cert(&s);
    s.inventory["revision"] = json!(2);
    assert_ne!(
        decide(&s, &record("作品标题 3"), &[c]).disposition,
        "PROVEN_NEW"
    );
}
#[test]
fn zero_spelling_range_and_extra_do_not_prove_new() {
    for (a, b) in [
        ("作品标题 03", "作品标题 3"),
        ("作品标题 第1–5話", "作品标题 第3話"),
        ("作品标题 Extra", "作品标题"),
        ("作品标题 前篇", "作品标题 后篇"),
    ] {
        let s = state(vec![work("W1", json!(a))]);
        assert_ne!(
            decide(&s, &record(b), &[cert(&s)]).disposition,
            "PROVEN_NEW"
        );
    }
}
#[test]
fn duplicate_exact_candidates_are_review() {
    let s = state(vec![
        work("W1", json!("作品标题")),
        work("W2", json!("作品标题")),
    ]);
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "AMBIGUOUS_EXISTING_IDENTITY"
    );
}
#[test]
fn null_primary_cannot_hide_behind_an_exact_candidate() {
    let s = state(vec![work("W1", json!("作品标题")), work("W2", Value::Null)]);
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "LOCAL_PRIMARY_MISSING"
    );
}
#[test]
fn unknown_local_type_cannot_hide_a_collision() {
    let mut w = work("W2", json!("作品标题"));
    w["versions"][0]["content"]["type"] = Value::Null;
    let s = state(vec![work("W1", json!("作品标题")), w]);
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "LOCAL_IDENTITY_COLLISION_UNRESOLVED"
    );
}
#[test]
fn unknown_source_type_and_categories_do_not_default_to_manga() {
    let s = state(vec![work("W1", json!("作品标题"))]);
    let mut r = record("作品标题");
    r.metadata = json!({"categories":["短篇","同人"],"epsCount":1,"pagesCount":50});
    let o = decide(&s, &r, &[]);
    assert_eq!(o.reason, "TITLE_WITNESS_UNKNOWN_SOURCE_CONTENT_TYPE");
    assert_eq!(o.source_identity.fields.content_type, None);
}
#[test]
fn explicit_type_conflict_and_different_type_never_auto_bind() {
    let s = state(vec![work("W1", json!("作品标题"))]);
    for ty in ["cg", "artbook", "novel", "settings"] {
        let mut r = record("作品标题");
        r.metadata = json!({"content_type":ty});
        assert_ne!(decide(&s, &r, &[]).disposition, "AUTO_EXISTING");
    }
    let mut r = record("作品标题");
    r.metadata["tags"] = json!(["CG"]);
    assert_eq!(decide(&s, &r, &[]).reason, "CONTENT_TYPE_CONFLICT");
}
#[test]
fn human_same_and_same_site_ids_remain_authoritative() {
    let mut s = state(vec![work("W1", Value::Null)]);
    let mut r = record("[broken");
    r.author.clear();
    r.metadata = json!({});
    s.decisions.positive_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W1".into(),
    });
    assert_eq!(decide(&s, &r, &[]).reason, "HUMAN_SAME");
    s.decisions.positive_mappings.clear();
    s.inventory["works"][0]["source_mappings"]["jm"] = json!(["123"]);
    assert_eq!(decide(&s, &r, &[]).reason, "SAME_SITE_SOURCE_ID");
    r.source = "pica".into();
    assert_eq!(decide(&s, &r, &[]).reason, "UNCONFIRMED_AUTHOR");
}
#[test]
fn not_same_vetoes_automatic_and_conflicting_authority() {
    let mut s = state(vec![work("W1", json!("作品标题"))]);
    s.decisions.negative_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W1".into(),
    });
    assert_ne!(
        decide(&s, &record("作品标题"), &[]).disposition,
        "AUTO_EXISTING"
    );
    s.decisions.positive_mappings = s.decisions.negative_mappings.clone();
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "CONTRADICTORY_AUTHORITY"
    );
}
#[test]
fn multiple_authorities_and_disagreement_are_review() {
    let mut s = state(vec![work("W1", Value::Null), work("W2", Value::Null)]);
    for w in s.inventory["works"].as_array_mut().unwrap() {
        w["source_mappings"]["jm"] = json!(["123"]);
    }
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "CONFLICTING_SOURCE_MAPPINGS"
    );
    s.inventory["works"][1]["source_mappings"]["jm"] = json!([]);
    s.decisions.positive_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W2".into(),
    });
    assert_eq!(
        decide(&s, &record("作品标题"), &[]).reason,
        "CONTRADICTORY_AUTHORITY"
    );
}
#[test]
fn repeated_replay_is_idempotent_and_decisions_reanalyze_without_network() {
    let mut s = state(vec![work("W1", json!("作品标题"))]);
    let mut audit = ReplayAudit::default();
    let r = record("作品标题");
    assert_eq!(replay(&mut s, std::slice::from_ref(&r), &mut audit, &[]), 1);
    let old = hash(&s);
    assert_eq!(replay(&mut s, std::slice::from_ref(&r), &mut audit, &[]), 0);
    assert_eq!(hash(&s), old);
    s.decisions.negative_mappings.push(Mapping {
        source_key: "jm:123".into(),
        work_id: "W1".into(),
    });
    assert_eq!(replay(&mut s, std::slice::from_ref(&r), &mut audit, &[]), 1);
    assert_eq!(s.catalog["jm:123"].work_id, None);
    s.decisions.ignored_source_records.push("jm:123".into());
    replay(&mut s, &[r], &mut audit, &[]);
    assert_eq!(s.catalog["jm:123"].record.processing_result, "IGNORED");
}
#[test]
fn changed_source_metadata_invalidates_cached_identity() {
    let mut s = state(vec![work("W1", json!("作品标题"))]);
    let mut a = ReplayAudit::default();
    let mut r = record("作品标题");
    replay(&mut s, &[r.clone()], &mut a, &[]);
    r.metadata = json!({"content_type":"novel"});
    assert_eq!(replay(&mut s, &[r], &mut a, &[]), 1);
    assert_eq!(s.catalog["jm:123"].work_id, None);
}
#[test]
fn repair_is_auditable_idempotent_and_refuses_overwrite() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut s = load(&root.join("monitor-state")).unwrap();
    let repair: Value = serde_json::from_str(include_str!(
        "../../../fixtures/matcher-m2/inventory-primary-repair.json"
    ))
    .unwrap();
    let first = repair_primary(&mut s.inventory, &repair).unwrap();
    let hash1 = hash(&s.inventory);
    assert_eq!(repair_primary(&mut s.inventory, &repair).unwrap(), first);
    assert_eq!(hash(&s.inventory), hash1);
    let w = s.inventory["works"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|w| w["work_id"] == "WORK_02657")
        .unwrap();
    w["title_candidates"][0]["primary"] = json!("human changed title");
    assert_eq!(
        repair_primary(&mut s.inventory, &repair).unwrap_err(),
        "REPAIR_OVERWRITE_REFUSED"
    );
}

#[test]
fn repair_refuses_an_unrelated_inventory_hash_even_when_target_row_matches() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut s = load(&root.join("monitor-state")).unwrap();
    let repair: Value = serde_json::from_str(include_str!(
        "../../../fixtures/matcher-m2/inventory-primary-repair.json"
    ))
    .unwrap();
    s.inventory["unrelated_drift"] = json!(true);
    assert_eq!(
        repair_primary(&mut s.inventory, &repair).unwrap_err(),
        "REPAIR_INVENTORY_HASH_MISMATCH"
    );
}

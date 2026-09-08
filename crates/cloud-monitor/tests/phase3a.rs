use cloud_monitor::{monitor::*, persistence::*};
use serde_json::json;
use state_model::{Record, SearchPage};
use std::collections::{BTreeMap, BTreeSet};

fn state() -> State {
    let mut s = State {
        authors: json!({"schema_version":1,"authors":[{"name":"Writer","enabled":true}]}),
        inventory: json!({"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    s.begin(
        "one",
        "2026-09-06T00:00:00Z",
        vec!["Writer".into()],
        "full",
        5,
    )
    .unwrap();
    s
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
fn same(s: &mut State, source: &str, id: &str, work_id: &str) {
    s.decisions.positive_mappings.push(Mapping {
        source_key: format!("{source}:{id}"),
        work_id: work_id.into(),
    });
}
fn observe(s: &mut State, r: &Record) {
    s.accept(r, r).unwrap();
}
fn page(ids: &[&str], n: u64, total: u64) -> SearchPage {
    SearchPage {
        page: n,
        reported_total: Some(total),
        reported_pages: None,
        reported_limit: None,
        response_fields: vec![],
        record_fields: vec![],
        redirect_to_detail: false,
        records: ids.iter().map(|id| record(id, "Long work title")).collect(),
    }
}
#[test]
fn discovery_once_and_repeat_idempotent() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    let r = record("1", "Long work title");
    observe(&mut s, &r);
    assert_eq!(s.pending.len(), 1);
    assert_eq!(s.scan.events.len(), 1);
    s.begin(
        "two",
        "2026-09-07T00:00:00Z",
        vec!["Writer".into()],
        "full",
        5,
    )
    .unwrap();
    observe(&mut s, &r);
    assert!(s.scan.events.is_empty());
    assert_eq!(s.catalog["jm:1"].analysis_count, 1);
}
#[test]
fn pending_upgrade_keeps_identity_and_increments_revision() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    same(&mut s, "jm", "2", "CONFIRMED_WORK");
    let mut a = record("1", "Long work title");
    a.metadata = json!({"content_type":"manga","tags":["中文","有码","黑白"]});
    observe(&mut s, &a);
    let old = s.pending.values().next().unwrap().clone();
    let mut b = record("2", "Long work title");
    b.metadata = json!({"content_type":"manga","tags":["中文","无码","黑白"]});
    observe(&mut s, &b);
    let new = s.pending.values().next().unwrap();
    assert_eq!(old.task_id, new.task_id);
    assert_eq!(old.first_seen, new.first_seen);
    assert_eq!(new.task_revision, 2);
    assert_eq!(new.target.source_key, "jm:2");
    assert!(!s.complete_task(
        &old.work_id,
        1,
        &json!({"work_id":old.work_id,"chinese":true,"uncensored":true,"color":false})
    ));
}
#[test]
fn fingerprint_change_requires_reanalysis() {
    let mut s = state();
    let a = record("1", "Long work title");
    observe(&mut s, &a);
    let b = record("1", "Long work title changed");
    assert!(s.needs_detail(&b));
    observe(&mut s, &b);
    assert_eq!(s.catalog["jm:1"].analysis_count, 2);
}
#[test]
fn search_and_direct_errors_never_count_unavailable() {
    let mut s = state();
    observe(&mut s, &record("1", "Long work title"));
    s.source_error("jm", "Writer", "TIMEOUT");
    for i in 0..4 {
        s.direct(
            "jm:1",
            Detail::SourceError("HTTP_404".into()),
            &i.to_string(),
        )
        .unwrap();
    }
    assert_eq!(s.catalog["jm:1"].unavailable_streak, 0);
    assert!(s.catalog["jm:1"].active);
    assert_eq!(s.scan.progress["jm|Writer"].boundary, "SOURCE_ERROR");
}
#[test]
fn three_explicit_checks_not_three_retries_inactivate() {
    let mut s = state();
    same(&mut s, "pica", "1", "CONFIRMED_WORK");
    let mut item = record("1", "Long work title");
    item.source = "pica".into();
    observe(&mut s, &item);
    s.direct("pica:1", Detail::ExplicitUnavailable, "a")
        .unwrap();
    s.direct("pica:1", Detail::ExplicitUnavailable, "a")
        .unwrap();
    assert_eq!(s.catalog["pica:1"].unavailable_streak, 1);
    s.direct("pica:1", Detail::ExplicitUnavailable, "b")
        .unwrap();
    assert!(s.catalog["pica:1"].active);
    s.direct("pica:1", Detail::ExplicitUnavailable, "c")
        .unwrap();
    assert!(!s.catalog["pica:1"].active);
    assert_eq!(s.pending.values().next().unwrap().status, "inactive");
}
#[test]
fn success_resets_unavailable_streak() {
    let mut s = state();
    let mut r = record("1", "Long work title");
    r.source = "pica".into();
    observe(&mut s, &r);
    s.direct("pica:1", Detail::ExplicitUnavailable, "a")
        .unwrap();
    s.direct("pica:1", Detail::Available(Box::new(r)), "b")
        .unwrap();
    assert_eq!(s.catalog["pica:1"].unavailable_streak, 0);
}
#[test]
fn incremental_five_historical_boundary_does_not_remove_unseen() {
    let mut s = state();
    for id in ["1", "2", "3", "4", "5", "6"] {
        observe(&mut s, &record(id, &format!("Long work title {id}")));
    }
    s.scan
        .last_full
        .insert("jm|Writer".into(), "2026-09-01T00:00:00Z".into());
    s.begin(
        "two",
        "2026-09-06T00:00:00Z",
        vec!["Writer".into()],
        "incremental",
        5,
    )
    .unwrap();
    assert!(s.page_boundary("jm", "Writer", &page(&["1", "2", "3", "4", "5"], 1, 6)));
    assert_eq!(
        s.scan.progress["jm|Writer"].boundary,
        "EARLY_STOP_HEURISTIC"
    );
    assert!(s.catalog["jm:6"].active);
    assert_eq!(s.catalog["jm:6"].unavailable_streak, 0);
}
#[test]
fn newly_discovered_ids_do_not_become_historical_mid_scan() {
    let mut s = state();
    s.scan.requested_mode = "incremental".into();
    s.scan
        .last_full
        .insert("jm|Writer".into(), s.scan.started_at.clone());
    for id in ["1", "2", "3", "4", "5"] {
        observe(&mut s, &record(id, "Long work title"));
    }
    assert!(!s.page_boundary("jm", "Writer", &page(&["1", "2", "3", "4", "5"], 1, 10)));
}
#[test]
fn full_ignores_threshold_and_six_month_recovery() {
    let mut s = state();
    for id in ["1", "2", "3", "4", "5"] {
        observe(&mut s, &record(id, "Long work title"));
    }
    s.begin(
        "two",
        "2026-09-06T00:00:00Z",
        vec!["Writer".into()],
        "full",
        5,
    )
    .unwrap();
    assert!(!s.page_boundary("jm", "Writer", &page(&["1", "2", "3", "4", "5"], 1, 10)));
    s.scan.requested_mode = "incremental".into();
    s.scan
        .last_full
        .insert("jm|Writer".into(), "2026-03-06T00:00:00Z".into());
    assert_eq!(s.effective_mode("jm", "Writer"), "full");
}
#[test]
fn decisions_persist_and_override_missing_author() {
    let mut s = state();
    s.decisions.positive_mappings.push(Mapping {
        source_key: "jm:1".into(),
        work_id: "CONFIRMED_WORK".into(),
    });
    let mut r = record("1", "1");
    r.author.clear();
    observe(&mut s, &r);
    assert_eq!(s.catalog["jm:1"].work_id.as_deref(), Some("CONFIRMED_WORK"));
    let mut restored: State = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    restored
        .decisions
        .ignored_source_records
        .push("jm:1".into());
    observe(&mut restored, &r);
    assert_eq!(restored.catalog["jm:1"].record.processing_result, "IGNORED");
    assert_eq!(restored.pending.values().next().unwrap().status, "ignored");
}
#[test]
fn negative_mapping_prevents_automatic_merge() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    let r = record("1", "Long work title");
    observe(&mut s, &r);
    let wid = s.catalog["jm:1"].work_id.clone().unwrap();
    s.decisions.negative_mappings.push(Mapping {
        source_key: "jm:2".into(),
        work_id: wid.clone(),
    });
    observe(&mut s, &record("2", "Long work title"));
    assert_ne!(s.catalog["jm:2"].work_id.as_ref(), Some(&wid));
}
#[test]
fn unknown_and_noisy_author_go_to_review() {
    for authors in [
        vec![],
        vec!["Circle (Writre)".into()],
        vec!["Writer".into(), "Other".into()],
    ] {
        let mut s = state();
        let mut r = record("1", "Long work title");
        r.author = authors;
        observe(&mut s, &r);
        assert!(s.pending.is_empty());
        assert_eq!(s.review.len(), 1);
        assert_eq!(s.author_names().len(), 1);
    }
}

#[test]
fn confirmed_author_evidence_is_normalized_without_changing_raw_fields() {
    let cases = [
        ("ｗＲＩＴＥＲ", "Writer", "DIRECT_NORMALIZED"),
        (
            "Example Circle (Writer)",
            "Writer",
            "PARENTHESIZED_OFFICIAL_TOKEN",
        ),
    ];
    for (raw, canonical, rule) in cases {
        let mut s = state();
        same(&mut s, "jm", "1", "CONFIRMED_WORK");
        let mut r = record("1", "Long work title");
        r.author = vec![raw.into()];
        observe(&mut s, &r);
        let entry = &s.catalog["jm:1"];
        assert_eq!(entry.record.author, vec![raw]);
        assert_eq!(
            entry.author_evidence.canonical_author.as_deref(),
            Some(canonical)
        );
        assert_eq!(entry.author_evidence.rule, rule);
        assert_eq!(s.author_names(), BTreeSet::from(["Writer".into()]));
        assert_eq!(s.pending.len(), 1);
    }
}

#[test]
fn confirmed_ten_station_glyph_equivalence_is_closed_and_audited() {
    let mut s = state();
    s.authors = json!({"authors":[{"name":"10驛","enabled":true}]});
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    let mut r = record("1", "Long work title");
    r.author = vec!["10駅".into()];
    observe(&mut s, &r);
    assert_eq!(s.catalog["jm:1"].record.author, vec!["10駅"]);
    assert_eq!(
        s.catalog["jm:1"]
            .author_evidence
            .canonical_author
            .as_deref(),
        Some("10驛")
    );
    assert_eq!(s.pending.values().next().unwrap().target.author, "10驛");
}

#[test]
fn parenthesized_author_requires_one_unambiguous_official_token() {
    let mut s = state();
    s.authors = json!({"authors":[
        {"name":"Writer","enabled":true},
        {"name":"Second","enabled":true}
    ]});
    for (id, raw) in [
        ("1", "Circle (Writre)"),
        ("2", "Circle Writer"),
        ("3", "Circle (Writer, Second)"),
    ] {
        let mut r = record(id, &format!("Long work title {id}"));
        r.author = vec![raw.into()];
        observe(&mut s, &r);
        assert!(s.catalog[&format!("jm:{id}")]
            .author_evidence
            .canonical_author
            .is_none());
    }
    assert_eq!(
        s.review
            .values()
            .filter(|review| review.status == "REVIEW_REQUIRED")
            .count(),
        3
    );
}
#[test]
fn ambiguous_inventory_never_auto_matches() {
    let mut s = state();
    s.inventory = json!({"works":[
        {"work_id":"A","authors_confirmed":["Writer"],"title_candidates":[{"primary":"Long work title"}],"versions":[{"content":{"type":"manga"}}]},
        {"work_id":"B","authors_confirmed":["Writer"],"title_candidates":[{"primary":"Long work title"}],"versions":[{"content":{"type":"manga"}}]}
    ]});
    observe(&mut s, &record("1", "Long work title"));
    assert!(s.pending.is_empty());
    assert_eq!(
        s.review.values().next().unwrap().reason,
        "AMBIGUOUS_EXISTING_IDENTITY"
    );
}
#[test]
fn unknown_cannot_replace_known_candidate() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    same(&mut s, "jm", "2", "CONFIRMED_WORK");
    let mut r = record("1", "Long work title");
    r.metadata = json!({"content_type":"manga","tags":["中文","有码","黑白"]});
    observe(&mut s, &r);
    observe(&mut s, &record("2", "Long work title"));
    assert_eq!(s.pending.values().next().unwrap().task_revision, 1);
    assert!(s
        .review
        .values()
        .any(|r| r.reason == "UNKNOWN_CANDIDATE_COMPARISON"));
}
#[test]
fn structure_is_preserved_and_unknown_coverage_never_authorizes_delete() {
    assert_ne!(
        rules_core::conservative_title("Story 1-5"),
        rules_core::conservative_title("Story 15")
    );
    assert_ne!(
        rules_core::conservative_title("Story Part 1"),
        rules_core::conservative_title("Story Part 2")
    );
    assert!(!rules_core::old_coverage_preserved(
        &Default::default(),
        &Default::default(),
        true
    ));
}
#[test]
fn checkpoint_and_eight_files_round_trip() {
    let mut s = state();
    observe(&mut s, &record("1", "Long work title"));
    s.page_boundary("jm", "Writer", &page(&["1"], 1, 3));
    let dir = std::env::temp_dir().join(format!("manga-phase3a-test-{}", hash(&now())));
    save(&dir, &s).unwrap();
    assert!(dir.join("review-export.json").exists());
    assert!(dir.join("review-export.csv").exists());
    assert!(dir.join("review-reason-summary.json").exists());
    assert!(dir.join("sanitized-state-sample.json").exists());
    let restored = load(&dir).unwrap();
    assert_eq!(hash(&s), hash(&restored));
    assert_eq!(restored.scan.progress["jm|Writer"].next_page, 2);
    save(&dir, &restored).unwrap();
    assert_eq!(hash(&s), hash(&load_checkpoint(&dir).unwrap()));
}

#[test]
fn changed_decisions_apply_before_any_network_request() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    observe(&mut s, &record("1", "Long work title"));
    s.decisions.ignored_source_records.push("jm:1".into());
    s.begin(
        "two",
        "2026-09-07T00:00:00Z",
        vec!["Writer".into()],
        "full",
        5,
    )
    .unwrap();
    s.source_error("jm", "Writer", "TIMEOUT");
    assert_eq!(s.pending.values().next().unwrap().status, "ignored");
}
#[test]
fn source_mapping_is_authoritative_without_title_author_inference() {
    let mut s = state();
    s.inventory = json!({"works":[{"work_id":"KNOWN","owned":true,"source_mappings":{"jm":["1"]},"versions":[]}]});
    let mut r = record("1", "1");
    r.author.clear();
    observe(&mut s, &r);
    assert_eq!(s.catalog["jm:1"].work_id.as_deref(), Some("KNOWN"));
    assert!(s.pending.is_empty());
}
#[test]
fn conflicting_version_evidence_is_unknown() {
    let r = record("1", "中文 全彩 黑白 無碼 有碼");
    let v = version(&r);
    assert_eq!(v.chinese, Some(true));
    assert_eq!(v.color, None);
    assert_eq!(v.uncensored, None);
}
#[test]
fn repeated_page_without_progress_stops_as_partial() {
    let mut s = state();
    assert!(!s.page_boundary("jm", "Writer", &page(&["1"], 1, 9)));
    assert!(s.page_boundary("jm", "Writer", &page(&["1"], 2, 9)));
    assert_eq!(
        s.scan.progress["jm|Writer"].boundary,
        "INCOMPLETE_PAGINATION"
    );
}
#[test]
fn available_after_inactive_restores_pending_once() {
    let mut s = state();
    same(&mut s, "pica", "1", "CONFIRMED_WORK");
    let mut r = record("1", "Long work title");
    r.source = "pica".into();
    observe(&mut s, &r);
    for id in ["a", "b", "c"] {
        s.direct("pica:1", Detail::ExplicitUnavailable, id).unwrap();
    }
    s.direct("pica:1", Detail::Available(Box::new(r.clone())), "d")
        .unwrap();
    assert_eq!(s.pending.values().next().unwrap().status, "pending");
    let count = s.scan.events.len();
    s.direct("pica:1", Detail::Available(Box::new(r)), "e")
        .unwrap();
    assert_eq!(s.scan.events.len(), count);
}

#[test]
fn jm_unavailable_claim_is_never_accepted_as_a_certificate() {
    let mut s = state();
    observe(&mut s, &record("1", "Long work title"));
    s.direct("jm:1", Detail::ExplicitUnavailable, "cycle-a")
        .unwrap();
    assert_eq!(s.catalog["jm:1"].unavailable_streak, 0);
    assert!(s.catalog["jm:1"].active);
    assert_eq!(
        s.scan.direct_failures["jm:1"],
        "UNAVAILABLE_NOT_CERTIFIED_FOR_SOURCE"
    );
}

#[test]
fn core_title_only_removes_confirmed_metadata_brackets() {
    assert_eq!(
        core_title(&record("1", "[Writer] Long title Part 1 [Chinese]")),
        "long title part 1"
    );
    assert_ne!(
        core_title(&record("1", "Long title [1-5]")),
        core_title(&record("2", "Long title [15]"))
    );
    assert_ne!(
        core_title(&record("1", "Long title [番外]")),
        core_title(&record("2", "Long title"))
    );
    assert_eq!(
        core_title(&record("1", "[Unknown] Long title")),
        "[unknown] long title"
    );
}
#[test]
fn explicit_metadata_wrappers_allow_conservative_cross_source_match() {
    let mut s = state();
    same(&mut s, "jm", "1", "CONFIRMED_WORK");
    same(&mut s, "jm", "2", "CONFIRMED_WORK");
    let mut a = record("1", "[Writer] Long title [Chinese]");
    a.metadata = json!({"content_type":"manga","tags":["有码","黑白"]});
    observe(&mut s, &a);
    let mut b = record("2", "Long title [中文]");
    b.metadata = json!({"content_type":"manga","tags":["无码","黑白"]});
    observe(&mut s, &b);
    assert_eq!(s.pending.len(), 1);
    assert_eq!(s.pending.values().next().unwrap().task_revision, 2);
}

#[test]
fn seed_black_white_value_allows_explicit_color_upgrade() {
    let mut s = state();
    s.inventory = json!({"works":[{"work_id":"KNOWN","owned":true,"local_item_ids":["LOCAL_ITEM_1"],"authors_confirmed":["Writer"],"title_candidates":[{"primary":"Long title"}],"versions":[{"language":{"chinese":"confirmed"},"version":{"censorship":"uncensored","color":"black_white","sample_or_preview":false},"content":{"type":"manga"}}]}]});
    let mut r = record("1", "Long title");
    r.metadata = json!({"content_type":"manga","tags":["中文","无码","全彩"]});
    observe(&mut s, &r);
    assert_eq!(s.pending["KNOWN"].action, "upgrade");
    assert_eq!(s.pending["KNOWN"].old_local_item_ids, vec!["LOCAL_ITEM_1"]);
}

#[test]
fn unchanged_review_reason_does_not_notify_again_after_fingerprint_change() {
    let mut s = state();
    let mut r = record("1", "Long title");
    r.author.clear();
    observe(&mut s, &r);
    assert_eq!(s.scan.events.len(), 1);
    s.begin(
        "two",
        "2026-09-07T00:00:00Z",
        vec!["Writer".into()],
        "full",
        5,
    )
    .unwrap();
    r.metadata = json!({"updated":"changed"});
    observe(&mut s, &r);
    assert!(s.scan.events.is_empty());
    assert_eq!(
        s.review
            .values()
            .filter(|r| r.status == "REVIEW_REQUIRED")
            .count(),
        1
    );
}

#[test]
fn explicit_untranslated_is_false_and_conflict_is_unknown() {
    assert_eq!(
        version(&record("1", "未漢化 Long title")).chinese,
        Some(false)
    );
    assert_eq!(
        version(&record("2", "未漢化 Long title [中文]")).chinese,
        None
    );
}

#[test]
fn distinct_content_types_do_not_auto_match() {
    let mut s = state();
    s.inventory = json!({"works":[{
        "work_id":"MANGA", "owned":true, "authors_confirmed":["Writer"],
        "title_candidates":[{"primary":"Long title CG集"}],
        "versions":[{"content":{"type":"manga"}}]
    }]});
    let mut source = record("1", "Long title CG集");
    source.metadata = json!({"content_type":"cg"});
    observe(&mut s, &source);
    assert_ne!(s.catalog["jm:1"].work_id.as_deref(), Some("MANGA"));
    assert!(s.review.values().any(|r| r.status == "REVIEW_REQUIRED"));
    assert_eq!(content_type(&record("2", "Long title CG集")), "cg_set");
}

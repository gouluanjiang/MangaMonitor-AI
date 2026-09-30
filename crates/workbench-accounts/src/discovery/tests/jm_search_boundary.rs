use super::*;
use crate::{JmSearchBoundary, JmSearchBoundaryItem};

fn jm_page(number: u64, total: u64, ids: &[u64]) -> SourcePage {
    let mut response = page(
        number,
        total,
        ids.iter()
            .map(|id| work(Source::Jm, &id.to_string(), &["Author A"]))
            .collect(),
    );
    let edge = |work: &SourceWork| JmSearchBoundaryItem {
        work_id: work.work_id.clone(),
        // Synthetic raw evidence. Changing the projection alone must still fail.
        fingerprint: format!("{:x}", Sha256::digest(work.work_id.as_bytes())),
    };
    response.jm_search_boundary = Some(JmSearchBoundary {
        first: response.items.first().map(edge),
        last: response.items.last().map(edge),
        recent_rows: None,
    });
    response
}

#[test]
fn adjacent_identical_jm_edges_count_unique_slots_but_charge_every_raw_slot() {
    let mut traversal = Traversal::default();
    let first = traversal.append(&jm_page(1, 6, &[100, 101, 102])).unwrap();
    assert!(!first.complete);
    assert_eq!(first.skipped_leading_work, 0);
    let second = traversal.append(&jm_page(2, 6, &[102, 103, 104])).unwrap();
    assert!(!second.complete, "raw count alone must not satisfy total");
    assert_eq!(second.skipped_leading_work, 1);
    assert_eq!((traversal.raw_record_count, traversal.record_count), (6, 5));
    let third = traversal.append(&jm_page(3, 6, &[104, 105])).unwrap();
    assert!(third.complete);
    assert_eq!(third.skipped_leading_work, 1);
    assert_eq!((traversal.raw_record_count, traversal.record_count), (8, 6));
    assert_eq!(traversal.ids.len(), 6);
    assert_eq!(
        traversal.head_ids,
        ["100", "101", "102", "103", "104", "105"]
    );
}

#[test]
fn only_one_proven_adjacent_valid_work_can_overlap() {
    for scenario in 0..17 {
        let mut first = jm_page(1, 10, &[100, 101, 102]);
        let mut second = jm_page(2, 10, &[102, 103]);
        match scenario {
            0 => second.jm_search_boundary = None,
            1 => first.jm_search_boundary = None,
            2 => {
                let boundary = second.jm_search_boundary.as_mut().unwrap();
                boundary.first.as_mut().unwrap().fingerprint = "a".repeat(64);
            }
            3 => second.items[0].title = "Conflicting projection".into(),
            4 => second = jm_page(2, 10, &[103, 102]),
            5 => second = jm_page(2, 10, &[102, 103, 101]),
            6 => second = jm_page(2, 10, &[102, 103, 103]),
            7 => second = jm_page(2, 10, &[100, 101, 102]),
            8 => second = jm_page(2, 10, &[102]),
            9 => {
                second = jm_page(2, 10, &[102]);
                second.issues = vec![issue(2, 2, None)];
                second.jm_search_boundary.as_mut().unwrap().last = None;
            }
            10 => {
                first.total = None;
                second.total = None;
            }
            11 => {
                for item in first.items.iter_mut().chain(second.items.iter_mut()) {
                    item.source = Source::Pica;
                }
            }
            12 => second.total = Some(11),
            13 => second.page = 3,
            14 => {
                first.issues = vec![issue(1, 4, None)];
                first.jm_search_boundary.as_mut().unwrap().last = None;
            }
            15 => {
                second.issues = vec![issue(2, 1, None)];
                second.jm_search_boundary.as_mut().unwrap().first = None;
            }
            16 => {
                second.issues = vec![issue(2, 3, Some("102"))];
                second.jm_search_boundary.as_mut().unwrap().last = None;
            }
            _ => unreachable!(),
        }
        let mut traversal = Traversal::default();
        assert!(!traversal.append(&first).unwrap().complete);
        let prior_ids = traversal.ids.clone();
        assert_eq!(
            traversal.append(&second).unwrap_err().code,
            "DISCOVERY_PAGINATION_CHANGED",
            "scenario {scenario}"
        );
        assert_eq!(traversal.page, 1);
        assert_eq!(traversal.ids, prior_ids);
    }
}

#[test]
fn effective_shortfall_and_terminal_envelope_conflicts_remain_errors() {
    for scenario in 0..3 {
        let mut first = jm_page(1, 4, &[100, 101]);
        let mut second = jm_page(2, 4, &[101, 102]);
        match scenario {
            0 => second.has_more = Some(false),
            1 => {
                first.pages = Some(2);
                second.pages = Some(2);
            }
            2 => {
                second = jm_page(2, 4, &[101, 102, 103]);
                second.has_more = Some(true);
            }
            _ => unreachable!(),
        }
        let mut traversal = Traversal::default();
        assert!(!traversal.append(&first).unwrap().complete);
        assert_eq!(
            traversal.append(&second).unwrap_err().code,
            "DISCOVERY_PAGINATION_CHANGED"
        );
        assert_eq!((traversal.raw_record_count, traversal.record_count), (2, 2));
    }
}

#[test]
fn raw_budget_cannot_be_extended_by_verified_overlaps() {
    for remaining in [2, 3] {
        let mut traversal = Traversal::default();
        traversal.append(&jm_page(1, 10, &[100, 101])).unwrap();
        // Exercise the boundary without allocating a large synthetic catalog.
        traversal.raw_record_count = MAX_DISCOVERY_RECORDS - remaining;
        let second = jm_page(2, 10, &[101, 102, 103]);
        assert_eq!(traversal.append(&second).unwrap_err().code, "DISCOVERY_LIMIT");
        assert_eq!(traversal.record_count, 2);
        assert_eq!(traversal.raw_record_count, MAX_DISCOVERY_RECORDS - remaining);
    }
}

#[tokio::test]
async fn full_catalog_saves_unique_baseline_and_first_discovery_markers() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    for (number, ids) in [
        (1, vec![100, 101, 102]),
        (2, vec![102, 103, 104]),
        (3, vec![104, 105]),
    ] {
        backend.put(Source::Jm, "Author A", number, jm_page(number, 6, &ids));
    }
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let saved = finish(&service, &scopes).await;
    let range = jm_range(&saved);
    assert_eq!(range.state, DiscoveryRangeState::Complete);
    assert!(range.pages_complete);
    assert_eq!(range.pages_read, 3);
    assert_eq!(range.observed_count, 6);
    assert_eq!(range.baseline.as_ref().unwrap().total, 6);
    assert_eq!(
        range.baseline.as_ref().unwrap().head_ids,
        ["100", "101", "102", "103", "104", "105"]
    );
    assert_eq!(saved.records.len(), 6);
    assert!(saved.records.iter().all(|record| {
        record.first_discovered_run_id.as_deref() == Some(started.run_id.as_str())
    }));
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn incremental_overlap_preserves_old_baselines_and_counts_new_prefix_once() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    let first = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    let old_baseline = jm_range(&prior).baseline.as_ref().unwrap();
    backend.put(
        Source::Jm,
        "Author A",
        1,
        jm_page(1, 70, &(200..220).collect::<Vec<_>>()),
    );
    let second_ids = (219..225).chain(100..115).collect::<Vec<_>>();
    backend.put(Source::Jm, "Author A", 2, jm_page(2, 70, &second_ids));
    backend.put(
        Source::Jm,
        "Author A",
        3,
        jm_page(3, 70, &(114..130).collect::<Vec<_>>()),
    );
    backend.0.calls.lock().unwrap().clear();
    let second = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    let range = jm_range(&next);
    assert_eq!(range.state, DiscoveryRangeState::Complete);
    assert_eq!(range.pages_read, 3);
    assert!(!range.pages_complete, "the old tail was retained, not re-read");
    assert_eq!(range.last_check_mode, Some(DiscoveryMode::Incremental));
    assert_eq!(range.last_complete_at, jm_range(&prior).last_complete_at);
    assert_eq!(
        range.baseline.as_ref().unwrap().query_version,
        old_baseline.query_version
    );
    assert_eq!(
        range.baseline.as_ref().unwrap().established_at,
        old_baseline.established_at
    );
    assert_eq!(range.baseline.as_ref().unwrap().total, 70);
    assert_eq!(
        range.baseline.as_ref().unwrap().head_ids,
        (200..220).map(|id| id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(next.records.len(), 70);
    assert_eq!(
        next.records
            .iter()
            .filter(|record| {
                record.first_discovered_run_id.as_deref() == Some(second.run_id.as_str())
            })
            .count(),
        25
    );
    assert_eq!(
        next.records
            .iter()
            .filter(|record| {
                record.first_discovered_run_id.as_deref() == Some(first.run_id.as_str())
            })
            .count(),
        45
    );
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn new_work_after_an_overlapping_anchor_still_requires_full_pagination() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    let first_ids = (200..205).chain(100..110).collect::<Vec<_>>();
    let second_ids = (109..125).chain(std::iter::once(205)).collect::<Vec<_>>();
    let third_ids = std::iter::once(205).chain(125..145).collect::<Vec<_>>();
    for (number, ids) in [(1, first_ids), (2, second_ids), (3, third_ids)] {
        backend.put(Source::Jm, "Author A", number, jm_page(number, 51, &ids));
    }
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    let range = jm_range(&next);
    assert_eq!(range.state, DiscoveryRangeState::Complete);
    assert_eq!(range.pages_read, 3);
    assert!(range.pages_complete);
    assert_eq!(range.last_check_mode, Some(DiscoveryMode::Full));
    assert_eq!(range.baseline.as_ref().unwrap().total, 51);
    assert_eq!(next.records.len(), 51);
    assert_eq!(
        next.records
            .iter()
            .filter(|record| {
                record.first_discovered_run_id.as_deref() == Some(started.run_id.as_str())
            })
            .count(),
        6
    );
}

#[tokio::test]
async fn overlap_keeps_issue_slots_partial_and_preserves_previous_records() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &[100, 101, 102, 103]);
    let first = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    backend.put(Source::Jm, "Author A", 1, jm_page(1, 4, &[100, 101]));
    let mut second = jm_page(2, 4, &[101, 103]);
    second.issues = vec![issue(2, 2, Some("102"))];
    backend.put(Source::Jm, "Author A", 2, second);
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    let range = jm_range(&next);
    assert_eq!(range.state, DiscoveryRangeState::Partial);
    assert_eq!(range.error_code.as_deref(), Some("SOURCE_ITEMS_PARTIAL"));
    assert!(range.pages_complete);
    assert_eq!(range.issue_count, 1);
    assert_eq!(
        (range.issue_samples[0].page, range.issue_samples[0].index),
        (2, 2)
    );
    assert_eq!(range.issue_samples[0].work_id.as_deref(), Some("102"));
    assert!(range.baseline.is_none());
    assert_eq!(range.last_complete_at, jm_range(&prior).last_complete_at);
    assert_eq!(next.records.len(), 4);
    let retained = next
        .records
        .iter()
        .find(|record| record.work.work_id == "102")
        .unwrap();
    assert_eq!(retained.scan_id, first.run_id);
    assert_eq!(
        retained.first_discovered_run_id.as_deref(),
        Some(first.run_id.as_str())
    );
    assert_eq!(
        next.last_check.as_ref().unwrap().phase,
        DiscoveryCheckPhase::Partial
    );
}

#[test]
fn malformed_edge_evidence_never_skips_an_issue_or_an_unrelated_work() {
    for scenario in 0..8 {
        let mut response = jm_page(1, 5, &[100, 101]);
        let boundary = response.jm_search_boundary.as_mut().unwrap();
        match scenario {
            0 => boundary.first.as_mut().unwrap().work_id = "999".into(),
            1 => boundary.last.as_mut().unwrap().fingerprint = "x".repeat(64),
            2 => boundary.first.as_mut().unwrap().fingerprint = "a".repeat(63),
            3 => response.issues = vec![issue(1, 1, None)],
            4 => response.issues = vec![issue(1, 3, None)],
            5 => response.items[0].title.clear(),
            6 => boundary.first = None,
            7 => boundary.last = None,
            _ => unreachable!(),
        }
        assert!(
            !crate::service::jm_search_boundary_is_valid(&response),
            "scenario {scenario}"
        );
    }
    let mut singleton = jm_page(1, 5, &[100]);
    singleton
        .jm_search_boundary
        .as_mut()
        .unwrap()
        .last
        .as_mut()
        .unwrap()
        .fingerprint = "a".repeat(64);
    assert!(!crate::service::jm_search_boundary_is_valid(&singleton));
}

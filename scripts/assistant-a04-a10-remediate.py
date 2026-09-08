from pathlib import Path
import re

monitor = Path("crates/cloud-monitor/src/monitor.rs")
source = monitor.read_bytes().decode("utf-8")

cursor_start = source.index("#[derive(Clone, Debug, Default, Serialize, Deserialize)]\npub struct Cursor {")
cursor_end = source.index(
    "#[derive(Clone, Debug, Default, Serialize, Deserialize)]\npub struct ReviewMigrationAudit",
    cursor_start,
)
cursor_block = '''#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginationContract {
    pub reported_total: Option<u64>,
    pub reported_pages: Option<u64>,
    pub reported_limit: Option<u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cursor {
    pub next_page: u64,
    pub historical_streak: usize,
    pub observed_ids: BTreeSet<String>,
    pub boundary: String,
    pub mode: String,
    #[serde(default)]
    pub pagination_contract: Option<PaginationContract>,
}
'''
source = source[:cursor_start] + cursor_block + source[cursor_end:]

page_start = source.index(
    "    /// Page boundary is a resume cursor, never evidence for availability.\n"
    "    pub fn page_boundary"
)
page_end = source.index("    pub fn direct(", page_start)
page_block = '''    /// Page boundary is a resume cursor, never evidence for availability.
    pub fn page_boundary(&mut self, source: &str, author: &str, page: &SearchPage) -> bool {
        let ck = Self::cursor_key(source, author);
        let mode = self.effective_mode(source, author);
        let committed = self.scan.progress.get(&ck).cloned().unwrap_or_default();
        let previous_count = committed.observed_ids.len();
        let expected_page = if committed.next_page == 0 {
            1
        } else {
            committed.next_page
        };
        let page_contract = PaginationContract {
            reported_total: page.reported_total,
            reported_pages: page.reported_pages,
            reported_limit: page.reported_limit,
        };

        // Build the next cursor off to the side. A malformed page must not
        // consume its cursor, observations, historical streak, or the
        // first-page pagination contract: resume must retry the same page.
        let mut candidate = committed.clone();
        candidate.mode = mode.clone();
        candidate.next_page = page.page.saturating_add(1);
        let page_sequence_valid = page.page > 0 && page.page == expected_page;
        let contract_consistent = if expected_page == 1 {
            page_sequence_valid && committed.pagination_contract.is_none()
        } else {
            committed.pagination_contract.as_ref() == Some(&page_contract)
        };
        if page.page == 1 {
            candidate.pagination_contract = Some(page_contract.clone());
        }

        let mut page_ids = BTreeSet::new();
        let mut duplicate_page = false;
        let mut early = false;
        for r in &page.records {
            let k = key(r);
            if !page_ids.insert(k.clone()) || committed.observed_ids.contains(&k) {
                duplicate_page = true;
            }
            if candidate.observed_ids.insert(k.clone()) {
                if self.scan.historical_ids.contains(&k) {
                    candidate.historical_streak += 1;
                } else {
                    candidate.historical_streak = 0;
                }
                if mode == "incremental" && candidate.historical_streak >= self.scan.threshold {
                    early = true;
                }
            }
        }
        let observed_count = candidate.observed_ids.len() as u64;
        let limit_valid = page
            .reported_limit
            .is_none_or(|limit| limit > 0 && page.records.len() as u64 <= limit);
        let total_valid = page
            .reported_total
            .is_none_or(|total| observed_count <= total);
        let pages_valid = page
            .reported_pages
            .is_none_or(|pages| pages > 0 && page.page <= pages);
        let totals_agree = match (
            page.reported_total,
            page.reported_pages,
            page.reported_limit,
        ) {
            (Some(total), Some(pages), Some(limit)) if total > 0 && limit > 0 => {
                pages == total.div_ceil(limit)
            }
            (Some(0), Some(pages), _) => pages == 0 || pages == 1,
            _ => true,
        };
        let terminal_page = page
            .reported_pages
            .is_some_and(|pages| pages > 0 && page.page == pages);
        let terminal_count_valid = !(terminal_page
            && page
                .reported_total
                .is_some_and(|total| observed_count != total));
        let valid_empty = page.records.is_empty()
            && page.page == 1
            && page.reported_total == Some(0)
            && page
                .reported_pages
                .is_none_or(|pages| pages == 0 || pages == 1)
            && page_sequence_valid
            && contract_consistent;
        let metadata_consistent = page_sequence_valid
            && contract_consistent
            && limit_valid
            && total_valid
            && pages_valid
            && totals_agree
            && terminal_count_valid
            && !duplicate_page;
        // Only the canonical zero-result first page may be empty. Every
        // ordinary empty page fails before candidate commit so resume cannot
        // skip an unproven page.
        let page_valid = valid_empty || (!page.records.is_empty() && metadata_consistent);
        let redirect_complete = page.redirect_to_detail
            && page.records.len() == 1
            && metadata_consistent
            && page.page == 1
            && page.reported_total.is_none_or(|total| total == 1)
            && page.reported_pages.is_none_or(|pages| pages == 1);
        let exhausted = if valid_empty {
            true
        } else if metadata_consistent && !page.records.is_empty() {
            match (page.reported_total, page.reported_pages) {
                (Some(total), Some(pages)) => page.page == pages && observed_count == total,
                (None, Some(pages)) => page.page == pages,
                (Some(total), None) => observed_count == total,
                (None, None) => false,
            }
        } else {
            false
        };

        if !page_valid {
            let cursor = self.scan.progress.entry(ck.clone()).or_default();
            cursor.boundary = "INCOMPLETE_PAGINATION".into();
            self.event(
                format!("pagination:{}:{ck}", self.scan.scan_id),
                "SCAN_PARTIAL",
                json!({
                    "source": source,
                    "author": author,
                    "reason": "INCOMPLETE_PAGINATION",
                    "expected_page": expected_page,
                    "page": page.page,
                    "observed_count": previous_count,
                }),
            );
            return true;
        }

        // Commit the candidate only after every page invariant, including the
        // durable first-page pagination contract, has passed.
        self.scan.progress.insert(ck.clone(), candidate);
        if redirect_complete || exhausted {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "COMPLETE".into();
            if mode == "full" {
                self.scan.last_full.insert(ck, self.scan.started_at.clone());
            }
            true
        } else if early {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "EARLY_STOP_HEURISTIC".into();
            self.event(format!("early:{}:{ck}", self.scan.scan_id),"SCAN_PARTIAL",json!({"source":source,"author":author,"reason":"EARLY_STOP_HEURISTIC","boundary":"AFTER_FETCHED_PAGE"}));
            true
        } else {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "CHECKPOINT".into();
            false
        }
    }
'''
source = source[:page_start] + page_block + source[page_end:]
monitor.write_bytes(source.encode("utf-8"))

inventory = Path("monitor-state/inventory_index.json")
inv = inventory.read_bytes().decode("utf-8")
work_start = inv.index('"work_id": "WORK_02657"')
title_start = inv.index('"title_candidates": [', work_start)
owned_start = inv.index('"owned": true', title_start)
section = inv[title_start:owned_start]
pattern = re.compile(
    r'"primary": null,(\r?\n\s*)"normalized_key": null,(\r?\n\s*)"fandom_or_source": null'
)

def repaired(match):
    return (
        '"primary": "竿役募集してる推しの爆乳エロ配信者が妹になりました",'
        + match.group(1)
        + '"normalized_key": null,'
        + match.group(2)
        + '"fandom_or_source": "オリジナル"'
    )

section, count = pattern.subn(repaired, section)
if count != 1:
    raise SystemExit(f"expected one WORK_02657 repair target, got {count}")
inv = inv[:title_start] + section + inv[owned_start:]
inventory.write_bytes(inv.encode("utf-8"))

regression = Path("crates/cloud-monitor/tests/a04_a10_final_regressions.rs")
regression.write_text(
r'''use cloud_monitor::{matcher_m2, monitor::*, persistence};
use serde_json::{json, Value};
use state_model::{Record, SearchPage};
use std::{collections::BTreeMap, fs, path::Path};

fn state() -> State {
    let mut state = State {
        authors: json!({"schema_version":1,"authors":[{"name":"Writer","enabled":true}]}),
        inventory: json!({"schema_version":8,"works":[]}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::new(),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Decisions::default(),
        scan: Scan::default(),
    };
    state
        .begin(
            "a04-final",
            "2026-09-08T00:00:00Z",
            vec!["Writer".into()],
            "full",
            5,
        )
        .unwrap();
    state
}

fn record(id: &str) -> Record {
    Record::new(
        "jm",
        id.into(),
        vec!["Writer".into()],
        "Stable title".into(),
        json!({"content_type":"manga"}),
    )
}

fn page(
    number: u64,
    records: &[&str],
    total: Option<u64>,
    pages: Option<u64>,
    limit: Option<u64>,
) -> SearchPage {
    SearchPage {
        page: number,
        reported_total: total,
        reported_pages: pages,
        reported_limit: limit,
        response_fields: vec![],
        record_fields: vec![],
        redirect_to_detail: false,
        records: records.iter().map(|id| record(id)).collect(),
    }
}

#[test]
fn ordinary_empty_page_is_rejected_before_cursor_commit_and_retry_completes() {
    let mut state = state();
    assert!(!state.page_boundary("jm", "Writer", &page(1, &["one"], Some(3), Some(3), Some(1))));
    let committed = state.scan.progress["jm|Writer"].clone();
    assert!(committed.pagination_contract.is_some());

    assert!(state.page_boundary("jm", "Writer", &page(2, &[], Some(3), Some(3), Some(1))));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.historical_streak, committed.historical_streak);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    assert_eq!(rejected.boundary, "INCOMPLETE_PAGINATION");
    assert!(state.scan.last_full.is_empty());

    assert!(!state.page_boundary("jm", "Writer", &page(2, &["two"], Some(3), Some(3), Some(1))));
    assert!(state.page_boundary("jm", "Writer", &page(3, &["three"], Some(3), Some(3), Some(1))));
    assert_eq!(state.scan.progress["jm|Writer"].boundary, "COMPLETE");
    assert!(state.scan.last_full.contains_key("jm|Writer"));
}

#[test]
fn pagination_contract_cannot_shrink_mid_enumeration() {
    let mut state = state();
    assert!(!state.page_boundary("jm", "Writer", &page(1, &["one"], Some(3), Some(3), Some(1))));
    let committed = state.scan.progress["jm|Writer"].clone();

    assert!(state.page_boundary("jm", "Writer", &page(2, &["two"], Some(2), Some(2), Some(1))));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    assert!(state.scan.last_full.is_empty());

    assert!(!state.page_boundary("jm", "Writer", &page(2, &["two"], Some(3), Some(3), Some(1))));
    assert!(state.page_boundary("jm", "Writer", &page(3, &["three"], Some(3), Some(3), Some(1))));
    assert_eq!(state.scan.progress["jm|Writer"].boundary, "COMPLETE");
}

#[test]
fn durable_checkpoint_preserves_contract_and_rejects_shrink_after_reload() {
    let root = std::env::temp_dir().join(format!(
        "mangamonitor-a04-contract-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let mut state = state();
    assert!(!state.page_boundary("jm", "Writer", &page(1, &["one"], Some(3), Some(3), Some(1))));
    persistence::save(&root, &state).unwrap();

    let mut resumed = persistence::load_checkpoint(&root).unwrap();
    let committed = resumed.scan.progress["jm|Writer"].clone();
    assert_eq!(
        committed.pagination_contract,
        Some(PaginationContract {
            reported_total: Some(3),
            reported_pages: Some(3),
            reported_limit: Some(1),
        })
    );
    assert!(resumed.page_boundary("jm", "Writer", &page(2, &["two"], Some(2), Some(2), Some(1))));
    let rejected = &resumed.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, committed.next_page);
    assert_eq!(rejected.observed_ids, committed.observed_ids);
    assert_eq!(rejected.pagination_contract, committed.pagination_contract);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn legacy_mid_cycle_cursor_without_contract_fails_closed() {
    let mut state = state();
    let mut legacy = Cursor::default();
    legacy.next_page = 2;
    legacy.boundary = "CHECKPOINT".into();
    legacy.mode = "full".into();
    legacy.observed_ids.insert("jm:one".into());
    state.scan.progress.insert("jm|Writer".into(), legacy.clone());

    assert!(state.page_boundary("jm", "Writer", &page(2, &["two"], Some(3), Some(3), Some(1))));
    let rejected = &state.scan.progress["jm|Writer"];
    assert_eq!(rejected.next_page, 2);
    assert_eq!(rejected.observed_ids, legacy.observed_ids);
    assert!(rejected.pagination_contract.is_none());
    assert_eq!(rejected.boundary, "INCOMPLETE_PAGINATION");
}

#[test]
fn public_seed_is_already_at_audited_repair_after_state_and_repair_is_noop() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = persistence::load(&repo.join("monitor-state")).unwrap();
    let repair: Value = serde_json::from_slice(
        &fs::read(repo.join("fixtures/matcher-m2/inventory-primary-repair.json")).unwrap(),
    )
    .unwrap();
    let before_hash = hash(&state.inventory);
    assert_eq!(
        Some(before_hash.as_str()),
        repair["analysis_inventory_hash_after"].as_str()
    );

    let work = state.inventory["works"]
        .as_array()
        .unwrap()
        .iter()
        .find(|work| work["work_id"] == "WORK_02657")
        .unwrap();
    assert_eq!(
        work["title_candidates"],
        json!([{
            "primary":"竿役募集してる推しの爆乳エロ配信者が妹になりました",
            "normalized_key":null,
            "fandom_or_source":"オリジナル"
        }])
    );

    let mut repaired = state.inventory.clone();
    matcher_m2::repair_primary(&mut repaired, &repair).unwrap();
    assert_eq!(hash(&repaired), before_hash);
    assert_eq!(repaired, state.inventory);
}
''',
encoding="utf-8",
)

use super::*;
use crate::{Authenticated, FollowKind, SourceAccount};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tokio::sync::Notify;
use workbench_credentials::{test_support::MemoryVault, CredentialKind, StoredCredential};
use workbench_sources::{FavoritePageRequest, FavoriteUpdate};

type TestService = AccountService<FakeBackend, MemoryVault>;
type QueryKey = (String, String, u64);

#[derive(Default)]
struct FakeState {
    pages: Mutex<BTreeMap<QueryKey, Result<SourcePage>>>,
    details: Mutex<BTreeMap<String, Result<SourceWork>>>,
    calls: Mutex<Vec<QueryKey>>,
    detail_calls: AtomicUsize,
    block_call: AtomicUsize,
    started: Notify,
    release: Notify,
}

#[derive(Clone, Default)]
struct FakeBackend(Arc<FakeState>);
struct FakeSession {
    source: Source,
}

fn label(source: Source) -> String {
    if source == Source::Jm { "JM" } else { "Pica" }.into()
}
fn key(source: Source, author: &str, page: u64) -> QueryKey {
    (label(source), author.into(), page)
}
fn empty() -> SourcePage {
    SourcePage {
        page: 1,
        total: Some(0),
        pages: Some(1),
        has_more: Some(false),
        folders: vec![],
        items: vec![],
    }
}
fn work(source: Source, id: &str, authors: &[&str]) -> SourceWork {
    SourceWork {
        source,
        work_id: id.into(),
        title: "合成作品 [中文]".into(),
        authors: authors.iter().map(|author| (*author).into()).collect(),
        description: Some("合成元数据".into()),
        tags: vec!["中文".into()],
        favorite: None,
        chapter_count: Some(1),
        page_count: Some(20),
        cover_available: true,
    }
}
fn page(number: u64, total: u64, items: Vec<SourceWork>) -> SourcePage {
    SourcePage {
        page: number,
        total: Some(total),
        pages: None,
        has_more: None,
        folders: vec![],
        items,
    }
}

fn catalog(backend: &FakeBackend, ids: &[u64]) {
    for (index, chunk) in ids.chunks(20).enumerate() {
        backend.put(
            Source::Jm,
            "Author A",
            index as u64 + 1,
            page(
                index as u64 + 1,
                ids.len() as u64,
                chunk
                    .iter()
                    .map(|id| work(Source::Jm, &id.to_string(), &["Author A"]))
                    .collect(),
            ),
        );
    }
}

fn jm_range(snapshot: &DiscoverySnapshot) -> &DiscoveryAuthorRange {
    snapshot
        .authors
        .iter()
        .find(|range| range.author == "Author A" && range.source == workbench_storage::Source::Jm)
        .unwrap()
}

impl FakeBackend {
    fn put(&self, source: Source, author: &str, number: u64, page: SourcePage) {
        self.0
            .pages
            .lock()
            .unwrap()
            .insert(key(source, author, number), Ok(page));
    }
}

impl SourceBackend for FakeBackend {
    type Session = FakeSession;
    async fn login(
        &self,
        source: Source,
        username: &str,
        _: &str,
    ) -> Result<Authenticated<Self::Session>> {
        Ok(Authenticated {
            session: FakeSession { source },
            account: SourceAccount {
                source,
                account_id: username.into(),
                display_name: "fixture".into(),
            },
            credential: StoredCredential::new(
                username,
                if source == Source::Jm {
                    CredentialKind::SessionCookie
                } else {
                    CredentialKind::SessionToken
                },
                "synthetic-session",
            )
            .unwrap(),
        })
    }
    async fn restore(
        &self,
        source: Source,
        credential: &StoredCredential,
    ) -> Result<Authenticated<Self::Session>> {
        self.login(source, credential.account_name(), "fixture")
            .await
    }
    async fn favorites(&self, _: &Self::Session, _: FavoritePageRequest) -> Result<SourcePage> {
        panic!("discovery cannot read favorites")
    }
    async fn search(
        &self,
        session: &Self::Session,
        author: &str,
        number: u64,
    ) -> Result<SourcePage> {
        let query = key(session.source, author, number);
        let count = {
            let mut calls = self.0.calls.lock().unwrap();
            calls.push(query.clone());
            calls.len()
        };
        if self.0.block_call.load(Ordering::SeqCst) == count {
            self.0.started.notify_one();
            self.0.release.notified().await;
        }
        self.0
            .pages
            .lock()
            .unwrap()
            .get(&query)
            .cloned()
            .unwrap_or_else(|| Ok(empty()))
    }
    async fn detail(&self, _: &Self::Session, id: &str) -> Result<SourceWork> {
        self.0.detail_calls.fetch_add(1, Ordering::SeqCst);
        self.0
            .details
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or(Err(AccountError::new("SOURCE_UNAVAILABLE")))
    }
    async fn favorite(&self, _: &Self::Session, _: &str, _: bool) -> Result<FavoriteUpdate> {
        panic!("discovery cannot write source favorites")
    }
    async fn cover(&self, _: &Self::Session, _: &str) -> Result<Option<String>> {
        panic!("discovery cannot fetch media")
    }
}

async fn setup() -> (TempDir, FakeBackend, Arc<TestService>, Vec<DiscoveryScope>) {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = Arc::new(AccountService::new(
        backend.clone(),
        MemoryVault::new(),
        root.path().into(),
    ));
    let mut scopes = vec![];
    for source in [Source::Jm, Source::Pica] {
        let account = service
            .login(
                source,
                format!("{}-fixture", label(source)),
                "fixture-only".into(),
                false,
            )
            .await
            .unwrap();
        scopes.push(DiscoveryScope {
            source,
            session_id: account.session_id.unwrap(),
        });
    }
    (root, backend, service, scopes)
}

async fn follow(service: &TestService, scope: &DiscoveryScope, author: &str, desired: bool) {
    let following = service
        .following(scope.source, &scope.session_id)
        .await
        .unwrap();
    service
        .follow(
            scope.source,
            &scope.session_id,
            FollowKind::Author,
            author,
            desired,
            following.revision,
        )
        .await
        .unwrap();
}

async fn finish(service: &TestService, scopes: &[DiscoveryScope]) -> DiscoverySnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if !service.discovery.memory.lock().unwrap().active {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    service.discovery_read(scopes.to_vec()).await.unwrap()
}

/// Earlier accepted imports may contain names the runtime no longer queries.
/// Keep the old storage contract readable; bypass only the new-follow UI gate.
async fn legacy_followed_names(
    root: &TempDir,
    service: &TestService,
    scope: &DiscoveryScope,
    names: &[&str],
) {
    follow(service, scope, "Fixture Seed", true).await;
    let store = WorkbenchStore::open(root.path()).unwrap();
    let mut following = store.read_following().unwrap();
    following.value.accounts[0].authors = names.iter().map(|name| (*name).into()).collect();
    store
        .write_following(following.revision, following.value)
        .unwrap();
}

#[test]
fn author_query_guard_distinguishes_placeholders_broad_credits_and_valid_short_names() {
    for name in [
        "N/A",
        " n/a ",
        "　Ｎ／Ａ　",
        "N.A.",
        "UNKNOWN",
        "none",
        "null",
        "未知作者",
        "作者不明",
        "作者不详",
        "作者不詳",
    ] {
        assert_eq!(
            author_query_error(name),
            Some("AUTHOR_QUERY_PLACEHOLDER"),
            "{name}"
        );
    }
    for name in ["P", "p", " Ｐ ", "7", "７"] {
        assert_eq!(
            author_query_error(name),
            Some("AUTHOR_QUERY_TOO_BROAD"),
            "{name}"
        );
    }
    for name in [
        "甲",
        "あ",
        "NA",
        "P P",
        "Example Circle (P)",
        "Unknown Artist",
        "Author A",
    ] {
        assert_eq!(author_query_error(name), None, "{name}");
    }
}

#[tokio::test]
async fn all_blocked_legacy_names_are_partial_without_requests_in_both_check_modes() {
    let (root, backend, service, scopes) = setup().await;
    legacy_followed_names(&root, &service, &scopes[0], &["N/A", "P", "７"]).await;
    let before = std::fs::read(
        root.path()
            .join(workbench_storage::PRIVATE_DIRECTORY)
            .join("following.json"),
    )
    .unwrap();
    for mode in [DiscoveryMode::Full, DiscoveryMode::Incremental] {
        service
            .discovery_start_with_mode(scopes.clone(), vec![], mode)
            .await
            .unwrap();
        let snapshot = finish(&service, &scopes).await;
        let run = snapshot.run.unwrap();
        assert_eq!(run.phase, DiscoveryPhase::Partial);
        assert_eq!(run.requests_used, 0);
        assert_eq!(run.total_scopes, 6);
        assert_eq!(run.completed_scopes, 6);
        assert!(snapshot.records.is_empty());
        assert!(snapshot.authors.iter().all(|range| {
            range.state == DiscoveryRangeState::Partial
                && range.error_code.as_deref() == author_query_error(&range.author)
                && range.pages_read == 0
                && range.baseline.is_none()
                && range.last_complete_at.is_none()
                && range.last_checked_at.is_none()
        }));
    }
    assert!(backend.0.calls.lock().unwrap().is_empty());
    assert_eq!(
        before,
        std::fs::read(
            root.path()
                .join(workbench_storage::PRIVATE_DIRECTORY)
                .join("following.json")
        )
        .unwrap()
    );
}

#[tokio::test]
async fn skipped_names_do_not_block_valid_one_character_or_full_signature_queries() {
    let (root, backend, service, scopes) = setup().await;
    legacy_followed_names(
        &root,
        &service,
        &scopes[0],
        &["N/A", "P", "甲", "Example Circle (P)"],
    )
    .await;
    for (author, id) in [("甲", "100"), ("Example Circle (P)", "101")] {
        backend.put(
            Source::Jm,
            author,
            1,
            page(1, 1, vec![work(Source::Jm, id, &[author])]),
        );
    }
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(
        snapshot.run.as_ref().unwrap().phase,
        DiscoveryPhase::Partial
    );
    assert_eq!(snapshot.run.as_ref().unwrap().requests_used, 4);
    assert_eq!(snapshot.records.len(), 2);
    assert_eq!(
        snapshot
            .authors
            .iter()
            .filter(|range| range.state == DiscoveryRangeState::Complete)
            .count(),
        4
    );
    assert_eq!(
        *backend.0.calls.lock().unwrap(),
        vec![
            key(Source::Jm, "Example Circle (P)", 1),
            key(Source::Pica, "Example Circle (P)", 1),
            key(Source::Jm, "甲", 1),
            key(Source::Pica, "甲", 1),
        ]
    );
}

#[tokio::test]
async fn blocked_legacy_results_are_hidden_without_deleting_shared_or_stored_records() {
    let (root, backend, service, scopes) = setup().await;
    legacy_followed_names(&root, &service, &scopes[0], &["N/A", "P", "Author A"]).await;
    let context = service.discovery_context(scopes.clone()).await.unwrap();
    let mut saved_range = idle("N/A", Source::Jm);
    saved_range.state = DiscoveryRangeState::Complete;
    saved_range.pages_read = 2;
    saved_range.observed_count = 1;
    saved_range.last_complete_at = Some(10);
    saved_range.last_checked_at = Some(10);
    saved_range.last_check_mode = Some(DiscoveryMode::Full);
    saved_range.baseline = Some(DiscoveryBaseline {
        query_version: DISCOVERY_QUERY_VERSION,
        head_ids: vec!["100".into()],
        total: 1,
        established_at: 10,
    });
    let record = |id: &str, authors: &[&str]| DiscoveryRecord {
        work: discovery_work_from_source(work(Source::Jm, id, authors)),
        matched_authors: authors.iter().map(|author| (*author).into()).collect(),
        author_verified: true,
        observed_at: 10,
        scan_id: "a".repeat(64),
    };
    let store = WorkbenchStore::open(root.path()).unwrap();
    store
        .write_discovery(
            0,
            DiscoveryDocument {
                version: 1,
                accounts: vec![DiscoveryAccount {
                    account_key: context.account_key,
                    authors: vec![saved_range],
                    records: vec![record("100", &["N/A"]), record("101", &["P", "Author A"])],
                }],
            },
        )
        .unwrap();
    let before = std::fs::read(
        root.path()
            .join(workbench_storage::PRIVATE_DIRECTORY)
            .join("discovery.json"),
    )
    .unwrap();
    // Exercise disk projection and subsequent cached reads.
    for _ in 0..2 {
        let snapshot = service.discovery_read(scopes.clone()).await.unwrap();
        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].work.work_id, "101");
        assert_eq!(snapshot.records[0].matched_authors, ["Author A"]);
        let range = snapshot
            .authors
            .iter()
            .find(|range| range.author == "N/A" && range.source == workbench_storage::Source::Jm)
            .unwrap();
        assert_eq!(range.state, DiscoveryRangeState::Partial);
        assert_eq!(
            range.error_code.as_deref(),
            Some("AUTHOR_QUERY_PLACEHOLDER")
        );
        assert_eq!(range.last_complete_at, Some(10));
        assert!(range.baseline.is_none());
    }
    assert_eq!(
        before,
        std::fs::read(
            root.path()
                .join(workbench_storage::PRIVATE_DIRECTORY)
                .join("discovery.json")
        )
        .unwrap()
    );
    assert!(backend.0.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn adding_placeholders_is_rejected_but_legacy_unfollow_and_generic_search_remain_available() {
    let (root, backend, service, scopes) = setup().await;
    let scope = &scopes[0];
    for name in ["N/A", "　Ｎ／Ａ　", "UNKNOWN"] {
        assert_eq!(
            service
                .follow(
                    scope.source,
                    &scope.session_id,
                    FollowKind::Author,
                    name,
                    true,
                    0
                )
                .await
                .unwrap_err()
                .code,
            "AUTHOR_QUERY_PLACEHOLDER"
        );
    }
    legacy_followed_names(&root, &service, scope, &["N/A"]).await;
    follow(&service, scope, "N/A", false).await;
    follow(&service, scope, "P", true).await;
    assert_eq!(
        service
            .following(scope.source, &scope.session_id)
            .await
            .unwrap()
            .authors,
        ["P"]
    );
    for query in ["N/A", "P"] {
        service
            .query(
                scope.source,
                &scope.session_id,
                QueryKind::Search,
                query,
                None,
                1,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        *backend.0.calls.lock().unwrap(),
        [key(Source::Jm, "N/A", 1), key(Source::Jm, "P", 1)]
    );
}

#[tokio::test]
async fn eligible_author_catalogs_are_not_cut_off_at_a_small_page_limit() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    for number in 1..=125 {
        backend.put(
            Source::Jm,
            "Author A",
            number,
            page(
                number,
                125,
                vec![work(Source::Jm, &(100 + number).to_string(), &["Author A"])],
            ),
        );
    }
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(
        snapshot.run.as_ref().unwrap().phase,
        DiscoveryPhase::Complete
    );
    assert_eq!(jm_range(&snapshot).pages_read, 125);
    assert_eq!(snapshot.records.len(), 125);
    assert_eq!(backend.0.calls.lock().unwrap().len(), 126);
}

#[tokio::test]
async fn first_scan_publishes_old_works_and_searches_each_followed_author_on_both_sources() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    follow(&service, &scopes[1], "Author B", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    backend.put(
        Source::Pica,
        "Author A",
        1,
        page(
            1,
            1,
            vec![work(Source::Pica, &"a".repeat(24), &["Author A"])],
        ),
    );
    backend.put(
        Source::Pica,
        "Author B",
        1,
        page(
            1,
            1,
            vec![work(Source::Pica, &"b".repeat(24), &["Author B"])],
        ),
    );
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(
        snapshot.run.as_ref().unwrap().phase,
        DiscoveryPhase::Complete
    );
    assert_eq!(snapshot.records.len(), 3);
    assert_eq!(snapshot.authors.len(), 4);
    assert!(snapshot.records.iter().all(|record| record.author_verified
        && record.scan_id == started.run_id
        && record.work.tags == ["中文"]));
    assert_eq!(
        *backend.0.calls.lock().unwrap(),
        [
            key(Source::Jm, "Author A", 1),
            key(Source::Pica, "Author A", 1),
            key(Source::Jm, "Author B", 1),
            key(Source::Pica, "Author B", 1)
        ]
    );
    assert_eq!(snapshot.run.as_ref().unwrap().completed_scopes, 4);
    // An empty later source listing does not erase old omissions.
    backend.0.pages.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    assert_eq!(finish(&service, &scopes).await.records.len(), 3);
}

#[tokio::test]
async fn keyword_scope_retains_variant_blank_and_other_author_results_without_detail_fanout() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    for source in [Source::Jm, Source::Pica] {
        let id = |number: u64| {
            if source == Source::Jm {
                number.to_string()
            } else {
                format!("{number:024x}")
            }
        };
        backend.put(
            source,
            "Author A",
            1,
            page(
                1,
                6,
                vec![
                    work(source, &id(100), &["Author A"]),
                    work(source, &id(101), &["Circle (Author A)"]),
                    work(source, &id(102), &["Author A Author B"]),
                ],
            ),
        );
        backend.put(
            source,
            "Author A",
            2,
            page(
                2,
                6,
                vec![
                    work(source, &id(103), &[]),
                    work(source, &id(104), &["author a"]),
                    work(source, &id(105), &["another writer"]),
                ],
            ),
        );
    }
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.records.len(), 12);
    assert_eq!(
        snapshot
            .records
            .iter()
            .filter(|record| record.author_verified)
            .count(),
        2
    );
    assert!(snapshot
        .records
        .iter()
        .all(|record| record.matched_authors == ["Author A"]));
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 0);
    assert_eq!(snapshot.run.unwrap().phase, DiscoveryPhase::Complete);
    assert!(snapshot.authors.iter().all(|range| range.pages_read == 2
        && range.observed_count == 6
        && range.error_code.is_none()
        && range.last_complete_at.is_some()));
}

#[tokio::test]
async fn shared_keyword_hits_persist_both_query_scopes_and_keep_old_omissions() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    follow(&service, &scopes[1], "Author B", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    backend.put(
        Source::Jm,
        "Author B",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &[])]),
    );
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(
        snapshot.records[0].matched_authors,
        ["Author B", "Author A"]
    );
    assert_eq!(snapshot.records[0].work.authors, ["Author A"]);
    assert!(!snapshot.records[0].author_verified);
    assert!(snapshot
        .authors
        .iter()
        .filter(|range| range.source == storage_source(Source::Jm))
        .all(|range| range.observed_count == 1));
    let stored = WorkbenchStore::open(root.path())
        .unwrap()
        .read_discovery()
        .unwrap();
    assert_eq!(
        stored.value.accounts[0].records[0].matched_authors,
        ["Author B", "Author A"]
    );
    backend.0.pages.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let later = finish(&service, &scopes).await;
    assert_eq!(later.records.len(), 1);
    assert_eq!(later.records[0].matched_authors, ["Author B", "Author A"]);
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn duplicate_or_changed_pages_preserve_prior_content_and_report_the_source_range() {
    for changed_total in [false, true] {
        let (_root, backend, service, scopes) = setup().await;
        follow(&service, &scopes[0], "Author A", true).await;
        backend.put(
            Source::Jm,
            "Author A",
            1,
            page(1, 2, vec![work(Source::Jm, "100", &["Author A"])]),
        );
        backend.put(
            Source::Jm,
            "Author A",
            2,
            page(
                2,
                if changed_total { 3 } else { 2 },
                vec![work(
                    Source::Jm,
                    if changed_total { "101" } else { "100" },
                    &["Author A"],
                )],
            ),
        );
        service
            .discovery_start(scopes.clone(), vec![])
            .await
            .unwrap();
        let snapshot = finish(&service, &scopes).await;
        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.authors[0].pages_read, 1);
        assert_eq!(
            snapshot.authors[0].error_code.as_deref(),
            Some("DISCOVERY_PAGINATION_CHANGED")
        );
        assert_eq!(snapshot.run.unwrap().phase, DiscoveryPhase::Partial);
    }
}

#[tokio::test]
async fn progress_read_and_cancel_do_not_wait_for_in_flight_api_and_late_results_cannot_commit() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 2, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    backend.put(
        Source::Jm,
        "Author A",
        2,
        page(2, 2, vec![work(Source::Jm, "101", &["Author A"])]),
    );
    backend.0.block_call.store(2, Ordering::SeqCst);
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    backend.0.started.notified().await;
    let progress = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        service.discovery_read(scopes.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(progress.records.len(), 1);
    assert_eq!(progress.run.unwrap().current_page, 2);
    let cancelled = service.discovery_cancel(&started.run_id).unwrap();
    assert_eq!(cancelled.phase, DiscoveryPhase::Cancelled);
    backend.0.release.notify_one();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(snapshot.run.unwrap().phase, DiscoveryPhase::Cancelled);
    assert_eq!(backend.0.calls.lock().unwrap().len(), 2);
    let store = WorkbenchStore::open(root.path()).unwrap();
    assert_eq!(
        store.read_discovery().unwrap().value.accounts[0]
            .records
            .len(),
        1
    );
    assert!(service
        .discovery_run_is_current(&scopes, &started.run_id)
        .is_err());
}

#[tokio::test]
async fn following_drift_stops_a_blocked_scan_and_old_auto_download_guards() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    backend.0.block_call.store(1, Ordering::SeqCst);
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    backend.0.started.notified().await;
    follow(&service, &scopes[1], "Author B", true).await;
    assert_eq!(
        service
            .discovery_run_is_current(&scopes, &started.run_id)
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    backend.0.release.notify_one();
    let snapshot = finish(&service, &scopes).await;
    assert!(snapshot.records.is_empty());
    assert_eq!(
        snapshot.run.unwrap().error_code.as_deref(),
        Some("DISCOVERY_FOLLOWING_CHANGED")
    );
}

#[tokio::test]
async fn one_account_logout_invalidates_both_source_discovery_scope_and_drops_late_results() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    backend.0.block_call.store(1, Ordering::SeqCst);
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    backend.0.started.notified().await;
    service
        .logout(Source::Pica, Some(&scopes[1].session_id))
        .await
        .unwrap();
    assert_eq!(
        service
            .discovery_read(scopes.clone())
            .await
            .unwrap_err()
            .code,
        "SESSION_CHANGED"
    );
    backend.0.release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while service.discovery.memory.lock().unwrap().active {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(WorkbenchStore::open(root.path())
        .unwrap()
        .read_discovery()
        .unwrap()
        .value
        .accounts[0]
        .records
        .is_empty());
}

#[tokio::test]
async fn rate_limit_halts_before_another_author_or_source_and_retains_error() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.0.pages.lock().unwrap().insert(
        key(Source::Jm, "Author A", 1),
        Err(AccountError::new("SOURCE_RATE_LIMITED")),
    );
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.run.unwrap().phase, DiscoveryPhase::Error);
    assert_eq!(backend.0.calls.lock().unwrap().len(), 1);
    assert_eq!(
        snapshot.authors[0].error_code.as_deref(),
        Some("SOURCE_RATE_LIMITED")
    );
}

#[tokio::test]
async fn restart_reads_old_records_without_requests_and_account_pair_isolation_is_preserved() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]),
    );
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    let fresh = Arc::new(AccountService::new(
        backend.clone(),
        MemoryVault::new(),
        root.path().into(),
    ));
    let mut restored = vec![];
    for source in [Source::Jm, Source::Pica] {
        let account = fresh
            .login(
                source,
                format!("{}-fixture", label(source)),
                "fixture".into(),
                false,
            )
            .await
            .unwrap();
        restored.push(DiscoveryScope {
            source,
            session_id: account.session_id.unwrap(),
        });
    }
    let requests = backend.0.calls.lock().unwrap().len();
    let snapshot = fresh.discovery_read(restored.clone()).await.unwrap();
    assert_eq!(snapshot.records.len(), 1);
    assert!(snapshot.run.is_none());
    assert_eq!(backend.0.calls.lock().unwrap().len(), requests);
    let different = fresh
        .login(
            Source::Pica,
            "different-fixture".into(),
            "fixture".into(),
            false,
        )
        .await
        .unwrap();
    restored[1].session_id = different.session_id.unwrap();
    assert!(fresh
        .discovery_read(restored)
        .await
        .unwrap()
        .records
        .is_empty());
}

#[tokio::test]
async fn unfollow_filters_records_without_erasing_stored_evidence_and_verified_identity_never_downgrades(
) {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    follow(&service, &scopes[1], "Author B", true).await;
    backend.put(
        Source::Jm,
        "Author A",
        1,
        page(
            1,
            1,
            vec![work(Source::Jm, "100", &["Author A", "Author B"])],
        ),
    );
    backend.put(
        Source::Jm,
        "Author B",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &[])]),
    );
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.records.len(), 1);
    assert!(snapshot.records[0].author_verified);
    follow(&service, &scopes[0], "Author A", false).await;
    let remaining = service.discovery_read(scopes.clone()).await.unwrap();
    assert_eq!(remaining.records.len(), 1);
    assert_eq!(remaining.records[0].matched_authors, ["Author B"]);
    follow(&service, &scopes[1], "Author B", false).await;
    assert!(service
        .discovery_read(scopes)
        .await
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        WorkbenchStore::open(root.path())
            .unwrap()
            .read_discovery()
            .unwrap()
            .value
            .accounts[0]
            .records
            .len(),
        1
    );
}

#[tokio::test]
async fn unknown_author_and_single_source_start_fail_before_requests() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    assert_eq!(
        service
            .discovery_start(vec![scopes[0].clone()], vec![])
            .await
            .unwrap_err()
            .code,
        "DISCOVERY_SCOPES_REQUIRED"
    );
    assert_eq!(
        service
            .discovery_start(scopes, vec!["unfollowed".into()])
            .await
            .unwrap_err()
            .code,
        "DISCOVERY_AUTHOR_NOT_FOLLOWED"
    );
    assert!(backend.0.calls.lock().unwrap().is_empty());
}

#[test]
fn pagination_does_not_treat_unknown_totals_or_a_short_page_as_complete() {
    let mut traversal = Traversal::default();
    let mut response = page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]);
    response.total = None;
    assert!(!traversal.append(&response).unwrap());
    response = page(2, 2, vec![work(Source::Jm, "101", &["Author A"])]);
    assert!(traversal.append(&response).is_err());
}

#[test]
fn document_contention_retries_only_busy_and_preserves_other_error_codes() {
    let mut attempts = 0;
    let value = discovery_store_io(|| {
        attempts += 1;
        if attempts < 3 {
            Err(workbench_storage::StoreError { code: "BUSY" })
        } else {
            Ok(42)
        }
    })
    .unwrap();
    assert_eq!(value, 42);
    assert_eq!(attempts, 3);
    for code in [
        "REVISION_CONFLICT",
        "DOCUMENT_CORRUPT",
        "UNSUPPORTED_SCHEMA",
        "DISCOVERY_FOLLOWING_CHANGED",
    ] {
        let mut attempts = 0;
        let result: std::result::Result<(), _> = discovery_store_io(|| {
            attempts += 1;
            Err(workbench_storage::StoreError { code })
        });
        assert_eq!(result.unwrap_err().code, code);
        assert_eq!(attempts, 1);
    }
    let mut attempts = 0;
    let result: std::result::Result<(), _> = discovery_store_io(|| {
        attempts += 1;
        Err(workbench_storage::StoreError { code: "BUSY" })
    });
    assert_eq!(result.unwrap_err().code, "BUSY");
    assert_eq!(attempts, 5);
}

#[tokio::test]
async fn local_follow_changes_revoke_fast_image_guard_and_a_later_run_replaces_old_permission() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    let first = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    assert!(service
        .discovery_run_is_live(&scopes, &first.run_id)
        .is_ok());
    follow(&service, &scopes[1], "Author B", true).await;
    assert_eq!(
        service
            .discovery_run_is_live(&scopes, &first.run_id)
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    let second = service
        .discovery_start(scopes.clone(), vec!["Author A".into()])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    assert!(service
        .discovery_run_is_live(&scopes, &second.run_id)
        .is_ok());
    assert_eq!(
        service
            .discovery_run_is_live(&scopes, &first.run_id)
            .unwrap_err()
            .code,
        "DISCOVERY_RUN_CHANGED"
    );
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn default_first_check_builds_a_full_baseline_then_unchanged_catalog_reads_one_page() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    let first = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let complete = finish(&service, &scopes).await;
    let prior_range = jm_range(&complete).clone();
    assert_eq!(complete.run.unwrap().mode, DiscoveryMode::Incremental);
    assert_eq!(prior_range.pages_read, 3);
    assert_eq!(prior_range.last_check_mode, Some(DiscoveryMode::Full));
    assert_eq!(prior_range.baseline.as_ref().unwrap().head_ids.len(), 20);
    assert_eq!(prior_range.baseline.as_ref().unwrap().total, 45);
    backend.0.calls.lock().unwrap().clear();

    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    assert_eq!(next.run.as_ref().unwrap().phase, DiscoveryPhase::Complete);
    assert_eq!(jm_range(&next).pages_read, 1);
    assert_eq!(
        jm_range(&next).last_check_mode,
        Some(DiscoveryMode::Incremental)
    );
    assert_eq!(
        jm_range(&next).last_complete_at,
        prior_range.last_complete_at
    );
    assert_eq!(jm_range(&next).baseline, prior_range.baseline);
    assert_eq!(next.records.len(), 45);
    assert!(next
        .records
        .iter()
        .any(|record| { record.work.work_id == "144" && record.scan_id == first.run_id }));
    assert_eq!(
        *backend.0.calls.lock().unwrap(),
        [
            key(Source::Jm, "Author A", 1),
            key(Source::Pica, "Author A", 1)
        ]
    );
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn incremental_boundary_crosses_pages_keeps_old_omissions_and_advances_only_the_head() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    let ids = (200..205).chain(100..145).collect::<Vec<_>>();
    catalog(&backend, &ids);
    backend.0.calls.lock().unwrap().clear();

    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    let range = jm_range(&next);
    assert_eq!(range.pages_read, 2);
    assert_eq!(range.last_check_mode, Some(DiscoveryMode::Incremental));
    assert_eq!(range.last_complete_at, jm_range(&prior).last_complete_at);
    assert_eq!(range.baseline.as_ref().unwrap().total, 50);
    assert_eq!(
        range.baseline.as_ref().unwrap().head_ids,
        ids[..20]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(next.records.len(), 50);
    assert!(next
        .records
        .iter()
        .any(|record| record.work.work_id == "144"));
    assert_eq!(
        *backend.0.calls.lock().unwrap(),
        [
            key(Source::Jm, "Author A", 1),
            key(Source::Jm, "Author A", 2),
            key(Source::Pica, "Author A", 1)
        ]
    );
}

#[tokio::test]
async fn manual_full_check_refreshes_tail_and_never_uses_a_front_checkpoint() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    let mut updated = work(Source::Jm, "144", &["Author A"]);
    updated.title = "updated synthetic metadata".into();
    let mut tail = (140..144)
        .map(|id| work(Source::Jm, &id.to_string(), &["Author A"]))
        .collect::<Vec<_>>();
    tail.push(updated);
    backend.put(Source::Jm, "Author A", 3, page(3, 45, tail));
    backend.0.calls.lock().unwrap().clear();
    service
        .discovery_start_with_mode(scopes.clone(), vec![], DiscoveryMode::Full)
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    assert_eq!(next.run.as_ref().unwrap().mode, DiscoveryMode::Full);
    assert_eq!(jm_range(&next).pages_read, 3);
    assert_eq!(jm_range(&next).last_check_mode, Some(DiscoveryMode::Full));
    assert_eq!(
        jm_range(&next).last_checked_at,
        jm_range(&next).last_complete_at
    );
    assert_eq!(
        next.records
            .iter()
            .find(|record| record.work.work_id == "144")
            .unwrap()
            .work
            .title,
        "updated synthetic metadata"
    );
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn count_drift_reordering_or_new_ids_after_the_anchor_fall_through_to_full_pagination() {
    for scenario in 0..4 {
        let (_root, backend, service, scopes) = setup().await;
        follow(&service, &scopes[0], "Author A", true).await;
        catalog(&backend, &(100..145).collect::<Vec<_>>());
        service
            .discovery_start(scopes.clone(), vec![])
            .await
            .unwrap();
        finish(&service, &scopes).await;
        let ids = match scenario {
            0 => (100..144).collect::<Vec<_>>(),
            1 => std::iter::once(144).chain(100..144).collect(),
            2 => std::iter::once(200)
                .chain(100..120)
                .chain(std::iter::once(201))
                .chain(120..145)
                .collect(),
            _ => std::iter::once(200)
                .chain(100..110)
                .chain(111..145)
                .collect(),
        };
        catalog(&backend, &ids);
        backend.0.calls.lock().unwrap().clear();
        service
            .discovery_start(scopes.clone(), vec![])
            .await
            .unwrap();
        let next = finish(&service, &scopes).await;
        assert_eq!(jm_range(&next).pages_read, 3);
        assert_eq!(jm_range(&next).last_check_mode, Some(DiscoveryMode::Full));
        assert!(next
            .records
            .iter()
            .any(|record| record.work.work_id == "144"));
        assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
    }
}

#[tokio::test]
async fn source_terminal_page_takes_precedence_over_short_or_empty_incremental_checkpoints() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..105).collect::<Vec<_>>());
    for _ in 0..2 {
        service
            .discovery_start(scopes.clone(), vec![])
            .await
            .unwrap();
        let snapshot = finish(&service, &scopes).await;
        assert!(snapshot
            .authors
            .iter()
            .all(|range| range.last_check_mode == Some(DiscoveryMode::Full)));
        assert_eq!(
            jm_range(&snapshot)
                .baseline
                .as_ref()
                .unwrap()
                .head_ids
                .len(),
            5
        );
        let empty_range = snapshot
            .authors
            .iter()
            .find(|range| range.source == workbench_storage::Source::Pica)
            .unwrap();
        assert_eq!(empty_range.baseline.as_ref().unwrap().total, 0);
        assert!(empty_range.baseline.as_ref().unwrap().head_ids.is_empty());
        assert!(
            IncrementalBoundary::new(DiscoveryMode::Incremental, empty_range, &HashSet::new())
                .is_none()
        );
    }
}

#[tokio::test]
async fn unknown_source_total_does_not_allow_an_incremental_early_stop() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    for response in backend.0.pages.lock().unwrap().values_mut() {
        let response = response.as_mut().unwrap();
        response.total = None;
        response.has_more = Some(response.page < 3);
    }
    backend.0.calls.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    assert_eq!(jm_range(&next).pages_read, 3);
    assert_eq!(jm_range(&next).last_check_mode, Some(DiscoveryMode::Full));
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn duplicate_new_prefix_ids_keep_the_old_baseline_and_report_partial() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    let mut records = vec![
        work(Source::Jm, "200", &["Author A"]),
        work(Source::Jm, "200", &["Author A"]),
    ];
    records.extend((100..118).map(|id| work(Source::Jm, &id.to_string(), &["Author A"])));
    backend.put(Source::Jm, "Author A", 1, page(1, 47, records));
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let next = finish(&service, &scopes).await;
    assert_eq!(jm_range(&next).state, DiscoveryRangeState::Partial);
    assert_eq!(
        jm_range(&next).error_code.as_deref(),
        Some("DISCOVERY_PAGINATION_CHANGED")
    );
    assert_eq!(jm_range(&next).baseline, jm_range(&prior).baseline);
    assert_eq!(next.records.len(), 45);
}

#[tokio::test]
async fn failed_incremental_check_preserves_checkpoint_and_the_next_attempt_rebuilds() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    let ids = (200..205).chain(100..145).collect::<Vec<_>>();
    catalog(&backend, &ids);
    backend.0.pages.lock().unwrap().insert(
        key(Source::Jm, "Author A", 2),
        Err(AccountError::new("SOURCE_UNAVAILABLE")),
    );
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let failed = finish(&service, &scopes).await;
    assert_eq!(jm_range(&failed).state, DiscoveryRangeState::Partial);
    assert_eq!(jm_range(&failed).baseline, jm_range(&prior).baseline);
    assert_eq!(
        jm_range(&failed).last_checked_at,
        jm_range(&prior).last_checked_at
    );
    assert_eq!(failed.records.len(), 50);
    catalog(&backend, &ids);
    backend.0.calls.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let recovered = finish(&service, &scopes).await;
    assert_eq!(jm_range(&recovered).pages_read, 3);
    assert_eq!(
        jm_range(&recovered).last_check_mode,
        Some(DiscoveryMode::Full)
    );
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn legacy_complete_rows_without_checkpoints_must_build_a_new_full_baseline() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    let store = WorkbenchStore::open(root.path()).unwrap();
    let mut saved = store.read_discovery().unwrap();
    for range in &mut saved.value.accounts[0].authors {
        range.baseline = None;
        range.last_checked_at = None;
        range.last_check_mode = None;
    }
    store.write_discovery(saved.revision, saved.value).unwrap();
    backend.0.calls.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let rebuilt = finish(&service, &scopes).await;
    assert_eq!(jm_range(&rebuilt).pages_read, 3);
    assert_eq!(
        jm_range(&rebuilt).last_check_mode,
        Some(DiscoveryMode::Full)
    );
    assert!(jm_range(&rebuilt).baseline.is_some());
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn cancellation_preserves_the_old_baseline_and_does_not_resume_it_as_complete() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    catalog(&backend, &(100..145).collect::<Vec<_>>());
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let prior = finish(&service, &scopes).await;
    let ids = (200..205).chain(100..145).collect::<Vec<_>>();
    catalog(&backend, &ids);
    backend.0.calls.lock().unwrap().clear();
    backend.0.block_call.store(2, Ordering::SeqCst);
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    backend.0.started.notified().await;
    service.discovery_cancel(&started.run_id).unwrap();
    backend.0.release.notify_one();
    let cancelled = finish(&service, &scopes).await;
    assert_eq!(
        cancelled.run.as_ref().unwrap().phase,
        DiscoveryPhase::Cancelled
    );
    assert_eq!(jm_range(&cancelled).baseline, jm_range(&prior).baseline);
    let stored = WorkbenchStore::open(root.path())
        .unwrap()
        .read_discovery()
        .unwrap();
    assert_eq!(
        stored.value.accounts[0].authors[0].state,
        DiscoveryRangeState::Checking
    );
    assert_eq!(
        stored.value.accounts[0].authors[0].baseline,
        jm_range(&prior).baseline
    );
    backend.0.block_call.store(0, Ordering::SeqCst);
    backend.0.calls.lock().unwrap().clear();
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let recovered = finish(&service, &scopes).await;
    assert_eq!(jm_range(&recovered).pages_read, 3);
    assert_eq!(
        jm_range(&recovered).last_check_mode,
        Some(DiscoveryMode::Full)
    );
    assert_eq!(backend.0.calls.lock().unwrap().len(), 4);
}

#[test]
fn incremental_boundary_requires_a_current_successful_baseline_and_known_totals() {
    let ids = (100..145).map(|id| id.to_string()).collect::<HashSet<_>>();
    let mut range = idle("Author A", Source::Jm);
    range.state = DiscoveryRangeState::Complete;
    range.last_complete_at = Some(1);
    range.baseline = Some(DiscoveryBaseline {
        query_version: DISCOVERY_QUERY_VERSION,
        head_ids: (100..120).map(|id| id.to_string()).collect(),
        total: 45,
        established_at: 1,
    });
    let mut boundary = IncrementalBoundary::new(DiscoveryMode::Incremental, &range, &ids).unwrap();
    let mut response = page(
        1,
        45,
        (100..120)
            .map(|id| work(Source::Jm, &id.to_string(), &["Author A"]))
            .collect(),
    );
    response.total = None;
    assert!(!boundary.append(&response));
    assert!(!boundary.viable);
    assert!(IncrementalBoundary::new(DiscoveryMode::Full, &range, &ids).is_none());
    for state in [
        DiscoveryRangeState::Partial,
        DiscoveryRangeState::Checking,
        DiscoveryRangeState::Cancelled,
        DiscoveryRangeState::Error,
    ] {
        range.state = state;
        assert!(IncrementalBoundary::new(DiscoveryMode::Incremental, &range, &ids).is_none());
    }
    range.state = DiscoveryRangeState::Complete;
    range.baseline.as_mut().unwrap().query_version += 1;
    assert!(IncrementalBoundary::new(DiscoveryMode::Incremental, &range, &ids).is_none());
}

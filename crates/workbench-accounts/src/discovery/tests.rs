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

#[tokio::test]
async fn first_scan_publishes_old_works_and_searches_each_followed_author_on_both_sources() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "A", true).await;
    follow(&service, &scopes[1], "B", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["A"])]),
    );
    backend.put(
        Source::Pica,
        "A",
        1,
        page(1, 1, vec![work(Source::Pica, &"a".repeat(24), &["A"])]),
    );
    backend.put(
        Source::Pica,
        "B",
        1,
        page(1, 1, vec![work(Source::Pica, &"b".repeat(24), &["B"])]),
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
            key(Source::Jm, "A", 1),
            key(Source::Pica, "A", 1),
            key(Source::Jm, "B", 1),
            key(Source::Pica, "B", 1)
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
async fn author_metadata_is_checked_and_unknown_detail_is_review_not_false_complete() {
    let (_root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "A", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(
            1,
            3,
            vec![
                work(Source::Jm, "100", &[]),
                work(Source::Jm, "101", &[]),
                work(Source::Jm, "102", &["unrelated"]),
            ],
        ),
    );
    backend
        .0
        .details
        .lock()
        .unwrap()
        .insert("100".into(), Ok(work(Source::Jm, "100", &["A"])));
    service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    let snapshot = finish(&service, &scopes).await;
    assert_eq!(snapshot.records.len(), 2);
    assert!(
        snapshot
            .records
            .iter()
            .find(|record| record.work.work_id == "100")
            .unwrap()
            .author_verified
    );
    assert!(
        !snapshot
            .records
            .iter()
            .find(|record| record.work.work_id == "101")
            .unwrap()
            .author_verified
    );
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 2);
    assert_eq!(snapshot.run.unwrap().phase, DiscoveryPhase::Partial);
    assert_eq!(
        snapshot.authors[0].error_code.as_deref(),
        Some("DISCOVERY_AUTHOR_UNCONFIRMED")
    );
    assert_eq!(snapshot.authors[0].last_complete_at, None);
}

#[tokio::test]
async fn duplicate_or_changed_pages_preserve_prior_content_and_report_the_source_range() {
    for changed_total in [false, true] {
        let (_root, backend, service, scopes) = setup().await;
        follow(&service, &scopes[0], "A", true).await;
        backend.put(
            Source::Jm,
            "A",
            1,
            page(1, 2, vec![work(Source::Jm, "100", &["A"])]),
        );
        backend.put(
            Source::Jm,
            "A",
            2,
            page(
                2,
                if changed_total { 3 } else { 2 },
                vec![work(
                    Source::Jm,
                    if changed_total { "101" } else { "100" },
                    &["A"],
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
    follow(&service, &scopes[0], "A", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 2, vec![work(Source::Jm, "100", &["A"])]),
    );
    backend.put(
        Source::Jm,
        "A",
        2,
        page(2, 2, vec![work(Source::Jm, "101", &["A"])]),
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
    follow(&service, &scopes[0], "A", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["A"])]),
    );
    backend.0.block_call.store(1, Ordering::SeqCst);
    let started = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    backend.0.started.notified().await;
    follow(&service, &scopes[1], "B", true).await;
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
    follow(&service, &scopes[0], "A", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["A"])]),
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
    follow(&service, &scopes[0], "A", true).await;
    backend.0.pages.lock().unwrap().insert(
        key(Source::Jm, "A", 1),
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
    follow(&service, &scopes[0], "A", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["A"])]),
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
    follow(&service, &scopes[0], "A", true).await;
    follow(&service, &scopes[1], "B", true).await;
    backend.put(
        Source::Jm,
        "A",
        1,
        page(1, 1, vec![work(Source::Jm, "100", &["A", "B"])]),
    );
    backend.put(
        Source::Jm,
        "B",
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
    follow(&service, &scopes[0], "A", false).await;
    let remaining = service.discovery_read(scopes.clone()).await.unwrap();
    assert_eq!(remaining.records.len(), 1);
    assert_eq!(remaining.records[0].matched_authors, ["B"]);
    follow(&service, &scopes[1], "B", false).await;
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
    follow(&service, &scopes[0], "A", true).await;
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
    let mut response = page(1, 1, vec![work(Source::Jm, "100", &["A"])]);
    response.total = None;
    assert!(!traversal.append(&response).unwrap());
    response = page(2, 2, vec![work(Source::Jm, "101", &["A"])]);
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
    follow(&service, &scopes[0], "A", true).await;
    let first = service
        .discovery_start(scopes.clone(), vec![])
        .await
        .unwrap();
    finish(&service, &scopes).await;
    assert!(service
        .discovery_run_is_live(&scopes, &first.run_id)
        .is_ok());
    follow(&service, &scopes[1], "B", true).await;
    assert_eq!(
        service
            .discovery_run_is_live(&scopes, &first.run_id)
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    let second = service
        .discovery_start(scopes.clone(), vec!["A".into()])
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

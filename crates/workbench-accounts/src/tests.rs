use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::Notify;
use workbench_credentials::{
    test_support::MemoryVault, CredentialKind, StoredCredential, Vault, VaultError,
};
use workbench_sources::{FavoritePageRequest, FavoriteUpdate};

#[derive(Clone, Default)]
struct SharedVault(Arc<MemoryVault>);
impl Vault for SharedVault {
    fn compare_exchange(
        &self,
        source: Source,
        expected: Option<[u8; 32]>,
        next: Option<&StoredCredential>,
    ) -> workbench_credentials::Result<()> {
        self.0.compare_exchange(source, expected, next)
    }
    fn load(&self, source: Source) -> workbench_credentials::Result<Option<StoredCredential>> {
        self.0.load(source)
    }
    fn save(
        &self,
        source: Source,
        credential: &StoredCredential,
    ) -> workbench_credentials::Result<()> {
        self.0.save(source, credential)
    }
    fn delete(&self, source: Source) -> workbench_credentials::Result<()> {
        self.0.delete(source)
    }
}

#[derive(Default)]
struct FakeState {
    restore_calls: AtomicUsize,
    query_calls: AtomicUsize,
    favorite_calls: AtomicUsize,
    favorite: AtomicBool,
    unknown_write: AtomicBool,
    query_failure: Mutex<Option<&'static str>>,
    restore_failure: Mutex<Option<&'static str>>,
    work_title: Mutex<Option<String>>,
    block_cover: AtomicBool,
    cover_started: Notify,
    cover_release: Notify,
    detail_calls: AtomicUsize,
    cover_calls: AtomicUsize,
    cover_missing_once: AtomicBool,
    cover_image: Mutex<Option<String>>,
    query_batch_size: AtomicUsize,
    last_request: Mutex<Option<(u64, Option<String>, bool)>>,
    block_detail: AtomicBool,
    detail_started: Notify,
    detail_release: Notify,
}
#[derive(Clone, Default)]
struct FakeBackend(Arc<FakeState>);
struct FakeSession {
    source: Source,
}

fn credential(source: Source, name: &str) -> StoredCredential {
    StoredCredential::new(
        name,
        match source {
            Source::Jm => CredentialKind::SessionCookie,
            Source::Pica => CredentialKind::SessionToken,
        },
        format!("server-session-{name}"),
    )
    .unwrap()
}

fn work(source: Source, favorite: bool) -> SourceWork {
    SourceWork {
        source,
        work_id: "123".into(),
        title: "Synthetic source work".into(),
        authors: vec!["Synthetic Author".into()],
        description: None,
        tags: vec![],
        favorite: Some(favorite),
        chapter_count: None,
        page_count: None,
        cover_available: true,
    }
}

fn page(source: Source, favorite: bool) -> SourcePage {
    SourcePage {
        items: vec![work(source, favorite)],
        page: 1,
        total: Some(5),
        pages: None,
        has_more: None,
        folders: vec![],
    }
}

impl SourceBackend for FakeBackend {
    type Session = FakeSession;
    async fn login(
        &self,
        source: Source,
        username: &str,
        password: &str,
    ) -> Result<Authenticated<Self::Session>> {
        if password == "rejected" {
            return Err(AccountError::new("LOGIN_REJECTED"));
        }
        Ok(Authenticated {
            session: FakeSession { source },
            account: SourceAccount {
                source,
                account_id: username.into(),
                display_name: username.into(),
            },
            credential: credential(source, username),
        })
    }
    async fn restore(
        &self,
        source: Source,
        saved: &StoredCredential,
    ) -> Result<Authenticated<Self::Session>> {
        self.0.restore_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(code) = *self.0.restore_failure.lock().unwrap() {
            return Err(AccountError::new(code));
        }
        self.login(source, saved.account_name(), "fixture-only")
            .await
    }
    async fn favorites(
        &self,
        session: &Self::Session,
        request: FavoritePageRequest,
    ) -> Result<SourcePage> {
        self.0.query_calls.fetch_add(1, Ordering::SeqCst);
        *self.0.last_request.lock().unwrap() =
            Some((request.page, request.folder_id.clone(), request.reverse));
        if let Some(code) = *self.0.query_failure.lock().unwrap() {
            return Err(AccountError::new(code));
        }
        let mut result = page(session.source, self.0.favorite.load(Ordering::SeqCst));
        if let Some(title) = self.0.work_title.lock().unwrap().clone() {
            result.items[0].title = title;
        }
        let count = self.0.query_batch_size.load(Ordering::SeqCst);
        if count > 0 {
            result.items = (0..count)
                .map(|offset| {
                    let mut item = work(session.source, false);
                    item.work_id = ((request.page - 1) * count as u64 + offset as u64).to_string();
                    item
                })
                .collect();
            result.page = request.page;
            result.total = Some(3 * count as u64);
            result.pages = Some(3);
            result.has_more = Some(request.page < 3);
        }
        Ok(result)
    }
    async fn search(&self, session: &Self::Session, _: &str, _: u64) -> Result<SourcePage> {
        self.favorites(
            session,
            FavoritePageRequest {
                page: 1,
                folder_id: None,
                reverse: false,
            },
        )
        .await
    }
    async fn detail(&self, session: &Self::Session, id: &str) -> Result<SourceWork> {
        self.0.detail_calls.fetch_add(1, Ordering::SeqCst);
        if self.0.block_detail.load(Ordering::SeqCst) {
            self.0.detail_started.notify_one();
            self.0.detail_release.notified().await;
        }
        let mut item = work(session.source, self.0.favorite.load(Ordering::SeqCst));
        item.work_id = id.into();
        Ok(item)
    }
    async fn favorite(
        &self,
        _: &Self::Session,
        work_id: &str,
        desired: bool,
    ) -> Result<FavoriteUpdate> {
        self.0.favorite_calls.fetch_add(1, Ordering::SeqCst);
        if self.0.unknown_write.load(Ordering::SeqCst) {
            return Err(AccountError::new("FAVORITE_OUTCOME_UNKNOWN"));
        }
        let old = self.0.favorite.swap(desired, Ordering::SeqCst);
        Ok(FavoriteUpdate {
            work_id: work_id.into(),
            favorite: desired,
            changed: old != desired,
            verified: true,
        })
    }
    async fn cover(&self, _: &Self::Session, _: &str) -> Result<Option<String>> {
        self.0.cover_calls.fetch_add(1, Ordering::SeqCst);
        if self.0.block_cover.load(Ordering::SeqCst) {
            self.0.cover_started.notify_one();
            self.0.cover_release.notified().await;
        }
        if self.0.cover_missing_once.swap(false, Ordering::SeqCst) {
            return Err(AccountError::new("WORK_NOT_LOADED"));
        }
        Ok(self.0.cover_image.lock().unwrap().clone())
    }
}

fn service(
    root: &TempDir,
    backend: FakeBackend,
    vault: SharedVault,
) -> AccountService<FakeBackend, SharedVault> {
    AccountService::new(backend, vault, root.path().to_path_buf())
}

fn error<T>(result: Result<T>) -> &'static str {
    match result {
        Ok(_) => panic!("expected fixed error"),
        Err(error) => error.code,
    }
}

async fn login(
    service: &AccountService<FakeBackend, SharedVault>,
    source: Source,
    name: &str,
    remember: bool,
) -> String {
    service
        .login(
            source,
            name.into(),
            "password-canary-no-storage".into(),
            remember,
        )
        .await
        .unwrap()
        .session_id
        .unwrap()
}

async fn query(
    service: &AccountService<FakeBackend, SharedVault>,
    source: Source,
    session: &str,
) -> Result<QueryResult> {
    service
        .query(source, session, QueryKind::Favorites, "", None, 1)
        .await
}

#[tokio::test]
async fn empty_vault_never_authenticates_or_queries_a_source() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    assert!(service
        .accounts(false)
        .await
        .iter()
        .all(|a| a.state == AccountState::Disconnected));
    assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        error(query(&service, Source::Jm, "fabricated").await),
        "SESSION_CHANGED"
    );
    assert_eq!(backend.0.query_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn remember_saves_only_the_server_session_and_snapshot_contains_no_secret() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let session = login(&service, Source::Jm, "fixture-jm", true).await;
    let saved = vault.load(Source::Jm).unwrap().unwrap();
    assert_eq!(saved.secret(), "server-session-fixture-jm");
    let summaries = serde_json::to_string(&service.accounts(false).await).unwrap();
    assert!(!summaries.contains("password-canary"));
    assert!(!summaries.contains("server-session-"));
    assert!(summaries.contains(&session));
    assert!(vault.load(Source::Pica).unwrap().is_none());
}

#[tokio::test]
async fn saved_session_requires_source_validation_and_restores_only_once() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let vault = SharedVault::default();
    vault
        .save(Source::Pica, &credential(Source::Pica, "saved"))
        .unwrap();
    let service = service(&root, backend.clone(), vault);
    let first = service.accounts(false).await;
    assert_eq!(first[1].state, AccountState::Connected);
    let second = service.accounts(false).await;
    assert_eq!(first[1].session_id, second[1].session_id);
    assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_saved_session_is_preserved_and_manual_recheck_can_recover() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    let vault = SharedVault::default();
    vault
        .save(Source::Pica, &credential(Source::Pica, "saved"))
        .unwrap();
    let service = service(&root, backend.clone(), vault.clone());
    assert_eq!(
        service.accounts(false).await[1].state,
        AccountState::Expired
    );
    assert!(vault.load(Source::Pica).unwrap().is_some());
    *backend.0.restore_failure.lock().unwrap() = None;
    assert_eq!(
        service.accounts(true).await[1].state,
        AccountState::Connected
    );
}

#[tokio::test]
async fn rejected_account_switch_preserves_previous_session() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let old = login(&service, Source::Jm, "first", true).await;
    assert_eq!(
        error(
            service
                .login(Source::Jm, "second".into(), "rejected".into(), true)
                .await
        ),
        "LOGIN_REJECTED"
    );
    assert_eq!(
        service.accounts(false).await[0].session_id.as_deref(),
        Some(old.as_str())
    );
    assert_eq!(
        vault.load(Source::Jm).unwrap().unwrap().account_name(),
        "first"
    );
}

#[tokio::test]
async fn successful_account_switch_invalidates_old_scope_and_cached_work() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let old = login(&service, Source::Jm, "first", true).await;
    query(&service, Source::Jm, &old).await.unwrap();
    let new = login(&service, Source::Jm, "second", true).await;
    assert_ne!(old, new);
    assert_eq!(
        error(query(&service, Source::Jm, &old).await),
        "SESSION_CHANGED"
    );
    assert_eq!(
        error(service.favorite(Source::Jm, &new, "123", true).await),
        "WORK_NOT_LOADED"
    );
    assert_eq!(backend.0.query_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn persistence_failure_is_not_a_successful_account_switch() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let old = login(&service, Source::Jm, "first", true).await;
    vault
        .0
        .set_failure(Some(VaultError::ACCESS_DENIED))
        .unwrap();
    assert_eq!(
        error(
            service
                .login(Source::Jm, "second".into(), "valid".into(), true)
                .await
        ),
        "VAULT_ACCESS_DENIED"
    );
    vault.0.set_failure(None).unwrap();
    assert_eq!(
        service.accounts(false).await[0].session_id.as_deref(),
        Some(old.as_str())
    );
}

#[tokio::test]
async fn unremembered_login_removes_saved_session_and_logout_leaves_other_source() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    login(&service, Source::Jm, "old", true).await;
    let pica = login(&service, Source::Pica, "other", true).await;
    let jm = login(&service, Source::Jm, "temporary", false).await;
    assert!(vault.load(Source::Jm).unwrap().is_none());
    service.logout(Source::Jm, Some(&jm)).await.unwrap();
    assert_eq!(
        service.accounts(false).await[1].session_id.as_deref(),
        Some(pica.as_str())
    );
}

#[tokio::test]
async fn external_credential_change_blocks_old_requests_and_stale_logout() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let vault = SharedVault::default();
    let service = service(&root, backend.clone(), vault.clone());
    let old = login(&service, Source::Jm, "old", true).await;
    vault
        .save(Source::Jm, &credential(Source::Jm, "another-process"))
        .unwrap();
    assert_eq!(
        error(query(&service, Source::Jm, &old).await),
        "SESSION_CHANGED"
    );
    assert_eq!(
        error(service.logout(Source::Jm, Some(&old)).await),
        "SESSION_CHANGED"
    );
    assert_eq!(
        vault.load(Source::Jm).unwrap().unwrap().account_name(),
        "another-process"
    );
    assert_eq!(backend.0.query_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn network_failure_preserves_session_but_expiry_invalidates_it() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let session = login(&service, Source::Jm, "fixture", true).await;
    *backend.0.query_failure.lock().unwrap() = Some("SOURCE_ACCESS_DENIED");
    assert_eq!(
        error(query(&service, Source::Jm, &session).await),
        "SOURCE_ACCESS_DENIED"
    );
    assert_eq!(
        service.accounts(false).await[0].state,
        AccountState::Connected
    );
    *backend.0.query_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    assert_eq!(
        error(query(&service, Source::Jm, &session).await),
        "SESSION_EXPIRED"
    );
    assert_eq!(
        service.accounts(false).await[0].state,
        AccountState::Expired
    );
}

#[tokio::test]
async fn incomplete_page_and_unknown_metadata_remain_unknown() {
    let root = TempDir::new().unwrap();
    let service = service(&root, FakeBackend::default(), SharedVault::default());
    let session = login(&service, Source::Jm, "fixture", false).await;
    let result = query(&service, Source::Jm, &session).await.unwrap();
    assert_eq!(result.page.total, Some(5));
    assert_eq!(result.page.has_more, None);
    assert_eq!(result.page.items[0].page_count, None);
}

#[tokio::test]
async fn uncertain_favorite_cannot_be_toggled_again_before_target_is_observed() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let session = login(&service, Source::Jm, "fixture", false).await;
    query(&service, Source::Jm, &session).await.unwrap();
    backend.0.unknown_write.store(true, Ordering::SeqCst);
    assert_eq!(
        error(service.favorite(Source::Jm, &session, "123", true).await),
        "FAVORITE_OUTCOME_UNKNOWN"
    );
    query(&service, Source::Jm, &session).await.unwrap();
    assert_eq!(
        error(service.favorite(Source::Jm, &session, "123", true).await),
        "FAVORITE_RECONCILIATION_REQUIRED"
    );
    assert_eq!(backend.0.favorite_calls.load(Ordering::SeqCst), 1);
    backend.0.favorite.store(true, Ordering::SeqCst);
    backend.0.unknown_write.store(false, Ordering::SeqCst);
    query(&service, Source::Jm, &session).await.unwrap();
    assert!(
        !service
            .favorite(Source::Jm, &session, "123", true)
            .await
            .unwrap()
            .changed
    );
}

#[tokio::test]
async fn slow_cover_does_not_block_logout_and_its_old_result_is_discarded() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = Arc::new(service(&root, backend.clone(), SharedVault::default()));
    let session = login(&service, Source::Jm, "fixture", false).await;
    query(&service, Source::Jm, &session).await.unwrap();
    backend.0.block_cover.store(true, Ordering::SeqCst);
    let pending = {
        let service = Arc::clone(&service);
        let session = session.clone();
        tokio::spawn(async move { service.cover(Source::Jm, &session, "123").await })
    };
    backend.0.cover_started.notified().await;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        service.logout(Source::Jm, Some(&session)),
    )
    .await
    .unwrap()
    .unwrap();
    backend.0.cover_release.notify_one();
    assert_eq!(error(pending.await.unwrap()), "SESSION_CHANGED");
}

#[tokio::test]
async fn local_following_is_revisioned_account_scoped_and_survives_reopening() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let first = login(&service, Source::Jm, "first", true).await;
    query(&service, Source::Jm, &first).await.unwrap();
    let saved = service
        .follow(Source::Jm, &first, FollowKind::Work, "123", true, 0)
        .await
        .unwrap();
    assert_eq!(saved.works[0].title, "Synthetic source work");
    let second = login(&service, Source::Jm, "second", true).await;
    assert!(service
        .following(Source::Jm, &second)
        .await
        .unwrap()
        .works
        .is_empty());
    service
        .follow(
            Source::Jm,
            &second,
            FollowKind::Author,
            "Another Author",
            true,
            saved.revision,
        )
        .await
        .unwrap();
    assert_eq!(
        error(
            service
                .follow(
                    Source::Jm,
                    &second,
                    FollowKind::Author,
                    "Stale Write",
                    true,
                    saved.revision
                )
                .await
        ),
        "REVISION_CONFLICT"
    );
    login(&service, Source::Jm, "first", true).await;
    drop(service);
    let reopened = AccountService::new(FakeBackend::default(), vault, root.path().to_path_buf());
    let summaries = reopened.accounts(false).await;
    let current = summaries[0].session_id.as_ref().unwrap();
    let restored = reopened.following(Source::Jm, current).await.unwrap();
    assert_eq!(restored.works.len(), 1);
    assert!(restored.authors.is_empty());
}

#[tokio::test]
async fn same_work_id_from_other_source_cannot_use_the_first_sources_scope() {
    let root = TempDir::new().unwrap();
    let service = service(&root, FakeBackend::default(), SharedVault::default());
    let jm = login(&service, Source::Jm, "fixture", false).await;
    let pica = login(&service, Source::Pica, "fixture", false).await;
    query(&service, Source::Jm, &jm).await.unwrap();
    assert_eq!(
        error(service.favorite(Source::Pica, &jm, "123", true).await),
        "SESSION_CHANGED"
    );
    assert_eq!(
        error(service.favorite(Source::Pica, &pica, "123", true).await),
        "WORK_NOT_LOADED"
    );
}

#[tokio::test]
async fn following_label_bounds_unicode_without_changing_the_remote_work() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let original = "𠮷\n".repeat(210);
    *backend.0.work_title.lock().unwrap() = Some(original.clone());
    let service = service(&root, backend, SharedVault::default());
    let session = login(&service, Source::Jm, "long-label", false).await;
    query(&service, Source::Jm, &session).await.unwrap();
    let revision = service
        .following(Source::Jm, &session)
        .await
        .unwrap()
        .revision;
    let saved = service
        .follow(
            Source::Jm,
            &session,
            FollowKind::Work,
            "123",
            true,
            revision,
        )
        .await
        .unwrap();
    assert_eq!(saved.works[0].title.chars().count(), 200);
    assert!(!saved.works[0].title.chars().any(char::is_control));
    assert_eq!(
        query(&service, Source::Jm, &session)
            .await
            .unwrap()
            .page
            .items[0]
            .title,
        original
    );
}

#[derive(Clone)]
struct RacingVault {
    inner: SharedVault,
    replacement: Arc<Mutex<Option<StoredCredential>>>,
}
impl Vault for RacingVault {
    fn load(&self, source: Source) -> workbench_credentials::Result<Option<StoredCredential>> {
        self.inner.load(source)
    }
    fn save(&self, source: Source, value: &StoredCredential) -> workbench_credentials::Result<()> {
        self.inner.save(source, value)
    }
    fn delete(&self, source: Source) -> workbench_credentials::Result<()> {
        self.inner.delete(source)
    }
    fn compare_exchange(
        &self,
        source: Source,
        expected: Option<[u8; 32]>,
        next: Option<&StoredCredential>,
    ) -> workbench_credentials::Result<()> {
        if let Some(replacement) = self.replacement.lock().unwrap().take() {
            self.inner.save(source, &replacement)?;
        }
        self.inner.compare_exchange(source, expected, next)
    }
}
#[tokio::test]
async fn credential_change_between_logout_check_and_delete_is_preserved() {
    let root = TempDir::new().unwrap();
    let vault = RacingVault {
        inner: SharedVault::default(),
        replacement: Arc::new(Mutex::new(None)),
    };
    let service = AccountService::new(
        FakeBackend::default(),
        vault.clone(),
        root.path().to_path_buf(),
    );
    let old = service
        .login(Source::Jm, "old".into(), "synthetic".into(), true)
        .await
        .unwrap()
        .session_id
        .unwrap();
    *vault.replacement.lock().unwrap() = Some(credential(Source::Jm, "new-other-instance"));
    assert_eq!(
        error(service.logout(Source::Jm, Some(&old)).await),
        "CREDENTIAL_CHANGED"
    );
    assert_eq!(
        vault.load(Source::Jm).unwrap().unwrap().account_name(),
        "new-other-instance"
    );
    assert_ne!(
        service.accounts(false).await[0].state,
        AccountState::Connected
    );
}
#[tokio::test]
async fn concurrent_saved_login_is_not_overwritten_by_a_late_login_commit() {
    let root = TempDir::new().unwrap();
    let vault = RacingVault {
        inner: SharedVault::default(),
        replacement: Arc::new(Mutex::new(None)),
    };
    let service = AccountService::new(
        FakeBackend::default(),
        vault.clone(),
        root.path().to_path_buf(),
    );
    *vault.replacement.lock().unwrap() = Some(credential(Source::Pica, "new-other-instance"));
    assert_eq!(
        error(
            service
                .login(Source::Pica, "late".into(), "synthetic".into(), true)
                .await
        ),
        "CREDENTIAL_CHANGED"
    );
    assert_eq!(
        vault.load(Source::Pica).unwrap().unwrap().account_name(),
        "new-other-instance"
    );
}

fn catalog_snapshot() -> CatalogSnapshot {
    CatalogSnapshot {
        items: vec![work(Source::Jm, false)],
        page: 1,
        total: Some(1),
        pages: Some(1),
        has_more: Some(false),
        folders: vec![],
        complete: true,
        updated_at: crate::cache::now_ms().unwrap(),
        first_page_ids: vec!["123".into()],
    }
}
fn cover_image() -> String {
    use base64::Engine;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    )
}

#[tokio::test]
async fn three_thousand_network_items_keep_early_work_and_cover_rehydrates_sources_eviction_once() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    backend.0.query_batch_size.store(1000, Ordering::SeqCst);
    let service = service(&root, backend.clone(), SharedVault::default());
    let session = login(&service, Source::Jm, "fixture", false).await;
    for page in 1..=3 {
        service
            .query(Source::Jm, &session, QueryKind::Favorites, "", None, page)
            .await
            .unwrap();
    }
    backend.0.cover_missing_once.store(true, Ordering::SeqCst);
    service.cover(Source::Jm, &session, "0").await.unwrap();
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend.0.cover_calls.load(Ordering::SeqCst), 2);
    service
        .favorite(Source::Jm, &session, "0", true)
        .await
        .unwrap();
    service
        .favorite(Source::Jm, &session, "2999", true)
        .await
        .unwrap();
    assert_eq!(backend.0.favorite_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn catalog_restart_and_cover_rehydration_never_restore_favorite_authority() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let first = service(&root, backend.clone(), SharedVault::default());
    let generation = login(&first, Source::Jm, "first", false).await;
    first
        .catalog(
            Source::Jm,
            &generation,
            None,
            false,
            CatalogAction::Write,
            Some(catalog_snapshot()),
        )
        .await
        .unwrap();
    drop(first);
    let next = service(&root, backend.clone(), SharedVault::default());
    let session = login(&next, Source::Jm, "first", false).await;
    let restored = next
        .catalog(Source::Jm, &session, None, false, CatalogAction::Read, None)
        .await
        .unwrap();
    assert!(restored.snapshot.is_some());
    assert_eq!(restored.session_id, session);
    assert_eq!(backend.0.query_calls.load(Ordering::SeqCst), 0);
    let image = cover_image();
    *backend.0.cover_image.lock().unwrap() = Some(image.clone());
    assert_eq!(
        next.cover(Source::Jm, &session, "123")
            .await
            .unwrap()
            .data_url,
        Some(image.clone())
    );
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        error(next.favorite(Source::Jm, &session, "123", true).await),
        "WORK_NOT_LOADED"
    );
    assert_eq!(
        next.cover(Source::Jm, &session, "123")
            .await
            .unwrap()
            .data_url,
        Some(image)
    );
    assert_eq!(backend.0.cover_calls.load(Ordering::SeqCst), 1);
    let other = login(&next, Source::Jm, "second", false).await;
    assert!(next
        .catalog(Source::Jm, &other, None, false, CatalogAction::Read, None)
        .await
        .unwrap()
        .snapshot
        .is_none());
    assert_eq!(
        error(next.cover(Source::Jm, &session, "123").await),
        "SESSION_CHANGED"
    );
    assert_eq!(backend.0.favorite_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn catalog_requires_current_account_before_creating_cache_and_cache_errors_keep_session() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("app-data");
    let backend = FakeBackend::default();
    let service = AccountService::new(backend.clone(), SharedVault::default(), root.clone());
    assert_eq!(
        error(
            service
                .catalog(
                    Source::Jm,
                    "fabricated",
                    None,
                    false,
                    CatalogAction::Write,
                    Some(catalog_snapshot())
                )
                .await
        ),
        "SESSION_CHANGED"
    );
    assert!(!root.exists());
    let session = login(&service, Source::Jm, "fixture", false).await;
    let store = workbench_storage::WorkbenchStore::open(&root).unwrap();
    std::fs::write(
        root.join(workbench_storage::PRIVATE_DIRECTORY)
            .join("cache-registry-v1.json"),
        b"broken",
    )
    .unwrap();
    assert_eq!(
        error(
            service
                .catalog(
                    Source::Jm,
                    &session,
                    None,
                    false,
                    CatalogAction::Write,
                    Some(catalog_snapshot())
                )
                .await
        ),
        "CACHE_CORRUPT"
    );
    let image = cover_image();
    *backend.0.cover_image.lock().unwrap() = Some(image.clone());
    assert_eq!(
        service
            .cover(Source::Jm, &session, "123")
            .await
            .unwrap()
            .data_url,
        Some(image)
    );
    assert_eq!(
        service.accounts(false).await[0].state,
        AccountState::Connected
    );
    assert_eq!(store.read_preferences().unwrap().revision, 0);
}

#[tokio::test]
async fn ordered_query_passes_folder_and_reverse_and_legacy_query_defaults_forward() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let session = login(&service, Source::Pica, "fixture", false).await;
    service
        .query_ordered(
            Source::Pica,
            &session,
            QueryKind::Favorites,
            "",
            Some("7".into()),
            2,
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        *backend.0.last_request.lock().unwrap(),
        Some((2, Some("7".into()), true))
    );
    service
        .query(Source::Pica, &session, QueryKind::Favorites, "", None, 1)
        .await
        .unwrap();
    assert_eq!(
        *backend.0.last_request.lock().unwrap(),
        Some((1, None, false))
    );
}

#[tokio::test]
async fn logout_after_unknown_cover_does_not_start_a_fallback_detail_request() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = Arc::new(service(&root, backend.clone(), SharedVault::default()));
    let session = login(&service, Source::Jm, "fixture", false).await;
    query(&service, Source::Jm, &session).await.unwrap();
    backend.0.block_cover.store(true, Ordering::SeqCst);
    backend.0.cover_missing_once.store(true, Ordering::SeqCst);
    let pending = {
        let service = Arc::clone(&service);
        let session = session.clone();
        tokio::spawn(async move { service.cover(Source::Jm, &session, "123").await })
    };
    backend.0.cover_started.notified().await;
    service.logout(Source::Jm, Some(&session)).await.unwrap();
    backend.0.cover_release.notify_one();
    assert_eq!(error(pending.await.unwrap()), "SESSION_CHANGED");
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn logout_during_old_card_detail_prevents_the_next_cover_request() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = Arc::new(service(&root, backend.clone(), SharedVault::default()));
    let session = login(&service, Source::Jm, "fixture", false).await;
    backend.0.block_detail.store(true, Ordering::SeqCst);
    let pending = {
        let service = Arc::clone(&service);
        let session = session.clone();
        tokio::spawn(async move { service.cover(Source::Jm, &session, "old-card").await })
    };
    backend.0.detail_started.notified().await;
    service.logout(Source::Jm, Some(&session)).await.unwrap();
    backend.0.detail_release.notify_one();
    assert_eq!(error(pending.await.unwrap()), "SESSION_CHANGED");
    assert_eq!(backend.0.cover_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn validated_cover_hit_survives_corrupt_touch_without_network_retry() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let session = login(&service, Source::Jm, "fixture", false).await;
    let image = cover_image();
    *backend.0.cover_image.lock().unwrap() = Some(image.clone());
    assert_eq!(
        service
            .cover(Source::Jm, &session, "123")
            .await
            .unwrap()
            .data_url,
        Some(image.clone())
    );
    let registry = root
        .path()
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("cache-registry-v1.json");
    std::fs::write(&registry, b"damaged-touch-registry").unwrap();
    *backend.0.cover_image.lock().unwrap() = None;
    assert_eq!(
        service
            .cover(Source::Jm, &session, "123")
            .await
            .unwrap()
            .data_url,
        Some(image)
    );
    assert_eq!(backend.0.cover_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend.0.detail_calls.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(registry).unwrap(), b"damaged-touch-registry");
    assert_eq!(
        service.accounts(false).await[0].state,
        AccountState::Connected
    );
}

#[tokio::test]
async fn two_slow_network_covers_do_not_block_another_sources_disk_hit() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = Arc::new(service(&root, backend.clone(), SharedVault::default()));
    let jm = login(&service, Source::Jm, "fixture-jm", false).await;
    let pica = login(&service, Source::Pica, "fixture-pica", false).await;
    let image = cover_image();
    *backend.0.cover_image.lock().unwrap() = Some(image.clone());
    service.cover(Source::Pica, &pica, "123").await.unwrap();
    backend.0.block_cover.store(true, Ordering::SeqCst);
    let first = {
        let service = Arc::clone(&service);
        let session = jm.clone();
        tokio::spawn(async move { service.cover(Source::Jm, &session, "slow-a").await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.0.cover_started.notified(),
    )
    .await
    .unwrap();
    let second = {
        let service = Arc::clone(&service);
        let session = jm.clone();
        tokio::spawn(async move { service.cover(Source::Jm, &session, "slow-b").await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.0.cover_started.notified(),
    )
    .await
    .unwrap();
    let cached = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        service.cover(Source::Pica, &pica, "123"),
    )
    .await;
    let network_calls = backend.0.cover_calls.load(Ordering::SeqCst);
    backend.0.cover_release.notify_waiters();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
    })
    .await
    .unwrap();
    assert_eq!(cached.unwrap().unwrap().data_url, Some(image));
    assert_eq!(network_calls, 3);
}

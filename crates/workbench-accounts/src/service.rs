use crate::{
    cache, AccountError, AccountState, AccountSummary, Authenticated, CatalogAction, CatalogResult,
    CatalogSnapshot, CoverResult, FavoriteResult, FollowKind, FollowedWork, FollowingSnapshot,
    QueryKind, QueryResult, Result, Source, SourceAccount, SourceBackend, SourcePage,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, OnceCell, Semaphore};
use workbench_credentials::{StoredCredential, Vault};
use workbench_sources::FavoritePageRequest;
use zeroize::Zeroizing;

const MAX_QUERY_ITEMS: usize = 1000;

struct Slot<S> {
    initialized: bool,
    source: Source,
    session: Option<Arc<S>>,
    account: Option<SourceAccount>,
    session_id: Option<String>,
    state: AccountState,
    saved_fingerprint: Option<[u8; 32]>,
    error_code: Option<&'static str>,
    works: cache::WorkCache,
    uncertain_favorites: BTreeMap<String, bool>,
}

impl<S> Slot<S> {
    fn new(source: Source) -> Self {
        Self {
            initialized: false,
            source,
            session: None,
            account: None,
            session_id: None,
            state: AccountState::Disconnected,
            saved_fingerprint: None,
            error_code: None,
            works: cache::WorkCache::default(),
            uncertain_favorites: BTreeMap::new(),
        }
    }

    fn summary(&self) -> AccountSummary {
        AccountSummary {
            source: self.source,
            session_id: self.session_id.clone(),
            account_id: self.account.as_ref().map(|a| a.account_id.clone()),
            display_name: self.account.as_ref().map(|a| a.display_name.clone()),
            state: self.state,
            remembered: self.saved_fingerprint.is_some(),
            error_code: self.error_code,
        }
    }

    fn invalidate(&mut self, state: AccountState, code: Option<&'static str>) {
        self.session = None;
        self.account = None;
        self.session_id = None;
        self.state = state;
        self.error_code = code;
        self.works.clear();
        self.uncertain_favorites.clear();
    }
}

pub struct AccountService<B: SourceBackend, V: Vault> {
    backend: B,
    vault: Arc<V>,
    root: PathBuf,
    slots: [Mutex<Slot<B::Session>>; 2],
    cover_slots: Semaphore,
    // Catalog writes and explicitly requested cleanup share the fixed file lock.
    cache_io: Arc<Mutex<()>>,
    legacy_cover_cleanup: OnceCell<Result<()>>,
}

impl<B: SourceBackend, V: Vault + 'static> AccountService<B, V> {
    pub fn new(backend: B, vault: V, app_data_root: PathBuf) -> Self {
        Self {
            backend,
            vault: Arc::new(vault),
            root: app_data_root,
            cover_slots: Semaphore::new(2),
            cache_io: Arc::new(Mutex::new(())),
            legacy_cover_cleanup: OnceCell::new(),
            slots: [
                Mutex::new(Slot::new(Source::Jm)),
                Mutex::new(Slot::new(Source::Pica)),
            ],
        }
    }

    /// Explicit cleanup only; account reads never initiate filesystem migration.
    /// The desktop owner orders automatic cleanup before publishing its shared
    /// document store. This helper retains its result for explicit callers.
    pub async fn cleanup_legacy_cover_cache(&self) -> Result<()> {
        *self
            .legacy_cover_cleanup
            .get_or_init(|| async {
                for attempt in 0..3 {
                    let root = self.root.clone();
                    let cache_io = Arc::clone(&self.cache_io).lock_owned().await;
                    let result = tokio::task::spawn_blocking(move || {
                        let _cache_io = cache_io;
                        workbench_storage::WorkbenchStore::open(root)?.cleanup_legacy_cover_cache()
                    })
                    .await
                    .map_err(|_| AccountError::new("CACHE_UNAVAILABLE"))?
                    .map_err(|error| AccountError::new(error.code));
                    if result.as_ref().is_err_and(|error| error.code == "BUSY") && attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                    return result;
                }
                Err(AccountError::new("BUSY"))
            })
            .await
    }

    fn slot(&self, source: Source) -> &Mutex<Slot<B::Session>> {
        &self.slots[match source {
            Source::Jm => 0,
            Source::Pica => 1,
        }]
    }

    async fn initialize(&self, slot: &mut Slot<B::Session>) {
        if slot.initialized {
            return;
        }
        slot.initialized = true;
        let credential = match self.vault.load(slot.source) {
            Ok(Some(value)) => value,
            Ok(None) => return,
            Err(error) => {
                slot.invalidate(AccountState::Unavailable, Some(error.code));
                return;
            }
        };
        slot.saved_fingerprint = Some(fingerprint(&credential));
        match self.backend.restore(slot.source, &credential).await {
            Ok(auth) => {
                // A concurrent application may have replaced the credential while
                // the network validation was pending. Never publish that old profile.
                if self.check_saved(slot).is_ok() {
                    if fingerprint(&auth.credential) != fingerprint(&credential) {
                        if let Err(error) = self.vault.compare_exchange(
                            slot.source,
                            Some(fingerprint(&credential)),
                            Some(&auth.credential),
                        ) {
                            slot.invalidate(AccountState::Unavailable, Some(error.code));
                            return;
                        }
                    }
                    if let Err(error) = self.install(slot, auth, true) {
                        slot.invalidate(AccountState::Unavailable, Some(error.code));
                    } else {
                        let _ = self.check_saved(slot);
                    }
                }
            }
            Err(error) => slot.invalidate(
                if authentication_error(error.code) {
                    AccountState::Expired
                } else {
                    AccountState::Unavailable
                },
                Some(error.code),
            ),
        }
    }

    fn install(
        &self,
        slot: &mut Slot<B::Session>,
        auth: Authenticated<B::Session>,
        remembered: bool,
    ) -> Result<()> {
        if auth.account.source != slot.source
            || auth.account.account_id.is_empty()
            || auth.account.account_id.len() > 256
        {
            return Err(AccountError::new("ACCOUNT_PROFILE_INVALID"));
        }
        let mut random = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let session_id = hex(&random);
        let saved = remembered.then(|| fingerprint(&auth.credential));
        slot.invalidate(AccountState::Connected, None);
        slot.initialized = true;
        slot.account = Some(auth.account);
        slot.session = Some(Arc::new(auth.session));
        slot.session_id = Some(session_id);
        slot.saved_fingerprint = saved;
        Ok(())
    }

    pub async fn accounts(&self, refresh: bool) -> Vec<AccountSummary> {
        async fn read<B: SourceBackend, V: Vault + 'static>(
            service: &AccountService<B, V>,
            source: Source,
            refresh: bool,
        ) -> AccountSummary {
            let mut slot = service.slot(source).lock().await;
            if refresh && slot.session.is_none() {
                slot.initialized = false;
                slot.invalidate(AccountState::Disconnected, None);
                slot.saved_fingerprint = None;
            }
            service.initialize(&mut slot).await;
            if slot.session.is_some() {
                let _ = service.check_saved(&mut slot);
            }
            slot.summary()
        }
        let (jm, pica) = tokio::join!(
            read(self, Source::Jm, refresh),
            read(self, Source::Pica, refresh)
        );
        vec![jm, pica]
    }

    pub async fn login(
        &self,
        source: Source,
        username: String,
        password: String,
        remember: bool,
    ) -> Result<AccountSummary> {
        let username = Zeroizing::new(username);
        let password = Zeroizing::new(password);
        if username.trim().is_empty()
            || username.len() > 1024
            || password.is_empty()
            || password.len() > 4096
            || username.chars().any(char::is_control)
            || password.chars().any(|c| matches!(c, '\0' | '\r' | '\n'))
        {
            return Err(AccountError::new("LOGIN_INPUT_INVALID"));
        }
        let mut slot = self.slot(source).lock().await;
        let expected = self
            .vault
            .load(source)
            .map_err(|e| AccountError::new(e.code))?
            .as_ref()
            .map(fingerprint);
        let auth = self.backend.login(source, &username, &password).await?;
        if auth.account.source != source
            || auth.account.account_id.is_empty()
            || auth.account.account_id.len() > 256
        {
            return Err(AccountError::new("ACCOUNT_PROFILE_INVALID"));
        }
        // Successful login and requested persistence must both finish before the
        // renderer receives a connected summary. Passwords never enter the vault.
        self.vault
            .compare_exchange(source, expected, remember.then_some(&auth.credential))
            .map_err(|e| AccountError::new(e.code))?;
        self.install(&mut slot, auth, remember)?;
        self.check_saved(&mut slot)?;
        Ok(slot.summary())
    }

    pub async fn logout(&self, source: Source, session_id: Option<&str>) -> Result<AccountSummary> {
        let mut slot = self.slot(source).lock().await;
        if !slot.initialized || slot.session_id.as_deref() != session_id {
            return Err(AccountError::new("SESSION_CHANGED"));
        }
        self.check_saved(&mut slot)?;
        self.vault
            .compare_exchange(source, slot.saved_fingerprint, None)
            .map_err(|e| AccountError::new(e.code))?;
        slot.invalidate(AccountState::Disconnected, None);
        slot.saved_fingerprint = None;
        slot.initialized = true;
        Ok(slot.summary())
    }

    fn check_saved(&self, slot: &mut Slot<B::Session>) -> Result<()> {
        let expected = slot.saved_fingerprint;
        match self.vault.load(slot.source) {
            Ok(None) if expected.is_none() => Ok(()),
            Ok(Some(value)) if Some(fingerprint(&value)) == expected => Ok(()),
            Ok(_) => {
                slot.saved_fingerprint = None;
                slot.invalidate(AccountState::Disconnected, Some("SESSION_CHANGED"));
                Err(AccountError::new("SESSION_CHANGED"))
            }
            Err(error) => {
                slot.invalidate(AccountState::Unavailable, Some(error.code));
                Err(AccountError::new(error.code))
            }
        }
    }

    fn require_scope(&self, slot: &mut Slot<B::Session>, session_id: &str) -> Result<()> {
        if slot.session_id.as_deref() != Some(session_id) || slot.session.is_none() {
            return Err(AccountError::new("SESSION_CHANGED"));
        }
        self.check_saved(slot)
    }

    fn finish<T>(&self, slot: &mut Slot<B::Session>, result: Result<T>) -> Result<T> {
        self.check_saved(slot)?;
        if let Err(error) = &result {
            if authentication_error(error.code) {
                slot.invalidate(AccountState::Expired, Some(error.code));
            }
        }
        result
    }

    pub async fn query(
        &self,
        source: Source,
        session_id: &str,
        kind: QueryKind,
        query: &str,
        folder_id: Option<String>,
        page: u64,
    ) -> Result<QueryResult> {
        self.query_ordered(source, session_id, kind, query, folder_id, page, false)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn query_ordered(
        &self,
        source: Source,
        session_id: &str,
        kind: QueryKind,
        query: &str,
        folder_id: Option<String>,
        page: u64,
        reverse: bool,
    ) -> Result<QueryResult> {
        if reverse && !matches!(kind, QueryKind::Favorites) {
            return Err(AccountError::new("QUERY_INVALID"));
        }
        if !(1..=1000).contains(&page)
            || query.len() > 2048
            || query.chars().any(char::is_control)
            || folder_id
                .as_ref()
                .is_some_and(|id| id.len() > 80 || id.chars().any(char::is_control))
        {
            return Err(AccountError::new("QUERY_INVALID"));
        }
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        let session = slot
            .session
            .as_ref()
            .ok_or(AccountError::new("AUTH_REQUIRED"))?;
        let result = match kind {
            QueryKind::Favorites => {
                self.backend
                    .favorites(
                        session,
                        FavoritePageRequest {
                            page,
                            folder_id,
                            reverse,
                        },
                    )
                    .await
            }
            QueryKind::Search => self.backend.search(session, query.trim(), page).await,
            QueryKind::Detail => self
                .backend
                .detail(session, query.trim())
                .await
                .map(|work| SourcePage {
                    items: vec![work],
                    page: 1,
                    total: Some(1),
                    pages: Some(1),
                    has_more: Some(false),
                    folders: vec![],
                }),
        };
        let result = self.finish(&mut slot, result)?;
        if result.items.len() > MAX_QUERY_ITEMS
            || result.items.iter().any(|work| work.source != source)
        {
            return Err(AccountError::new("SOURCE_RESPONSE_INVALID"));
        }
        let sizes: Vec<_> = result
            .items
            .iter()
            .map(|work| cache::validate_work(source, work))
            .collect::<Result<_>>()?;
        for (work, bytes) in result.items.iter().zip(sizes) {
            if slot
                .uncertain_favorites
                .get(&work.work_id)
                .is_some_and(|desired| work.favorite == Some(*desired))
            {
                slot.uncertain_favorites.remove(&work.work_id);
            }
            slot.works.insert(work.clone(), bytes);
        }
        Ok(QueryResult {
            source,
            session_id: session_id.to_owned(),
            page: result,
        })
    }

    pub async fn favorite(
        &self,
        source: Source,
        session_id: &str,
        work_id: &str,
        desired: bool,
    ) -> Result<FavoriteResult> {
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        if !slot.works.contains_key(work_id) {
            return Err(AccountError::new("WORK_NOT_LOADED"));
        }
        if slot.uncertain_favorites.contains_key(work_id) {
            return Err(AccountError::new("FAVORITE_RECONCILIATION_REQUIRED"));
        }
        let result = self
            .backend
            .favorite(
                slot.session
                    .as_ref()
                    .ok_or(AccountError::new("AUTH_REQUIRED"))?,
                work_id,
                desired,
            )
            .await;
        let result = match self.finish(&mut slot, result) {
            Ok(result) => result,
            Err(error) => {
                if error.code == "FAVORITE_OUTCOME_UNKNOWN" {
                    slot.uncertain_favorites.insert(work_id.into(), desired);
                }
                return Err(error);
            }
        };
        if result.work_id != work_id || !result.verified || result.favorite != desired {
            slot.uncertain_favorites.insert(work_id.into(), desired);
            return Err(AccountError::new("FAVORITE_OUTCOME_UNKNOWN"));
        }
        if let Some(mut work) = slot.works.get(work_id).cloned() {
            work.favorite = Some(result.favorite);
            let bytes = cache::validate_work(source, &work)?;
            slot.works.insert(work, bytes);
        }
        Ok(FavoriteResult {
            source,
            session_id: session_id.into(),
            work_id: result.work_id,
            favorite: result.favorite,
            changed: result.changed,
            verified: true,
        })
    }

    pub async fn cover(
        &self,
        source: Source,
        session_id: &str,
        work_id: &str,
    ) -> Result<CoverResult> {
        let (session, known) = {
            let mut slot = self.slot(source).lock().await;
            self.require_scope(&mut slot, session_id)?;
            if !cache::valid_id(work_id) {
                return Err(AccountError::new("WORK_INPUT_INVALID"));
            }
            let known = slot.works.contains_key(work_id);
            (
                Arc::clone(
                    slot.session
                        .as_ref()
                        .ok_or(AccountError::new("AUTH_REQUIRED"))?,
                ),
                known,
            )
        };
        // Cover bytes live only for this request; the renderer owns run-scoped reuse.
        let _permit = self
            .cover_slots
            .acquire()
            .await
            .map_err(|_| AccountError::new("ACCOUNT_SERVICE_UNAVAILABLE"))?;
        {
            let mut slot = self.slot(source).lock().await;
            self.require_scope(&mut slot, session_id)?;
        }
        // A cache/old-card request may rehydrate metadata, but never installs a
        // work in the network-query authorization cache used by favorite/follow.
        let result = async {
            let mut hydrated = false;
            if !known {
                let work = self.backend.detail(&session, work_id).await?;
                cache::validate_work(source, &work)?;
                if work.work_id != work_id {
                    return Err(AccountError::new("SOURCE_RESPONSE_INVALID"));
                }
                hydrated = true;
            }
            {
                let mut slot = self.slot(source).lock().await;
                self.require_scope(&mut slot, session_id)?;
            }
            let first = self.backend.cover(&session, work_id).await;
            match first {
                Err(error) if error.code == "WORK_NOT_LOADED" && !hydrated => {
                    {
                        let mut slot = self.slot(source).lock().await;
                        self.require_scope(&mut slot, session_id)?;
                    }
                    let work = self.backend.detail(&session, work_id).await?;
                    cache::validate_work(source, &work)?;
                    if work.work_id != work_id {
                        return Err(AccountError::new("SOURCE_RESPONSE_INVALID"));
                    }
                    {
                        let mut slot = self.slot(source).lock().await;
                        self.require_scope(&mut slot, session_id)?;
                    }
                    self.backend.cover(&session, work_id).await
                }
                other => other,
            }
        }
        .await;
        let data_url = {
            let mut slot = self.slot(source).lock().await;
            self.require_scope(&mut slot, session_id)?;
            self.finish(&mut slot, result)?
        };
        if let Some(data_url) = &data_url {
            let image = data_url.clone();
            tokio::task::spawn_blocking(move || cache::validate_cover(&image))
                .await
                .map_err(|_| AccountError::new("SOURCE_COVER_INVALID"))??;
        }
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        Ok(CoverResult {
            source,
            session_id: session_id.into(),
            work_id: work_id.into(),
            data_url,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn catalog(
        &self,
        source: Source,
        session_id: &str,
        folder_id: Option<String>,
        reverse: bool,
        action: CatalogAction,
        snapshot: Option<CatalogSnapshot>,
    ) -> Result<CatalogResult> {
        // Keep this session generation ordered through the local transaction.
        // Persisted metadata never populates the operation-authorization cache.
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        let account = account_key(
            source,
            &slot
                .account
                .as_ref()
                .ok_or(AccountError::new("AUTH_REQUIRED"))?
                .account_id,
        );
        let root = self.root.clone();
        let generation = session_id.to_owned();
        let result = {
            let cache_io = Arc::clone(&self.cache_io).lock_owned().await;
            tokio::task::spawn_blocking(move || {
                let _cache_io = cache_io;
                cache::catalog(
                    &root,
                    &account,
                    source,
                    &generation,
                    folder_id.as_deref(),
                    reverse,
                    action,
                    snapshot,
                )
            })
            .await
            .map_err(|_| AccountError::new("CACHE_UNAVAILABLE"))?
        };
        self.check_saved(&mut slot)?;
        // Cache parsing or filesystem errors never expire a validated session.
        result
    }

    pub async fn following(&self, source: Source, session_id: &str) -> Result<FollowingSnapshot> {
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        let account = slot
            .account
            .as_ref()
            .ok_or(AccountError::new("AUTH_REQUIRED"))?;
        let account_key = account_key(source, &account.account_id);
        let root = self.root.clone();
        let document = tokio::task::spawn_blocking(move || {
            workbench_storage::WorkbenchStore::open(root)?.read_following()
        })
        .await
        .map_err(|_| AccountError::new("STORE_UNAVAILABLE"))?
        .map_err(|error| AccountError::new(error.code))?;
        self.check_saved(&mut slot)?;
        Ok(following_snapshot(
            source,
            session_id,
            &account_key,
            document,
        ))
    }

    #[allow(clippy::too_many_arguments)] // Match the explicit account-scoped command contract.
    pub async fn follow(
        &self,
        source: Source,
        session_id: &str,
        kind: FollowKind,
        value: &str,
        desired: bool,
        expected_revision: u64,
    ) -> Result<FollowingSnapshot> {
        if value.trim().is_empty()
            || value.chars().count() > 200
            || value.chars().any(char::is_control)
        {
            return Err(AccountError::new("FOLLOWING_INPUT_INVALID"));
        }
        let mut slot = self.slot(source).lock().await;
        self.require_scope(&mut slot, session_id)?;
        let key = account_key(
            source,
            &slot
                .account
                .as_ref()
                .ok_or(AccountError::new("AUTH_REQUIRED"))?
                .account_id,
        );
        let mut title = None;
        if matches!(kind, FollowKind::Work) && desired {
            let Some(work) = slot.works.get(value) else {
                return Err(AccountError::new("WORK_NOT_LOADED"));
            };
            let label: String = work
                .title
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(200)
                .collect();
            title = Some(if label.trim().is_empty() {
                work.work_id.clone()
            } else {
                label
            });
        }
        let root = self.root.clone();
        let item = value.to_owned();
        let document_key = key.clone();
        let document = tokio::task::spawn_blocking(move || {
            let store = workbench_storage::WorkbenchStore::open(root)
                .map_err(|e| AccountError::new(e.code))?;
            let document = store
                .read_following()
                .map_err(|e| AccountError::new(e.code))?;
            if document.revision != expected_revision {
                return Err(AccountError::new("REVISION_CONFLICT"));
            }
            let mut next = document.value;
            let storage_source = storage_source(source);
            let index = next.accounts.iter().position(|account| {
                account.source == storage_source && account.account_key == document_key
            });
            let index = match index {
                Some(index) => index,
                None if !desired => {
                    return Ok(workbench_storage::Document {
                        revision: document.revision,
                        value: next,
                    })
                }
                None => {
                    next.accounts.push(workbench_storage::FollowedAccount {
                        source: storage_source,
                        account_key: document_key,
                        works: vec![],
                        authors: vec![],
                    });
                    next.accounts.len() - 1
                }
            };
            let account = &mut next.accounts[index];
            let changed = match kind {
                FollowKind::Work => {
                    let current = account.works.iter().position(|work| work.work_id == item);
                    if let Some(index) = current.filter(|_| !desired) {
                        account.works.remove(index);
                        true
                    } else if current.is_none() && desired {
                        account.works.push(workbench_storage::FollowedWork {
                            work_id: item,
                            title: title.ok_or(AccountError::new("WORK_NOT_LOADED"))?,
                        });
                        true
                    } else {
                        false
                    }
                }
                FollowKind::Author => {
                    let current = account.authors.iter().position(|name| name == &item);
                    if let Some(index) = current.filter(|_| !desired) {
                        account.authors.remove(index);
                        true
                    } else if current.is_none() && desired {
                        account.authors.push(item);
                        true
                    } else {
                        false
                    }
                }
            };
            if !changed {
                return Ok(workbench_storage::Document {
                    revision: document.revision,
                    value: next,
                });
            }
            // Keep scopes only while they have relationships. Removing a follow
            // never deletes the work, saved booklists or any library content.
            next.accounts
                .retain(|account| !account.works.is_empty() || !account.authors.is_empty());
            store
                .write_following(expected_revision, next)
                .map_err(|e| AccountError::new(e.code))
        })
        .await
        .map_err(|_| AccountError::new("STORE_UNAVAILABLE"))??;
        self.check_saved(&mut slot)?;
        Ok(following_snapshot(source, session_id, &key, document))
    }
}

fn fingerprint(credential: &StoredCredential) -> [u8; 32] {
    credential.fingerprint()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn authentication_error(code: &str) -> bool {
    matches!(code, "AUTH_REQUIRED" | "SESSION_EXPIRED")
}

fn storage_source(source: Source) -> workbench_storage::Source {
    match source {
        Source::Jm => workbench_storage::Source::Jm,
        Source::Pica => workbench_storage::Source::Pica,
    }
}

fn account_key(source: Source, account_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(match source {
        Source::Jm => b"JM\0".as_slice(),
        Source::Pica => b"Pica\0".as_slice(),
    });
    digest.update(account_id.as_bytes());
    hex(&digest.finalize())
}

fn following_snapshot(
    source: Source,
    session_id: &str,
    key: &str,
    document: workbench_storage::Document<workbench_storage::AccountFollowing>,
) -> FollowingSnapshot {
    let account = document
        .value
        .accounts
        .into_iter()
        .find(|account| account.source == storage_source(source) && account.account_key == key);
    let (works, authors) = account
        .map(|account| {
            (
                account
                    .works
                    .into_iter()
                    .map(|work| FollowedWork {
                        work_id: work.work_id,
                        title: work.title,
                    })
                    .collect(),
                account.authors,
            )
        })
        .unwrap_or_default();
    FollowingSnapshot {
        source,
        session_id: session_id.into(),
        revision: document.revision,
        works,
        authors,
    }
}

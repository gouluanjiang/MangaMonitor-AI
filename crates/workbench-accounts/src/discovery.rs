//! Explicit, sequential dual-source author catalog reads. Never downloads media.
use crate::{
    AccountError, AccountService, QueryKind, Result, SessionLease, Source, SourceBackend,
    SourcePage, SourceWork,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_credentials::Vault;
use workbench_storage::{
    discovery_author_is_valid, DiscoveryAccount, DiscoveryAuthorRange, DiscoveryDocument,
    DiscoveryRangeState, DiscoveryRecord, DiscoveryWork, Document, WorkbenchStore,
    MAX_DISCOVERY_AUTHORS, MAX_DISCOVERY_PAGES, MAX_DISCOVERY_RECORDS, MAX_SAFE_INTEGER,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryScope {
    pub source: Source,
    pub session_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryPhase {
    Checking,
    Complete,
    Partial,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryRun {
    pub id: String,
    pub phase: DiscoveryPhase,
    pub current_author: Option<String>,
    pub current_source: Option<Source>,
    pub current_page: u64,
    pub requests_used: u64,
    pub completed_scopes: usize,
    pub total_scopes: usize,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySnapshot {
    pub scopes: Vec<DiscoveryScope>,
    pub revision: u64,
    pub run: Option<DiscoveryRun>,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub records: Vec<DiscoveryRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStart {
    pub run_id: String,
    pub snapshot: DiscoverySnapshot,
}

#[derive(Clone)]
pub(crate) struct DiscoveryIdentity {
    pub scope: DiscoveryScope,
    pub account_key: String,
    pub lease: SessionLease,
    pub fingerprint: Option<[u8; 32]>,
}

#[derive(Clone)]
pub(crate) struct DiscoveryContext {
    pub identities: [DiscoveryIdentity; 2],
    pub root: PathBuf,
    pub following_revision: u64,
    pub authors: Vec<String>,
    pub account_key: String,
}

impl DiscoveryContext {
    pub(crate) fn new(
        identities: [DiscoveryIdentity; 2],
        root: PathBuf,
        following: Document<workbench_storage::AccountFollowing>,
    ) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"discovery-pair-v1\0");
        for identity in &identities {
            digest.update(identity.account_key.as_bytes());
            digest.update(b"\0");
        }
        let mut result = Self {
            identities,
            root,
            following_revision: following.revision,
            authors: vec![],
            account_key: format!("{:x}", digest.finalize()),
        };
        result.authors = result.followed_authors(&following.value);
        result
    }

    fn followed_authors(&self, following: &workbench_storage::AccountFollowing) -> Vec<String> {
        following
            .accounts
            .iter()
            .filter(|account| {
                self.identities.iter().any(|identity| {
                    account.source == storage_source(identity.scope.source)
                        && account.account_key == identity.account_key
                })
            })
            .flat_map(|account| {
                account
                    .authors
                    .iter()
                    .map(|author| author.trim().to_owned())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn scopes(&self) -> Vec<DiscoveryScope> {
        self.identities
            .iter()
            .map(|identity| identity.scope.clone())
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct DiscoveryControl {
    memory: Mutex<DiscoveryMemory>,
}

#[derive(Default)]
struct DiscoveryMemory {
    context: Option<DiscoveryContext>,
    snapshot: Option<DiscoverySnapshot>,
    active: bool,
    cancelled: bool,
    invalidated: Option<&'static str>,
}

fn unavailable() -> AccountError {
    AccountError::new("DISCOVERY_UNAVAILABLE")
}
fn store_error(error: workbench_storage::StoreError) -> AccountError {
    AccountError::new(error.code)
}

/// Bounded private document lock contention only; never source IO, auth or CAS.
pub(crate) fn discovery_store_io<T>(
    mut operation: impl FnMut() -> std::result::Result<T, workbench_storage::StoreError>,
) -> std::result::Result<T, workbench_storage::StoreError> {
    for attempt in 0..5 {
        let result = operation();
        if attempt < 4 && result.as_ref().is_err_and(|error| error.code == "BUSY") {
            std::thread::sleep(std::time::Duration::from_millis(20));
        } else {
            return result;
        }
    }
    unreachable!("the final bounded attempt always returns")
}
fn storage_source(source: Source) -> workbench_storage::Source {
    match source {
        Source::Jm => workbench_storage::Source::Jm,
        Source::Pica => workbench_storage::Source::Pica,
    }
}

pub(crate) fn canonical_scopes(mut scopes: Vec<DiscoveryScope>) -> Result<Vec<DiscoveryScope>> {
    if scopes.len() != 2
        || scopes[0].source == scopes[1].source
        || scopes.iter().any(|scope| {
            scope.session_id.trim().is_empty()
                || scope.session_id.len() > 2048
                || scope.session_id.chars().any(char::is_control)
        })
    {
        return Err(AccountError::new("DISCOVERY_SCOPES_REQUIRED"));
    }
    scopes.sort_by_key(|scope| if scope.source == Source::Jm { 0 } else { 1 });
    Ok(scopes)
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or_else(unavailable)
}

fn idle(author: &str, source: Source) -> DiscoveryAuthorRange {
    DiscoveryAuthorRange {
        author: author.into(),
        source: storage_source(source),
        state: DiscoveryRangeState::Idle,
        last_attempt_at: None,
        last_complete_at: None,
        observed_count: 0,
        pages_read: 0,
        error_code: None,
    }
}

fn project(
    context: &DiscoveryContext,
    document: &Document<DiscoveryDocument>,
) -> DiscoverySnapshot {
    let account = document
        .value
        .accounts
        .iter()
        .find(|account| account.account_key == context.account_key);
    let mut authors = vec![];
    for author in &context.authors {
        for source in [Source::Jm, Source::Pica] {
            let mut range = account
                .and_then(|account| {
                    account
                        .authors
                        .iter()
                        .find(|row| row.author == *author && row.source == storage_source(source))
                })
                .cloned()
                .unwrap_or_else(|| idle(author, source));
            // No persisted job is resumed just because a document was opened.
            if range.state == DiscoveryRangeState::Checking {
                range.state = DiscoveryRangeState::Partial;
                range.error_code = Some("DISCOVERY_INTERRUPTED".into());
            }
            authors.push(range);
        }
    }
    DiscoverySnapshot {
        scopes: context.scopes(),
        revision: document.revision,
        run: None,
        authors,
        records: account
            .map(|account| {
                account
                    .records
                    .iter()
                    .filter_map(|record| {
                        let mut record = record.clone();
                        record
                            .matched_authors
                            .retain(|author| context.authors.contains(author));
                        (!record.matched_authors.is_empty()).then_some(record)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

impl DiscoveryControl {
    pub(crate) fn following_changed(&self, revision: u64) {
        if let Ok(mut memory) = self.memory.lock() {
            if memory
                .context
                .as_ref()
                .is_some_and(|context| context.following_revision != revision)
            {
                memory.invalidated = Some("DISCOVERY_FOLLOWING_CHANGED");
                if let Some(run) = memory
                    .snapshot
                    .as_mut()
                    .and_then(|snapshot| snapshot.run.as_mut())
                {
                    if run.phase != DiscoveryPhase::Cancelled {
                        run.phase = DiscoveryPhase::Error;
                        run.error_code = Some("DISCOVERY_FOLLOWING_CHANGED".into());
                    }
                }
            }
        }
    }

    fn check(&self, run_id: &str) -> Result<()> {
        let memory = self.memory.lock().map_err(|_| unavailable())?;
        if let Some(code) = memory.invalidated {
            return Err(AccountError::new(code));
        }
        if !memory.active
            || memory.cancelled
            || memory
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.run.as_ref())
                .is_none_or(|run| run.id != run_id)
        {
            return Err(AccountError::new("DISCOVERY_CANCELLED"));
        }
        Ok(())
    }

    fn progress(
        &self,
        run_id: &str,
        author: &str,
        source: Source,
        page: u64,
        request: bool,
    ) -> Result<()> {
        let mut memory = self.memory.lock().map_err(|_| unavailable())?;
        if let Some(code) = memory.invalidated {
            return Err(AccountError::new(code));
        }
        if !memory.active || memory.cancelled {
            return Err(AccountError::new("DISCOVERY_CANCELLED"));
        }
        let snapshot = memory.snapshot.as_mut().ok_or_else(unavailable)?;
        let run = snapshot
            .run
            .as_mut()
            .filter(|run| run.id == run_id)
            .ok_or_else(unavailable)?;
        run.current_author = Some(author.into());
        run.current_source = Some(source);
        run.current_page = page;
        if request {
            run.requests_used += 1;
        }
        if let Some(range) = snapshot
            .authors
            .iter_mut()
            .find(|range| range.author == author && range.source == storage_source(source))
        {
            range.state = DiscoveryRangeState::Checking;
            range.error_code = None;
        }
        Ok(())
    }

    /// A small disk transaction shares cancellation's lock, never the network await.
    pub(crate) fn commit(
        &self,
        run_id: &str,
        context: &DiscoveryContext,
        revision: u64,
        value: DiscoveryDocument,
    ) -> Result<Document<DiscoveryDocument>> {
        let mut memory = self.memory.lock().map_err(|_| unavailable())?;
        if let Some(code) = memory.invalidated {
            return Err(AccountError::new(code));
        }
        if !memory.active
            || memory.cancelled
            || memory
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.run.as_ref())
                .is_none_or(|run| run.id != run_id)
        {
            return Err(AccountError::new("DISCOVERY_CANCELLED"));
        }
        for identity in &context.identities {
            identity.lease.require_current()?;
        }
        if value
            .accounts
            .iter()
            .map(|account| account.records.len())
            .sum::<usize>()
            > MAX_DISCOVERY_RECORDS
        {
            return Err(AccountError::new("DISCOVERY_LIMIT"));
        }
        let store = WorkbenchStore::open(&context.root).map_err(store_error)?;
        let document = discovery_store_io(|| {
            store.write_discovery_for_following(revision, context.following_revision, value.clone())
        })
        .map_err(|error| {
            if error.code == "DOCUMENT_TOO_LARGE" {
                AccountError::new("DISCOVERY_LIMIT")
            } else {
                store_error(error)
            }
        })?;
        let run = memory
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.run.clone());
        let mut snapshot = project(context, &document);
        snapshot.run = run;
        // In-process checking rows belong to this run, not an interrupted process.
        if let Some(account) = document
            .value
            .accounts
            .iter()
            .find(|account| account.account_key == context.account_key)
        {
            for range in &mut snapshot.authors {
                if let Some(saved) = account
                    .authors
                    .iter()
                    .find(|saved| saved.author == range.author && saved.source == range.source)
                {
                    *range = saved.clone();
                }
            }
        }
        memory.snapshot = Some(snapshot);
        Ok(document)
    }

    fn completed_scope(&self, run_id: &str) {
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(run) = memory
                .snapshot
                .as_mut()
                .and_then(|snapshot| snapshot.run.as_mut())
                .filter(|run| run.id == run_id)
            {
                run.completed_scopes += 1;
            }
        }
    }

    fn finish(&self, run_id: &str, outcome: Result<bool>) {
        if let Ok(mut memory) = self.memory.lock() {
            let cancelled = memory.cancelled;
            if let Some(snapshot) = memory.snapshot.as_mut() {
                if let Some(run) = snapshot.run.as_mut().filter(|run| run.id == run_id) {
                    let (phase, code) = if cancelled {
                        (
                            DiscoveryPhase::Cancelled,
                            Some("DISCOVERY_CANCELLED".into()),
                        )
                    } else {
                        match outcome {
                            Ok(false) => (DiscoveryPhase::Complete, None),
                            Ok(true) => (DiscoveryPhase::Partial, Some("DISCOVERY_PARTIAL".into())),
                            Err(error) => (DiscoveryPhase::Error, Some(error.code.into())),
                        }
                    };
                    run.phase = phase;
                    run.error_code = code.clone();
                    for range in &mut snapshot.authors {
                        if range.state == DiscoveryRangeState::Checking {
                            range.state = if cancelled {
                                DiscoveryRangeState::Cancelled
                            } else {
                                DiscoveryRangeState::Partial
                            };
                            range.error_code = code.clone();
                        }
                    }
                    memory.active = false;
                }
            }
        }
    }
}

impl<B: SourceBackend, V: Vault + 'static> AccountService<B, V> {
    /// Fast verification uses native identity leases and saved credential fingerprints.
    /// It never waits behind a remote metadata request or accepts an account key.
    pub fn discovery_validate_scopes(&self, scopes: &[DiscoveryScope]) -> Result<()> {
        let scopes = canonical_scopes(scopes.to_vec())?;
        let context = self
            .discovery
            .memory
            .lock()
            .map_err(|_| unavailable())?
            .context
            .clone()
            .ok_or(AccountError::new("DISCOVERY_NOT_LOADED"))?;
        if context.scopes() != scopes {
            return Err(AccountError::new("SESSION_CHANGED"));
        }
        self.discovery_validate_context(&context)
    }

    pub fn discovery_run_is_current(&self, scopes: &[DiscoveryScope], run_id: &str) -> Result<()> {
        self.discovery_run_is_live(scopes, run_id)?;
        self.discovery_validate_scopes(scopes)?;
        let memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
        if memory.cancelled
            || memory
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.run.as_ref())
                .is_none_or(|run| {
                    run.id != run_id
                        || matches!(run.phase, DiscoveryPhase::Cancelled | DiscoveryPhase::Error)
                })
        {
            return Err(AccountError::new("DISCOVERY_RUN_CHANGED"));
        }
        let context = memory.context.as_ref().ok_or_else(unavailable)?;
        let following =
            discovery_store_io(|| WorkbenchStore::open(&context.root)?.read_following())
                .map_err(store_error)?;
        if following.revision != context.following_revision {
            return Err(AccountError::new("DISCOVERY_FOLLOWING_CHANGED"));
        }
        Ok(())
    }

    /// Image-lifecycle check: no file, vault, remote request or source lock.
    /// Full external revision/fingerprint checks belong at the work boundary.
    pub fn discovery_run_is_live(&self, scopes: &[DiscoveryScope], run_id: &str) -> Result<()> {
        let scopes = canonical_scopes(scopes.to_vec())?;
        let memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
        if let Some(code) = memory.invalidated {
            return Err(AccountError::new(code));
        }
        let context = memory
            .context
            .as_ref()
            .ok_or(AccountError::new("DISCOVERY_NOT_LOADED"))?;
        if context.scopes() != scopes {
            return Err(AccountError::new("SESSION_CHANGED"));
        }
        for identity in &context.identities {
            identity.lease.require_current()?;
        }
        if memory.cancelled
            || memory
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.run.as_ref())
                .is_none_or(|run| {
                    run.id != run_id
                        || matches!(run.phase, DiscoveryPhase::Cancelled | DiscoveryPhase::Error)
                })
        {
            return Err(AccountError::new("DISCOVERY_RUN_CHANGED"));
        }
        Ok(())
    }

    pub async fn discovery_read(&self, scopes: Vec<DiscoveryScope>) -> Result<DiscoverySnapshot> {
        let scopes = canonical_scopes(scopes)?;
        let cached = {
            let memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
            memory
                .context
                .as_ref()
                .filter(|context| context.scopes() == scopes)
                .zip(memory.snapshot.as_ref())
                .map(|(context, snapshot)| (context.clone(), snapshot.clone()))
        };
        if let Some((context, mut snapshot)) = cached {
            self.discovery_validate_context(&context)?;
            let root = context.root.clone();
            let following = tokio::task::spawn_blocking(move || {
                discovery_store_io(|| WorkbenchStore::open(&root)?.read_following())
            })
            .await
            .map_err(|_| unavailable())?
            .map_err(store_error)?;
            self.discovery_validate_context(&context)?;
            let authors = context.followed_authors(&following.value);
            snapshot
                .authors
                .retain(|range| authors.contains(&range.author));
            for record in &mut snapshot.records {
                record
                    .matched_authors
                    .retain(|author| authors.contains(author));
            }
            snapshot.records.retain(|record| {
                record
                    .matched_authors
                    .iter()
                    .any(|author| authors.contains(author))
            });
            for author in authors {
                for source in [Source::Jm, Source::Pica] {
                    if !snapshot.authors.iter().any(|range| {
                        range.author == author && range.source == storage_source(source)
                    }) {
                        snapshot.authors.push(idle(&author, source));
                    }
                }
            }
            return Ok(snapshot);
        }
        let context = self.discovery_context(scopes).await?;
        let root = context.root.clone();
        let document = tokio::task::spawn_blocking(move || {
            discovery_store_io(|| WorkbenchStore::open(&root)?.read_discovery())
        })
        .await
        .map_err(|_| unavailable())?
        .map_err(store_error)?;
        self.discovery_validate_context(&context)?;
        let snapshot = project(&context, &document);
        let mut memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
        if !memory.active {
            memory.context = Some(context);
            memory.snapshot = Some(snapshot.clone());
        }
        Ok(snapshot)
    }

    pub async fn discovery_start(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        authors: Vec<String>,
    ) -> Result<DiscoveryStart> {
        let scopes = canonical_scopes(scopes)?;
        let context = self.discovery_context(scopes).await?;
        let authors = if authors.is_empty() {
            context.authors.clone()
        } else {
            let mut selected = BTreeSet::new();
            for author in authors {
                if !discovery_author_is_valid(&author)
                    || !context
                        .authors
                        .iter()
                        .any(|followed| followed == author.trim())
                {
                    return Err(AccountError::new("DISCOVERY_AUTHOR_NOT_FOLLOWED"));
                }
                selected.insert(author.trim().to_owned());
            }
            selected.into_iter().collect()
        };
        if authors.is_empty() || authors.len() > MAX_DISCOVERY_AUTHORS {
            return Err(AccountError::new("DISCOVERY_AUTHORS_REQUIRED"));
        }
        let root = context.root.clone();
        let document = tokio::task::spawn_blocking(move || {
            discovery_store_io(|| WorkbenchStore::open(&root)?.read_discovery())
        })
        .await
        .map_err(|_| unavailable())?
        .map_err(store_error)?;
        self.discovery_validate_context(&context)?;
        let mut random = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let run_id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut snapshot = project(&context, &document);
        snapshot.run = Some(DiscoveryRun {
            id: run_id.clone(),
            phase: DiscoveryPhase::Checking,
            current_author: None,
            current_source: None,
            current_page: 0,
            requests_used: 0,
            completed_scopes: 0,
            total_scopes: authors.len() * 2,
            error_code: None,
        });
        {
            let mut memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
            if memory.active {
                return Err(AccountError::new("DISCOVERY_BUSY"));
            }
            memory.context = Some(context.clone());
            memory.snapshot = Some(snapshot.clone());
            memory.active = true;
            memory.cancelled = false;
            memory.invalidated = None;
        }
        let service = Arc::clone(self);
        let id = run_id.clone();
        tokio::spawn(async move {
            let outcome = service
                .discovery_scan(&context, &id, &authors, document)
                .await;
            service.discovery.finish(&id, outcome);
        });
        Ok(DiscoveryStart { run_id, snapshot })
    }

    /// Cancellation shares only the brief metadata commit lock, never the API lock.
    pub fn discovery_cancel(&self, run_id: &str) -> Result<DiscoveryRun> {
        let mut memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
        let snapshot = memory
            .snapshot
            .as_mut()
            .ok_or(AccountError::new("DISCOVERY_RUN_CHANGED"))?;
        let run = snapshot
            .run
            .as_mut()
            .filter(|run| run.id == run_id)
            .ok_or(AccountError::new("DISCOVERY_RUN_CHANGED"))?;
        if run.phase == DiscoveryPhase::Checking {
            run.phase = DiscoveryPhase::Cancelled;
            run.error_code = Some("DISCOVERY_CANCELLED".into());
            for range in &mut snapshot.authors {
                if range.state == DiscoveryRangeState::Checking {
                    range.state = DiscoveryRangeState::Cancelled;
                    range.error_code = Some("DISCOVERY_CANCELLED".into());
                }
            }
        }
        let result = run.clone();
        memory.cancelled = true;
        Ok(result)
    }

    async fn discovery_guard(&self, context: &DiscoveryContext, run_id: &str) -> Result<()> {
        self.discovery.check(run_id)?;
        self.discovery_validate_context(context)?;
        let root = context.root.clone();
        let revision = tokio::task::spawn_blocking(move || {
            discovery_store_io(|| {
                WorkbenchStore::open(&root)?
                    .read_following()
                    .map(|document| document.revision)
            })
        })
        .await
        .map_err(|_| unavailable())?
        .map_err(store_error)?;
        self.discovery.check(run_id)?;
        self.discovery_validate_context(context)?;
        if revision != context.following_revision {
            return Err(AccountError::new("DISCOVERY_FOLLOWING_CHANGED"));
        }
        Ok(())
    }

    async fn discovery_query(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        author: &str,
        source: Source,
        page: u64,
        detail: Option<&str>,
    ) -> Result<SourcePage> {
        #[cfg(not(test))]
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        self.discovery_guard(context, run_id).await?;
        self.discovery
            .progress(run_id, author, source, page, true)?;
        let scope = &context.identities[if source == Source::Jm { 0 } else { 1 }].scope;
        let result = self
            .query(
                source,
                &scope.session_id,
                if detail.is_some() {
                    QueryKind::Detail
                } else {
                    QueryKind::Search
                },
                detail.unwrap_or(author),
                None,
                if detail.is_some() { 1 } else { page },
            )
            .await;
        self.discovery_guard(context, run_id).await?;
        result.map(|result| result.page)
    }

    async fn discovery_scan(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        authors: &[String],
        mut document: Document<DiscoveryDocument>,
    ) -> Result<bool> {
        let index = match document
            .value
            .accounts
            .iter()
            .position(|account| account.account_key == context.account_key)
        {
            Some(index) => index,
            None => {
                document.value.accounts.push(DiscoveryAccount {
                    account_key: context.account_key.clone(),
                    authors: vec![],
                    records: vec![],
                });
                document.value.accounts.len() - 1
            }
        };
        // Unfollowed rows are not an unbounded history; observed records remain intact.
        document.value.accounts[index]
            .authors
            .retain(|range| context.authors.contains(&range.author));
        let mut partial = false;
        for author in authors {
            for source in [Source::Jm, Source::Pica] {
                self.discovery_guard(context, run_id).await?;
                self.discovery.progress(run_id, author, source, 0, false)?;
                let account = &mut document.value.accounts[index];
                let range_index = account
                    .authors
                    .iter()
                    .position(|range| {
                        range.author == *author && range.source == storage_source(source)
                    })
                    .unwrap_or_else(|| {
                        account.authors.push(idle(author, source));
                        account.authors.len() - 1
                    });
                let range = &mut account.authors[range_index];
                range.state = DiscoveryRangeState::Checking;
                range.last_attempt_at = Some(now()?);
                range.pages_read = 0;
                range.observed_count = 0;
                range.error_code = None;
                document = self.discovery_commit(context, run_id, document).await?;
                let mut traversal = Traversal::default();
                let mut unresolved = false;
                let mut page = 1;
                loop {
                    let response = self
                        .discovery_query(context, run_id, author, source, page, None)
                        .await;
                    let response = match response {
                        Ok(response) => response,
                        Err(error) => {
                            if stops_run(error.code) {
                                if matches!(
                                    error.code,
                                    "SOURCE_RATE_LIMITED" | "SOURCE_ACCESS_DENIED"
                                ) {
                                    let range =
                                        &mut document.value.accounts[index].authors[range_index];
                                    range.state = DiscoveryRangeState::Partial;
                                    range.error_code = Some(error.code.into());
                                    self.discovery_commit(context, run_id, document).await?;
                                }
                                return Err(error);
                            }
                            let range = &mut document.value.accounts[index].authors[range_index];
                            range.state = DiscoveryRangeState::Partial;
                            range.error_code = Some(error.code.into());
                            document = self.discovery_commit(context, run_id, document).await?;
                            partial = true;
                            break;
                        }
                    };
                    let complete = match traversal.append(&response) {
                        Ok(complete) => complete,
                        Err(error) => {
                            let range = &mut document.value.accounts[index].authors[range_index];
                            range.state = DiscoveryRangeState::Partial;
                            range.error_code = Some(error.code.into());
                            document = self.discovery_commit(context, run_id, document).await?;
                            partial = true;
                            break;
                        }
                    };
                    for mut work in response.items {
                        let mut verified =
                            work.authors.iter().any(|name| name.trim() == author.trim());
                        if !verified && work.authors.is_empty() {
                            match self
                                .discovery_query(
                                    context,
                                    run_id,
                                    author,
                                    source,
                                    page,
                                    Some(&work.work_id),
                                )
                                .await
                            {
                                Ok(detail) => {
                                    if detail.items.len() != 1
                                        || detail.items[0].work_id != work.work_id
                                        || detail.items[0].source != source
                                    {
                                        return Err(AccountError::new("SOURCE_RESPONSE_INVALID"));
                                    }
                                    work =
                                        detail.items.into_iter().next().ok_or_else(unavailable)?;
                                    verified = work
                                        .authors
                                        .iter()
                                        .any(|name| name.trim() == author.trim());
                                }
                                Err(error) if stops_run(error.code) => return Err(error),
                                Err(_) => {
                                    unresolved = true;
                                }
                            }
                        }
                        if !verified && !work.authors.is_empty() {
                            continue;
                        }
                        if !verified {
                            unresolved = true;
                        }
                        merge_record(
                            &mut document.value.accounts[index],
                            DiscoveryRecord {
                                work: discovery_work_from_source(work),
                                matched_authors: vec![author.clone()],
                                author_verified: verified,
                                observed_at: now()?,
                                scan_id: run_id.into(),
                            },
                        )?;
                    }
                    let account = &mut document.value.accounts[index];
                    let observed_count = account
                        .records
                        .iter()
                        .filter(|record| {
                            record.work.source == storage_source(source)
                                && record.matched_authors.contains(author)
                        })
                        .count();
                    let range = &mut account.authors[range_index];
                    range.pages_read = page;
                    range.observed_count = observed_count;
                    if complete {
                        range.state = if unresolved {
                            DiscoveryRangeState::Partial
                        } else {
                            DiscoveryRangeState::Complete
                        };
                        range.error_code =
                            unresolved.then(|| "DISCOVERY_AUTHOR_UNCONFIRMED".into());
                        if !unresolved {
                            range.last_complete_at = Some(now()?);
                        }
                    }
                    document = self.discovery_commit(context, run_id, document).await?;
                    if complete {
                        partial |= unresolved;
                        break;
                    }
                    page += 1;
                }
                self.discovery.completed_scope(run_id);
            }
        }
        Ok(partial)
    }
}

fn stops_run(code: &str) -> bool {
    matches!(
        code,
        "AUTH_REQUIRED"
            | "SESSION_EXPIRED"
            | "SESSION_CHANGED"
            | "SOURCE_RATE_LIMITED"
            | "RATE_LIMITED"
            | "SOURCE_ACCESS_DENIED"
            | "DISCOVERY_CANCELLED"
            | "DISCOVERY_FOLLOWING_CHANGED"
            | "CREDENTIAL_STORE_UNAVAILABLE"
            | "VAULT_UNAVAILABLE"
            | "VAULT_ACCESS_DENIED"
            | "VAULT_BUSY"
    )
}

fn clean_text(text: String) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Stable metadata projection used when comparing a later source detail read.
pub fn discovery_work_from_source(work: SourceWork) -> DiscoveryWork {
    DiscoveryWork {
        source: storage_source(work.source),
        work_id: work.work_id,
        title: clean_text(work.title),
        authors: work
            .authors
            .into_iter()
            .map(clean_text)
            .filter(|author| !author.trim().is_empty())
            .collect(),
        description: work.description.map(clean_text),
        tags: work
            .tags
            .into_iter()
            .map(clean_text)
            .filter(|tag| !tag.trim().is_empty())
            .collect(),
        favorite: work.favorite,
        chapter_count: work.chapter_count,
        page_count: work.page_count,
        cover_available: work.cover_available,
    }
}

fn merge_record(account: &mut DiscoveryAccount, mut incoming: DiscoveryRecord) -> Result<()> {
    if let Some(existing) = account.records.iter_mut().find(|record| {
        record.work.source == incoming.work.source && record.work.work_id == incoming.work.work_id
    }) {
        // Unresolved search metadata cannot downgrade an already verified identity.
        if existing.author_verified && !incoming.author_verified {
            for author in incoming.matched_authors {
                if !existing.matched_authors.contains(&author)
                    && existing
                        .work
                        .authors
                        .iter()
                        .any(|name| name.trim() == author.trim())
                {
                    existing.matched_authors.push(author);
                }
            }
            return Ok(());
        }
        for author in &existing.matched_authors {
            if !incoming.matched_authors.contains(author)
                && (!incoming.author_verified
                    || incoming
                        .work
                        .authors
                        .iter()
                        .any(|name| name.trim() == author.trim()))
            {
                incoming.matched_authors.push(author.clone());
            }
        }
        *existing = incoming;
    } else {
        if account.records.len() >= MAX_DISCOVERY_RECORDS {
            return Err(AccountError::new("DISCOVERY_LIMIT"));
        }
        account.records.push(incoming);
    }
    Ok(())
}

#[derive(Default)]
struct Traversal {
    page: u64,
    total: Option<u64>,
    pages: Option<u64>,
    ids: HashSet<String>,
}

impl Traversal {
    fn append(&mut self, page: &SourcePage) -> Result<bool> {
        let invalid = || AccountError::new("DISCOVERY_PAGINATION_CHANGED");
        if page.page != self.page + 1
            || page.page > MAX_DISCOVERY_PAGES
            || page.items.len() > 1000
            || (self.page > 0 && (self.total != page.total || self.pages != page.pages))
        {
            return Err(invalid());
        }
        if page.items.is_empty()
            && !(page.page == 1 && page.total == Some(0) && page.has_more != Some(true))
        {
            return Err(invalid());
        }
        let mut current = HashSet::new();
        if page
            .items
            .iter()
            .any(|work| self.ids.contains(&work.work_id) || !current.insert(work.work_id.clone()))
        {
            return Err(invalid());
        }
        let count = self.ids.len() + current.len();
        if count > MAX_DISCOVERY_RECORDS {
            return Err(AccountError::new("DISCOVERY_LIMIT"));
        }
        let terminal = page.pages.is_some_and(|pages| page.page == pages.max(1));
        let complete = page.has_more == Some(false)
            || (terminal && page.has_more != Some(true))
            || page.total == Some(count as u64);
        if page.total.is_some_and(|total| {
            count as u64 > total
                || ((page.has_more == Some(false) || terminal) && count as u64 != total)
                || (page.has_more == Some(true) && count as u64 >= total)
        }) || page.pages.is_some_and(|pages| {
            page.page > pages.max(1)
                || (complete && pages > page.page)
                || (terminal && page.has_more == Some(true))
        }) {
            return Err(invalid());
        }
        if !complete && (count == MAX_DISCOVERY_RECORDS || page.page == MAX_DISCOVERY_PAGES) {
            return Err(AccountError::new("DISCOVERY_LIMIT"));
        }
        self.ids.extend(current);
        self.page = page.page;
        self.total = page.total;
        self.pages = page.pages;
        Ok(complete)
    }
}

#[cfg(test)]
mod tests;

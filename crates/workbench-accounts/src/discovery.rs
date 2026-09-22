//! Explicit, sequential dual-source author catalog reads. Never downloads media.
use crate::{
    AccountError, AccountService, QueryKind, Result, SessionLease, Source, SourceBackend,
    SourcePage, SourceWork,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_credentials::Vault;
pub use workbench_storage::DiscoveryMode;
use workbench_storage::{
    discovery_author_is_valid, discovery_record_matches_author, AuthorQueryDocument,
    AuthorQueryPolicy, DiscoveryAccount, DiscoveryAuthorRange, DiscoveryBaseline,
    DiscoveryDocument, DiscoveryItemIssue, DiscoveryItemIssueCode, DiscoveryPagePatch,
    DiscoveryQueryBaseline, DiscoveryRangeState, DiscoveryRecord, DiscoveryWork, Document,
    WorkbenchStore, MAX_DISCOVERY_AUTHORS, MAX_DISCOVERY_HEAD_IDS, MAX_DISCOVERY_ISSUE_SAMPLES,
    MAX_DISCOVERY_PAGES, MAX_DISCOVERY_RAW_RECORDS, MAX_DISCOVERY_RECORDS, MAX_SAFE_INTEGER,
};

const DISCOVERY_QUERY_VERSION: u32 = 1;

/// Eligibility for an automatic author-keyword catalog read, not an author identity
/// verdict. In particular, a one-letter credit may be real but cannot constrain
/// the sources' general keyword search. Stored names and generic searches remain valid.
pub(crate) fn author_query_error(author: &str) -> Option<&'static str> {
    let folded: String = author
        .chars()
        .map(|character| match character {
            '\u{ff01}'..='\u{ff5e}' => {
                char::from_u32(character as u32 - 0xfee0).expect("ASCII width fold")
            }
            _ => character,
        })
        .collect();
    let normalized = folded.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "n/a"
            | "n.a."
            | "unknown"
            | "none"
            | "null"
            | "未知作者"
            | "作者不明"
            | "作者不详"
            | "作者不詳"
    ) {
        Some("AUTHOR_QUERY_PLACEHOLDER")
    } else if normalized.len() == 1 && normalized.as_bytes()[0].is_ascii_alphanumeric() {
        Some("AUTHOR_QUERY_TOO_BROAD")
    } else {
        None
    }
}

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
    pub mode: DiscoveryMode,
    pub current_strategy: Option<DiscoveryMode>,
    pub phase: DiscoveryPhase,
    pub current_author: Option<String>,
    pub current_source: Option<Source>,
    pub current_page: u64,
    pub current_query_index: Option<usize>,
    pub current_query_count: Option<usize>,
    pub requests_used: u64,
    pub completed_scopes: usize,
    pub total_scopes: usize,
    pub error_code: Option<String>,
    pub storage_warning_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySnapshot {
    pub scopes: Vec<DiscoveryScope>,
    pub revision: u64,
    pub run: Option<DiscoveryRun>,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub author_policies: Vec<AuthorQueryPolicy>,
    pub records: Vec<DiscoveryRecord>,
    pub other_record_count: usize,
    pub includes_other: bool,
}

/// Frequent progress reads never copy or serialize saved work metadata.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryProgress {
    pub scopes: Vec<DiscoveryScope>,
    pub revision: u64,
    pub run: Option<DiscoveryRun>,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub author_policies: Vec<AuthorQueryPolicy>,
    pub record_count: usize,
    pub other_record_count: usize,
}

impl From<&DiscoverySnapshot> for DiscoveryProgress {
    fn from(snapshot: &DiscoverySnapshot) -> Self {
        Self {
            scopes: snapshot.scopes.clone(),
            revision: snapshot.revision,
            run: snapshot.run.clone(),
            authors: snapshot.authors.clone(),
            author_policies: snapshot.author_policies.clone(),
            record_count: snapshot.records.len()
                + if snapshot.includes_other {
                    0
                } else {
                    snapshot.other_record_count
                },
            other_record_count: snapshot.other_record_count,
        }
    }
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
    pub policy_revision: u64,
    policies: HashMap<workbench_storage::Source, HashMap<String, AuthorQueryPolicy>>,
    work_credits:
        HashMap<workbench_storage::Source, HashMap<String, workbench_storage::AuthorWorkCredit>>,
    pub authors: Vec<String>,
    pub account_key: String,
}

impl DiscoveryContext {
    pub(crate) fn new(
        identities: [DiscoveryIdentity; 2],
        root: PathBuf,
        following: Document<workbench_storage::AccountFollowing>,
        policies: Document<AuthorQueryDocument>,
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
            policy_revision: policies.revision,
            policies: HashMap::new(),
            work_credits: HashMap::new(),
            authors: vec![],
            account_key: format!("{:x}", digest.finalize()),
        };
        result.authors = result.followed_authors(&following.value);
        result.set_policies(&policies);
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

    fn set_policies(&mut self, document: &Document<AuthorQueryDocument>) {
        self.policy_revision = document.revision;
        self.policies.clear();
        self.work_credits.clear();
        for identity in &self.identities {
            let source = storage_source(identity.scope.source);
            if let Some(account) = document.value.accounts.iter().find(|account| {
                account.source == source && account.account_key == identity.account_key
            }) {
                self.work_credits.insert(
                    source,
                    account
                        .work_credits
                        .iter()
                        .map(|rule| (rule.work_id.clone(), rule.clone()))
                        .collect(),
                );
            }
            self.policies.insert(
                source,
                self.authors
                    .iter()
                    .map(|author| {
                        (
                            author.clone(),
                            document
                                .value
                                .resolve(source, &identity.account_key, author),
                        )
                    })
                    .collect(),
            );
        }
    }

    fn policy(
        &self,
        source: workbench_storage::Source,
        author: &str,
    ) -> Option<&AuthorQueryPolicy> {
        self.policies
            .get(&source)
            .and_then(|policies| policies.get(author))
    }

    fn author_policies(&self) -> Vec<AuthorQueryPolicy> {
        self.authors
            .iter()
            .flat_map(|author| {
                [
                    workbench_storage::Source::Jm,
                    workbench_storage::Source::Pica,
                ]
                .into_iter()
                .filter_map(|source| self.policy(source, author).cloned())
            })
            .collect()
    }

    fn record_matches(&self, record: &DiscoveryRecord) -> bool {
        self.corrected_record_matches(record)
            || record.matched_authors.iter().any(|author| {
                self.policy(record.work.source, author)
                    .is_some_and(|policy| {
                        policy_error(policy).is_none()
                            && policy
                                .matches_work_credits(&record.work.work_id, &record.work.authors)
                    })
            })
    }

    /// A reviewed work can have been discovered only under an incorrect author.
    /// Admit its actual currently followed author without inventing a query hit
    /// or altering that author's pagination/baseline. The indexed guard keeps
    /// ordinary records on their original attribution path.
    fn corrected_record_matches(&self, record: &DiscoveryRecord) -> bool {
        self.work_credits
            .get(&record.work.source)
            .and_then(|rules| rules.get(&record.work.work_id))
            .filter(|rule| rule.matches_expected(&record.work.authors))
            .is_some_and(|rule| {
                self.policies
                    .get(&record.work.source)
                    .is_some_and(|policies| {
                        policies.values().any(|policy| {
                            policy_error(policy).is_none()
                                && policy.matches_credits(&rule.corrected_authors)
                        })
                    })
            })
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
    snapshot: Option<DiscoveryProgress>,
    active: bool,
    cancelled: bool,
    invalidated: Option<&'static str>,
}

struct DiscoveryCommitState {
    store: Arc<WorkbenchStore>,
    retain_authors: Option<Vec<String>>,
    record_count: usize,
    other_record_count: usize,
    confirmed_count: usize,
}

fn unavailable() -> AccountError {
    AccountError::new("DISCOVERY_UNAVAILABLE")
}
fn store_error(error: workbench_storage::StoreError) -> AccountError {
    AccountError::new(error.code)
}

/// Storage owns the bounded lock wait. Do not replay a compound read/commit or
/// multiply that budget here after persistent contention, source IO or CAS.
pub(crate) fn discovery_store_io<T>(
    operation: impl FnOnce() -> std::result::Result<T, workbench_storage::StoreError>,
) -> std::result::Result<T, workbench_storage::StoreError> {
    operation()
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

fn policy_error(policy: &AuthorQueryPolicy) -> Option<&'static str> {
    policy
        .queries
        .iter()
        .find_map(|query| author_query_error(query))
}

fn invalidate_changed_query(range: &mut DiscoveryAuthorRange, policy: &AuthorQueryPolicy) {
    let unchanged = match &range.query_fingerprint {
        Some(fingerprint) => fingerprint == &policy.query_fingerprint,
        None => policy.is_original_query(),
    };
    if !unchanged {
        range.state = DiscoveryRangeState::Partial;
        range.error_code = Some("AUTHOR_QUERY_POLICY_CHANGED".into());
        range.baseline = None;
        range.query_baselines.clear();
        range.completed_queries.clear();
        range.pages_complete = false;
    }
}

fn idle(author: &str, source: Source) -> DiscoveryAuthorRange {
    DiscoveryAuthorRange {
        author: author.into(),
        source: storage_source(source),
        state: DiscoveryRangeState::Idle,
        last_attempt_at: None,
        last_complete_at: None,
        last_checked_at: None,
        last_check_mode: None,
        baseline: None,
        query_fingerprint: None,
        query_baselines: vec![],
        completed_queries: vec![],
        observed_count: 0,
        pages_read: 0,
        error_code: None,
        issue_count: 0,
        issue_samples: vec![],
        pages_complete: false,
    }
}

fn project_view(
    context: &DiscoveryContext,
    document: &Document<DiscoveryDocument>,
    include_other: bool,
) -> DiscoverySnapshot {
    let account = document
        .value
        .accounts
        .iter()
        .find(|account| account.account_key == context.account_key);
    let followed: HashSet<&str> = context.authors.iter().map(String::as_str).collect();
    let saved_ranges: HashMap<_, _> = account
        .into_iter()
        .flat_map(|account| &account.authors)
        .map(|range| ((range.author.as_str(), range.source), range))
        .collect();
    let mut authors = vec![];
    for author in &context.authors {
        for source in [Source::Jm, Source::Pica] {
            let mut range = saved_ranges
                .get(&(author.as_str(), storage_source(source)))
                .map(|range| (**range).clone())
                .unwrap_or_else(|| idle(author, source));
            if let Some(policy) = context.policy(range.source, author) {
                invalidate_changed_query(&mut range, policy);
            }
            // No persisted job is resumed just because a document was opened.
            if range.state == DiscoveryRangeState::Checking {
                range.state = DiscoveryRangeState::Partial;
                range.error_code = Some("DISCOVERY_INTERRUPTED".into());
            }
            authors.push(range);
        }
    }
    let mut snapshot = DiscoverySnapshot {
        scopes: context.scopes(),
        revision: document.revision,
        run: None,
        authors,
        author_policies: context.author_policies(),
        records: account
            .map(|account| {
                account
                    .records
                    .iter()
                    .filter_map(|record| {
                        if context.corrected_record_matches(record) {
                            return Some(record.clone());
                        }
                        if !record
                            .matched_authors
                            .iter()
                            .any(|author| followed.contains(author.as_str()))
                        {
                            return None;
                        }
                        let mut record = record.clone();
                        record
                            .matched_authors
                            .retain(|author| followed.contains(author.as_str()));
                        (!record.matched_authors.is_empty()).then_some(record)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        other_record_count: 0,
        includes_other: include_other,
    };
    project_author_query_guards(context, &mut snapshot);
    snapshot.other_record_count = snapshot
        .records
        .iter()
        .filter(|record| !context.record_matches(record))
        .count();
    if !include_other {
        snapshot
            .records
            .retain(|record| context.record_matches(record));
    }
    snapshot
}

/// Only page-sized changed records use this projection during a scan.
fn record_counts(
    context: &DiscoveryContext,
    record: &DiscoveryRecord,
    followed: &HashSet<&str>,
) -> (usize, usize) {
    if context.corrected_record_matches(record) {
        return (1, 0);
    }
    let mut projected = record.clone();
    projected.matched_authors.retain(|author| {
        followed.contains(author.as_str())
            && context
                .policy(record.work.source, author)
                .is_some_and(|policy| policy_error(policy).is_none())
    });
    if projected.matched_authors.is_empty() {
        (0, 0)
    } else {
        (1, usize::from(!context.record_matches(&projected)))
    }
}

/// Hide only associations that cannot be queried as authors. Do not delete saved
/// results: a record shared with an eligible author remains available.
fn project_author_query_guards(context: &DiscoveryContext, snapshot: &mut DiscoverySnapshot) {
    let blocked: HashSet<(workbench_storage::Source, &str)> = snapshot
        .authors
        .iter_mut()
        .filter_map(|range| {
            let code = policy_error(context.policy(range.source, &range.author)?)?;
            range.state = DiscoveryRangeState::Partial;
            range.error_code = Some(code.into());
            range.baseline = None;
            Some((range.source, range.author.as_str()))
        })
        .collect();
    for record in &mut snapshot.records {
        if context.corrected_record_matches(record) {
            continue;
        }
        record
            .matched_authors
            .retain(|author| !blocked.contains(&(record.work.source, author.as_str())));
    }
    snapshot
        .records
        .retain(|record| !record.matched_authors.is_empty());
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

    pub(crate) fn check(&self, run_id: &str) -> Result<()> {
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
            range.pages_complete = false;
            range.error_code = None;
        }
        Ok(())
    }

    /// A page-sized transaction shares cancellation's lock, never the network await.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit(
        &self,
        run_id: &str,
        context: &DiscoveryContext,
        store: &WorkbenchStore,
        revision: u64,
        patch: DiscoveryPagePatch,
        record_count: usize,
        other_record_count: usize,
    ) -> Result<u64> {
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
        let next_revision = discovery_store_io(|| {
            store.apply_discovery_patch_for_policy(
                revision,
                context.following_revision,
                Some(context.policy_revision),
                patch.clone(),
            )
        })
        .map_err(|error| {
            if error.code == "DOCUMENT_TOO_LARGE" {
                AccountError::new("DISCOVERY_LIMIT")
            } else {
                store_error(error)
            }
        })?;
        let snapshot = memory.snapshot.as_mut().ok_or_else(unavailable)?;
        snapshot.revision = next_revision;
        snapshot.record_count = record_count;
        snapshot.other_record_count = other_record_count;
        for changed in patch.authors {
            if let Some(range) = snapshot
                .authors
                .iter_mut()
                .find(|range| range.author == changed.author && range.source == changed.source)
            {
                *range = changed;
            } else {
                snapshot.authors.push(changed);
            }
        }
        Ok(next_revision)
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

    fn query_position(&self, run_id: &str, index: usize, count: usize) {
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(run) = memory
                .snapshot
                .as_mut()
                .and_then(|snapshot| snapshot.run.as_mut())
                .filter(|run| run.id == run_id)
            {
                run.current_query_index = Some(index);
                run.current_query_count = Some(count);
            }
        }
    }

    fn strategy(&self, run_id: &str, strategy: DiscoveryMode) {
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(run) = memory
                .snapshot
                .as_mut()
                .and_then(|snapshot| snapshot.run.as_mut())
                .filter(|run| run.id == run_id)
            {
                run.current_strategy = Some(strategy);
            }
        }
    }

    fn checkpoint_failed(&self, run_id: &str) {
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(run) = memory
                .snapshot
                .as_mut()
                .and_then(|snapshot| snapshot.run.as_mut())
                .filter(|run| run.id == run_id)
            {
                run.storage_warning_code = Some("DISCOVERY_CHECKPOINT_FAILED".into());
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
        let policies = discovery_store_io(|| {
            WorkbenchStore::open(&context.root)?.read_author_query_policies()
        })
        .map_err(store_error)?;
        if policies.revision != context.policy_revision {
            return Err(AccountError::new("DISCOVERY_POLICY_CHANGED"));
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

    /// Compatibility read of all saved keyword records. Frequent callers use
    /// discovery_progress; UI defaults should use discovery_read_view(false).
    pub async fn discovery_read(&self, scopes: Vec<DiscoveryScope>) -> Result<DiscoverySnapshot> {
        self.discovery_read_view(scopes, true).await
    }

    pub async fn discovery_read_view(
        &self,
        scopes: Vec<DiscoveryScope>,
        include_other: bool,
    ) -> Result<DiscoverySnapshot> {
        let scopes = canonical_scopes(scopes)?;
        let cached_context = {
            let memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
            memory
                .context
                .as_ref()
                .filter(|context| context.scopes() == scopes)
                .cloned()
        };
        let mut context = match cached_context {
            Some(context) => context,
            None => self.discovery_context(scopes.clone()).await?,
        };
        self.discovery_validate_context(&context)?;
        let root = context.root.clone();
        let (following, policies, document) = tokio::task::spawn_blocking(move || {
            discovery_store_io(|| {
                let store = WorkbenchStore::open(&root)?;
                Ok((
                    store.read_following()?,
                    store.read_author_query_policies()?,
                    store.read_discovery()?,
                ))
            })
        })
        .await
        .map_err(|_| unavailable())?
        .map_err(store_error)?;
        self.discovery_validate_context(&context)?;
        context.authors = context.followed_authors(&following.value);
        context.following_revision = following.revision;
        context.set_policies(&policies);
        let mut snapshot = project_view(&context, &document, include_other);
        let mut memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
        if memory.context.as_ref().is_some_and(|cached| {
            cached.scopes() == scopes
                && cached.account_key == context.account_key
                && cached.policy_revision == context.policy_revision
        }) {
            if let Some(current) = &memory.snapshot {
                // A final commit may finish while the catalog is being read.
                // Never publish a terminal run alongside an older record set.
                if current.revision > snapshot.revision
                    && current
                        .run
                        .as_ref()
                        .is_some_and(|run| run.phase != DiscoveryPhase::Checking)
                {
                    return Err(AccountError::new("BUSY"));
                }
                snapshot.run = current.run.clone();
                if current.revision == snapshot.revision
                    && memory.context.as_ref().is_some_and(|cached| {
                        cached.following_revision == context.following_revision
                            && cached.policy_revision == context.policy_revision
                    })
                {
                    snapshot.authors = current.authors.clone();
                } else if memory.active {
                    for range in &mut snapshot.authors {
                        if range.error_code.as_deref() == Some("DISCOVERY_INTERRUPTED")
                            && current.authors.iter().any(|latest| {
                                latest.author == range.author
                                    && latest.source == range.source
                                    && latest.state == DiscoveryRangeState::Checking
                            })
                        {
                            range.state = DiscoveryRangeState::Checking;
                            range.error_code = None;
                        }
                    }
                }
            }
        }
        if memory.active
            && memory.context.as_ref().is_some_and(|cached| {
                cached.scopes() == scopes && cached.policy_revision != context.policy_revision
            })
        {
            snapshot.run = memory
                .snapshot
                .as_ref()
                .and_then(|current| current.run.clone())
                .map(|mut run| {
                    run.phase = DiscoveryPhase::Error;
                    run.error_code = Some("DISCOVERY_POLICY_CHANGED".into());
                    run
                });
        }
        if !memory.active {
            memory.context = Some(context);
            memory.snapshot = Some(DiscoveryProgress::from(&snapshot));
        }
        Ok(snapshot)
    }

    /// Local polling copies only author-range metadata, never the work catalog.
    pub async fn discovery_progress(
        &self,
        scopes: Vec<DiscoveryScope>,
    ) -> Result<DiscoveryProgress> {
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
        if let Some((context, progress)) = cached {
            self.discovery_validate_context(&context)?;
            let root = context.root.clone();
            let (revision, policy_revision) = tokio::task::spawn_blocking(move || {
                discovery_store_io(|| {
                    let store = WorkbenchStore::open(&root)?;
                    Ok((
                        store.read_following()?.revision,
                        store.read_author_query_policies()?.revision,
                    ))
                })
            })
            .await
            .map_err(|_| unavailable())?
            .map_err(store_error)?;
            self.discovery_validate_context(&context)?;
            if revision != context.following_revision {
                return Err(AccountError::new("DISCOVERY_FOLLOWING_CHANGED"));
            }
            if policy_revision != context.policy_revision {
                return self
                    .discovery_read_view(scopes, false)
                    .await
                    .map(|snapshot| DiscoveryProgress::from(&snapshot));
            }
            return Ok(progress);
        }
        self.discovery_read_view(scopes, false)
            .await
            .map(|snapshot| DiscoveryProgress::from(&snapshot))
    }
    pub async fn discovery_start(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        authors: Vec<String>,
    ) -> Result<DiscoveryStart> {
        self.discovery_start_with_mode(scopes, authors, DiscoveryMode::Incremental)
            .await
    }

    pub async fn discovery_start_with_mode(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        authors: Vec<String>,
        mode: DiscoveryMode,
    ) -> Result<DiscoveryStart> {
        self.discovery_start_selected(scopes, authors, mode, false)
            .await
    }

    /// Retry only incomplete author/source pairs. This does not refresh pairs
    /// that already completed and does not claim a new all-author check time.
    pub async fn discovery_start_unfinished(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        authors: Vec<String>,
    ) -> Result<DiscoveryStart> {
        self.discovery_start_selected(scopes, authors, DiscoveryMode::Incremental, true)
            .await
    }

    async fn discovery_start_selected(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        authors: Vec<String>,
        mode: DiscoveryMode,
        only_unfinished: bool,
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
        let mut snapshot = project_view(&context, &document, false);
        let ranges: Vec<_> = authors
            .into_iter()
            .flat_map(|author| [Source::Jm, Source::Pica].map(|source| (author.clone(), source)))
            .filter(|(author, source)| {
                !only_unfinished
                    || !snapshot.authors.iter().any(|range| {
                        range.author == *author
                            && range.source == storage_source(*source)
                            && range.state == DiscoveryRangeState::Complete
                    })
            })
            .collect();
        if ranges.is_empty() {
            return Err(AccountError::new("DISCOVERY_NO_UNFINISHED"));
        }
        let mut random = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let run_id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        snapshot.run = Some(DiscoveryRun {
            id: run_id.clone(),
            mode,
            current_strategy: None,
            phase: DiscoveryPhase::Checking,
            current_author: None,
            current_source: None,
            current_page: 0,
            current_query_index: None,
            current_query_count: None,
            requests_used: 0,
            completed_scopes: 0,
            total_scopes: ranges.len(),
            error_code: None,
            storage_warning_code: None,
        });
        {
            let mut memory = self.discovery.memory.lock().map_err(|_| unavailable())?;
            if memory.active {
                return Err(AccountError::new("DISCOVERY_BUSY"));
            }
            memory.context = Some(context.clone());
            memory.snapshot = Some(DiscoveryProgress::from(&snapshot));
            memory.active = true;
            memory.cancelled = false;
            memory.invalidated = None;
        }
        let service = Arc::clone(self);
        let id = run_id.clone();
        tokio::spawn(async move {
            let outcome = service
                .discovery_scan(&context, &id, &ranges, mode, document)
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
        let (revision, policy_revision) = tokio::task::spawn_blocking(move || {
            discovery_store_io(|| {
                let store = WorkbenchStore::open(&root)?;
                Ok((
                    store.read_following()?.revision,
                    store.read_author_query_policies()?.revision,
                ))
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
        if policy_revision != context.policy_revision {
            return Err(AccountError::new("DISCOVERY_POLICY_CHANGED"));
        }
        Ok(())
    }

    async fn discovery_query(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        author: &str,
        query: &str,
        source: Source,
        page: u64,
    ) -> Result<SourcePage> {
        let scope = &context.identities[if source == Source::Jm { 0 } else { 1 }].scope;
        for attempt in 0..3 {
            self.discovery_guard(context, run_id).await?;
            #[cfg(not(test))]
            tokio::time::sleep(std::time::Duration::from_millis([250, 500, 1500][attempt])).await;
            self.discovery_guard(context, run_id).await?;
            self.discovery
                .progress(run_id, author, source, page, true)?;
            let result = self
                .query(
                    source,
                    &scope.session_id,
                    QueryKind::Search,
                    query,
                    None,
                    page,
                )
                .await;
            self.discovery_guard(context, run_id).await?;
            if attempt < 2
                && result.as_ref().is_err_and(|error| {
                    matches!(
                        error.code,
                        "SOURCE_CONNECTION_FAILED" | "SOURCE_TIMEOUT" | "SOURCE_REQUEST_FAILED"
                    )
                })
            {
                continue;
            }
            return result.map(|result| result.page);
        }
        unreachable!("the bounded final source attempt always returns")
    }

    #[allow(clippy::too_many_arguments)]
    async fn discovery_save_page(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        document: &mut Document<DiscoveryDocument>,
        account_index: usize,
        range_index: usize,
        state: &mut DiscoveryCommitState,
        records: Vec<DiscoveryRecord>,
    ) -> Result<()> {
        let patch = DiscoveryPagePatch {
            account_key: context.account_key.clone(),
            authors: vec![document.value.accounts[account_index].authors[range_index].clone()],
            records,
            retain_authors: state.retain_authors.clone(),
        };
        document.revision = self
            .discovery_commit(
                context,
                run_id,
                Arc::clone(&state.store),
                document.revision,
                patch,
                state.record_count,
                state.other_record_count,
            )
            .await?;
        state.retain_authors = None;
        Ok(())
    }
    async fn discovery_scan(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        ranges: &[(String, Source)],
        mode: DiscoveryMode,
        document: Document<DiscoveryDocument>,
    ) -> Result<bool> {
        let store = Arc::new(WorkbenchStore::open(&context.root).map_err(store_error)?);
        let outcome = self
            .discovery_scan_inner(context, run_id, ranges, mode, document, Arc::clone(&store))
            .await;
        // Layout maintenance does not change the safety or truth of the saved
        // source results. Cancellation and changed identities skip maintenance.
        if self.discovery_guard(context, run_id).await.is_ok() {
            let revision = self
                .discovery
                .memory
                .lock()
                .ok()
                .and_then(|memory| memory.snapshot.as_ref().map(|snapshot| snapshot.revision));
            if let Some(revision) = revision {
                if let Err(error) = self
                    .discovery_checkpoint(context, run_id, store, revision)
                    .await
                {
                    if !stops_run(error.code) {
                        self.discovery.checkpoint_failed(run_id);
                    }
                }
            }
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    async fn discovery_scan_inner(
        &self,
        context: &DiscoveryContext,
        run_id: &str,
        ranges: &[(String, Source)],
        mode: DiscoveryMode,
        mut document: Document<DiscoveryDocument>,
        store: Arc<WorkbenchStore>,
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
        let mut record_index: HashMap<_, _> = document.value.accounts[index]
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| ((record.work.source, record.work.work_id.clone()), index))
            .collect();
        let followed: HashSet<&str> = context.authors.iter().map(String::as_str).collect();
        let (record_count, other_record_count) = document.value.accounts[index]
            .records
            .iter()
            .map(|record| record_counts(context, record, &followed))
            .fold((0, 0), |sum, count| (sum.0 + count.0, sum.1 + count.1));
        let mut state = DiscoveryCommitState {
            store,
            retain_authors: Some(context.authors.clone()),
            record_count,
            other_record_count,
            // The hot storage budget is global across saved account pairs and
            // retained history. UI counts separately project current follows.
            confirmed_count: document
                .value
                .accounts
                .iter()
                .map(|account| {
                    account
                        .records
                        .iter()
                        .filter(|record| {
                            if account.account_key == context.account_key {
                                context.record_matches(record)
                            } else {
                                discovery_record_matches_author(record)
                            }
                        })
                        .count()
                })
                .sum(),
        };
        let mut partial = false;
        for (author, source) in ranges {
            let source = *source;
            self.discovery_guard(context, run_id).await?;
            self.discovery.progress(run_id, author, source, 0, false)?;
            let account = &mut document.value.accounts[index];
            let range_index = account
                .authors
                .iter()
                .position(|range| range.author == *author && range.source == storage_source(source))
                .unwrap_or_else(|| {
                    account.authors.push(idle(author, source));
                    account.authors.len() - 1
                });
            let mut known_ids: HashSet<String> = account
                .records
                .iter()
                .filter(|record| {
                    record.work.source == storage_source(source)
                        && record.matched_authors.contains(author)
                })
                .map(|record| record.work.work_id.clone())
                .collect();
            let policy = context
                .policy(storage_source(source), author)
                .cloned()
                .ok_or_else(unavailable)?;
            let range = &mut account.authors[range_index];
            invalidate_changed_query(range, &policy);
            let old_range = range.clone();
            range.last_attempt_at = Some(now()?);
            range.pages_read = 0;
            range.issue_count = 0;
            range.issue_samples.clear();
            range.pages_complete = false;
            range.observed_count = known_ids.len();
            range.error_code = None;
            // Retain the last successful checkpoint as historical metadata.
            // A failed or cancelled current attempt cannot authorize its reuse.
            range.completed_queries.clear();
            range.query_fingerprint = Some(policy.query_fingerprint.clone());
            if let Some(code) = policy_error(&policy) {
                range.state = DiscoveryRangeState::Partial;
                range.error_code = Some(code.into());
                range.baseline = None;
                range.query_baselines.clear();
                self.discovery_save_page(
                    context,
                    run_id,
                    &mut document,
                    index,
                    range_index,
                    &mut state,
                    vec![],
                )
                .await?;
                partial = true;
                self.discovery.completed_scope(run_id);
                continue;
            }
            range.state = DiscoveryRangeState::Checking;
            self.discovery_save_page(
                context,
                run_id,
                &mut document,
                index,
                range_index,
                &mut state,
                vec![],
            )
            .await?;
            let mut scope_error = None;
            let mut all_queries_full = true;
            let mut all_queries_reached = true;
            for (query_index, query) in policy.queries.iter().enumerate() {
                self.discovery
                    .query_position(run_id, query_index + 1, policy.queries.len());
                let mut query_range = old_range.clone();
                if let Some(saved) = old_range.query_baselines.iter().find(|saved| {
                    saved.query == *query
                        && (old_range.state == DiscoveryRangeState::Complete
                            || old_range.completed_queries.contains(query))
                }) {
                    query_range.baseline = Some(saved.baseline.clone());
                    query_range.state = DiscoveryRangeState::Complete;
                    query_range.last_complete_at = Some(saved.baseline.established_at);
                } else if !policy.is_original_query()
                    || old_range
                        .query_fingerprint
                        .as_ref()
                        .is_some_and(|fingerprint| fingerprint != &policy.query_fingerprint)
                {
                    query_range.baseline = None;
                }
                let mut boundary = IncrementalBoundary::new(mode, &query_range, &known_ids);
                self.discovery.strategy(
                    run_id,
                    if boundary.is_some() {
                        DiscoveryMode::Incremental
                    } else {
                        DiscoveryMode::Full
                    },
                );
                let mut query_issue_count = 0;
                let mut traversal = Traversal::default();
                let mut page = 1;
                loop {
                    let response = self
                        .discovery_query(context, run_id, author, query, source, page)
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
                                    self.discovery_save_page(
                                        context,
                                        run_id,
                                        &mut document,
                                        index,
                                        range_index,
                                        &mut state,
                                        vec![],
                                    )
                                    .await?;
                                }
                                return Err(error);
                            }
                            let range = &mut document.value.accounts[index].authors[range_index];
                            range.state = DiscoveryRangeState::Partial;
                            range.error_code = Some(error.code.into());
                            self.discovery_save_page(
                                context,
                                run_id,
                                &mut document,
                                index,
                                range_index,
                                &mut state,
                                vec![],
                            )
                            .await?;
                            scope_error = Some(error.code.to_owned());
                            all_queries_reached = false;
                            break;
                        }
                    };
                    let complete = match traversal.append(&response) {
                        Ok(complete) => complete,
                        Err(error) => {
                            let range = &mut document.value.accounts[index].authors[range_index];
                            range.state = DiscoveryRangeState::Partial;
                            range.error_code = Some(error.code.into());
                            self.discovery_save_page(
                                context,
                                run_id,
                                &mut document,
                                index,
                                range_index,
                                &mut state,
                                vec![],
                            )
                            .await?;
                            scope_error = Some(error.code.to_owned());
                            all_queries_reached = false;
                            break;
                        }
                    };
                    // An isolated source slot is not a safe incremental anchor.
                    // Revisit the tail and keep the scope incomplete until a later
                    // clean traversal actually obtains those missing records.
                    if !response.issues.is_empty() {
                        boundary = None;
                        let range = &mut document.value.accounts[index].authors[range_index];
                        range.baseline = None;
                        range.query_baselines.retain(|saved| saved.query != *query);
                        range
                            .completed_queries
                            .retain(|completed| completed != query);
                        self.discovery.strategy(run_id, DiscoveryMode::Full);
                    }
                    let incremental_complete = boundary
                        .as_mut()
                        .is_some_and(|boundary| boundary.append(&response));
                    if complete || boundary.as_ref().is_some_and(|boundary| !boundary.viable) {
                        self.discovery.strategy(run_id, DiscoveryMode::Full);
                    }
                    // Stage only the incoming page. Retain every prior result if
                    // the page exceeds either storage budget or validation fails.
                    let mut changed = Vec::with_capacity(response.items.len());
                    let mut record_count = state.record_count;
                    let mut other_record_count = state.other_record_count;
                    let mut confirmed_count = state.confirmed_count;
                    let mut added = 0usize;
                    for work in response.items {
                        let verified = work.authors.iter().any(|name| name.trim() == author.trim());
                        let incoming = DiscoveryRecord {
                            work: discovery_work_from_source(work),
                            matched_authors: vec![author.clone()],
                            author_verified: verified,
                            observed_at: now()?,
                            scan_id: run_id.into(),
                        };
                        let existing = record_index
                            .get(&(incoming.work.source, incoming.work.work_id.clone()))
                            .map(|position| &document.value.accounts[index].records[*position]);
                        if let Some(existing) = existing {
                            let counts = record_counts(context, existing, &followed);
                            record_count -= counts.0;
                            other_record_count -= counts.1;
                            confirmed_count -= usize::from(context.record_matches(existing));
                        } else {
                            added += 1;
                        }
                        let record = merged_record(existing, incoming);
                        let counts = record_counts(context, &record, &followed);
                        record_count += counts.0;
                        other_record_count += counts.1;
                        confirmed_count += usize::from(context.record_matches(&record));
                        changed.push(record);
                    }
                    let current_records: usize = document
                        .value
                        .accounts
                        .iter()
                        .map(|account| account.records.len())
                        .sum();
                    if current_records.saturating_add(added) > MAX_DISCOVERY_RAW_RECORDS
                        || confirmed_count > MAX_DISCOVERY_RECORDS
                    {
                        let range = &mut document.value.accounts[index].authors[range_index];
                        range.state = DiscoveryRangeState::Partial;
                        range.error_code = Some("DISCOVERY_LIMIT".into());
                        self.discovery_save_page(
                            context,
                            run_id,
                            &mut document,
                            index,
                            range_index,
                            &mut state,
                            vec![],
                        )
                        .await?;
                        return Err(AccountError::new("DISCOVERY_LIMIT"));
                    }
                    state.record_count = record_count;
                    state.other_record_count = other_record_count;
                    state.confirmed_count = confirmed_count;
                    for record in &changed {
                        known_ids.insert(record.work.work_id.clone());
                        let key = (record.work.source, record.work.work_id.clone());
                        if let Some(position) = record_index.get(&key) {
                            document.value.accounts[index].records[*position] = record.clone();
                        } else {
                            record_index.insert(key, document.value.accounts[index].records.len());
                            document.value.accounts[index].records.push(record.clone());
                        }
                    }
                    let account = &mut document.value.accounts[index];
                    let range = &mut account.authors[range_index];
                    range.pages_read += 1;
                    range.observed_count = known_ids.len();
                    range.issue_count += response.issues.len();
                    query_issue_count += response.issues.len();
                    range.issue_samples.extend(
                        response
                            .issues
                            .iter()
                            .take(
                                MAX_DISCOVERY_ISSUE_SAMPLES
                                    .saturating_sub(range.issue_samples.len()),
                            )
                            .map(|issue| DiscoveryItemIssue {
                                query: (policy.queries.len() > 1).then(|| query.clone()),
                                page: issue.page,
                                index: issue.index,
                                work_id: issue.work_id.clone(),
                                code: match issue.code {
                                    crate::SourceItemIssueCode::Invalid => {
                                        DiscoveryItemIssueCode::Invalid
                                    }
                                    crate::SourceItemIssueCode::MetadataMissing => {
                                        DiscoveryItemIssueCode::MetadataMissing
                                    }
                                },
                            }),
                    );
                    if complete || incremental_complete {
                        all_queries_full &= complete;
                        if !response.issues.is_empty() || query_issue_count > 0 {
                            scope_error = Some("SOURCE_ITEMS_PARTIAL".into());
                        } else {
                            let checked_at = now()?;
                            let baseline = DiscoveryBaseline {
                                query_version: DISCOVERY_QUERY_VERSION,
                                head_ids: traversal.head_ids.clone(),
                                total: if complete {
                                    traversal.record_count as u64
                                } else {
                                    traversal
                                        .total
                                        .expect("incremental boundary requires total")
                                },
                                established_at: if complete {
                                    checked_at
                                } else {
                                    boundary
                                        .as_ref()
                                        .expect("incremental boundary")
                                        .baseline
                                        .established_at
                                },
                            };
                            range.query_baselines.retain(|saved| saved.query != *query);
                            range.query_baselines.push(DiscoveryQueryBaseline {
                                query: query.clone(),
                                baseline: baseline.clone(),
                            });
                            range.completed_queries.push(query.clone());
                            if policy.queries.len() == 1 {
                                range.baseline = Some(baseline);
                            }
                        }
                    }
                    self.discovery_save_page(
                        context,
                        run_id,
                        &mut document,
                        index,
                        range_index,
                        &mut state,
                        changed,
                    )
                    .await?;
                    if complete || incremental_complete {
                        break;
                    }
                    page += 1;
                }
            }
            let range = &mut document.value.accounts[index].authors[range_index];
            range.pages_complete = all_queries_reached && all_queries_full;
            if all_queries_reached {
                let checked_at = now()?;
                range.last_checked_at = Some(checked_at);
                range.last_check_mode = Some(if all_queries_full {
                    DiscoveryMode::Full
                } else {
                    DiscoveryMode::Incremental
                });
                if scope_error.is_none() && all_queries_full {
                    range.last_complete_at = Some(checked_at);
                }
            }
            if let Some(error) = scope_error {
                range.state = DiscoveryRangeState::Partial;
                range.error_code = Some(error);
                if range.issue_count > 0 {
                    range.baseline = None;
                }
                partial = true;
            } else {
                range.state = DiscoveryRangeState::Complete;
                range.error_code = None;
            }
            self.discovery_save_page(
                context,
                run_id,
                &mut document,
                index,
                range_index,
                &mut state,
                vec![],
            )
            .await?;
            self.discovery.completed_scope(run_id);
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
            | "DISCOVERY_POLICY_CHANGED"
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
        source_updated_at: work.source_updated_at,
        cover_available: work.cover_available,
    }
}

fn merged_record(
    existing: Option<&DiscoveryRecord>,
    mut incoming: DiscoveryRecord,
) -> DiscoveryRecord {
    if let Some(existing) = existing {
        let source_updated_at = incoming
            .work
            .source_updated_at
            .clone()
            .or_else(|| existing.work.source_updated_at.clone());
        // Preserve useful prior metadata when a later list omits authors, but
        // keep every query that returned this source ID. A query association
        // does not assert an author identity and never grants ownership.
        if incoming.work.authors.is_empty() && !existing.work.authors.is_empty() {
            incoming.work = existing.work.clone();
        }
        incoming.work.source_updated_at = source_updated_at;
        for author in &existing.matched_authors {
            if !incoming.matched_authors.contains(author) {
                incoming.matched_authors.push(author.clone());
            }
        }
        // This legacy evidence flag remains strict; it is not a display filter.
        incoming.author_verified = incoming.matched_authors.iter().all(|author| {
            incoming
                .work
                .authors
                .iter()
                .any(|name| name.trim() == author.trim())
        });
    }
    incoming
}

/// An ordered checkpoint for the pinned newest-first source query. This only
/// establishes a checked front boundary, never a fresh audit of the old tail.
/// Any visible drift falls through to the normal complete pagination path.
struct IncrementalBoundary {
    baseline: DiscoveryBaseline,
    known_ids: HashSet<String>,
    prefix_count: u64,
    matched_head: usize,
    viable: bool,
}

impl IncrementalBoundary {
    fn new(
        mode: DiscoveryMode,
        range: &DiscoveryAuthorRange,
        known_ids: &HashSet<String>,
    ) -> Option<Self> {
        let baseline = range.baseline.as_ref()?;
        if mode != DiscoveryMode::Incremental
            || range.state != DiscoveryRangeState::Complete
            || range.last_complete_at.is_none()
            || baseline.query_version != DISCOVERY_QUERY_VERSION
            || baseline.head_ids.is_empty()
            || baseline.head_ids.iter().any(|id| !known_ids.contains(id))
        {
            return None;
        }
        Some(Self {
            baseline: baseline.clone(),
            known_ids: known_ids.clone(),
            prefix_count: 0,
            matched_head: 0,
            viable: true,
        })
    }

    fn append(&mut self, page: &SourcePage) -> bool {
        if !self.viable
            || !page.issues.is_empty()
            || page.total.is_none_or(|total| total < self.baseline.total)
        {
            self.viable = false;
            return false;
        }
        for work in &page.items {
            let id = &work.work_id;
            if self.matched_head == self.baseline.head_ids.len() {
                // Inspect the rest of the boundary page too: a new ID here is
                // evidence that latest-first prefix assumptions did not hold.
                if !self.known_ids.contains(id) {
                    self.viable = false;
                    return false;
                }
            } else if id == &self.baseline.head_ids[self.matched_head] {
                self.matched_head += 1;
            } else if self.matched_head == 0 && !self.known_ids.contains(id) {
                self.prefix_count += 1;
            } else {
                self.viable = false;
                return false;
            }
        }
        if self.matched_head == self.baseline.head_ids.len() {
            // A matching anchor alone is insufficient: removed/inserted items
            // and moved records must not silently reduce the checked scope.
            self.viable = page.total == self.baseline.total.checked_add(self.prefix_count);
            return self.viable;
        }
        false
    }
}

#[derive(Default)]
struct Traversal {
    page: u64,
    total: Option<u64>,
    pages: Option<u64>,
    ids: HashSet<String>,
    head_ids: Vec<String>,
    record_count: usize,
}

impl Traversal {
    fn append(&mut self, page: &SourcePage) -> Result<bool> {
        let invalid = || AccountError::new("DISCOVERY_PAGINATION_CHANGED");
        if page.page != self.page + 1
            || page.page > MAX_DISCOVERY_PAGES
            || page.record_count() > 1000
            || (self.page > 0 && (self.total != page.total || self.pages != page.pages))
        {
            return Err(invalid());
        }
        if page.record_count() == 0
            && !(page.page == 1 && page.total == Some(0) && page.has_more != Some(true))
        {
            return Err(invalid());
        }
        let mut current = HashSet::new();
        if page
            .items
            .iter()
            .map(|work| &work.work_id)
            .chain(
                page.issues
                    .iter()
                    .filter_map(|issue| issue.work_id.as_ref()),
            )
            .any(|id| self.ids.contains(id) || !current.insert(id.clone()))
        {
            return Err(invalid());
        }
        let count = self.record_count + page.record_count();
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
        self.head_ids.extend(
            page.items
                .iter()
                .take(MAX_DISCOVERY_HEAD_IDS.saturating_sub(self.head_ids.len()))
                .map(|work| work.work_id.clone()),
        );
        self.ids.extend(current);
        self.record_count = count;
        self.page = page.page;
        self.total = page.total;
        self.pages = page.pages;
        Ok(complete)
    }
}

#[cfg(test)]
mod tests;

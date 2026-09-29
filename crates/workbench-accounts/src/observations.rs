//! Retain source metadata discovered outside author queries and audit recent-feed coverage.
//! This module never downloads media or changes author-pagination baselines.
use crate::{
    cache, discovery_work_from_source, AccountError, AccountService, DiscoveryScope, QueryKind,
    Result, Source, SourceBackend, SourcePage, SourceWork,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use workbench_credentials::Vault;
use workbench_storage::{ObservedWork, RecentCoverage, WorkbenchStore};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentHistoryResult {
    pub source: Source,
    pub session_id: String,
    pub items: Vec<SourceWork>,
    pub revision: u64,
    pub coverage: RecentCoverage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownAuthorWorksResult {
    pub source: Source,
    pub session_id: String,
    pub items: Vec<SourceWork>,
    pub checked_at: Option<u64>,
    pub discovery_revision: u64,
    pub history_complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCheckResult {
    pub source: Source,
    pub pages_read: u64,
    pub records_read: usize,
    pub reached_end: bool,
    pub joined_previous: bool,
    pub initial_window: bool,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCheckRun {
    pub id: String,
    pub phase: String,
    pub current_source: Option<Source>,
    pub current_page: u64,
    pub pages_read: u64,
    pub records_read: usize,
    pub error_code: Option<String>,
    pub results: Vec<RecentCheckResult>,
}

#[derive(Default)]
pub(crate) struct RecentControl {
    state: Mutex<RecentState>,
    replay: tokio::sync::Mutex<Option<(String, u64, u64, u64)>>,
    favorite_seed: tokio::sync::Mutex<HashSet<String>>,
}

#[derive(Default)]
struct RecentState {
    run: Option<RecentCheckRun>,
    context: Option<crate::discovery::DiscoveryContext>,
}

fn unavailable() -> AccountError {
    AccountError::new("STORE_UNAVAILABLE")
}
fn storage_source(source: Source) -> workbench_storage::Source {
    match source {
        Source::Jm => workbench_storage::Source::Jm,
        Source::Pica => workbench_storage::Source::Pica,
    }
}
fn source_work(record: &ObservedWork) -> SourceWork {
    from_discovery(&record.work, record.categories.clone())
}
fn from_discovery(
    work: &workbench_storage::DiscoveryWork,
    categories: Option<Vec<String>>,
) -> SourceWork {
    SourceWork {
        source: match work.source {
            workbench_storage::Source::Jm => Source::Jm,
            workbench_storage::Source::Pica => Source::Pica,
        },
        work_id: work.work_id.clone(),
        title: work.title.clone(),
        authors: work.authors.clone(),
        description: work.description.clone(),
        tags: work.tags.clone(),
        categories,
        favorite: work.favorite,
        chapter_count: work.chapter_count,
        page_count: work.page_count,
        source_updated_at: work.source_updated_at.clone(),
        cover_available: work.cover_available,
    }
}
fn observed(work: SourceWork, at: u64, detail: bool, via: &str) -> ObservedWork {
    let categories = work.categories.clone();
    ObservedWork {
        work: discovery_work_from_source(work),
        categories,
        observed_at: at,
        metadata_detail_at: detail.then_some(at),
        via: vec![via.into()],
    }
}

impl<B: SourceBackend, V: Vault + 'static> AccountService<B, V> {
    pub(crate) async fn observe_query(
        &self,
        source: Source,
        session_id: &str,
        kind: QueryKind,
        page: &SourcePage,
        at: u64,
    ) -> Result<Option<u64>> {
        let (key, root, lease) = self.observation_identity(source, session_id).await?;
        let via = match kind {
            QueryKind::Favorites => "favorites",
            QueryKind::Search => "search",
            QueryKind::Author => "author",
            QueryKind::Tag => "tag",
            QueryKind::Category => "category",
            QueryKind::Ranking => "ranking",
            QueryKind::Recent => "recent",
            QueryKind::Detail => "detail",
        };
        let detail = matches!(kind, QueryKind::Detail);
        let records: Vec<_> = page
            .items
            .iter()
            .cloned()
            .map(|work| observed(work, at, detail, via))
            .collect();
        let recent_page = matches!(kind, QueryKind::Recent).then_some(page.page);
        let captured = lease.clone();
        tokio::task::spawn_blocking(move || {
            captured.require_current()?;
            let store = WorkbenchStore::open(&root).map_err(|e| AccountError::new(e.code))?;
            store
                .merge_observed_works(&key, storage_source(source), records, recent_page)
                .map_err(|e| AccountError::new(e.code))?;
            captured.require_current()
        })
        .await
        .map_err(|_| unavailable())??;
        lease.require_current()?;
        if self.discovery_is_active()? {
            return Ok(None);
        }
        let Ok(scopes) = self.current_discovery_scopes().await else {
            return Ok(None);
        };
        self.discovery_observe(scopes, page.items.clone(), detail, at)
            .await
    }

    /// Import existing favorite caches once per authenticated account generation.
    /// The saved timestamp is evidence age, not the time of this local replay.
    async fn seed_favorite_observations(&self, source: Source, session_id: &str) -> Result<()> {
        let (key, root, lease) = self.observation_identity(source, session_id).await?;
        let marker = format!("{key}:{session_id}");
        let mut seeds = self.recent_checks.favorite_seed.lock().await;
        if seeds.contains(&marker) {
            return Ok(());
        }
        tokio::task::spawn_blocking(move || {
            lease.require_current()?;
            let works = cache::observed_catalog_works(&root, &key, source)?;
            let store = WorkbenchStore::open(&root).map_err(|e| AccountError::new(e.code))?;
            for chunk in works.chunks(1000) {
                lease.require_current()?;
                let records = chunk
                    .iter()
                    .map(|(work, at)| observed(work.clone(), *at, false, "favorites"))
                    .collect();
                store
                    .merge_observed_works(&key, storage_source(source), records, None)
                    .map_err(|e| AccountError::new(e.code))?;
            }
            lease.require_current()
        })
        .await
        .map_err(|_| unavailable())??;
        seeds.insert(marker);
        Ok(())
    }

    pub(crate) async fn discovery_replay_observations(
        &self,
        scopes: Vec<DiscoveryScope>,
    ) -> Result<Option<u64>> {
        if self.discovery_is_active()? {
            return Ok(None);
        }
        let mut replay = self.recent_checks.replay.lock().await;
        if self.discovery_is_active()? {
            return Ok(None);
        }
        let context = self.discovery_context(scopes.clone()).await?;
        for scope in &scopes {
            self.seed_favorite_observations(scope.source, &scope.session_id)
                .await?;
        }
        let root = context.root.clone();
        let document =
            tokio::task::spawn_blocking(move || WorkbenchStore::open(&root)?.read_observed_works())
                .await
                .map_err(|_| unavailable())?
                .map_err(|e| AccountError::new(e.code))?;
        let signature = (
            context.account_key.clone(),
            document.revision,
            context.following_revision,
            context.policy_revision,
        );
        if replay.as_ref() == Some(&signature) {
            return Ok(None);
        }
        let records = document
            .value
            .accounts
            .into_iter()
            .filter(|account| {
                context.identities.iter().any(|identity| {
                    identity.account_key == account.account_key
                        && storage_source(identity.scope.source) == account.source
                })
            })
            .flat_map(|account| account.records)
            .map(|record| ((record.work.source, record.work.work_id.clone()), record))
            .collect::<HashMap<_, _>>()
            .into_values()
            .collect();
        let result = self.discovery_observe_records(scopes, records).await?;
        if result.is_some() {
            *replay = Some(signature);
        }
        Ok(result)
    }

    pub async fn source_recent_history(
        &self,
        source: Source,
        session_id: &str,
    ) -> Result<RecentHistoryResult> {
        let (key, root, lease) = self.observation_identity(source, session_id).await?;
        let document =
            tokio::task::spawn_blocking(move || WorkbenchStore::open(&root)?.read_observed_works())
                .await
                .map_err(|_| unavailable())?
                .map_err(|e| AccountError::new(e.code))?;
        lease.require_current()?;
        let account =
            document.value.accounts.iter().find(|account| {
                account.account_key == key && account.source == storage_source(source)
            });
        let mut items = vec![];
        if let Some(account) = account {
            let works: HashMap<_, _> = account
                .records
                .iter()
                .map(|record| (record.work.work_id.as_str(), record))
                .collect();
            items = account
                .recent_ids
                .iter()
                .filter_map(|id| works.get(id.as_str()).map(|record| source_work(record)))
                .collect();
        }
        Ok(RecentHistoryResult {
            source,
            session_id: session_id.into(),
            items,
            revision: document.revision,
            coverage: account
                .map_or_else(RecentCoverage::default, |account| account.coverage.clone()),
        })
    }

    pub async fn source_author_known_works(
        &self,
        source: Source,
        session_id: &str,
        author: &str,
    ) -> Result<KnownAuthorWorksResult> {
        if author.trim().is_empty() || author.chars().count() > 200 {
            return Err(AccountError::new("INVALID_INPUT"));
        }
        let mut observation_error_code = self
            .seed_favorite_observations(source, session_id)
            .await
            .err()
            .map(|error| error.code.to_owned());
        let (key, root, lease) = self.observation_identity(source, session_id).await?;
        let name = author.to_owned();
        let (document, policy) = tokio::task::spawn_blocking(move || {
            let store = WorkbenchStore::open(&root)?;
            Ok::<_, workbench_storage::StoreError>((
                store.read_observed_works(),
                store.read_author_query_policies()?.value.resolve(
                    storage_source(source),
                    &key,
                    &name,
                ),
            ))
        })
        .await
        .map_err(|_| unavailable())?
        .map_err(|e| AccountError::new(e.code))?;
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                observation_error_code = Some(error.code.into());
                workbench_storage::Document {
                    revision: 0,
                    value: workbench_storage::ObservedDocument::default(),
                }
            }
        };
        let (key, _, _) = self.observation_identity(source, session_id).await?;
        let mut known = HashMap::new();
        let mut checked_at = None;
        let mut discovery_revision = 0;
        let mut history_complete = false;
        if let Ok(scopes) = self.current_discovery_scopes().await {
            if let Err(error) = self.discovery_replay_observations(scopes.clone()).await {
                observation_error_code = Some(error.code.into());
            }
            let context = self.discovery_context(scopes).await?;
            let root = context.root.clone();
            let catalog =
                tokio::task::spawn_blocking(move || WorkbenchStore::open(&root)?.read_discovery())
                    .await
                    .map_err(|_| unavailable())
                    .and_then(|result| result.map_err(|error| AccountError::new(error.code)));
            self.discovery_validate_context(&context)?;
            // This endpoint supports arbitrary authors, including authors no
            // longer followed. The follow-filtered UI projection is not the raw
            // account-pair catalog and must not be used as its history source.
            match catalog {
                Ok(catalog) => {
                    discovery_revision = catalog.revision;
                    history_complete = observation_error_code.is_none();
                    for account in catalog
                        .value
                        .accounts
                        .into_iter()
                        .filter(|account| account.account_key == context.account_key)
                    {
                        for record in account
                            .records
                            .into_iter()
                            .filter(|record| record.work.source == storage_source(source))
                        {
                            known.insert(record.work.work_id.clone(), record);
                        }
                    }
                }
                Err(error) => observation_error_code = Some(error.code.into()),
            }
        }
        if let Some(account) =
            document.value.accounts.iter().find(|account| {
                account.account_key == key && account.source == storage_source(source)
            })
        {
            for record in &account.records {
                let incoming = crate::discovery::observation_record(
                    &key,
                    record.work.clone(),
                    record.observed_at,
                    record.metadata_detail_at,
                );
                let merged =
                    crate::discovery::merged_record(known.get(&record.work.work_id), incoming);
                known.insert(record.work.work_id.clone(), merged);
            }
        }
        lease.require_current()?;
        let mut items = vec![];
        for record in known.into_values() {
            if policy.matches_work_credits(&record.work.work_id, &record.work.authors)
                && !(source == Source::Jm
                    && record
                        .work
                        .tags
                        .iter()
                        .any(|tag| workbench_sources::is_jm_english_category(tag)))
            {
                checked_at = Some(checked_at.unwrap_or(0).max(record.observed_at));
                items.push(from_discovery(&record.work, None));
            }
        }
        items.sort_by(|a, b| a.work_id.cmp(&b.work_id));
        Ok(KnownAuthorWorksResult {
            source,
            session_id: session_id.into(),
            items,
            checked_at,
            discovery_revision,
            history_complete,
            observation_error_code,
        })
    }

    pub fn recent_check_progress(&self) -> Result<Option<RecentCheckRun>> {
        let memory = self.recent_checks.state.lock().map_err(|_| unavailable())?;
        if memory
            .context
            .as_ref()
            .is_none_or(|context| self.discovery_validate_context(context).is_err())
        {
            return Ok(None);
        }
        Ok(memory.run.clone())
    }
    pub fn recent_check_cancel(&self, run_id: &str) -> Result<RecentCheckRun> {
        let mut memory = self.recent_checks.state.lock().map_err(|_| unavailable())?;
        let context = memory
            .context
            .as_ref()
            .ok_or(AccountError::new("SESSION_CHANGED"))?;
        self.discovery_validate_context(context)?;
        let run = memory
            .run
            .as_mut()
            .filter(|run| run.id == run_id)
            .ok_or(AccountError::new("INVALID_INPUT"))?;
        if run.phase == "checking" {
            run.phase = "cancelled".into();
        }
        Ok(run.clone())
    }
    fn recent_running(&self, id: &str) -> bool {
        self.recent_checks.state.lock().is_ok_and(|memory| {
            memory
                .context
                .as_ref()
                .is_some_and(|context| self.discovery_validate_context(context).is_ok())
                && memory
                    .run
                    .as_ref()
                    .is_some_and(|run| run.id == id && run.phase == "checking")
        })
    }
    pub async fn recent_check_start(
        self: &Arc<Self>,
        scopes: Vec<DiscoveryScope>,
        max_pages: Option<u64>,
    ) -> Result<RecentCheckRun> {
        let scopes = crate::discovery::canonical_scopes(scopes)?;
        let context = self.discovery_context(scopes.clone()).await?;
        let max_pages = max_pages.unwrap_or(8);
        if !(2..=1000).contains(&max_pages) {
            return Err(AccountError::new("INVALID_INPUT"));
        }
        let run = RecentCheckRun {
            id: format!("recent-{}-{}", cache::now_ms()?, rand::random::<u64>()),
            phase: "checking".into(),
            current_source: None,
            current_page: 0,
            pages_read: 0,
            records_read: 0,
            error_code: None,
            results: vec![],
        };
        {
            let mut memory = self.recent_checks.state.lock().map_err(|_| unavailable())?;
            if memory
                .context
                .as_ref()
                .is_some_and(|context| self.discovery_validate_context(context).is_ok())
                && memory
                    .run
                    .as_ref()
                    .is_some_and(|run| run.phase == "checking")
            {
                return Err(AccountError::new("CHECK_BUSY"));
            }
            *memory = RecentState {
                run: Some(run.clone()),
                context: Some(context),
            };
        }
        let service = Arc::clone(self);
        let run_id = run.id.clone();
        tokio::spawn(async move {
            service.run_recent_check(run_id, scopes, max_pages).await;
        });
        Ok(run)
    }

    async fn run_recent_check(
        self: Arc<Self>,
        id: String,
        scopes: Vec<DiscoveryScope>,
        max_pages: u64,
    ) {
        for scope in scopes {
            if !self.recent_running(&id) {
                return;
            }
            let result = self.check_recent_scope(&id, &scope, max_pages).await;
            if let Ok(mut memory) = self.recent_checks.state.lock() {
                if let Some(run) = memory.run.as_mut().filter(|run| run.id == id) {
                    if let Some(error) = &result.error_code {
                        run.error_code = Some(error.clone());
                    }
                    run.results.push(result);
                }
            }
        }
        if let Ok(mut memory) = self.recent_checks.state.lock() {
            if let Some(run) = memory
                .run
                .as_mut()
                .filter(|run| run.id == id && run.phase == "checking")
            {
                run.phase = if run.error_code.is_some() {
                    "partial"
                } else {
                    "complete"
                }
                .into();
                run.current_source = None;
            }
        }
    }

    async fn check_recent_scope(
        &self,
        id: &str,
        scope: &DiscoveryScope,
        max_pages: u64,
    ) -> RecentCheckResult {
        let mut result = RecentCheckResult {
            source: scope.source,
            pages_read: 0,
            records_read: 0,
            reached_end: false,
            joined_previous: false,
            initial_window: false,
            error_code: None,
        };
        let outcome: Result<()> = async {
            let previous = self
                .source_recent_history(scope.source, &scope.session_id)
                .await?
                .coverage;
            let first_window = previous.head_ids.is_empty();
            // Initial coverage is an explicitly bounded window. Later checks must
            // join that window; a limit never silently becomes a new checkpoint.
            let limit = if first_window { max_pages } else { 1000 };
            let mut ids = Vec::new();
            let mut sequence = crate::discovery::Traversal::default();
            let mut head = Vec::new();
            for page in 1..=limit {
                if !self.recent_running(id) {
                    return Err(AccountError::new("CHECK_CANCELLED"));
                }
                {
                    let mut memory = self.recent_checks.state.lock().map_err(|_| unavailable())?;
                    if let Some(run) = memory.run.as_mut().filter(|run| run.id == id) {
                        run.current_source = Some(scope.source);
                        run.current_page = page;
                    }
                }
                let response = self
                    .query(
                        scope.source,
                        &scope.session_id,
                        QueryKind::Recent,
                        "",
                        None,
                        page,
                    )
                    .await?;
                if response.observation_error_code.is_some() {
                    return Err(AccountError::new("OBSERVATION_COMMIT_FAILED"));
                }
                let batch = response.page;
                result.pages_read = page;
                result.records_read += batch.record_count();
                {
                    let mut memory = self.recent_checks.state.lock().map_err(|_| unavailable())?;
                    if let Some(run) = memory.run.as_mut().filter(|run| run.id == id) {
                        run.pages_read += 1;
                        run.records_read += batch.record_count();
                    }
                }
                // Retain raw metadata first. Check continuity and effective
                // counts before joining or advancing a coverage checkpoint.
                result.reached_end =
                    append_recent_page(&mut sequence, &batch, &mut ids, &mut head)?;
                if !first_window && page >= 2 {
                    result.joined_previous = contiguous_join(&ids, &previous.head_ids);
                }
                if result.reached_end || result.joined_previous {
                    break;
                }
                if page == limit {
                    if first_window {
                        result.initial_window = true;
                    } else {
                        return Err(AccountError::new("RECENT_RANGE_INCOMPLETE"));
                    }
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            if !self.recent_running(id) {
                return Err(AccountError::new("CHECK_CANCELLED"));
            }
            let (key, root, lease) = self
                .observation_identity(scope.source, &scope.session_id)
                .await?;
            let coverage = RecentCoverage {
                head_ids: head,
                checked_at: Some(cache::now_ms()?),
                pages_read: result.pages_read,
                reached_end: result.reached_end,
                joined_previous: result.joined_previous,
                initial_window: result.initial_window,
                error_code: None,
            };
            tokio::task::spawn_blocking(move || {
                lease.require_current()?;
                WorkbenchStore::open(&root)
                    .and_then(|store| store.set_recent_coverage(&key, coverage))
                    .map_err(|e| AccountError::new(e.code))?;
                lease.require_current()
            })
            .await
            .map_err(|_| unavailable())??;
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            result.error_code = Some(error.code.into());
        }
        result
    }
}

fn append_recent_page(
    traversal: &mut crate::discovery::Traversal,
    page: &SourcePage,
    ids: &mut Vec<String>,
    head: &mut Vec<String>,
) -> Result<bool> {
    if !page.issues.is_empty() {
        return Err(AccountError::new("SOURCE_ITEM_ISSUES"));
    }
    // Reuse the same strict source-boundary proof as author traversal. Only a
    // byte-identical raw JM boundary and equal SourceWork may skip one row.
    let accepted = traversal.append(page)?;
    let effective = page.items.iter().skip(accepted.skipped_leading_work);
    ids.extend(effective.clone().map(|work| work.work_id.clone()));
    if page.page <= 2 {
        head.extend(effective.map(|work| work.work_id.clone()));
        head.truncate(200);
    }
    Ok(accepted.complete)
}

fn contiguous_join(ids: &[String], previous: &[String]) -> bool {
    !previous.is_empty() && ids.windows(previous.len()).any(|window| window == previous)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_requires_the_entire_contiguous_sequence() {
        let ids = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(!contiguous_join(
            &ids(&["new", "a", "gap", "b"]),
            &ids(&["a", "b"])
        ));
        assert!(contiguous_join(&ids(&["new", "a", "b"]), &ids(&["a", "b"])));
        assert!(!contiguous_join(&[], &[]));
    }
}

#[cfg(test)]
mod recent_sequence_tests {
    use super::*;
    use crate::{JmSearchBoundary, JmSearchBoundaryItem};

    fn page(number: u64, total: u64, ids: &[&str], end: bool) -> SourcePage {
        let items: Vec<_> = ids
            .iter()
            .map(|id| SourceWork {
                source: Source::Jm,
                work_id: (*id).into(),
                title: format!("Fixture {id}"),
                authors: vec!["Writer".into()],
                description: None,
                tags: vec![],
                categories: None,
                favorite: None,
                chapter_count: None,
                page_count: None,
                source_updated_at: None,
                cover_available: false,
            })
            .collect();
        let edge = |work: &SourceWork| JmSearchBoundaryItem {
            work_id: work.work_id.clone(),
            fingerprint: format!("{:0>64}", work.work_id),
        };
        let boundary = JmSearchBoundary {
            first: items.first().map(edge),
            last: items.last().map(edge),
        };
        SourcePage {
            page: number,
            total: Some(total),
            pages: None,
            has_more: Some(!end),
            folders: vec![],
            items,
            issues: vec![],
            jm_search_boundary: Some(boundary),
        }
    }

    #[test]
    fn recent_verified_jm_boundary_overlap_does_not_break_contiguous_checkpoint() {
        let mut traversal = crate::discovery::Traversal::default();
        let (mut ids, mut head) = (vec![], vec![]);
        assert!(!append_recent_page(
            &mut traversal,
            &page(1, 3, &["100", "101"], false),
            &mut ids,
            &mut head
        )
        .unwrap());
        assert!(append_recent_page(
            &mut traversal,
            &page(2, 3, &["101", "102"], true),
            &mut ids,
            &mut head
        )
        .unwrap());
        assert_eq!(ids, ["100", "101", "102"]);
        assert_eq!(head, ids);
        assert!(contiguous_join(&ids, &["101".into(), "102".into()]));
    }

    #[test]
    fn recent_overlap_cannot_satisfy_a_source_total_with_too_few_effective_rows() {
        let mut traversal = crate::discovery::Traversal::default();
        let (mut ids, mut head) = (vec![], vec![]);
        append_recent_page(
            &mut traversal,
            &page(1, 4, &["100", "101"], false),
            &mut ids,
            &mut head,
        )
        .unwrap();
        assert!(append_recent_page(
            &mut traversal,
            &page(2, 4, &["101", "102"], true),
            &mut ids,
            &mut head
        )
        .is_err());
        assert_eq!(ids, ["100", "101"]);
        assert_eq!(head, ids);
    }

    #[test]
    fn changed_totals_unproven_overlap_or_page_repetition_do_not_advance_recent_checkpoint() {
        for kind in 0..7 {
            let mut traversal = crate::discovery::Traversal::default();
            let (mut ids, mut head) = (vec![], vec![]);
            append_recent_page(
                &mut traversal,
                &page(1, 4, &["100", "101"], false),
                &mut ids,
                &mut head,
            )
            .unwrap();
            let mut next = page(2, 4, &["102", "103"], true);
            match kind {
                0 => next.total = Some(3),
                1 => next.pages = Some(2),
                2 => next.page = 3,
                3 => {
                    next = page(2, 4, &["101", "102"], false);
                    next.jm_search_boundary = None;
                }
                4 => {
                    next = page(2, 4, &["101", "102"], false);
                    next.items[0].title = "Changed metadata".into();
                }
                5 => {
                    next = page(2, 4, &["101", "102"], false);
                    next.jm_search_boundary
                        .as_mut()
                        .unwrap()
                        .first
                        .as_mut()
                        .unwrap()
                        .fingerprint = "a".repeat(64);
                }
                _ => next = page(2, 4, &["102", "102"], true),
            }
            assert!(append_recent_page(&mut traversal, &next, &mut ids, &mut head).is_err());
            assert_eq!(ids, ["100", "101"]);
            assert_eq!(head, ids);
        }
    }
}

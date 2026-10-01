//! Cold-start special checks join the existing discovery executor, never a second scanner.
use crate::{AccountError, AccountService, DiscoveryMode, DiscoveryPhase, DiscoveryScope, Result, SourceBackend};
use serde::Serialize;
use std::{collections::{HashMap, HashSet}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use workbench_credentials::Vault;
use workbench_storage::{DiscoveryRangeState, LibraryReference, SpecialAccount, SpecialUpdate, WorkbenchStore};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialRun {
    pub id: u64,
    pub phase: String,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub new_count: usize,
    pub error_code: Option<String>,
}

#[derive(Default)]
struct Memory {
    run: SpecialRun,
    active: bool,
    owned_scan: Option<String>,
    dirty: bool,
}

#[derive(Default)]
pub(crate) struct SpecialControl {
    boot_checked: AtomicBool,
    memory: Mutex<Memory>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialAuthorView {
    pub author: String,
    pub enabled: bool,
    pub baselines_complete: usize,
    pub error_codes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialSnapshot {
    pub scopes: Vec<DiscoveryScope>,
    pub authors: Vec<SpecialAuthorView>,
    pub updates: Vec<SpecialUpdate>,
    pub run: SpecialRun,
}

fn unavailable() -> AccountError { AccountError::new("SPECIAL_UNAVAILABLE") }

impl<B: SourceBackend, V: Vault + 'static> AccountService<B, V> {
    pub fn special_run(&self) -> Result<SpecialRun> {
        Ok(self.special.memory.lock().map_err(|_| unavailable())?.run.clone())
    }

    /// Called after account restoration; the process-owned gate survives webview
    /// reloads and opening another reader/main window. No credentials are added.
    pub fn special_cold_start(self: &Arc<Self>) {
        if !self.special.boot_checked.swap(true, Ordering::AcqRel) {
            let _ = self.special_start();
        }
    }

    pub fn special_start(self: &Arc<Self>) -> Result<SpecialRun> {
        let run = {
            let mut memory = self.special.memory.lock().map_err(|_| unavailable())?;
            if memory.active { return Ok(memory.run.clone()); }
            memory.run = SpecialRun { id: memory.run.id + 1, phase: "waiting".into(), started_at: Some(crate::cache::now_ms()?), ..Default::default() };
            memory.active = true;
            memory.owned_scan = None;
            memory.dirty = false;
            memory.run.clone()
        };
        let service = Arc::clone(self);
        let captured = run.clone();
        tokio::spawn(async move {
          loop {
            let outcome = service.special_check(captured.id, captured.started_at.unwrap()).await;
            if let Ok(mut memory) = service.special.memory.lock() {
                if memory.run.id != captured.id || !memory.active { return; }
                if memory.dirty {
                    memory.dirty = false;
                    continue;
                }
                memory.active = false;
                memory.owned_scan = None;
                memory.run.finished_at = crate::cache::now_ms().ok();
                match outcome {
                    Ok((phase, count)) => { memory.run.phase = phase; memory.run.new_count = count; }
                    Err(error) => {
                        memory.run.phase = if matches!(error.code, "AUTH_REQUIRED" | "SESSION_EXPIRED" | "SESSION_CHANGED" | "LOGIN_REQUIRED" | "STALE_SESSION" | "DISCOVERY_SCOPES_REQUIRED") { "unavailable" } else { "error" }.into();
                        memory.run.error_code = Some(error.code.into());
                    }
                }
            }
            break;
          }
        });
        Ok(run)
    }

    fn special_current(&self, id: u64) -> bool {
        self.special.memory.lock().is_ok_and(|memory| memory.active && memory.run.id == id)
    }

    pub fn special_cancel(&self) -> Result<SpecialRun> {
        let (run, owned) = {
            let mut memory = self.special.memory.lock().map_err(|_| unavailable())?;
            memory.active = false;
            memory.run.phase = "cancelled".into();
            memory.run.finished_at = crate::cache::now_ms().ok();
            (memory.run.clone(), memory.owned_scan.take())
        };
        if let Some(id) = owned { let _ = self.discovery_cancel(&id); }
        Ok(run)
    }

    async fn special_account(&self, context: &crate::discovery::DiscoveryContext) -> Result<SpecialAccount> {
        let root = context.root.clone();
        let key = context.account_key.clone();
        let account = tokio::task::spawn_blocking(move || {
            let document = WorkbenchStore::open(root)?.read_special_follows()?;
            Ok::<_, workbench_storage::StoreError>(document.value.accounts.into_iter().find(|account| account.account_key == key).unwrap_or(SpecialAccount { account_key: key, ..Default::default() }))
        }).await.map_err(|_| unavailable())?.map_err(|error| AccountError::new(error.code))?;
        self.discovery_validate_context(context)?;
        Ok(account)
    }

    pub async fn special_read(&self, scopes: Vec<DiscoveryScope>) -> Result<SpecialSnapshot> {
        let context = self.discovery_context(scopes.clone()).await?;
        let account = self.special_account(&context).await?;
        let enabled: HashSet<_> = account.authors.iter().filter(|row| row.enabled && context.authors.contains(&row.author)).map(|row| row.author.clone()).collect();
        Ok(SpecialSnapshot {
            scopes,
            authors: account.authors.into_iter().filter(|row| context.authors.contains(&row.author)).map(|row| {
                let baselines_complete = row.ranges.iter().filter(|range| range.baseline_complete && context.policy(range.source, &row.author).is_some_and(|policy| range.query_fingerprint.as_ref() == Some(&policy.query_fingerprint))).count();
                SpecialAuthorView { author: row.author, enabled: row.enabled, baselines_complete,
                error_codes: row.ranges.iter().filter_map(|range| range.error_code.clone()).collect() }
            }).collect(),
            updates: account.updates.into_iter().filter(|update| update.authors.iter().any(|author| enabled.contains(author))).collect(),
            run: self.special_run()?,
        })
    }

    pub async fn special_set(self: &Arc<Self>, scopes: Vec<DiscoveryScope>, author: String, enabled: bool) -> Result<SpecialSnapshot> {
        let context = self.discovery_context(scopes.clone()).await?;
        if !context.authors.contains(&author) { return Err(AccountError::new("DISCOVERY_AUTHOR_NOT_FOLLOWED")); }
        let key = context.account_key.clone();
        let revisions = (context.following_revision, context.policy_revision);
        self.special_store_operation(&context, move |store| store.edit_special_follows(&key, revisions, |account| account.set_author(&author, enabled))).await?;
        // If a check is active, it will pick up the new pending baseline in its
        // next pass. The ordinary following document is never written here.
        if enabled {
            {
                let mut memory = self.special.memory.lock().map_err(|_| unavailable())?;
                if memory.active { memory.dirty = true; }
            }
            self.special_start()?;
        }
        self.special_read(scopes).await
    }

    pub async fn special_mark_read(&self, scopes: Vec<DiscoveryScope>, identity: Option<LibraryReference>) -> Result<SpecialSnapshot> {
        if identity.as_ref().is_some_and(|key| !key.is_valid()) { return Err(AccountError::new("INVALID_INPUT")); }
        let context = self.discovery_context(scopes.clone()).await?;
        let key = context.account_key.clone();
        let now = crate::cache::now_ms()?;
        let revisions = (context.following_revision, context.policy_revision);
        self.special_store_operation(&context, move |store| store.edit_special_follows(&key, revisions, |account| account.mark_read(identity.as_ref(), now))).await?;
        self.special_read(scopes).await
    }

    async fn special_check(self: &Arc<Self>, id: u64, started: u64) -> Result<(String, usize)> {
        let scopes = self.current_discovery_scopes().await?;
        let mut attempted = HashSet::new();
        let mut partial = false;
        loop {
            if !self.special_current(id) { return Ok(("cancelled".into(), 0)); }
            let context = self.discovery_context(scopes.clone()).await?;
            let account = self.special_account(&context).await?;
            let enabled: HashSet<_> = account.authors.iter().filter(|row| row.enabled && context.authors.contains(&row.author)).map(|row| row.author.clone()).collect();
            if enabled.is_empty() { return Ok(("idle".into(), 0)); }
            let progress = self.discovery_progress(scopes.clone()).await?;
            if progress.run.as_ref().is_some_and(|run| run.phase == DiscoveryPhase::Checking) {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            // A concurrent manual scan may already have satisfied these pairs.
            let snapshot = self.discovery_read(scopes.clone()).await?;
            let checked: HashSet<_> = snapshot.authors.iter().filter(|range| range.last_checked_at.or(range.last_complete_at).is_some_and(|time| time >= started) && range.state == DiscoveryRangeState::Complete).map(|range| (range.author.clone(), range.source)).collect();
            let mut wanted = HashSet::new();
            let mut needs_baseline = false;
            for author in account.authors.iter().filter(|row| enabled.contains(&row.author)) {
                for range in &author.ranges {
                    let key = (author.author.clone(), range.source);
                    if !attempted.contains(&key) && !checked.contains(&key) {
                        wanted.insert(key);
                        needs_baseline |= !range.baseline_complete || context.policy(range.source, &author.author).is_some_and(|policy| range.query_fingerprint.as_ref() != Some(&policy.query_fingerprint));
                    }
                }
            }
            if !checked.is_empty() || !attempted.is_empty() {
                let checked_or_attempted: HashSet<_> = checked.union(&attempted).cloned().collect();
                let mut groups: HashMap<(String, workbench_storage::Source), Vec<workbench_storage::DiscoveryWork>> = HashMap::new();
                for record in &snapshot.records {
                    for author in context.confirmed_authors(record) {
                        if enabled.contains(&author) { groups.entry((author, record.work.source)).or_default().push(record.work.clone()); }
                    }
                }
                let observations: Vec<_> = snapshot.authors.iter().filter(|range| checked_or_attempted.contains(&(range.author.clone(), range.source)) && enabled.contains(&range.author)).filter_map(|range| {
                    let fingerprint = context.policy(range.source, &range.author)?.query_fingerprint.clone();
                    let complete = range.state == DiscoveryRangeState::Complete;
                    partial |= !complete;
                    Some((range.clone(), fingerprint, groups.remove(&(range.author.clone(), range.source)).unwrap_or_default(), complete))
                }).collect();
                let key = context.account_key.clone();
                let now = crate::cache::now_ms()?;
                let revisions = (context.following_revision, context.policy_revision);
                self.special_store_operation(&context, move |store| store.edit_special_follows(&key, revisions, |account| {
                    for (range, fingerprint, works, complete) in observations {
                        account.observe(&range.author, range.source, &fingerprint, &works, complete, now, range.error_code);
                    }
                })).await?;
                attempted.extend(checked);
            }
            if wanted.is_empty() {
                let saved = self.special_account(&context).await?;
                let count = saved.updates.iter().filter(|update| update.discovered_at >= started && update.authors.iter().any(|author| enabled.contains(author))).count();
                return Ok((if partial { "partial" } else { "complete" }.into(), count));
            }
            let names: Vec<_> = wanted.iter().map(|(author,_)| author.clone()).collect::<HashSet<_>>().into_iter().collect();
            let mode = if needs_baseline { DiscoveryMode::Full } else { DiscoveryMode::Incremental };
            match self.discovery_start_selected(scopes.clone(), names, mode, false, Some(wanted.clone())).await {
                Ok(scan) => {
                    attempted.extend(wanted);
                    let cancelled = {
                        let mut memory = self.special.memory.lock().map_err(|_| unavailable())?;
                        if memory.run.id == id && memory.active { memory.owned_scan = Some(scan.run_id.clone()); memory.run.phase = "checking".into(); false } else { true }
                    };
                    if cancelled { let _ = self.discovery_cancel(&scan.run_id); return Ok(("cancelled".into(), 0)); }
                }
                Err(error) if error.code == "DISCOVERY_BUSY" => { tokio::time::sleep(Duration::from_secs(1)).await; }
                Err(error) => return Err(error),
            }
        }
    }
}

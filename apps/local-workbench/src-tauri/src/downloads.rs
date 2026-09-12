use crate::{accounts, library, require_main, DesktopStore};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{DownloadSession, QueryKind, Source};
use workbench_downloads::{
    Control, DownloadPlan, DownloadService, DownloadSnapshot, JmDownloadMetadata,
    PreparedSelection, TaskSelection,
};
use workbench_storage::{
    LibraryReference, Source as LibrarySource, StoreError, WorkbenchStore, MAX_DOWNLOAD_BATCH,
};

/// Session bindings are deliberately process-local. A reopened task needs an
/// explicit continue action under the currently authenticated source session.
#[derive(Default)]
pub(crate) struct DesktopDownloads {
    service: DownloadService,
    plans: Mutex<HashMap<String, DownloadSession>>,
    batches: Mutex<HashMap<String, PreparedBatch>>,
    scheduler: Mutex<Scheduler<DownloadSession>>,
}

#[derive(Clone)]
struct PreparedBatch {
    plans: Vec<PreparedSelection>,
    session: DownloadSession,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadBatchPlan {
    batch_id: Option<String>,
    plans: Vec<DownloadPlan>,
    issues: Vec<DownloadBatchIssue>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadBatchIssue {
    input: String,
    error_code: &'static str,
}
struct Scheduled<T> {
    task_id: String,
    revision: u64,
    session: T,
}
/// One driver processes the complete work lifecycle, including PC registration,
/// before taking the next item. Sessions and pending admissions are never saved.
struct Scheduler<T> {
    pending: VecDeque<Scheduled<T>>,
    running: bool,
}
impl<T> Default for Scheduler<T> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            running: false,
        }
    }
}
impl<T> Scheduler<T> {
    fn enqueue(&mut self, items: Vec<Scheduled<T>>) -> bool {
        for item in items {
            self.pending.retain(|old| old.task_id != item.task_id);
            self.pending.push_back(item);
        }
        if self.running || self.pending.is_empty() {
            false
        } else {
            self.running = true;
            true
        }
    }
    fn take_next(&mut self) -> Option<Scheduled<T>> {
        let item = self.pending.pop_front();
        if item.is_none() {
            self.running = false;
        }
        item
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DownloadScope {
    source: Source,
    session_id: String,
}

fn error(code: &'static str) -> StoreError {
    StoreError { code }
}

fn parse_input(source: Source, input: &str) -> Result<String, StoreError> {
    let trimmed = input.trim();
    let normalized = if source == Source::Jm
        && trimmed
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("JM"))
        && trimmed[2..].bytes().all(|c| c.is_ascii_digit())
    {
        &trimmed[2..]
    } else {
        trimmed
    };
    workbench_sources::parse_work_id(source, normalized).map_err(|e| error(e.code))
}

fn storage_source(source: Source) -> LibrarySource {
    match source {
        Source::Jm => LibrarySource::Jm,
        Source::Pica => LibrarySource::Pica,
    }
}

fn live_execution_allowed() -> Result<(), StoreError> {
    if std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v.eq_ignore_ascii_case("true")) {
        Err(error("DOWNLOAD_LIVE_EXECUTION_DISABLED_IN_CI"))
    } else {
        Ok(())
    }
}

async fn open_store(state: Arc<DesktopStore>) -> Result<Arc<WorkbenchStore>, StoreError> {
    tauri::async_runtime::spawn_blocking(move || state.open())
        .await
        .map_err(|_| error("STORE_UNAVAILABLE"))?
}

async fn lease(
    accounts: Arc<accounts::DesktopAccounts>,
    scope: &DownloadScope,
) -> Result<DownloadSession, StoreError> {
    accounts::service(accounts)
        .await
        .map_err(|e| error(e.code))?
        .download_session(scope.source, &scope.session_id)
        .await
        .map_err(|e| error(e.code))
}

#[tauri::command]
pub(crate) async fn jm_download_read<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    recheck_files: Option<bool>,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    let downloads = Arc::clone(downloads.inner());
    let store = open_store(Arc::clone(store.inner())).await?;
    tauri::async_runtime::spawn_blocking(move || {
        downloads
            .service
            .read_with_file_check(&store, recheck_files.unwrap_or(true))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn jm_download_prepare<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    scope: DownloadScope,
    input: String,
    root_id: String,
    generation: u64,
) -> Result<DownloadPlan, StoreError> {
    require_main(window.label())?;
    live_execution_allowed()?;
    let work_id = parse_input(scope.source, &input)?;
    let session = lease(Arc::clone(accounts.inner()), &scope).await?;
    let account_service = accounts::service(Arc::clone(accounts.inner()))
        .await
        .map_err(|e| error(e.code))?;
    let detail = account_service
        .query(
            scope.source,
            &scope.session_id,
            QueryKind::Detail,
            &work_id,
            None,
            1,
        )
        .await
        .map_err(|e| error(e.code))?;
    session.require_current().map_err(|e| error(e.code))?;
    let work = detail
        .page
        .items
        .into_iter()
        .next()
        .filter(|w| w.source == scope.source && w.work_id == work_id)
        .ok_or(error("DOWNLOAD_METADATA_INVALID"))?;
    let metadata = JmDownloadMetadata {
        work_id: work.work_id,
        title: work.title,
        authors: work.authors,
        tags: work.tags,
        description: work.description,
    };
    let downloads = Arc::clone(downloads.inner());
    let store = open_store(Arc::clone(store.inner())).await?;
    tauri::async_runtime::spawn_blocking(move || {
        session.require_current().map_err(|e| error(e.code))?;
        let plan = downloads.service.prepare_for_source(
            &store,
            &root_id,
            generation,
            storage_source(scope.source),
            metadata,
        )?;
        let mut plans = downloads
            .plans
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        // Prepared confirmations are short-lived UI state, never an unbounded queue.
        if plans.len() >= 16 {
            downloads
                .service
                .discard_plans(&plans.keys().cloned().collect::<Vec<_>>())?;
            plans.clear();
        }
        plans.insert(plan.plan_id.clone(), session);
        Ok(plan)
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn jm_download_batch_prepare<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    scope: DownloadScope,
    inputs: Vec<String>,
    root_id: String,
    generation: u64,
) -> Result<DownloadBatchPlan, StoreError> {
    require_main(window.label())?;
    live_execution_allowed()?;
    if inputs.is_empty()
        || inputs.len() > MAX_DOWNLOAD_BATCH
        || inputs.iter().any(|input| input.len() > 2048)
    {
        return Err(error("DOWNLOAD_BATCH_LIMIT"));
    }
    let downloads = Arc::clone(downloads.inner());
    let store = open_store(Arc::clone(store.inner())).await?;
    let session = lease(Arc::clone(accounts.inner()), &scope).await?;
    let account_service = accounts::service(Arc::clone(accounts.inner()))
        .await
        .map_err(|e| error(e.code))?;
    // Replacing a prepared batch expires its confirmations, not any queued work.
    {
        let mut batches = downloads
            .batches
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        let old: Vec<_> = batches
            .values()
            .flat_map(|batch| batch.plans.iter().map(|plan| plan.plan_id.clone()))
            .collect();
        downloads.service.discard_plans(&old)?;
        batches.clear();
    }
    let mut seen = HashSet::new();
    let mut result = DownloadBatchPlan {
        batch_id: None,
        plans: Vec::new(),
        issues: Vec::new(),
    };
    for input in inputs {
        let input = input.trim().to_owned();
        let prepared = async {
            session.require_current().map_err(|e| error(e.code))?;
            let work_id = parse_input(scope.source, &input)?;
            if !seen.insert(work_id.clone()) {
                return Err(error("DOWNLOAD_BATCH_DUPLICATE"));
            }
            let detail = account_service
                .query(
                    scope.source,
                    &scope.session_id,
                    QueryKind::Detail,
                    &work_id,
                    None,
                    1,
                )
                .await
                .map_err(|e| error(e.code))?;
            session.require_current().map_err(|e| error(e.code))?;
            let work = detail
                .page
                .items
                .into_iter()
                .next()
                .filter(|work| work.source == scope.source && work.work_id == work_id)
                .ok_or(error("DOWNLOAD_METADATA_INVALID"))?;
            let metadata = JmDownloadMetadata {
                work_id: work.work_id,
                title: work.title,
                authors: work.authors,
                tags: work.tags,
                description: work.description,
            };
            let downloads = Arc::clone(&downloads);
            let store = Arc::clone(&store);
            let root_id = root_id.clone();
            let session = session.clone();
            tauri::async_runtime::spawn_blocking(move || {
                session.require_current().map_err(|e| error(e.code))?;
                downloads.service.prepare_for_source(
                    &store,
                    &root_id,
                    generation,
                    storage_source(scope.source),
                    metadata,
                )
            })
            .await
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
        }
        .await;
        match prepared {
            Ok(plan) => result.plans.push(plan),
            Err(problem) => result.issues.push(DownloadBatchIssue {
                input,
                error_code: problem.code,
            }),
        }
        if let Err(problem) = session.require_current() {
            downloads.service.discard_plans(
                &result
                    .plans
                    .iter()
                    .map(|p| p.plan_id.clone())
                    .collect::<Vec<_>>(),
            )?;
            return Err(error(problem.code));
        }
    }
    if let Some(first) = result.plans.first() {
        let batch_id = first.plan_id.clone();
        let batch = PreparedBatch {
            plans: result
                .plans
                .iter()
                .map(|plan| PreparedSelection {
                    plan_id: plan.plan_id.clone(),
                    expected_revision: plan.revision,
                })
                .collect(),
            session,
        };
        let mut batches = downloads
            .batches
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        // A bounded concurrent caller cannot accumulate unbounded credentials/plans.
        if batches.len() >= 4 {
            downloads.service.discard_plans(
                &batch
                    .plans
                    .iter()
                    .map(|p| p.plan_id.clone())
                    .collect::<Vec<_>>(),
            )?;
            return Err(error("DOWNLOAD_PLAN_LIMIT_REACHED"));
        }
        batches.insert(batch_id.clone(), batch);
        result.batch_id = Some(batch_id);
    }
    Ok(result)
}

fn launch(
    downloads: Arc<DesktopDownloads>,
    store_state: Arc<DesktopStore>,
    store: Arc<WorkbenchStore>,
    library_state: Arc<library::DesktopLibrary>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(async move {
            loop {
                let scheduled = match downloads.scheduler.lock() {
                    Ok(mut scheduler) => scheduler.take_next(),
                    Err(_) => return,
                };
                let Some(scheduled) = scheduled else {
                    return;
                };
                run_one(
                    Arc::clone(&downloads),
                    Arc::clone(&store_state),
                    Arc::clone(&store),
                    Arc::clone(&library_state),
                    scheduled,
                )
                .await;
            }
        })
    });
}

async fn run_one(
    downloads: Arc<DesktopDownloads>,
    store_state: Arc<DesktopStore>,
    store: Arc<WorkbenchStore>,
    library_state: Arc<library::DesktopLibrary>,
    scheduled: Scheduled<DownloadSession>,
) {
    let task_id = scheduled.task_id;
    let revision = scheduled.revision;
    let session = scheduled.session;
    let token = require_task_scope(
        &downloads.service,
        &store,
        &task_id,
        revision,
        session.source(),
    )
    .and_then(|()| session.pica_token().map_err(|problem| error(problem.code)));
    let result = match token {
        Ok(token) => {
            downloads
                .service
                .run_selected_with_token(&store, &task_id, revision, token, || {
                    session.require_current().map_err(|e| error(e.code))
                })
                .await
        }
        Err(problem) => Err(error(problem.code)),
    };
    let receipt = match result {
        Ok(Some(receipt)) => receipt,
        Ok(None) => return,
        Err(problem) => {
            let _ = downloads
                .service
                .fail_queued(&store, &task_id, revision, problem.code);
            return;
        }
    };
    if let Err(problem) = downloads.service.validate_receipt(&store, &receipt) {
        let _ = downloads
            .service
            .index_failed(&store, &receipt, problem.code);
        return;
    }
    let reference = LibraryReference {
        source: receipt.source,
        work_id: receipt.work_id.clone(),
    };
    let root_id = receipt.root_id.clone();
    let generation = receipt.generation;
    let relative_path = receipt.relative_path.clone();
    let expected_pages = receipt.expected_pages;
    let indexed = library::with_library(library_state, store_state, move |library, store| {
        library.register_completed(
            store,
            &root_id,
            generation,
            &relative_path,
            &reference,
            expected_pages,
        )
    })
    .await;
    let finished = match indexed {
        Ok(snapshot) => {
            if let Some(entry) = snapshot
                .items
                .iter()
                .find(|v| v.relative_path == receipt.relative_path)
            {
                downloads.service.mark_indexed(&store, &receipt, &entry.id)
            } else {
                downloads
                    .service
                    .index_failed(&store, &receipt, "DOWNLOAD_INDEX_MISSING")
            }
        }
        Err(problem) => downloads
            .service
            .index_failed(&store, &receipt, problem.code),
    };
    // The service retains a retryable receipt if a checkpoint fails. No
    // renderer response or error log includes a media path or source secret.
    if let Err(problem) = finished {
        let _ = downloads
            .service
            .index_failed(&store, &receipt, problem.code);
    }
}

fn scheduled_tasks(
    snapshot: &DownloadSnapshot,
    ids: &[String],
    session: &DownloadSession,
) -> Result<Vec<Scheduled<DownloadSession>>, StoreError> {
    ids.iter()
        .map(|id| {
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == *id && task.source == storage_source(session.source()))
                .ok_or(error("DOWNLOAD_TASK_MISSING"))?;
            Ok(Scheduled {
                task_id: id.clone(),
                revision: task.revision,
                session: session.clone(),
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) async fn jm_download_confirm<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    plan_id: String,
    expected_revision: u64,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    live_execution_allowed()?;
    let downloads = Arc::clone(downloads.inner());
    let store_state = Arc::clone(store.inner());
    let library_state = Arc::clone(library.inner());
    let store = open_store(Arc::clone(&store_state)).await?;
    let worker_downloads = Arc::clone(&downloads);
    let worker_store = Arc::clone(&store);
    let (snapshot, starts) = tauri::async_runtime::spawn_blocking(move || {
        let mut scheduler = worker_downloads
            .scheduler
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        let session = worker_downloads
            .plans
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
            .get(&plan_id)
            .cloned()
            .ok_or(error("DOWNLOAD_PLAN_EXPIRED"))?;
        session.require_current().map_err(|e| error(e.code))?;
        let snapshot =
            worker_downloads
                .service
                .confirm(&worker_store, &plan_id, expected_revision)?;
        let scheduled = scheduled_tasks(&snapshot, std::slice::from_ref(&plan_id), &session)?;
        worker_downloads
            .plans
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
            .remove(&plan_id);
        Ok::<_, StoreError>((snapshot, scheduler.enqueue(scheduled)))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    if starts {
        launch(downloads, store_state, store, library_state);
    }
    Ok(snapshot)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn jm_download_control<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    scope: DownloadScope,
    task_id: String,
    expected_revision: u64,
    action: Control,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    let starts = !matches!(action, Control::Pause);
    let session = if starts {
        live_execution_allowed()?;
        Some(lease(Arc::clone(accounts.inner()), &scope).await?)
    } else {
        None
    };
    let downloads = Arc::clone(downloads.inner());
    let store_state = Arc::clone(store.inner());
    let library_state = Arc::clone(library.inner());
    let store = open_store(Arc::clone(&store_state)).await?;
    let worker_downloads = Arc::clone(&downloads);
    let worker_store = Arc::clone(&store);
    let worker_id = task_id.clone();
    let (snapshot, starts) = tauri::async_runtime::spawn_blocking(move || {
        let mut scheduler = worker_downloads
            .scheduler
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        if let Some(session) = &session {
            session.require_current().map_err(|e| error(e.code))?;
        }
        require_task_scope(
            &worker_downloads.service,
            &worker_store,
            &worker_id,
            expected_revision,
            scope.source,
        )?;
        let snapshot = worker_downloads.service.control(
            &worker_store,
            &worker_id,
            expected_revision,
            action,
        )?;
        let starts = if let Some(session) = session {
            scheduler.enqueue(scheduled_tasks(&snapshot, &[worker_id], &session)?)
        } else {
            scheduler.pending.retain(|item| item.task_id != worker_id);
            false
        };
        Ok::<_, StoreError>((snapshot, starts))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    if starts {
        launch(downloads, store_state, store, library_state);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn jm_download_batch_confirm<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    batch_id: String,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    live_execution_allowed()?;
    let downloads = Arc::clone(downloads.inner());
    let store_state = Arc::clone(store.inner());
    let library_state = Arc::clone(library.inner());
    let store = open_store(Arc::clone(&store_state)).await?;
    let worker_downloads = Arc::clone(&downloads);
    let worker_store = Arc::clone(&store);
    let (snapshot, starts) = tauri::async_runtime::spawn_blocking(move || {
        let mut scheduler = worker_downloads
            .scheduler
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        let mut batches = worker_downloads
            .batches
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        let batch = batches
            .get(&batch_id)
            .ok_or(error("DOWNLOAD_PLAN_EXPIRED"))?;
        batch.session.require_current().map_err(|e| error(e.code))?;
        let snapshot = worker_downloads
            .service
            .confirm_many(&worker_store, &batch.plans)?;
        let ids: Vec<_> = batch
            .plans
            .iter()
            .map(|plan| plan.plan_id.clone())
            .collect();
        let scheduled = scheduled_tasks(&snapshot, &ids, &batch.session)?;
        batches.remove(&batch_id);
        Ok::<_, StoreError>((snapshot, scheduler.enqueue(scheduled)))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    if starts {
        launch(downloads, store_state, store, library_state);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn jm_download_pause_all<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    let downloads = Arc::clone(downloads.inner());
    let store = open_store(Arc::clone(store.inner())).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut scheduler = downloads
            .scheduler
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        let snapshot = downloads.service.pause_all(&store)?;
        scheduler.pending.clear();
        Ok(snapshot)
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
}

#[tauri::command]
pub(crate) async fn jm_download_resume_many<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    scope: DownloadScope,
    tasks: Vec<TaskSelection>,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    live_execution_allowed()?;
    let session = lease(Arc::clone(accounts.inner()), &scope).await?;
    let downloads = Arc::clone(downloads.inner());
    let store_state = Arc::clone(store.inner());
    let library_state = Arc::clone(library.inner());
    let store = open_store(Arc::clone(&store_state)).await?;
    let worker_downloads = Arc::clone(&downloads);
    let worker_store = Arc::clone(&store);
    let (snapshot, starts) = tauri::async_runtime::spawn_blocking(move || {
        let mut scheduler = worker_downloads
            .scheduler
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?;
        session.require_current().map_err(|e| error(e.code))?;
        let snapshot = worker_downloads.service.resume_many(
            &worker_store,
            storage_source(scope.source),
            &tasks,
        )?;
        let ids: Vec<_> = tasks.into_iter().map(|task| task.task_id).collect();
        let scheduled = scheduled_tasks(&snapshot, &ids, &session)?;
        Ok::<_, StoreError>((snapshot, scheduler.enqueue(scheduled)))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    if starts {
        launch(downloads, store_state, store, library_state);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn jm_download_history_remove<R: Runtime>(
    window: WebviewWindow<R>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    store: State<'_, Arc<DesktopStore>>,
    tasks: Vec<TaskSelection>,
) -> Result<DownloadSnapshot, StoreError> {
    require_main(window.label())?;
    let downloads = Arc::clone(downloads.inner());
    let store = open_store(Arc::clone(store.inner())).await?;
    tauri::async_runtime::spawn_blocking(move || downloads.service.remove_history(&store, &tasks))
        .await
        .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
}

fn require_task_scope(
    service: &DownloadService,
    store: &WorkbenchStore,
    task_id: &str,
    expected_revision: u64,
    source: Source,
) -> Result<(), StoreError> {
    if service.task_source(store, task_id, expected_revision)? != storage_source(source) {
        return Err(error("DOWNLOAD_SOURCE_MISMATCH"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scheduled(id: &str, revision: u64) -> Scheduled<&'static str> {
        Scheduled {
            task_id: id.into(),
            revision,
            session: "synthetic memory lease",
        }
    }
    #[test]
    fn queue_driver_is_single_and_fifo_even_when_new_work_is_confirmed_mid_download() {
        let mut scheduler = Scheduler::default();
        assert!(scheduler.enqueue(vec![scheduled("first", 1), scheduled("second", 1)]));
        assert_eq!(scheduler.take_next().unwrap().task_id, "first");
        assert!(
            scheduler.running,
            "driver remains reserved through materialize and registration"
        );
        assert!(
            !scheduler.enqueue(vec![scheduled("third", 1)]),
            "no second driver"
        );
        // The same driver takes the next item after success, an ordinary source
        // failure, or a pause; no detached per-work worker is spawned.
        assert_eq!(scheduler.take_next().unwrap().task_id, "second");
        assert_eq!(scheduler.take_next().unwrap().task_id, "third");
        assert!(scheduler.take_next().is_none());
        assert!(!scheduler.running);
        assert!(scheduler.enqueue(vec![scheduled("fourth", 1)]));
    }
    #[test]
    fn resumed_epochs_replace_pending_entries_and_pause_all_drops_memory_leases() {
        let mut scheduler = Scheduler::default();
        scheduler.enqueue(vec![scheduled("first", 1), scheduled("second", 1)]);
        scheduler.take_next().unwrap();
        assert!(!scheduler.enqueue(vec![scheduled("second", 3)]));
        assert_eq!(scheduler.pending.len(), 1);
        assert_eq!(scheduler.pending.front().unwrap().revision, 3);
        scheduler.pending.clear();
        assert!(scheduler.take_next().is_none());
        let reopened: Scheduler<&str> = Scheduler::default();
        assert!(reopened.pending.is_empty());
        assert!(!reopened.running);
    }
    #[test]
    fn batch_tokens_and_task_controls_accept_only_the_declared_identifier_revision_shape() {
        let valid: TaskSelection = serde_json::from_value(serde_json::json!({
            "taskId": "a".repeat(64), "expectedRevision": 3,
        }))
        .unwrap();
        assert_eq!(valid.expected_revision, 3);
        for extra in ["path", "token", "source", "mediaUrl"] {
            let mut input = serde_json::json!({ "taskId": "a".repeat(64), "expectedRevision": 3 });
            input[extra] = "renderer authority is not accepted".into();
            assert!(serde_json::from_value::<TaskSelection>(input).is_err());
        }
    }
    #[test]
    fn manual_jm_prefix_is_normalized_without_accepting_arbitrary_paths_or_hosts() {
        for input in ["JM123", "jm123", " 123 "] {
            assert_eq!(parse_input(Source::Jm, input).unwrap(), "123");
        }
        for input in [
            "JM",
            "JM../123",
            "C:/private/123",
            "https://example.invalid/album/123",
        ] {
            assert!(parse_input(Source::Jm, input).is_err());
        }
    }

    #[test]
    fn manual_pica_input_is_normalized_and_remains_bound_to_its_source() {
        let id = "0123456789abcdef01234567";
        for input in [
            id.to_owned(),
            format!(" {id} "),
            format!("https://picaapi.picacomic.com/comics/{id}"),
        ] {
            assert_eq!(parse_input(Source::Pica, &input).unwrap(), id);
        }
        for input in [
            "JM123",
            "123",
            "C:/private/0123456789abcdef01234567",
            "https://example.invalid/comics/0123456789abcdef01234567",
        ] {
            assert!(parse_input(Source::Pica, input).is_err());
        }
        assert!(parse_input(Source::Jm, id).is_err());
    }

    #[test]
    fn queue_control_checks_the_tasks_source_and_revision_even_for_pause() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("media");
        std::fs::create_dir(&media).unwrap();
        let store = WorkbenchStore::open(temp.path().join("app")).unwrap();
        let mut library = workbench_library::LibraryService::new();
        let mut state = library.choose(&store, &media).unwrap();
        while state.phase == workbench_library::LibraryPhase::Reading {
            state = library
                .scan(
                    &store,
                    state.root_id.as_deref().unwrap(),
                    state.generation,
                    workbench_library::ScanAction::Next,
                )
                .unwrap();
        }
        let service = DownloadService::new();
        let plan = service
            .prepare_for_source(
                &store,
                state.root_id.as_deref().unwrap(),
                state.generation,
                LibrarySource::Pica,
                JmDownloadMetadata {
                    work_id: "0123456789abcdef01234567".into(),
                    title: "Synthetic source-bound task".into(),
                    authors: vec![],
                    tags: vec![],
                    description: None,
                },
            )
            .unwrap();
        let queue = service
            .confirm(&store, &plan.plan_id, plan.revision)
            .unwrap();
        let task = &queue.tasks[0];
        let before = serde_json::to_value(store.read_downloads().unwrap()).unwrap();
        assert_eq!(
            require_task_scope(&service, &store, &task.id, task.revision, Source::Jm)
                .unwrap_err()
                .code,
            "DOWNLOAD_SOURCE_MISMATCH"
        );
        assert!(
            require_task_scope(&service, &store, &task.id, task.revision, Source::Pica).is_ok()
        );
        assert!(
            require_task_scope(&service, &store, &task.id, task.revision + 1, Source::Pica)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(store.read_downloads().unwrap()).unwrap(),
            before
        );
    }
}

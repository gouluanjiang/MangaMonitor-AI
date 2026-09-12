use crate::{accounts, library, require_main, DesktopStore};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{DownloadSession, QueryKind, Source};
use workbench_downloads::{
    Control, DownloadPlan, DownloadService, DownloadSnapshot, JmDownloadMetadata,
};
use workbench_storage::{LibraryReference, Source as LibrarySource, StoreError, WorkbenchStore};

/// Session bindings are deliberately process-local. A reopened task needs an
/// explicit continue action under the currently authenticated source session.
#[derive(Default)]
pub(crate) struct DesktopDownloads {
    service: DownloadService,
    plans: Mutex<HashMap<String, DownloadSession>>,
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
            plans.clear();
        }
        plans.insert(plan.plan_id.clone(), session);
        Ok(plan)
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
}

fn launch(
    downloads: Arc<DesktopDownloads>,
    store_state: Arc<DesktopStore>,
    store: Arc<WorkbenchStore>,
    library_state: Arc<library::DesktopLibrary>,
    task_id: String,
    session: DownloadSession,
) {
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(async move {
            let result = match session.pica_token() {
                Ok(token) => {
                    downloads
                        .service
                        .run_with_token(&store, &task_id, token, || {
                            session.require_current().map_err(|e| error(e.code))
                        })
                        .await
                }
                Err(problem) => Err(error(problem.code)),
            };
            let Ok(Some(receipt)) = result else {
                return;
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
            let indexed =
                library::with_library(library_state, store_state, move |library, store| {
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
        })
    });
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
    let (snapshot, session, task_id) = tauri::async_runtime::spawn_blocking(move || {
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
        let task_id = snapshot
            .tasks
            .iter()
            .find(|task| task.id == plan_id && task.source == storage_source(session.source()))
            .map(|task| task.id.clone())
            .ok_or(error("DOWNLOAD_TASK_MISSING"))?;
        worker_downloads
            .plans
            .lock()
            .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))?
            .remove(&plan_id);
        Ok::<_, StoreError>((snapshot, session, task_id))
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    launch(
        downloads,
        store_state,
        store,
        library_state,
        task_id,
        session,
    );
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
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        require_task_scope(
            &worker_downloads.service,
            &worker_store,
            &worker_id,
            expected_revision,
            scope.source,
        )?;
        worker_downloads
            .service
            .control(&worker_store, &worker_id, expected_revision, action)
    })
    .await
    .map_err(|_| error("DOWNLOAD_UNAVAILABLE"))??;
    if let Some(session) = session {
        launch(
            downloads,
            store_state,
            store,
            library_state,
            task_id,
            session,
        );
    }
    Ok(snapshot)
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

//! A user-started, process-local Chinese replacement admission. No startup,
//! renderer result, or imported status can authorize a download.
use crate::{accounts, downloads, library, require_main, with_store, DesktopStore};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{
    AccountService, DiscoveryPhase, DiscoveryScope, DiscoverySnapshot, QueryKind, Source,
    SourceBackend,
};
use workbench_credentials::Vault;
use workbench_downloads::JmDownloadMetadata;
use workbench_storage::{
    CompletenessCandidateKind, CompletenessLanguage, CompletenessMember, CompletenessSettings,
    CompletenessSnapshot, StoreError,
};

fn error(code: &'static str) -> StoreError {
    StoreError { code }
}
fn account_error(problem: workbench_accounts::AccountError) -> StoreError {
    error(problem.code)
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomaticStatus {
    run_id: Option<String>,
    phase: AutoPhase,
    queued: usize,
    skipped: usize,
    error_code: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum AutoPhase {
    #[default]
    Idle,
    Waiting,
    Enqueueing,
    Complete,
    Cancelled,
    Error,
}

#[derive(Default)]
pub(crate) struct DesktopCompleteness {
    status: Mutex<AutomaticStatus>,
    permit: Mutex<Option<Arc<AtomicBool>>>,
    starting: AtomicBool,
}
impl DesktopCompleteness {
    fn status(&self, run: Option<&str>) -> Result<AutomaticStatus, StoreError> {
        let value = self
            .status
            .lock()
            .map_err(|_| error("COMPLETENESS_UNAVAILABLE"))?;
        Ok(if value.run_id.as_deref() == run {
            value.clone()
        } else {
            AutomaticStatus::default()
        })
    }
    fn update(&self, run: &str, change: impl FnOnce(&mut AutomaticStatus)) {
        if let Ok(mut status) = self.status.lock() {
            if status.run_id.as_deref() == Some(run) {
                change(&mut status);
            }
        }
    }
    fn revoke(&self) -> Result<(), StoreError> {
        if let Some(permit) = self
            .permit
            .lock()
            .map_err(|_| error("COMPLETENESS_UNAVAILABLE"))?
            .take()
        {
            permit.store(false, Ordering::Release);
        }
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompletionView {
    discovery: DiscoverySnapshot,
    completeness: CompletenessSnapshot,
    automatic: AutomaticStatus,
}

async fn project(
    state: Arc<DesktopStore>,
    snapshot: DiscoverySnapshot,
    control: &DesktopCompleteness,
) -> Result<CompletionView, StoreError> {
    let automatic = control.status(snapshot.run.as_ref().map(|run| run.id.as_str()))?;
    let revision = snapshot.revision;
    let records = snapshot.records.clone();
    let completeness = with_store(state, move |store| {
        workbench_storage::completeness_project(store, revision, &records)
    })
    .await?;
    Ok(CompletionView {
        discovery: snapshot,
        completeness,
        automatic,
    })
}

async fn verify_reused_translations(
    state: Arc<DesktopStore>,
    library: Arc<library::DesktopLibrary>,
    snapshot: &DiscoverySnapshot,
) -> Result<(), StoreError> {
    let revision = snapshot.revision;
    let records = snapshot.records.clone();
    let ids = with_store(Arc::clone(&state), move |store| {
        let model = workbench_storage::completeness_project(store, revision, &records)?;
        Ok(model
            .groups
            .into_iter()
            .filter(|group| {
                group.status == workbench_storage::CompletenessStatus::TranslationDownloaded
            })
            .flat_map(|group| group.computer)
            .filter(|copy| copy.language == CompletenessLanguage::Chinese)
            .filter_map(|copy| match copy.member {
                CompletenessMember::Computer { item_id } => Some(item_id),
                _ => None,
            })
            .collect::<Vec<_>>())
    })
    .await?;
    if !ids.is_empty() {
        library::with_library(library, state, move |service, store| {
            service.verify_known_copies(store, &ids)
        })
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn completeness_read<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    completion: State<'_, Arc<DesktopCompleteness>>,
    scopes: Vec<DiscoveryScope>,
    recheck_files: Option<bool>,
) -> Result<CompletionView, StoreError> {
    require_main(window.label())?;
    let service = accounts::service(Arc::clone(accounts.inner()))
        .await
        .map_err(account_error)?;
    let snapshot = service
        .discovery_read(scopes.clone())
        .await
        .map_err(account_error)?;
    if recheck_files.unwrap_or(false) {
        library::with_library(
            Arc::clone(library.inner()),
            Arc::clone(store.inner()),
            |service, store| service.recheck_known_entries(store),
        )
        .await?;
        verify_reused_translations(
            Arc::clone(store.inner()),
            Arc::clone(library.inner()),
            &snapshot,
        )
        .await?;
    }
    let result = project(Arc::clone(store.inner()), snapshot, completion.inner()).await?;
    service
        .discovery_validate_scopes(&scopes)
        .map_err(account_error)?;
    Ok(result)
}

struct StartLock(Arc<DesktopCompleteness>);
impl Drop for StartLock {
    fn drop(&mut self) {
        self.0.starting.store(false, Ordering::Release);
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn completeness_start<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    store: State<'_, Arc<DesktopStore>>,
    library: State<'_, Arc<library::DesktopLibrary>>,
    downloads: State<'_, Arc<downloads::DesktopDownloads>>,
    completion: State<'_, Arc<DesktopCompleteness>>,
    scopes: Vec<DiscoveryScope>,
    authors: Vec<String>,
    automatic: bool,
    root_id: Option<String>,
    generation: u64,
) -> Result<CompletionView, StoreError> {
    require_main(window.label())?;
    if completion.starting.swap(true, Ordering::AcqRel) {
        return Err(error("COMPLETENESS_BUSY"));
    }
    let _starting = StartLock(Arc::clone(completion.inner()));
    let service = accounts::service(Arc::clone(accounts.inner()))
        .await
        .map_err(account_error)?;
    if automatic {
        downloads::live_execution_allowed()?;
    }
    let root_id = if automatic {
        let root_id = root_id.ok_or(error("LIBRARY_NOT_CONFIGURED"))?;
        let id = root_id.clone();
        library::with_library(
            Arc::clone(library.inner()),
            Arc::clone(store.inner()),
            move |service, store| {
                let snapshot = service.recheck_known_entries(store)?;
                if snapshot.root_id.as_deref() != Some(id.as_str())
                    || snapshot.generation != generation
                {
                    return Err(error("LIBRARY_SCOPE_CHANGED"));
                }
                Ok(snapshot)
            },
        )
        .await?;
        root_id
    } else {
        String::new()
    };
    let started = service
        .discovery_start(scopes.clone(), authors)
        .await
        .map_err(account_error)?;
    completion.revoke()?;
    let permit = Arc::new(AtomicBool::new(automatic));
    *completion
        .permit
        .lock()
        .map_err(|_| error("COMPLETENESS_UNAVAILABLE"))? = Some(Arc::clone(&permit));
    *completion
        .status
        .lock()
        .map_err(|_| error("COMPLETENESS_UNAVAILABLE"))? = AutomaticStatus {
        run_id: Some(started.run_id.clone()),
        phase: if automatic {
            AutoPhase::Waiting
        } else {
            AutoPhase::Idle
        },
        ..AutomaticStatus::default()
    };
    if automatic {
        let control = Arc::clone(completion.inner());
        let store_state = Arc::clone(store.inner());
        let library = Arc::clone(library.inner());
        let downloads = Arc::clone(downloads.inner());
        let service = Arc::clone(&service);
        let id = started.run_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = tauri::async_runtime::block_on(automatic_worker(
                service,
                Arc::clone(&control),
                permit,
                store_state,
                library,
                downloads,
                scopes,
                id.clone(),
                root_id,
                generation,
            ));
            if let Err(problem) = result {
                control.update(&id, |status| {
                    if status.phase != AutoPhase::Cancelled {
                        status.phase = AutoPhase::Error;
                        status.error_code = Some(problem.code.into());
                    }
                });
            }
        });
    }
    project(
        Arc::clone(store.inner()),
        started.snapshot,
        completion.inner(),
    )
    .await
}

#[tauri::command]
pub(crate) async fn completeness_cancel<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<accounts::DesktopAccounts>>,
    completion: State<'_, Arc<DesktopCompleteness>>,
    run_id: String,
) -> Result<(), StoreError> {
    require_main(window.label())?;
    if completion
        .status
        .lock()
        .map_err(|_| error("COMPLETENESS_UNAVAILABLE"))?
        .run_id
        .as_deref()
        != Some(run_id.as_str())
    {
        return Err(error("DISCOVERY_RUN_CHANGED"));
    }
    completion.revoke()?;
    completion.update(&run_id, |status| status.phase = AutoPhase::Cancelled);
    let service = accounts::service(Arc::clone(accounts.inner()))
        .await
        .map_err(account_error)?;
    // A finished metadata run may still be adding download tasks.
    let _ = service.discovery_cancel(&run_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn automatic_worker<B: SourceBackend + 'static, V: Vault + 'static>(
    service: Arc<AccountService<B, V>>,
    control: Arc<DesktopCompleteness>,
    permit: Arc<AtomicBool>,
    store_state: Arc<DesktopStore>,
    library: Arc<library::DesktopLibrary>,
    downloads: Arc<downloads::DesktopDownloads>,
    scopes: Vec<DiscoveryScope>,
    run_id: String,
    root_id: String,
    generation: u64,
) -> Result<(), StoreError> {
    let store = store_state.open()?;
    let snapshot = loop {
        require_permit(&permit)?;
        service
            .discovery_run_is_current(&scopes, &run_id)
            .map_err(account_error)?;
        let snapshot = service
            .discovery_read(scopes.clone())
            .await
            .map_err(account_error)?;
        match snapshot.run.as_ref().map(|r| r.phase) {
            Some(DiscoveryPhase::Checking) => {
                std::thread::sleep(std::time::Duration::from_millis(1000))
            }
            Some(DiscoveryPhase::Complete | DiscoveryPhase::Partial) => break snapshot,
            _ => return Err(error("DISCOVERY_RUN_CHANGED")),
        }
    };
    control.update(&run_id, |status| status.phase = AutoPhase::Enqueueing);
    library::with_library(
        Arc::clone(&library),
        Arc::clone(&store_state),
        |service, store| service.recheck_known_entries(store),
    )
    .await?;
    verify_reused_translations(Arc::clone(&store_state), Arc::clone(&library), &snapshot).await?;
    let initial =
        workbench_storage::completeness_project(&store, snapshot.revision, &snapshot.records)?;
    let base_records = Arc::new(snapshot.records.clone());
    for group in initial.groups {
        require_permit(&permit)?;
        service
            .discovery_run_is_current(&scopes, &run_id)
            .map_err(account_error)?;
        let Some(candidate) = group
            .eligible
            .filter(|candidate| candidate.kind == CompletenessCandidateKind::Translation)
        else {
            continue;
        };
        let Some(record) = snapshot.records.iter().find(|record| {
            record.scan_id == run_id
                && record.author_verified
                && record.work.source == candidate.reference.source
                && record.work.work_id == candidate.reference.work_id
        }) else {
            continue;
        };
        let source = match candidate.reference.source {
            workbench_storage::Source::Jm => Source::Jm,
            workbench_storage::Source::Pica => Source::Pica,
        };
        let scope = scopes
            .iter()
            .find(|scope| scope.source == source)
            .ok_or(error("DISCOVERY_SCOPES_REQUIRED"))?;
        let result = async {
            let detail = service
                .query(
                    source,
                    &scope.session_id,
                    QueryKind::Detail,
                    &candidate.reference.work_id,
                    None,
                    1,
                )
                .await
                .map_err(account_error)?;
            let work = detail
                .page
                .items
                .into_iter()
                .next()
                .filter(|work| work.source == source && work.work_id == record.work.work_id)
                .map(workbench_accounts::discovery_work_from_source)
                .filter(|work| {
                    record
                        .matched_authors
                        .iter()
                        .all(|author| work.authors.iter().any(|name| name.trim() == author.trim()))
                })
                .ok_or(error("COMPLETENESS_METADATA_CHANGED"))?;
            // Detail metadata may legitimately contain a longer title or more
            // tags than search. Re-project its actual language/identity evidence.
            let mut current_records = snapshot.records.clone();
            let current = current_records
                .iter_mut()
                .find(|r| {
                    r.work.source == record.work.source && r.work.work_id == record.work.work_id
                })
                .ok_or(error("COMPLETENESS_METADATA_CHANGED"))?;
            current.work = workbench_storage::DiscoveryWork {
                source: record.work.source,
                work_id: work.work_id.clone(),
                title: work.title.clone(),
                authors: work.authors.clone(),
                description: work.description.clone(),
                tags: work.tags.clone(),
                favorite: work.favorite,
                chapter_count: work.chapter_count,
                page_count: work.page_count,
                cover_available: work.cover_available,
            };
            let checked_detail = current.clone();
            let fresh = workbench_storage::completeness_project(
                &store,
                snapshot.revision,
                &current_records,
            )?;
            let eligible = fresh
                .groups
                .iter()
                .find(|g| g.group_id == candidate.group_id)
                .and_then(|g| g.eligible.as_ref())
                .filter(|c| {
                    c.reference == candidate.reference
                        && c.kind == CompletenessCandidateKind::Translation
                })
                .ok_or(error("COMPLETENESS_EVIDENCE_CHANGED"))?;
            let session = service
                .download_session(source, &scope.session_id)
                .await
                .map_err(account_error)?;
            let check_service = Arc::clone(&service);
            let check_scopes = scopes.clone();
            let check_id = run_id.clone();
            let check_permit = Arc::clone(&permit);
            let check_store = Arc::clone(&store);
            let check_root = root_id.clone();
            let phone_revision = fresh.phone_revision;
            let settings_revision = fresh.revision;
            let matches_revision = fresh.matches_revision;
            let live_service = Arc::clone(&service);
            let live_scopes = scopes.clone();
            let live_id = run_id.clone();
            let live_permit = Arc::clone(&permit);
            let check_records = Arc::clone(&base_records);
            let check_revision = snapshot.revision;
            let check_candidate = eligible.clone();
            let check = Arc::new(downloads::AdmissionCheck {
                before_work: Box::new(move || {
                    require_permit(&check_permit)?;
                    check_service
                        .discovery_run_is_current(&check_scopes, &check_id)
                        .map_err(account_error)?;
                    let root = check_store.read_library_shared()?;
                    if root.value.root.as_ref().map(|r| r.id.as_str()) != Some(check_root.as_str())
                        || root.value.generation != generation
                    {
                        return Err(error("LIBRARY_SCOPE_CHANGED"));
                    }
                    if workbench_storage::phone_library_read(&check_store)?.revision
                        != phone_revision
                        || workbench_storage::completeness_settings_read(&check_store)?.revision
                            != settings_revision
                        || workbench_storage::source_matches_read(&check_store)?.revision
                            != matches_revision
                    {
                        return Err(error("COMPLETENESS_EVIDENCE_CHANGED"));
                    }
                    let mut latest_records = check_records.as_ref().clone();
                    if let Some(record) = latest_records.iter_mut().find(|r| {
                        r.work.source == checked_detail.work.source
                            && r.work.work_id == checked_detail.work.work_id
                    }) {
                        *record = checked_detail.clone();
                    }
                    let current = workbench_storage::completeness_project(
                        &check_store,
                        check_revision,
                        &latest_records,
                    )?;
                    if !current.groups.iter().any(|group| {
                        group.group_id == check_candidate.group_id
                            && group.eligible.as_ref().is_some_and(|c| {
                                c.kind == CompletenessCandidateKind::Translation
                                    && c.reference == check_candidate.reference
                            })
                    }) {
                        return Err(error("COMPLETENESS_EVIDENCE_CHANGED"));
                    }
                    Ok(())
                }),
                while_running: Box::new(move || {
                    require_permit(&live_permit)?;
                    live_service
                        .discovery_run_is_live(&live_scopes, &live_id)
                        .map_err(account_error)
                }),
            });
            // Evidence is derived here from the native catalog, never supplied by JS.
            if eligible.evidence_hash != fresh.evidence_hash {
                return Err(error("COMPLETENESS_EVIDENCE_CHANGED"));
            }
            downloads::enqueue_translation(
                Arc::clone(&downloads),
                Arc::clone(&store_state),
                Arc::clone(&library),
                session,
                JmDownloadMetadata {
                    work_id: work.work_id,
                    title: work.title,
                    authors: work.authors,
                    tags: work.tags,
                    description: work.description,
                },
                root_id.clone(),
                generation,
                check,
            )
            .await
        }
        .await;
        control.update(&run_id, |status| {
            if result.is_ok() {
                status.queued += 1;
            } else {
                status.skipped += 1;
                status.error_code = result.as_ref().err().map(|e| e.code.into());
            }
        });
        if let Err(problem) = result {
            if matches!(
                problem.code,
                "SOURCE_RATE_LIMITED"
                    | "RATE_LIMITED"
                    | "SOURCE_ACCESS_DENIED"
                    | "AUTH_REQUIRED"
                    | "SESSION_CHANGED"
                    | "DISCOVERY_FOLLOWING_CHANGED"
                    | "SOURCE_SESSION_EXPIRED"
            ) {
                return Err(problem);
            }
        }
    }
    require_permit(&permit)?;
    control.update(&run_id, |status| status.phase = AutoPhase::Complete);
    Ok(())
}
fn require_permit(permit: &AtomicBool) -> Result<(), StoreError> {
    if permit.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(error("COMPLETENESS_CANCELLED"))
    }
}

#[tauri::command]
pub(crate) async fn completeness_settings_read<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<CompletenessSettings, StoreError> {
    require_main(window.label())?;
    with_store(
        Arc::clone(store.inner()),
        workbench_storage::completeness_settings_read,
    )
    .await
}
#[tauri::command]
pub(crate) async fn completeness_family_confirm<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    members: Vec<CompletenessMember>,
) -> Result<CompletenessSettings, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::completeness_family_confirm(store, revision, members)
    })
    .await
}
#[tauri::command]
pub(crate) async fn completeness_family_unlink<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    family_id: String,
) -> Result<CompletenessSettings, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::completeness_family_unlink(store, revision, &family_id)
    })
    .await
}
#[tauri::command]
pub(crate) async fn completeness_language_set<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    member: CompletenessMember,
    language: Option<CompletenessLanguage>,
) -> Result<CompletenessSettings, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::completeness_language_set(store, revision, member, language)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_run_or_reopen_cannot_reuse_automatic_permission() {
        let control = DesktopCompleteness::default();
        let permit = Arc::new(AtomicBool::new(true));
        *control.permit.lock().unwrap() = Some(Arc::clone(&permit));
        assert!(require_permit(&permit).is_ok());
        control.revoke().unwrap();
        assert_eq!(
            require_permit(&permit).unwrap_err().code,
            "COMPLETENESS_CANCELLED"
        );
        assert_eq!(control.status(None).unwrap().phase, AutoPhase::Idle);
    }
}

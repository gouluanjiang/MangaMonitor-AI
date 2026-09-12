use crate::{
    adapter, error, hash, materialize, now, presence, DownloadPhase, JmDownloadMetadata,
    LocalFiles, Result, Source,
};
use cloud_monitor::{
    isolated_staging_execution::StagingCheckpoint,
    local_execution_orchestrator::{self, LocalExecutionReport},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use workbench_storage::{
    Document, DownloadRecord, DownloadWorkspace, DownloadsDocument, LibraryDocument, LibraryPhase,
    WorkbenchStore, MAX_DOWNLOAD_TASKS, MAX_SAFE_INTEGER,
};

static PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Control {
    Pause,
    Resume,
    Retry,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlan {
    pub plan_id: String,
    pub revision: u64,
    pub source: Source,
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub destination_display: String,
    pub root_id: String,
    pub generation: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub revision: u64,
    pub source: Source,
    pub work_id: String,
    pub title: String,
    pub phase: DownloadPhase,
    pub files_done: u64,
    pub files_total: Option<u64>,
    pub bytes_done: u64,
    pub error_code: Option<String>,
    pub allowed_actions: Vec<Control>,
    pub library_entry_id: Option<String>,
    pub updated_at: u64,
    pub destination_display: String,
    pub local_files: Option<LocalFiles>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSnapshot {
    pub revision: u64,
    pub tasks: Vec<DownloadTask>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AwaitingIndexReceipt {
    pub task_id: String,
    pub task_revision: u64,
    pub source: Source,
    pub root_id: String,
    pub generation: u64,
    pub relative_path: String,
    pub work_id: String,
    pub expected_pages: u64,
    pub manifest_hash: String,
}
struct Prepared {
    record: DownloadRecord,
    document_revision: u64,
}
struct Active {
    task_id: String,
    revision: u64,
    _workspace: DownloadWorkspace,
}
#[derive(Default)]
struct Runtime {
    plans: BTreeMap<String, Prepared>,
    active: Option<Active>,
    local_files: BTreeMap<String, (u64, LocalFiles)>,
}
#[derive(Default)]
pub struct DownloadService {
    runtime: Mutex<Runtime>,
    validated_downloads: Mutex<Option<Arc<Document<DownloadsDocument>>>>,
}
impl DownloadService {
    pub fn new() -> Self {
        Self::default()
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Runtime>> {
        self.runtime
            .lock()
            .map_err(|_| error("DOWNLOAD_WORKER_BUSY"))
    }
    fn load_shared(&self, store: &WorkbenchStore) -> Result<Arc<Document<DownloadsDocument>>> {
        // The store rereads exact bytes before returning this Arc. Identity here
        // only avoids repeating semantic checks on that same immutable value.
        let document = store.read_downloads_shared()?;
        let mut validated = self
            .validated_downloads
            .lock()
            .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?;
        if !validated
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, &document))
        {
            validate_downloads(&document)?;
            *validated = Some(Arc::clone(&document));
        }
        Ok(document)
    }
    fn load(&self, store: &WorkbenchStore) -> Result<Document<DownloadsDocument>> {
        self.load_shared(store).map(Arc::unwrap_or_clone)
    }
    pub fn prepare(
        &self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        metadata: JmDownloadMetadata,
    ) -> Result<DownloadPlan> {
        self.prepare_for_source(store, root_id, generation, Source::Jm, metadata)
    }
    pub fn prepare_for_source(
        &self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        source: Source,
        metadata: JmDownloadMetadata,
    ) -> Result<DownloadPlan> {
        if !metadata.is_valid_for(source) {
            return Err(error("DOWNLOAD_METADATA_INVALID"));
        }
        let mut runtime = self.lock()?;
        let document = self.load_shared(store)?;
        let library = store.read_library_shared()?;
        let root = library
            .value
            .root
            .clone()
            .ok_or(error("LIBRARY_NOT_CONFIGURED"))?;
        if matches!(
            library.value.phase,
            LibraryPhase::Reading | LibraryPhase::Paused
        ) {
            return Err(error("LIBRARY_BUSY"));
        }
        if root.id != root_id || library.value.generation != generation {
            return Err(error("DOWNLOAD_ROOT_CHANGED"));
        }
        if document.value.tasks.len() >= MAX_DOWNLOAD_TASKS {
            return Err(error("DOWNLOAD_LIMIT_REACHED"));
        }
        let destination = destination(source, &metadata);
        let updated_at = now()?;
        let sequence = PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let id = hash(
            serde_json::to_string(&(
                root_id,
                generation,
                &metadata,
                updated_at,
                sequence,
                document.revision,
            ))
            .map_err(|_| error("DOWNLOAD_METADATA_INVALID"))?
            .as_bytes(),
        );
        let mut record = DownloadRecord {
            source,
            jpeg_output: source == Source::Jm,
            id: id.clone(),
            origin: "manual".into(),
            revision: 1,
            approval_revision: 1,
            target_hash: String::new(),
            root,
            generation,
            metadata,
            destination,
            phase: DownloadPhase::Queued,
            files_done: 0,
            files_total: None,
            bytes_done: 0,
            error_code: None,
            library_entry_id: None,
            updated_at,
            checkpoint_json: None,
            staging_report_json: None,
            output_identity: None,
            output_files: Vec::new(),
            output_manifest_hash: None,
        };
        record.target_hash = binding(&record)?;
        check_new_download(&record, &document.value, &library.value)?;
        if runtime.plans.len() >= 8 {
            runtime.plans.clear();
        }
        let plan = DownloadPlan {
            plan_id: id.clone(),
            revision: document.revision,
            source,
            work_id: record.metadata.work_id.clone(),
            title: record.metadata.title.clone(),
            authors: record.metadata.authors.clone(),
            destination_display: display(&record),
            root_id: root_id.into(),
            generation,
        };
        runtime.plans.insert(
            id,
            Prepared {
                record,
                document_revision: document.revision,
            },
        );
        Ok(plan)
    }
    pub fn confirm(
        &self,
        store: &WorkbenchStore,
        plan_id: &str,
        expected_revision: u64,
    ) -> Result<DownloadSnapshot> {
        let mut runtime = self.lock()?;
        if runtime.active.is_some() {
            return Err(error("DOWNLOAD_WORKER_BUSY"));
        }
        let prepared = runtime
            .plans
            .get(plan_id)
            .ok_or(error("DOWNLOAD_PLAN_STALE"))?;
        let mut document = self.load(store)?;
        if document.revision != expected_revision || prepared.document_revision != expected_revision
        {
            return Err(error("DOWNLOAD_PLAN_STALE"));
        }
        require_library(store, &prepared.record)?;
        let library = store.read_library_shared()?;
        let missing = check_new_download(&prepared.record, &document.value, &library.value)?;
        if !missing.is_empty() {
            // Only explicitly confirmed, positively absent private index rows
            // are removed. No selected-root files are changed. A failed later
            // queue write leaves a corrected index and does not start work.
            let mut updated = library.value.clone();
            updated.records.retain(|r| !missing.contains(&r.item.id));
            updated.updated_at = Some(now()?);
            store.write_library(library.revision, updated)?;
        }
        require_library(store, &prepared.record)?;
        document.value.tasks.push(prepared.record.clone());
        let saved = store.write_downloads(document.revision, document.value)?;
        runtime.plans.remove(plan_id);
        Ok(snapshot(&saved, &mut runtime, false, false))
    }
    /// This is a read-only projection. An orphaned running state is displayed
    /// paused; its persisted approval is not executed or silently changed.
    pub fn read(&self, store: &WorkbenchStore) -> Result<DownloadSnapshot> {
        self.read_with_file_check(store, true)
    }
    /// Frequent progress polling may reuse advisory file status. Authorization
    /// in prepare/confirm always performs fresh checks independently of it.
    pub fn read_with_file_check(
        &self,
        store: &WorkbenchStore,
        recheck_files: bool,
    ) -> Result<DownloadSnapshot> {
        let mut runtime = self.lock()?;
        let document = self.load_shared(store)?;
        Ok(snapshot(&document, &mut runtime, true, recheck_files))
    }
    /// Native session selection must use the same control revision as the
    /// subsequent command. This exposes no token, path, or mutable task data.
    pub fn task_source(
        &self,
        store: &WorkbenchStore,
        task_id: &str,
        expected_revision: u64,
    ) -> Result<Source> {
        let document = self.load_shared(store)?;
        let task = document
            .value
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
        if task.revision != expected_revision {
            return Err(error("DOWNLOAD_TASK_STALE"));
        }
        Ok(task.source)
    }
    pub fn control(
        &self,
        store: &WorkbenchStore,
        task_id: &str,
        expected_revision: u64,
        action: Control,
    ) -> Result<DownloadSnapshot> {
        let mut runtime = self.lock()?;
        let mut document = self.load(store)?;
        let task = document
            .value
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
        if task.revision != expected_revision {
            return Err(error("DOWNLOAD_TASK_STALE"));
        }
        let active = runtime
            .active
            .as_ref()
            .is_some_and(|a| a.task_id == task_id);
        match action {
            Control::Pause => {
                if !matches!(
                    task.phase,
                    DownloadPhase::Queued | DownloadPhase::Downloading | DownloadPhase::Verifying
                ) {
                    return Err(error("DOWNLOAD_CONTROL_INVALID"));
                }
                task.phase = DownloadPhase::Paused;
            }
            Control::Resume | Control::Retry => {
                // Never enqueue a second worker while a cancelled in-flight GET
                // is still unwinding. The caller can explicitly resume afterward.
                if runtime.active.is_some() {
                    return Err(error("DOWNLOAD_WORKER_BUSY"));
                }
                let orphan = !active
                    && matches!(
                        task.phase,
                        DownloadPhase::Queued
                            | DownloadPhase::Downloading
                            | DownloadPhase::Verifying
                            | DownloadPhase::Saving
                    );
                if !((action == Control::Resume && (task.phase == DownloadPhase::Paused || orphan))
                    || (action == Control::Retry && task.phase == DownloadPhase::Error))
                {
                    return Err(error("DOWNLOAD_CONTROL_INVALID"));
                }
                require_library(store, task)?;
                task.phase = DownloadPhase::Queued;
                task.error_code = None;
            }
        }
        task.revision = next(task.revision)?;
        task.updated_at = now()?;
        let saved = store.write_downloads(document.revision, document.value)?;
        Ok(snapshot(&saved, &mut runtime, false, false))
    }
    fn require_run(
        &self,
        store: &WorkbenchStore,
        id: &str,
        revision: u64,
    ) -> Result<DownloadRecord> {
        let runtime = self.lock()?;
        if !runtime
            .active
            .as_ref()
            .is_some_and(|a| a.task_id == id && a.revision == revision)
        {
            return Err(error("DOWNLOAD_PAUSED"));
        }
        let document = self.load_shared(store)?;
        let task = document
            .value
            .tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
        if task.revision != revision
            || matches!(
                task.phase,
                DownloadPhase::Paused | DownloadPhase::Error | DownloadPhase::Downloaded
            )
        {
            return Err(error("DOWNLOAD_PAUSED"));
        }
        require_library(store, task)?;
        Ok(task.clone())
    }
    fn update_run(
        &self,
        store: &WorkbenchStore,
        id: &str,
        revision: u64,
        mutate: impl FnOnce(&mut DownloadRecord) -> Result<()>,
    ) -> Result<DownloadRecord> {
        let _runtime = self.lock()?;
        let mut document = self.load(store)?;
        let task = document
            .value
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
        if task.revision != revision || task.phase == DownloadPhase::Paused {
            return Err(error("DOWNLOAD_PAUSED"));
        }
        mutate(task)?;
        task.updated_at = now()?;
        let result = task.clone();
        store.write_downloads(document.revision, document.value)?;
        Ok(result)
    }
    fn checkpoint(
        &self,
        store: &WorkbenchStore,
        initial: &DownloadRecord,
        checkpoint: &StagingCheckpoint,
    ) -> Result<()> {
        if checkpoint.expected_files == 0
            || checkpoint.expected_files > workbench_storage::MAX_DOWNLOAD_FILES as u64
        {
            return Err(error("DOWNLOAD_LIMIT_REACHED"));
        }
        let runtime = self.lock()?;
        if !runtime
            .active
            .as_ref()
            .is_some_and(|a| a.task_id == initial.id && a.revision == initial.revision)
        {
            return Err(error("DOWNLOAD_PAUSED"));
        }
        let mut document = self.load(store)?;
        let task = document
            .value
            .tasks
            .iter_mut()
            .find(|t| t.id == initial.id)
            .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
        if task.approval_revision != initial.approval_revision
            || task.target_hash != initial.target_hash
            || (task.revision != initial.revision
                && !(task.phase == DownloadPhase::Paused && checkpoint.pending.is_none()))
        {
            return Err(error("DOWNLOAD_PAUSED"));
        }
        // Completing a previously committed write intent may record its proof
        // after pause, while preserving the newer control revision and phase.
        if task.revision != initial.revision {
            let previous: StagingCheckpoint = serde_json::from_str(
                task.checkpoint_json
                    .as_deref()
                    .ok_or(error("DOWNLOAD_DOCUMENT_INVALID"))?,
            )
            .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?;
            let pending = previous.pending.as_ref().ok_or(error("DOWNLOAD_PAUSED"))?;
            if checkpoint.descriptor_hash != previous.descriptor_hash
                || checkpoint.expected_files != previous.expected_files
                || checkpoint.artifacts.len() != previous.artifacts.len() + 1
                || !checkpoint.artifacts.starts_with(&previous.artifacts)
                || checkpoint.artifacts.last() != Some(pending)
            {
                return Err(error("DOWNLOAD_PAUSED"));
            }
        }
        task.checkpoint_json = Some(
            serde_json::to_string(checkpoint).map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?,
        );
        task.files_done = checkpoint.artifacts.len() as u64;
        task.files_total = Some(checkpoint.expected_files);
        task.bytes_done = checkpoint
            .artifacts
            .iter()
            .try_fold(0_u64, |sum, f| sum.checked_add(f.size_bytes))
            .ok_or(error("DOWNLOAD_LIMIT_REACHED"))?;
        task.updated_at = now()?;
        store.write_downloads(document.revision, document.value)?;
        Ok(())
    }
    /// Called only after an explicit confirm/resume/retry. The caller should run
    /// this whole worker on its dedicated blocking thread plus async runtime.
    pub async fn run(
        &self,
        store: &WorkbenchStore,
        task_id: &str,
        current_scope: impl Fn() -> Result<()> + Send + Sync,
    ) -> Result<Option<AwaitingIndexReceipt>> {
        self.run_with_token(store, task_id, None, current_scope)
            .await
    }
    pub async fn run_with_token(
        &self,
        store: &WorkbenchStore,
        task_id: &str,
        pica_token: Option<&str>,
        current_scope: impl Fn() -> Result<()> + Send + Sync,
    ) -> Result<Option<AwaitingIndexReceipt>> {
        current_scope()?;
        let (initial, path) = {
            let mut runtime = self.lock()?;
            if runtime.active.is_some() {
                return Err(error("DOWNLOAD_WORKER_BUSY"));
            }
            let record = self
                .load_shared(store)?
                .value
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .cloned()
                .ok_or(error("DOWNLOAD_TASK_NOT_FOUND"))?;
            if record.phase != DownloadPhase::Queued {
                return Err(error("DOWNLOAD_CONTROL_INVALID"));
            }
            match (record.source, pica_token) {
                (Source::Jm, Some(_)) => return Err(error("DOWNLOAD_SOURCE_MISMATCH")),
                (Source::Pica, None) => return Err(error("DOWNLOAD_SOURCE_TOKEN_REQUIRED")),
                (Source::Pica, Some(token))
                    if token.is_empty()
                        || token.len() > 8192
                        || token.trim() != token
                        || token.chars().any(char::is_control) =>
                {
                    return Err(error("DOWNLOAD_SOURCE_TOKEN_INVALID"));
                }
                _ => {}
            }
            require_library(store, &record)?;
            let workspace = store.open_download_workspace()?;
            let path = workspace.path().to_path_buf();
            runtime.active = Some(Active {
                task_id: task_id.into(),
                revision: record.revision,
                _workspace: workspace,
            });
            (record, path)
        };
        let mut guard = RunGuard {
            service: self,
            task_id: task_id.into(),
            revision: initial.revision,
            retained: false,
        };
        let revision = initial.revision;
        let require_record = || {
            current_scope()?;
            let current = self.require_run(store, task_id, revision)?;
            if current.target_hash != initial.target_hash
                || current.source != initial.source
                || current.approval_revision != initial.approval_revision
                || current.jpeg_output != initial.jpeg_output
            {
                return Err(error("DOWNLOAD_TASK_STALE"));
            }
            Ok(current)
        };
        let require = || require_record().map(|_| ());
        let result = async {
            let mut record = self.update_run(store, task_id, revision, |t| {
                t.phase = DownloadPhase::Downloading;
                Ok(())
            })?;
            let report = if let Some(value) = &record.staging_report_json {
                serde_json::from_str::<LocalExecutionReport>(value)
                    .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?
            } else {
                let resume = record
                    .checkpoint_json
                    .as_deref()
                    .map(serde_json::from_str::<StagingCheckpoint>)
                    .transpose()
                    .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?;
                let (state, ledger, command) = adapter::current(&record)?;
                let report = local_execution_orchestrator::execute_live_resumable_with_output(
                    &state,
                    &ledger,
                    &command,
                    &path,
                    pica_token,
                    None,
                    || {
                        let current = require_record().map_err(|e| e.code.to_string())?;
                        let (s, g, _) =
                            adapter::current(&current).map_err(|e| e.code.to_string())?;
                        Ok((s, g))
                    },
                    resume.as_ref(),
                    record.jpeg_output,
                    |checkpoint| {
                        if checkpoint.pending.is_some() || checkpoint.artifacts.is_empty() {
                            require().map_err(|e| e.code.to_string())?;
                        }
                        self.checkpoint(store, &initial, checkpoint)
                            .map_err(|e| e.code.to_string())
                    },
                )
                .await
                .map_err(|code| classify(&code))?;
                self.update_run(store, task_id, revision, |t| {
                    t.staging_report_json = Some(
                        serde_json::to_string(&report)
                            .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?,
                    );
                    t.phase = DownloadPhase::Verifying;
                    Ok(())
                })?;
                report
            };
            require()?;
            record = self.update_run(store, task_id, revision, |t| {
                t.phase = DownloadPhase::Saving;
                Ok(())
            })?;
            materialize::save(&mut record, &report, &path, &require, &mut |updated| {
                self.update_run(store, task_id, revision, |task| {
                    task.output_identity.clone_from(&updated.output_identity);
                    task.output_files.clone_from(&updated.output_files);
                    task.output_manifest_hash
                        .clone_from(&updated.output_manifest_hash);
                    Ok(())
                })
                .map(|_| ())
            })?;
            require()?;
            receipt(&record)
        }
        .await;
        match result {
            Ok(receipt) => {
                guard.retained = true;
                Ok(Some(receipt))
            }
            Err(failure) => {
                // A pause or replaced control epoch already owns the new state.
                // An old worker must never overwrite it with its trailing error.
                let _ = self.update_run(store, task_id, revision, |t| {
                    t.phase = DownloadPhase::Error;
                    t.error_code = Some(failure.code.into());
                    Ok(())
                });
                Err(failure)
            }
        }
    }
    pub fn validate_receipt(
        &self,
        store: &WorkbenchStore,
        expected: &AwaitingIndexReceipt,
    ) -> Result<()> {
        let task = self.require_run(store, &expected.task_id, expected.task_revision)?;
        if task.phase != DownloadPhase::Saving || receipt(&task)? != *expected {
            return Err(error("DOWNLOAD_TASK_STALE"));
        }
        materialize::verify_output(&task)
    }
    pub fn mark_indexed(
        &self,
        store: &WorkbenchStore,
        expected: &AwaitingIndexReceipt,
        library_entry_id: &str,
    ) -> Result<DownloadSnapshot> {
        let _guard = RunGuard {
            service: self,
            task_id: expected.task_id.clone(),
            revision: expected.task_revision,
            retained: false,
        };
        self.validate_receipt(store, expected)?;
        let library = store.read_library_shared()?;
        let found = library.value.records.iter().any(|r| {
            r.item.id == library_entry_id
                && r.item.relative_path == expected.relative_path
                && r.item
                    .source_ref
                    .as_ref()
                    .is_some_and(|r| r.source == expected.source && r.work_id == expected.work_id)
                && r.item.page_count == Some(expected.expected_pages)
        });
        if !found {
            return Err(error("DOWNLOAD_INDEX_REQUIRED"));
        }
        let completed = self.update_run(store, &expected.task_id, expected.task_revision, |t| {
            t.phase = DownloadPhase::Downloaded;
            t.library_entry_id = Some(library_entry_id.into());
            t.error_code = None;
            t.revision = next(t.revision)?;
            Ok(())
        })?;
        let staging = self
            .lock()?
            .active
            .as_ref()
            .filter(|a| a.task_id == expected.task_id && a.revision == expected.task_revision)
            .map(|a| a._workspace.path().to_path_buf());
        if let Some(staging) = staging {
            let _ = materialize::cleanup_completed(&completed, &staging);
        }
        self.clear(&expected.task_id, expected.task_revision);
        self.read_with_file_check(store, false)
    }
    pub fn index_failed(
        &self,
        store: &WorkbenchStore,
        expected: &AwaitingIndexReceipt,
        _code: &'static str,
    ) -> Result<DownloadSnapshot> {
        let _guard = RunGuard {
            service: self,
            task_id: expected.task_id.clone(),
            revision: expected.task_revision,
            retained: false,
        };
        // A changed library generation can itself be the registration failure;
        // record it without requiring that now-invalid root to become valid.
        self.update_run(store, &expected.task_id, expected.task_revision, |t| {
            if receipt(t)? != *expected {
                return Err(error("DOWNLOAD_TASK_STALE"));
            }
            t.phase = DownloadPhase::Error;
            t.error_code = Some("DOWNLOAD_INDEX_FAILED".into());
            Ok(())
        })?;
        self.clear(&expected.task_id, expected.task_revision);
        self.read_with_file_check(store, false)
    }
    fn clear(&self, id: &str, revision: u64) {
        if let Ok(mut runtime) = self.runtime.lock() {
            if runtime
                .active
                .as_ref()
                .is_some_and(|a| a.task_id == id && a.revision == revision)
            {
                runtime.active = None;
            }
        }
    }
}
struct RunGuard<'a> {
    service: &'a DownloadService,
    task_id: String,
    revision: u64,
    retained: bool,
}
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        if !self.retained {
            self.service.clear(&self.task_id, self.revision);
        }
    }
}
fn next(value: u64) -> Result<u64> {
    value
        .checked_add(1)
        .filter(|v| *v <= MAX_SAFE_INTEGER)
        .ok_or(error("REVISION_EXHAUSTED"))
}
fn binding(record: &DownloadRecord) -> Result<String> {
    serde_json::to_vec(&(
        match (record.source, record.jpeg_output) {
            (Source::Jm, true) => "manual-JM-layout-v1-jpeg",
            (Source::Jm, false) => "manual-JM-layout-v1",
            (Source::Pica, false) => "manual-Pica-layout-v1",
            (Source::Pica, true) => return Err(error("DOWNLOAD_DOCUMENT_INVALID")),
        },
        &record.root,
        record.generation,
        &record.metadata,
        &record.destination,
        record.approval_revision,
    ))
    .map(|bytes| hash(&bytes))
    .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))
}
fn validate_downloads(document: &Document<DownloadsDocument>) -> Result<()> {
    for record in &document.value.tasks {
        if record.target_hash != binding(record)? {
            return Err(error("DOWNLOAD_DOCUMENT_INVALID"));
        }
        if let Some(v) = &record.checkpoint_json {
            serde_json::from_str::<StagingCheckpoint>(v)
                .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?;
        }
        if let Some(v) = &record.staging_report_json {
            serde_json::from_str::<LocalExecutionReport>(v)
                .map_err(|_| error("DOWNLOAD_DOCUMENT_INVALID"))?;
        }
    }
    Ok(())
}
fn require_library(store: &WorkbenchStore, record: &DownloadRecord) -> Result<()> {
    let library = store.read_library_shared()?;
    if matches!(
        library.value.phase,
        LibraryPhase::Reading | LibraryPhase::Paused
    ) {
        return Err(error("LIBRARY_BUSY"));
    }
    if library.value.root.as_ref() != Some(&record.root)
        || library.value.generation != record.generation
    {
        return Err(error("DOWNLOAD_ROOT_CHANGED"));
    }
    materialize::require_root(record)?;
    Ok(())
}
fn receipt(record: &DownloadRecord) -> Result<AwaitingIndexReceipt> {
    Ok(AwaitingIndexReceipt {
        task_id: record.id.clone(),
        task_revision: record.revision,
        source: record.source,
        root_id: record.root.id.clone(),
        generation: record.generation,
        relative_path: record.destination.clone(),
        work_id: record.metadata.work_id.clone(),
        expected_pages: record.files_total.ok_or(error("DOWNLOAD_PROOF_INVALID"))?,
        manifest_hash: record
            .output_manifest_hash
            .clone()
            .ok_or(error("DOWNLOAD_PROOF_INVALID"))?,
    })
}
fn display(record: &DownloadRecord) -> String {
    std::path::Path::new(&record.root.path)
        .join(&record.destination)
        .to_string_lossy()
        .into_owned()
}
fn destination(source: Source, metadata: &JmDownloadMetadata) -> String {
    let mut name = format!("[{}{}] ", crate::source_label(source), metadata.work_id);
    for character in metadata.title.chars() {
        let character = if character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
            '_'
        } else {
            character
        };
        if name.encode_utf16().count() + character.len_utf16() > 160 {
            break;
        }
        name.push(character);
    }
    name.trim_end_matches([' ', '.']).to_owned()
}
/// Returns only matching private index rows whose recorded paths are positively
/// absent. This never grants adoption of an existing or partially missing tree.
fn check_new_download(
    record: &DownloadRecord,
    downloads: &DownloadsDocument,
    library: &LibraryDocument,
) -> Result<BTreeSet<String>> {
    if library.root.as_ref() != Some(&record.root) || library.generation != record.generation {
        return Err(error("DOWNLOAD_ROOT_CHANGED"));
    }
    if matches!(library.phase, LibraryPhase::Reading | LibraryPhase::Paused) {
        return Err(error("LIBRARY_BUSY"));
    }
    let root = materialize::require_root(record)?;
    for old in &downloads.tasks {
        if old.source != record.source
            || old.metadata.work_id != record.metadata.work_id
            || old.root.id != record.root.id
        {
            continue;
        }
        if old.phase != DownloadPhase::Downloaded {
            return Err(error("DOWNLOAD_ALREADY_PRESENT"));
        }
        // Picking a recreated root authorizes its new identity. A completion
        // bound to the old identity remains unavailable history, not a duplicate
        // in this new root. Merely re-reading the old root cannot reach here.
        if old.root.file_key != record.root.file_key {
            continue;
        }
        match presence::check(old) {
            LocalFiles::Missing => {}
            LocalFiles::Present => return Err(error("DOWNLOAD_ALREADY_PRESENT")),
            LocalFiles::Incomplete => return Err(error("DOWNLOAD_LOCAL_FILES_INCOMPLETE")),
            LocalFiles::Unavailable => return Err(error("DOWNLOAD_LOCAL_FILES_UNAVAILABLE")),
        }
    }
    let mut missing = BTreeSet::new();
    for old in &library.records {
        let matches_reference = old.item.source_ref.as_ref().is_some_and(|reference| {
            reference.source == record.source && reference.work_id == record.metadata.work_id
        });
        let previous_association = downloads.tasks.iter().any(|task| {
            task.root == record.root
                && task.source == record.source
                && task.metadata.work_id == record.metadata.work_id
                && task.library_entry_id.as_ref() == Some(&old.item.id)
        });
        if !matches_reference {
            if old.item.relative_path == record.destination
                || (old.manual_override && previous_association)
            {
                return Err(error("LIBRARY_IDENTITY_CONFLICT"));
            }
            continue;
        }
        if presence::probe_path(&root, &old.item.relative_path)?.is_some() {
            return Err(error("DOWNLOAD_ALREADY_PRESENT"));
        }
        missing.insert(old.item.id.clone());
    }
    if root
        .names()?
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&record.destination))
    {
        return Err(error("DOWNLOAD_DESTINATION_EXISTS"));
    }
    materialize::require_root(record)?;
    Ok(missing)
}

fn snapshot(
    document: &Document<DownloadsDocument>,
    runtime: &mut Runtime,
    restore: bool,
    recheck_files: bool,
) -> DownloadSnapshot {
    runtime.local_files.retain(|id, (revision, _)| {
        document.value.tasks.iter().any(|task| {
            task.id == *id && task.revision == *revision && task.phase == DownloadPhase::Downloaded
        })
    });
    for task in &document.value.tasks {
        if task.phase == DownloadPhase::Downloaded
            && (recheck_files || !runtime.local_files.contains_key(&task.id))
        {
            runtime
                .local_files
                .insert(task.id.clone(), (task.revision, presence::check(task)));
        }
    }
    DownloadSnapshot {
        revision: document.revision,
        tasks: document
            .value
            .tasks
            .iter()
            .map(|t| {
                let active = runtime.active.as_ref().is_some_and(|a| a.task_id == t.id);
                let phase = if restore
                    && !active
                    && matches!(
                        t.phase,
                        DownloadPhase::Queued
                            | DownloadPhase::Downloading
                            | DownloadPhase::Verifying
                            | DownloadPhase::Saving
                    ) {
                    DownloadPhase::Paused
                } else {
                    t.phase
                };
                let allowed_actions = match phase {
                    DownloadPhase::Queued
                    | DownloadPhase::Downloading
                    | DownloadPhase::Verifying => vec![Control::Pause],
                    DownloadPhase::Paused if !active => vec![Control::Resume],
                    DownloadPhase::Error if !active => vec![Control::Retry],
                    _ => Vec::new(),
                };
                DownloadTask {
                    id: t.id.clone(),
                    revision: t.revision,
                    source: t.source,
                    work_id: t.metadata.work_id.clone(),
                    title: t.metadata.title.clone(),
                    phase,
                    files_done: t.files_done,
                    files_total: t.files_total,
                    bytes_done: t.bytes_done,
                    error_code: t.error_code.clone(),
                    allowed_actions,
                    library_entry_id: t.library_entry_id.clone(),
                    updated_at: t.updated_at,
                    destination_display: display(t),
                    local_files: runtime.local_files.get(&t.id).map(|(_, state)| *state),
                }
            })
            .collect(),
    }
}
fn classify(code: &str) -> crate::StoreError {
    match code {
        "HTTP_401" | "API_CODE_401" | "SESSION_EXPIRED" => error("SESSION_EXPIRED"),
        "SESSION_CHANGED" => error("SESSION_CHANGED"),
        "PICA_MEDIA_FETCH_TIMEOUT" => error("DOWNLOAD_MEDIA_TIMEOUT"),
        "PICA_MEDIA_FETCH_CONNECT_ERROR" | "PICA_MEDIA_FETCH_TRANSPORT_ERROR" => {
            error("DOWNLOAD_MEDIA_NETWORK_ERROR")
        }
        "INVALID_PICA_MEDIA_URL"
        | "PICA_MEDIA_URL_NORMALIZATION_DRIFT"
        | "PICA_MEDIA_REQUEST_URL_MISMATCH"
        | "PICA_MEDIA_RESPONSE_URL_MISMATCH" => error("DOWNLOAD_MEDIA_ADDRESS_UNSUPPORTED"),
        "PICA_MEDIA_REDIRECT_REJECTED"
        | "PICA_MEDIA_REDIRECT_LOCATION_INVALID"
        | "PICA_MEDIA_REDIRECT_LOOP"
        | "PICA_MEDIA_REDIRECT_LIMIT" => error("DOWNLOAD_MEDIA_REDIRECT_FAILED"),
        "PICA_MEDIA_HTTP_401" | "PICA_MEDIA_HTTP_403" => error("DOWNLOAD_MEDIA_ACCESS_DENIED"),
        "PICA_MEDIA_HTTP_404" => error("DOWNLOAD_MEDIA_UNAVAILABLE"),
        "PICA_MEDIA_HTTP_429" => error("DOWNLOAD_MEDIA_RATE_LIMITED"),
        "PICA_MEDIA_HTTP_500"
        | "PICA_MEDIA_HTTP_502"
        | "PICA_MEDIA_HTTP_503"
        | "PICA_MEDIA_HTTP_504" => error("DOWNLOAD_MEDIA_SERVER_ERROR"),
        "PICA_MEDIA_RESPONSE_EMPTY" => error("DOWNLOAD_MEDIA_EMPTY"),
        "PICA_MEDIA_RESPONSE_TOO_LARGE" => error("DOWNLOAD_MEDIA_TOO_LARGE"),
        "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED"
        | "LIVE_MEDIA_PROCESSED_IMAGE_DECODE_FAILED"
        | "LIVE_MEDIA_SOURCE_FORMAT_MISMATCH" => error("DOWNLOAD_MEDIA_IMAGE_INVALID"),
        "MEDIA_TRANSFER_AUTHORIZATION_GENERATION_CHANGED" => error("DOWNLOAD_SOURCE_CHANGED"),
        "PICA_PAGINATION_CHANGED" => error("DOWNLOAD_SOURCE_CHANGED"),
        "PICA_PAGINATION_INVALID"
        | "PICA_PAGINATION_INCOMPLETE"
        | "INCOMPLETE_CHAPTER_PAGINATION"
        | "PICA_PREFLIGHT_CHAPTER_PAGINATION_INCOMPLETE"
        | "PICA_PREFLIGHT_IMAGE_PAGINATION_INCOMPLETE"
        | "SOURCE_PREFLIGHT_ENUMERATION_INCOMPLETE" => error("DOWNLOAD_SOURCE_INCOMPLETE"),
        "DOWNLOAD_PAUSED" => error("DOWNLOAD_PAUSED"),
        "DOWNLOAD_ROOT_CHANGED" => error("DOWNLOAD_ROOT_CHANGED"),
        "DOWNLOAD_LIMIT_REACHED" => error("DOWNLOAD_LIMIT_REACHED"),
        "LIBRARY_BUSY" => error("LIBRARY_BUSY"),
        "LOCAL_EXECUTOR_GITHUB_ACTIONS_FORBIDDEN" => {
            error("LOCAL_EXECUTOR_GITHUB_ACTIONS_FORBIDDEN")
        }
        "STAGING_CHECKPOINT_FILE_CHANGED" | "STAGING_CHECKPOINT_UNRECORDED_FILE" => {
            error("DOWNLOAD_STAGING_CHANGED")
        }
        "STAGING_CHECKPOINT_GENERATION_CHANGED" => error("DOWNLOAD_SOURCE_CHANGED"),
        "STAGING_ARTIFACT_ALREADY_EXISTS" | "STAGING_COMMAND_DIRECTORY_ALREADY_EXISTS" => {
            error("DOWNLOAD_STAGING_CONFLICT")
        }
        _ => error("DOWNLOAD_FAILED"),
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

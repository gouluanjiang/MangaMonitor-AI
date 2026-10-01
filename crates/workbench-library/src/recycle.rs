//! One native confirmation authorizes one exact indexed archive. The host must
//! recycle only; failures never authorize permanent deletion. Tests use only
//! reversible moves between isolated synthetic directories.
use crate::{
    error,
    paths::{self, Root, SafeFile},
    service::{now, require_scope, snapshot},
    LibraryFormat, LibraryItemState, LibraryPhase, LibraryService, LibrarySnapshot, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use workbench_storage::{
    library_hash_is_valid, Document, DownloadPhase, LibraryDocument, LibraryFileIdentity,
    WorkbenchStore, MAX_SAFE_INTEGER,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryRecycleRequest {
    pub root_id: String,
    pub generation: u64,
    pub entry_id: String,
    pub expected_revision: u64,
}

/// Created from verified stored data, never renderer-provided title or paths.
pub struct LibraryRecyclePreview {
    pub title: String,
    pub relative_path: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRecycleResult {
    pub snapshot: LibrarySnapshot,
    pub recycled: bool,
    pub error_code: Option<String>,
}

/// A live file/parent/root lease for the native recycle adapter. Its private
/// constructor prevents an IPC caller from supplying a filesystem target.
pub struct VerifiedRecycleTarget {
    root: Root,
    file: SafeFile,
    relative_path: String,
    path: PathBuf,
    identity: LibraryFileIdentity,
}

impl VerifiedRecycleTarget {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Recheck before dispatch and from the Shell pre-delete callback. The
    /// Windows lease pins all ancestors and refuses content writers; readers
    /// opened by this app deny delete and therefore prevent this lease.
    ///
    /// Shell still needs delete sharing and operates by pathname. This is not
    /// isolation against a hostile same-user process swapping a leaf between
    /// the callback and the OS operation. The desktop admission guard excludes
    /// this application's own download/confirmation races.
    pub fn verify(&self) -> Result<()> {
        self.root.verify()?;
        if paths::identity(&self.file.file)? != self.identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let current = self.root.recycle_file(&self.relative_path)?;
        if paths::identity(&current.file)? != self.identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        Ok(())
    }

    /// Read-only verification of the destination reported by the OS. It is
    /// never accepted as another mutation target or returned to the renderer.
    pub fn matches_recycled_file(&self, path: &Path) -> Result<bool> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            let file = std::fs::OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)
                .map_err(|_| error("LIBRARY_RECYCLE_RESULT_UNCERTAIN"))?;
            let metadata = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
            Ok(metadata.is_file()
                && metadata.file_attributes() & 0x400 == 0
                && paths::identity(&file)? == self.identity)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(error("LIBRARY_RECYCLE_UNSUPPORTED"))
        }
    }

    fn is_missing(&self) -> Result<bool> {
        self.root.verify()?;
        match self.root.recycle_file(&self.relative_path) {
            Err(problem) if problem.code == "LIBRARY_ENTRY_MISSING" => Ok(true),
            Err(problem) => Err(problem),
            Ok(_) => Ok(false),
        }
    }
}

impl LibraryService {
    /// The host holds its library mutex and download exclusion guard throughout
    /// confirmation, Shell execution, and registration. Confirmation does not
    /// bypass the subsequent scope, revision, identity, or download checks.
    pub fn recycle_confirmed(
        &mut self,
        store: &WorkbenchStore,
        request: &LibraryRecycleRequest,
        confirm: impl FnOnce(&LibraryRecyclePreview) -> bool,
        recycle: impl FnOnce(Arc<VerifiedRecycleTarget>) -> Result<()>,
    ) -> Result<Option<LibraryRecycleResult>> {
        if !library_hash_is_valid(&request.entry_id)
            || request.expected_revision == 0
            || request.expected_revision > MAX_SAFE_INTEGER
        {
            return Err(error("VALIDATION_FAILED"));
        }
        let before = store.read_library()?;
        require_scope(&before.value, &request.root_id, request.generation)?;
        if before.revision != request.expected_revision {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        if matches!(before.value.phase, LibraryPhase::Reading | LibraryPhase::Paused) {
            return Err(error("LIBRARY_BUSY"));
        }
        let record = before.value.records.iter()
            .find(|record| record.item.id == request.entry_id)
            .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))?;
        if !matches!(record.item.format, LibraryFormat::Zip | LibraryFormat::Cbz)
            || !Path::new(&record.item.relative_path).extension().and_then(|s| s.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip")
                    || extension.eq_ignore_ascii_case("cbz"))
        {
            return Err(error("LIBRARY_RECYCLE_FORMAT_UNSUPPORTED"));
        }
        if record.item.error_code.as_deref() == Some("LIBRARY_RECYCLED") {
            return Err(error("LIBRARY_ENTRY_MISSING"));
        }
        let root = Root::restore(before.value.root.as_ref().ok_or(error("LIBRARY_NOT_CONFIGURED"))?)?;
        let file = root.recycle_file(&record.item.relative_path)?;
        let identity = paths::identity(&file.file)?;
        if record.identity.as_ref() != Some(&identity) {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let target = Arc::new(VerifiedRecycleTarget {
            path: Path::new(&root.saved.path).join(&record.item.relative_path),
            root,
            file,
            relative_path: record.item.relative_path.clone(),
            identity,
        });
        require_no_active_download(store, &request.root_id)?;
        if !confirm(&LibraryRecyclePreview {
            title: record.item.title.clone(),
            relative_path: record.item.relative_path.clone(),
            bytes: record.item.bytes,
        }) {
            return Ok(None);
        }
        target.verify()?;
        let current = store.read_library()?;
        require_scope(&current.value, &request.root_id, request.generation)?;
        if current.revision != before.revision {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        require_no_active_download(store, &request.root_id)?;
        let operation = recycle(Arc::clone(&target));
        // OS success alone does not establish absence. If a failure leaves the
        // original path present, do not change any stored registration.
        match target.is_missing() {
            Ok(false) => return Err(operation.err().unwrap_or(error("LIBRARY_RECYCLE_NOT_COMPLETED"))),
            Err(_) => {
                if let Err(problem) = operation {
                    return Err(problem);
                }
                return Ok(Some(recycle_projection(before, request, true, "LIBRARY_RECYCLE_REFRESH_REQUIRED")));
            }
            Ok(true) => {}
        }
        let recycled = operation.is_ok();
        let mut latest = match store.read_library() {
            Ok(latest) => latest,
            Err(_) => return Ok(Some(recycle_projection(before, request, recycled, "LIBRARY_RECYCLE_SAVE_FAILED"))),
        };
        if require_scope(&latest.value, &request.root_id, request.generation).is_err() {
            return Ok(Some(recycle_projection(before, request, recycled, "LIBRARY_RECYCLE_REFRESH_REQUIRED")));
        }
        if latest.revision != before.revision {
            return Ok(Some(recycle_projection(latest, request, recycled, "LIBRARY_RECYCLE_REFRESH_REQUIRED")));
        }
        mark_absent(&mut latest.value, request, recycled);
        latest.value.updated_at = Some(now());
        let fallback = latest.clone();
        self.job = None;
        match store.write_library(latest.revision, latest.value) {
            Ok(saved) => Ok(Some(LibraryRecycleResult {
                snapshot: snapshot(saved, false),
                recycled,
                error_code: (!recycled).then(|| "LIBRARY_RECYCLE_RESULT_UNCERTAIN".into()),
            })),
            Err(_) => Ok(Some(recycle_projection(fallback, request, recycled, "LIBRARY_RECYCLE_SAVE_FAILED"))),
        }
    }
}

fn mark_absent(document: &mut LibraryDocument, request: &LibraryRecycleRequest, recycled: bool) {
    if require_scope(document, &request.root_id, request.generation).is_err() {
        return;
    }
    if let Some(record) = document.records.iter_mut().find(|r| r.item.id == request.entry_id) {
        // Keep identity, metadata, manual associations and admission time for
        // restore + scan. A missing path is not proof of successful recycling.
        record.item.state = LibraryItemState::Unreadable;
        record.item.error_code = Some(if recycled { "LIBRARY_RECYCLED" } else { "LIBRARY_RECYCLE_RESULT_UNCERTAIN" }.into());
        record.item.cover_available = false;
        record.cover = None;
    }
}

fn recycle_projection(
    mut document: Document<LibraryDocument>,
    request: &LibraryRecycleRequest,
    recycled: bool,
    code: &str,
) -> LibraryRecycleResult {
    mark_absent(&mut document.value, request, recycled);
    LibraryRecycleResult {
        snapshot: snapshot(document, false),
        recycled,
        error_code: Some(code.into()),
    }
}

pub(crate) fn retained_metadata(record: &workbench_storage::LibraryRecord) -> bool {
    record.item.state == LibraryItemState::Unreadable && record.identity.is_some()
        && matches!(record.item.error_code.as_deref(), Some("LIBRARY_RECYCLED" | "LIBRARY_RECYCLE_RESULT_UNCERTAIN"))
}

fn require_no_active_download(store: &WorkbenchStore, root_id: &str) -> Result<()> {
    if store.read_downloads_shared()?.value.tasks.iter().any(|task| {
        task.root.id == root_id && matches!(task.phase,
            DownloadPhase::Queued | DownloadPhase::Downloading | DownloadPhase::Verifying | DownloadPhase::Saving)
    }) {
        Err(error("LIBRARY_ITEM_BUSY"))
    } else {
        Ok(())
    }
}

//! Fixed private desktop download ledger. Never stores credentials or source URLs.
use crate::{
    model::ValidatedDocument,
    store::{
        check_directory_tree, check_open_regular, check_optional_regular, ensure_directory_tree,
        safe_options,
    },
    Document, LibraryRoot, Result, Source, StoreError, WorkbenchStore, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::File,
    path::{Path, PathBuf},
};

pub const MAX_DOWNLOAD_TASKS: usize = 50;
// The existing library scanner counts the one root cover toward its 10k limit.
pub const MAX_DOWNLOAD_FILES: usize = 9_999;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JmDownloadMetadata {
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
}
impl JmDownloadMetadata {
    pub fn is_valid(&self) -> bool {
        crate::LibraryReference {
            source: Source::Jm,
            work_id: self.work_id.clone(),
        }
        .is_valid()
            && self.work_id.parse::<i64>().is_ok_and(|id| id > 0)
            && text(&self.title, 500)
            && self.authors.len() <= 100
            && self.authors.iter().all(|v| text(v, 200))
            && self.tags.len() <= 200
            && self.tags.iter().all(|v| text(v, 200))
            && self.description.as_ref().is_none_or(|v| {
                v.len() <= 32768
                    && !v
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            })
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadPhase {
    Queued,
    Downloading,
    Verifying,
    Saving,
    Paused,
    Error,
    Downloaded,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadFile {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadRecord {
    pub id: String,
    pub origin: String,
    pub revision: u64,
    pub approval_revision: u64,
    pub target_hash: String,
    pub root: LibraryRoot,
    pub generation: u64,
    pub metadata: JmDownloadMetadata,
    pub destination: String,
    pub phase: DownloadPhase,
    pub files_done: u64,
    pub files_total: Option<u64>,
    pub bytes_done: u64,
    pub error_code: Option<String>,
    pub library_entry_id: Option<String>,
    pub updated_at: u64,
    pub checkpoint_json: Option<String>,
    pub staging_report_json: Option<String>,
    pub output_identity: Option<String>,
    pub output_files: Vec<DownloadFile>,
    pub output_manifest_hash: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadsDocument {
    pub version: u32,
    pub tasks: Vec<DownloadRecord>,
}
impl Default for DownloadsDocument {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
        }
    }
}
fn text(v: &str, limit: usize) -> bool {
    !v.trim().is_empty() && v.chars().count() <= limit && !v.chars().any(char::is_control)
}
fn hash(v: &str) -> bool {
    crate::library_hash_is_valid(v)
}
fn json(v: &Option<String>, limit: usize) -> bool {
    v.as_ref()
        .is_none_or(|v| v.len() <= limit && serde_json::from_str::<serde_json::Value>(v).is_ok())
}
impl ValidatedDocument for DownloadsDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("DOWNLOAD_DOCUMENT_INVALID");
        if self.version != 1 || self.tasks.len() > MAX_DOWNLOAD_TASKS {
            return Err(invalid());
        }
        let mut ids = HashSet::new();
        for t in &self.tasks {
            if !hash(&t.id)
                || !ids.insert(&t.id)
                || t.origin != "manual"
                || t.revision == 0
                || t.revision > MAX_SAFE_INTEGER
                || t.approval_revision == 0
                || t.approval_revision > MAX_SAFE_INTEGER
                || !hash(&t.target_hash)
                || !hash(&t.root.id)
                || !hash(&t.root.file_key)
                || !Path::new(&t.root.path).is_absolute()
                || t.root.path.len() > 32768
                || t.root.path.chars().any(char::is_control)
                || t.generation > MAX_SAFE_INTEGER
                || !t.metadata.is_valid()
                || !crate::library_relative_path_is_valid(&t.destination)
                || t.destination.contains('/')
                || t.destination.encode_utf16().count() > 180
                || t.files_done > MAX_DOWNLOAD_FILES as u64
                || t.files_total
                    .is_some_and(|v| v == 0 || v > MAX_DOWNLOAD_FILES as u64 || v < t.files_done)
                || t.bytes_done > MAX_SAFE_INTEGER
                || t.updated_at > MAX_SAFE_INTEGER
                || t.error_code.as_ref().is_some_and(|v| {
                    v.is_empty()
                        || v.len() > 100
                        || !v
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                })
                || t.library_entry_id.as_ref().is_some_and(|v| !hash(v))
                || !json(&t.checkpoint_json, 8 * 1024 * 1024)
                || !json(&t.staging_report_json, 12 * 1024 * 1024)
                || t.output_identity.as_ref().is_some_and(|v| !hash(v))
                || t.output_manifest_hash.as_ref().is_some_and(|v| !hash(v))
                || t.output_files.len() > MAX_DOWNLOAD_FILES + 202
            {
                return Err(invalid());
            }
            let mut paths = HashSet::new();
            for file in &t.output_files {
                if !crate::library_relative_path_is_valid(&file.relative_path)
                    || !paths.insert(file.relative_path.to_lowercase())
                    || file.size_bytes == 0
                    || file.size_bytes > MAX_SAFE_INTEGER
                    || !hash(&file.sha256)
                {
                    return Err(invalid());
                }
            }
            if t.phase == DownloadPhase::Downloaded
                && (t.library_entry_id.is_none() || t.output_manifest_hash.is_none())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

/// Exclusive process-independent worker reservation. It does not hold the
/// documents lock while network requests run, so read/control remain responsive.
pub struct DownloadWorkspace {
    path: PathBuf,
    file: File,
    _directories: Vec<File>,
}
impl DownloadWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for DownloadWorkspace {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
impl WorkbenchStore {
    pub fn read_downloads(&self) -> Result<Document<DownloadsDocument>> {
        self.read("downloads.json", 32 * 1024 * 1024)
    }
    pub fn write_downloads(
        &self,
        expected_revision: u64,
        value: DownloadsDocument,
    ) -> Result<Document<DownloadsDocument>> {
        self.write("downloads.json", 32 * 1024 * 1024, expected_revision, value)
    }
    pub fn open_download_workspace(&self) -> Result<DownloadWorkspace> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("BUSY"))?;
        let _documents = self.acquire_lock()?;
        check_directory_tree(&self.root)?;
        let path = self.root.join("download-staging-v1");
        let mut directories = ensure_directory_tree(&path.join("commands"))?;
        let lock = path.join("worker.lock");
        check_optional_regular(&lock)?;
        let file = safe_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .map_err(|_| StoreError::new("DOWNLOAD_STORAGE_UNAVAILABLE"))?;
        check_open_regular(&file)?;
        file.try_lock()
            .map_err(|_| StoreError::new("DOWNLOAD_WORKER_BUSY"))?;
        directories.shrink_to_fit();
        Ok(DownloadWorkspace {
            path,
            file,
            _directories: directories,
        })
    }
}

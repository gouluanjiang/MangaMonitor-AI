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

pub const MAX_DOWNLOAD_TASKS: usize = 500;
/// Abandoned tasks retain their staging proofs until explicit cleanup succeeds.
pub const MAX_ABANDONED_DOWNLOAD_TASKS: usize = 500;
pub const MAX_DOWNLOAD_BATCH: usize = 50;
/// One reviewed selection may fill the existing queue; transport chunks stay small.
pub const MAX_DOWNLOAD_SELECTION: usize = MAX_DOWNLOAD_TASKS;
pub const MAX_DOWNLOAD_HISTORY_EVIDENCE: usize = 20_000;
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
    /// Immutable source date snapshot. Omission preserves legacy task bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_updated_at: Option<String>,
}
impl JmDownloadMetadata {
    pub fn is_valid(&self) -> bool {
        self.is_valid_for(Source::Jm)
    }
    /// The legacy name/API remains usable by JM callers. Pica uses the same
    /// sanitized display fields with its own canonical source identity.
    pub fn is_valid_for(&self, source: Source) -> bool {
        self.validate_for(source).is_ok()
    }
    /// Explain the existing admission rules without rewriting source metadata.
    /// Only stable codes cross IPC; rejected field values never enter errors.
    pub fn validate_for(&self, source: Source) -> Result<()> {
        let reference = crate::LibraryReference {
            source,
            work_id: self.work_id.clone(),
        };
        if !reference.is_valid()
            || (source == Source::Jm && !self.work_id.parse::<i64>().is_ok_and(|id| id > 0))
        {
            return Err(StoreError::new("DOWNLOAD_METADATA_ID_INVALID"));
        }
        metadata_text(
            &self.title,
            500,
            [
                "DOWNLOAD_METADATA_TITLE_EMPTY",
                "DOWNLOAD_METADATA_TITLE_TOO_LONG",
                "DOWNLOAD_METADATA_TITLE_CONTROL",
            ],
        )?;
        if self.authors.len() > 100 {
            return Err(StoreError::new("DOWNLOAD_METADATA_AUTHORS_TOO_MANY"));
        }
        for author in &self.authors {
            metadata_text(
                author,
                200,
                [
                    "DOWNLOAD_METADATA_AUTHOR_EMPTY",
                    "DOWNLOAD_METADATA_AUTHOR_TOO_LONG",
                    "DOWNLOAD_METADATA_AUTHOR_CONTROL",
                ],
            )?;
        }
        if self.tags.len() > 200 {
            return Err(StoreError::new("DOWNLOAD_METADATA_TAGS_TOO_MANY"));
        }
        for tag in &self.tags {
            metadata_text(
                tag,
                200,
                [
                    "DOWNLOAD_METADATA_TAG_EMPTY",
                    "DOWNLOAD_METADATA_TAG_TOO_LONG",
                    "DOWNLOAD_METADATA_TAG_CONTROL",
                ],
            )?;
        }
        if self
            .version_updated_at
            .as_deref()
            .is_some_and(|date| !crate::work_date_is_valid(date))
        {
            return Err(StoreError::new("DOWNLOAD_METADATA_DATE_INVALID"));
        }
        if let Some(description) = &self.description {
            if description.len() > 32768 {
                return Err(StoreError::new("DOWNLOAD_METADATA_DESCRIPTION_TOO_LONG"));
            }
            if description
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            {
                return Err(StoreError::new("DOWNLOAD_METADATA_DESCRIPTION_CONTROL"));
            }
        }
        Ok(())
    }
}

fn metadata_text(value: &str, maximum: usize, codes: [&'static str; 3]) -> Result<()> {
    if value.trim().is_empty() {
        return Err(StoreError::new(codes[0]));
    }
    if value.chars().count() > maximum {
        return Err(StoreError::new(codes[1]));
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::new(codes[2]));
    }
    Ok(())
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
    Abandoned,
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
    /// Records created before Pica support retain their exact JM interpretation.
    #[serde(default = "default_source", skip_serializing_if = "is_jm")]
    pub source: Source,
    pub origin: String,
    pub revision: u64,
    pub approval_revision: u64,
    pub target_hash: String,
    pub root: LibraryRoot,
    pub generation: u64,
    pub metadata: JmDownloadMetadata,
    pub destination: String,
    /// Old saved tasks retain their original WEBP policy and checkpoint hashes.
    #[serde(default)]
    pub jpeg_output: bool,
    /// Missing on old tasks: their directory layout and approval hash stay valid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub zip_output: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_file: Option<DownloadFile>,
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
fn is_false(value: &bool) -> bool {
    !*value
}
const fn default_source() -> Source {
    Source::Jm
}
fn is_jm(source: &Source) -> bool {
    *source == Source::Jm
}
/// Compact identity evidence retained when the user clears completed history.
/// It prevents history housekeeping from removing duplicate/manual-match
/// protection. It contains neither image manifests nor source credentials.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadHistoryEvidence {
    pub source: Source,
    pub work_id: String,
    pub root: LibraryRoot,
    pub destination: String,
    pub library_entry_id: String,
    /// Original output identity from the completed task, never a later scan.
    /// Older compact receipts cannot establish unchanged-file ownership alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_identity: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadsDocument {
    pub version: u32,
    pub tasks: Vec<DownloadRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history_evidence: Vec<DownloadHistoryEvidence>,
}
impl Default for DownloadsDocument {
    fn default() -> Self {
        Self {
            version: 2,
            tasks: Vec::new(),
            history_evidence: Vec::new(),
        }
    }
}
fn hash(v: &str) -> bool {
    crate::library_hash_is_valid(v)
}
fn json(v: &Option<String>, limit: usize) -> bool {
    v.as_ref()
        .is_none_or(|v| v.len() <= limit && serde_json::from_str::<serde_json::Value>(v).is_ok())
}
impl ValidatedDocument for DownloadsDocument {
    const VERSION: u32 = 2;
    fn migrate(&mut self) -> Result<()> {
        if self.version == 1 {
            // Version one never admitted this new state or more than 500 rows.
            if self.tasks.len() > MAX_DOWNLOAD_TASKS
                || self
                    .tasks
                    .iter()
                    .any(|task| task.phase == DownloadPhase::Abandoned)
            {
                return Err(StoreError::new("DOWNLOAD_DOCUMENT_INVALID"));
            }
            self.version = 2;
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("DOWNLOAD_DOCUMENT_INVALID");
        if self.version != 2
            || self
                .tasks
                .iter()
                .filter(|task| task.phase != DownloadPhase::Abandoned)
                .count()
                > MAX_DOWNLOAD_TASKS
            || self
                .tasks
                .iter()
                .filter(|task| task.phase == DownloadPhase::Abandoned)
                .count()
                > MAX_ABANDONED_DOWNLOAD_TASKS
            || self.history_evidence.len() > MAX_DOWNLOAD_HISTORY_EVIDENCE
        {
            return Err(invalid());
        }
        let mut evidence = HashSet::new();
        for old in &self.history_evidence {
            if !(crate::LibraryReference {
                source: old.source,
                work_id: old.work_id.clone(),
            })
            .is_valid()
                || !hash(&old.root.id)
                || !hash(&old.root.file_key)
                || !Path::new(&old.root.path).is_absolute()
                || old.root.path.len() > 32768
                || old.root.path.chars().any(char::is_control)
                || !crate::library_relative_path_is_valid(&old.destination)
                || old.destination.contains('/')
                || old.destination.encode_utf16().count() > 180
                || !hash(&old.library_entry_id)
                || old
                    .output_identity
                    .as_ref()
                    .is_some_and(|identity| !hash(identity))
                || !evidence.insert((
                    old.source,
                    &old.work_id,
                    &old.root.id,
                    &old.root.file_key,
                    &old.destination,
                    &old.library_entry_id,
                    &old.output_identity,
                ))
            {
                return Err(invalid());
            }
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
                || !t.metadata.is_valid_for(t.source)
                || (t.source == Source::Pica && t.jpeg_output)
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
                || (t.zip_output && !t.destination.ends_with(".zip"))
                || (!t.zip_output && t.archive_file.is_some())
                || t.archive_file.as_ref().is_some_and(|file| {
                    file.relative_path != t.destination
                        || file.size_bytes == 0
                        || file.size_bytes > MAX_SAFE_INTEGER
                        || !hash(&file.sha256)
                })
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
            if t.phase == DownloadPhase::Abandoned && t.library_entry_id.is_some() {
                return Err(invalid());
            }
            if t.phase == DownloadPhase::Downloaded
                && (t.library_entry_id.is_none()
                    || t.output_manifest_hash.is_none()
                    || (t.zip_output && t.archive_file.is_none()))
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

use crate::{
    archive, cover, error,
    paths::{self, Node, Root},
    scan::{completed_work, ScanJob},
    LibraryCover, LibraryFreshness, LibraryPhase, LibraryReference, LibrarySnapshot, Result,
    ScanAction,
};
use std::{
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_storage::{
    library_hash_is_valid, library_relative_path_is_valid, Document, LibraryDocument,
    LibraryEvidence, LibraryFormat, LibraryRecord, WorkbenchStore, MAX_LIBRARY_ITEMS,
    MAX_LIBRARY_VISITED, MAX_SAFE_INTEGER,
};

#[derive(Default)]
pub struct LibraryService {
    pub(crate) job: Option<ScanJob>,
}

impl LibraryService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reveal one indexed item through the native host. Renderer paths are not
    /// accepted; verified root and item handles live through the host call.
    pub fn with_verified_location<T>(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
        reveal: impl FnOnce(&Path) -> Result<T>,
    ) -> Result<T> {
        let document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        let record = find_record(&document.value, entry_id)?;
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        let node = root.node(&record.item.relative_path)?;
        let identity = match &node {
            Node::File(file) if record.item.format != LibraryFormat::Directory => {
                paths::identity(&file.file)?
            }
            Node::Directory(directory) if record.item.format == LibraryFormat::Directory => {
                paths::identity(&directory.file)?
            }
            _ => return Err(error("LIBRARY_FILE_CHANGED")),
        };
        if record.identity.as_ref() != Some(&identity) {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        root.verify()?;
        let latest = store.read_library()?;
        require_scope(&latest.value, root_id, generation)?;
        if latest.revision != document.revision {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        let path = Path::new(&root.saved.path).join(&record.item.relative_path);
        reveal(&path)
    }

    /// Restores only private metadata. Does not enumerate or decode user files.
    pub fn read(&mut self, store: &WorkbenchStore) -> Result<LibrarySnapshot> {
        let document = store.read_library()?;
        let live = self.job_matches(&document);
        if !live {
            self.job = None;
        }
        Ok(snapshot(document, live))
    }

    /// Check previously indexed files: check only previously indexed entries,
    /// without enumerating new media or decoding covers. Changed/missing files
    /// lose positive PC evidence until the user refreshes the library scan.
    pub fn recheck_known_entries(&mut self, store: &WorkbenchStore) -> Result<LibrarySnapshot> {
        self.recheck_entries(store, false)
    }

    /// Startup/window re-entry checks known file identities without a full scan
    /// or image decoding. Restored cached records are not treated as presence.
    pub fn read_current_files(&mut self, store: &WorkbenchStore) -> Result<LibrarySnapshot> {
        self.recheck_entries(store, true)
    }

    fn recheck_entries(
        &mut self,
        store: &WorkbenchStore,
        allow_partial: bool,
    ) -> Result<LibrarySnapshot> {
        let mut document = store.read_library()?;
        let Some(saved) = document.value.root.as_ref() else {
            return Ok(snapshot(document, false));
        };
        if !allow_partial && document.value.phase != LibraryPhase::Complete {
            return Err(error("LIBRARY_NOT_READY"));
        }
        let root = Root::restore(saved)?;
        let mut changed = false;
        for record in &mut document.value.records {
            if record.item.state != crate::LibraryItemState::Indexed {
                continue;
            }
            let matches = match root.node(&record.item.relative_path) {
                Ok(Node::File(file)) if record.item.format != LibraryFormat::Directory => {
                    paths::identity(&file.file).ok().as_ref() == record.identity.as_ref()
                }
                Ok(Node::Directory(directory))
                    if record.item.format == LibraryFormat::Directory =>
                {
                    paths::identity(&directory.file).ok().as_ref() == record.identity.as_ref()
                }
                _ => false,
            };
            if !matches || record.identity.is_none() {
                record.item.state = crate::LibraryItemState::Unreadable;
                record.item.error_code = Some("LIBRARY_FILE_CHANGED".into());
                record.item.cover_available = false;
                record.cover = None;
                changed = true;
            }
        }
        root.verify()?;
        if changed {
            document = store.write_library(document.revision, document.value)?;
        }
        let live = self.job_matches(&document);
        Ok(snapshot(document, live))
    }

    /// The path comes exclusively from the native folder picker, never IPC.
    pub fn choose(&mut self, store: &WorkbenchStore, path: &Path) -> Result<LibrarySnapshot> {
        let previous = store.read_library()?;
        let root = Root::choose(path)?;
        self.begin(store, previous, root)
    }

    fn begin(
        &mut self,
        store: &WorkbenchStore,
        previous: Document<LibraryDocument>,
        root: Root,
    ) -> Result<LibrarySnapshot> {
        let generation = previous
            .value
            .generation
            .checked_add(1)
            .filter(|v| *v <= MAX_SAFE_INTEGER)
            .ok_or(error("REVISION_EXHAUSTED"))?;
        let mut job = ScanJob::new(root, generation, previous.revision, &previous.value.records)?;
        let value = LibraryDocument {
            reviewed_works: if previous.value.root.as_ref() == Some(&job.root.saved) {
                previous.value.reviewed_works.clone()
            } else {
                Vec::new()
            },
            relocations: if previous.value.root.as_ref() == Some(&job.root.saved) {
                previous.value.relocations.clone()
            } else {
                Vec::new()
            },
            root: Some(job.root.saved.clone()),
            generation,
            phase: LibraryPhase::Reading,
            updated_at: Some(now()),
            ..LibraryDocument::default()
        };
        self.job = None;
        let saved = store.write_library(previous.revision, value)?;
        job.revision = saved.revision;
        self.job = Some(job);
        Ok(snapshot(saved, true))
    }

    pub fn scan(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        action: ScanAction,
    ) -> Result<LibrarySnapshot> {
        let mut document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        if action == ScanAction::Start {
            let root = Root::restore(
                document
                    .value
                    .root
                    .as_ref()
                    .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
            )?;
            return self.begin(store, document, root);
        }
        if !self.job_matches(&document) {
            self.job = None;
            if matches!(
                document.value.phase,
                LibraryPhase::Complete | LibraryPhase::Error
            ) && action == ScanAction::Next
            {
                return Ok(snapshot(document, false));
            }
            return Err(error("LIBRARY_RESTART_REQUIRED"));
        }
        let mut job = self.job.take().ok_or(error("LIBRARY_RESTART_REQUIRED"))?;
        match action {
            ScanAction::Pause => document.value.phase = LibraryPhase::Paused,
            ScanAction::Resume => document.value.phase = LibraryPhase::Reading,
            ScanAction::Next => {
                if document.value.phase == LibraryPhase::Paused {
                    self.job = Some(job);
                    return Ok(snapshot(document, true));
                }
                if let Err(problem) = job.batch(&mut document.value) {
                    document.value.phase = LibraryPhase::Error;
                    document.value.error_code = Some(problem.code.into());
                }
            }
            ScanAction::Start => unreachable!(),
        }
        document.value.updated_at = Some(now());
        // A failed checkpoint drops the cursor. Continuing a consumed iterator
        // after a failed save could omit entries and falsely report completeness.
        let saved = store.write_library(document.revision, document.value)?;
        if matches!(
            saved.value.phase,
            LibraryPhase::Reading | LibraryPhase::Paused
        ) {
            job.revision = saved.revision;
            self.job = Some(job);
        }
        Ok(snapshot(saved, true))
    }

    pub fn cover(
        &self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
    ) -> Result<LibraryCover> {
        Self::read_cover(store, root_id, generation, entry_id)
    }

    /// Independent of the scan cursor. The native caller bounds parallel reads;
    /// persisted scope, revision and file identities are still checked on both sides.
    pub fn read_cover(
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
    ) -> Result<LibraryCover> {
        let document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        let record = find_record(&document.value, entry_id)?;
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        verify_record(&root, record)?;
        let Some(selection) = &record.cover else {
            return Ok(LibraryCover {
                root_id: root_id.into(),
                generation,
                entry_id: entry_id.into(),
                data_url: None,
            });
        };
        let bytes = if record.item.format == LibraryFormat::Directory {
            let Node::File(file) = root.node(&selection.relative_path)? else {
                return Err(error("LIBRARY_FILE_CHANGED"));
            };
            let identity = paths::identity(&file.file)?;
            if selection.identity.as_ref() != Some(&identity) {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            if identity.bytes > archive::MAX_IMAGE_BYTES as u64 {
                return Err(error("LIBRARY_COVER_LIMIT"));
            }
            let mut bytes = Vec::new();
            (&file.file)
                .take(archive::MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| error("LIBRARY_READ_FAILED"))?;
            if paths::identity(&file.file)? != identity {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            bytes
        } else {
            let Node::File(mut file) = root.node(&record.item.relative_path)? else {
                return Err(error("LIBRARY_FILE_CHANGED"));
            };
            if record.identity.as_ref() != Some(&paths::identity(&file.file)?) {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            let bytes = archive::cover(&mut file.file, &selection.relative_path)?;
            if record.identity.as_ref() != Some(&paths::identity(&file.file)?) {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            bytes
        };
        let data_url = cover::thumbnail(&bytes)?;
        root.verify()?;
        verify_record(&root, record)?;
        let latest = store.read_library()?;
        require_scope(&latest.value, root_id, generation)?;
        if latest.revision != document.revision {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        Ok(LibraryCover {
            root_id: root_id.into(),
            generation,
            entry_id: entry_id.into(),
            data_url: Some(data_url),
        })
    }

    /// Adds a private PC-index record for one command-finalized directory or ZIP.
    /// This is not a download completion, promotion or phone-presence authority.
    /// The command must have already validated its exact media tree and hashes.
    pub fn register_completed(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        relative_path: &str,
        expected_reference: &LibraryReference,
        expected_pages: u64,
    ) -> Result<LibrarySnapshot> {
        if !library_relative_path_is_valid(relative_path)
            || relative_path
                .split('/')
                .any(|part| part.starts_with(".下载中-"))
            || !expected_reference.is_valid()
            || !(1..=10_000).contains(&expected_pages)
        {
            return Err(error("VALIDATION_FAILED"));
        }
        let mut document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        if matches!(
            document.value.phase,
            LibraryPhase::Reading | LibraryPhase::Paused
        ) {
            return Err(error("LIBRARY_BUSY"));
        }
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        let (mut record, visited, skipped) = completed_work(&root, relative_path)?;
        record.item.added_at = Some(now());
        if record.item.source_ref.as_ref() != Some(expected_reference)
            || record.item.identity_evidence != Some(LibraryEvidence::Metadata)
            || record.item.page_count != Some(expected_pages)
        {
            return Err(error("LIBRARY_IDENTITY_CONFLICT"));
        }
        let existing = document
            .value
            .records
            .iter()
            .position(|old| old.item.relative_path == relative_path);
        if let Some(index) = existing {
            let old = &document.value.records[index];
            record.item.added_at = old.item.added_at;
            if old.identity.is_none() || old.identity != record.identity {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            if old.item.version_updated_at.is_some() {
                record
                    .item
                    .version_updated_at
                    .clone_from(&old.item.version_updated_at);
            }
            if old == &record {
                let latest = store.read_library()?;
                require_scope(&latest.value, root_id, generation)?;
                if latest.revision != document.revision {
                    return Err(error("LIBRARY_STALE_SNAPSHOT"));
                }
                return Ok(snapshot(document, false));
            }
            document.value.records[index] = record;
        } else {
            if document.value.records.len() >= MAX_LIBRARY_ITEMS {
                return Err(error("LIBRARY_LIMIT_REACHED"));
            }
            document.value.visited = document
                .value
                .visited
                .checked_add(visited)
                .filter(|value| *value <= MAX_LIBRARY_VISITED)
                .ok_or(error("LIBRARY_LIMIT_REACHED"))?;
            document.value.skipped = document
                .value
                .skipped
                .checked_add(skipped)
                .filter(|value| *value <= document.value.visited)
                .ok_or(error("LIBRARY_LIMIT_REACHED"))?;
            document.value.records.push(record);
        }
        root.verify()?;
        document.value.updated_at = Some(now());
        // Compare-and-swap rejects another window changing the root, generation
        // or index while this bounded directory inspection was in progress.
        let saved = store.write_library(document.revision, document.value)?;
        self.job = None;
        Ok(snapshot(saved, false))
    }

    fn job_matches(&self, document: &Document<LibraryDocument>) -> bool {
        self.job.as_ref().is_some_and(|job| {
            job.revision == document.revision
                && job.generation == document.value.generation
                && document
                    .value
                    .root
                    .as_ref()
                    .is_some_and(|root| root.id == job.root.saved.id)
        })
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|v| u64::try_from(v.as_millis()).ok())
        .unwrap_or(0)
        .min(MAX_SAFE_INTEGER)
}

pub(crate) fn require_scope(
    document: &LibraryDocument,
    root_id: &str,
    generation: u64,
) -> Result<()> {
    if !library_hash_is_valid(root_id) || generation == 0 || generation > MAX_SAFE_INTEGER {
        return Err(error("VALIDATION_FAILED"));
    }
    let root = document
        .root
        .as_ref()
        .ok_or(error("LIBRARY_NOT_CONFIGURED"))?;
    if root.id != root_id || document.generation != generation {
        return Err(error("LIBRARY_STALE_SNAPSHOT"));
    }
    Ok(())
}

fn find_record<'a>(document: &'a LibraryDocument, entry_id: &str) -> Result<&'a LibraryRecord> {
    if !library_hash_is_valid(entry_id) {
        return Err(error("VALIDATION_FAILED"));
    }
    document
        .records
        .iter()
        .find(|r| r.item.id == entry_id)
        .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))
}

pub(crate) fn verify_record(root: &Root, record: &LibraryRecord) -> Result<()> {
    let identity = match root.node(&record.item.relative_path)? {
        Node::Directory(directory) if record.item.format == LibraryFormat::Directory => {
            paths::identity(&directory.file)?
        }
        Node::File(file) if record.item.format != LibraryFormat::Directory => {
            paths::identity(&file.file)?
        }
        _ => return Err(error("LIBRARY_FILE_CHANGED")),
    };
    if record.identity.as_ref() != Some(&identity) {
        return Err(error("LIBRARY_FILE_CHANGED"));
    }
    Ok(())
}

pub(crate) fn snapshot(document: Document<LibraryDocument>, live: bool) -> LibrarySnapshot {
    let value = document.value;
    let freshness = if value.root.is_none() {
        LibraryFreshness::None
    } else if live {
        LibraryFreshness::Live
    } else {
        LibraryFreshness::Cached
    };
    let phase = if !live && value.phase == LibraryPhase::Reading {
        LibraryPhase::Paused
    } else {
        value.phase
    };
    LibrarySnapshot {
        revision: document.revision,
        root_id: value.root.as_ref().map(|root| root.id.clone()),
        root_path: value.root.map(|root| root.path),
        generation: value.generation,
        phase,
        freshness,
        items: value
            .records
            .into_iter()
            .map(|record| record.item)
            .collect(),
        visited: value.visited,
        skipped: value.skipped,
        updated_at: value.updated_at,
        error_code: value.error_code,
    }
}

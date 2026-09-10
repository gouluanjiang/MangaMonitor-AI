use crate::{
    archive, cover, error,
    paths::{self, Node, Root},
    scan::ScanJob,
    LibraryCover, LibraryFreshness, LibraryPhase, LibraryReference, LibrarySnapshot, Result,
    ScanAction,
};
use std::{
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_storage::{
    library_hash_is_valid, Document, LibraryDocument, LibraryEvidence, LibraryFormat,
    LibraryRecord, WorkbenchStore, MAX_SAFE_INTEGER,
};

#[derive(Default)]
pub struct LibraryService {
    job: Option<ScanJob>,
}

impl LibraryService {
    pub fn new() -> Self {
        Self::default()
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
        &mut self,
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

    pub fn link(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
        reference: Option<LibraryReference>,
    ) -> Result<LibrarySnapshot> {
        let reference = reference.map(|mut value| {
            value.work_id.make_ascii_lowercase();
            value
        });
        if reference.as_ref().is_some_and(|v| !v.is_valid()) {
            return Err(error("VALIDATION_FAILED"));
        }
        let mut document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        verify_record(&root, find_record(&document.value, entry_id)?)?;
        let was_live = self.job_matches(&document);
        let record = document
            .value
            .records
            .iter_mut()
            .find(|r| r.item.id == entry_id)
            .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))?;
        record.item.identity_evidence = reference.as_ref().map(|_| LibraryEvidence::Manual);
        record.item.source_ref = reference;
        record.manual_override = true;
        if record.item.error_code.as_deref() == Some("LIBRARY_IDENTITY_CONFLICT") {
            record.item.error_code = None;
        }
        document.value.updated_at = Some(now());
        root.verify()?;
        let saved = store.write_library(document.revision, document.value)?;
        if was_live {
            if let Some(job) = &mut self.job {
                job.revision = saved.revision;
            }
        } else {
            self.job = None;
        }
        Ok(snapshot(saved, was_live))
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|v| u64::try_from(v.as_millis()).ok())
        .unwrap_or(0)
        .min(MAX_SAFE_INTEGER)
}

fn require_scope(document: &LibraryDocument, root_id: &str, generation: u64) -> Result<()> {
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

fn verify_record(root: &Root, record: &LibraryRecord) -> Result<()> {
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

fn snapshot(document: Document<LibraryDocument>, live: bool) -> LibrarySnapshot {
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

//! One-time import of an externally reviewed identity list. No matching, source
//! requests, media writes or fabricated download history are performed here.
use crate::{
    error, hash,
    paths::{self, Node, Root},
    Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_storage::{
    library_hash_is_valid, library_relative_path_is_valid, Document, LibraryDocument,
    LibraryFormat, LibraryItemState, LibraryPhase, LibraryReference, LibraryRoot,
    ReviewedLibraryWork, WorkbenchStore, MAX_LIBRARY_DOCUMENT_BYTES, MAX_LIBRARY_ITEMS,
    MAX_SAFE_INTEGER,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    root: LibraryRoot,
    items: Vec<ManifestItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestItem {
    relative_path: String,
    bytes: u64,
    sha256: String,
    references: Vec<LibraryReference>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedLibraryImport {
    pub manifest_sha256: String,
    pub library_revision: u64,
    pub files: usize,
    pub references: usize,
    pub added: usize,
    pub unchanged: usize,
    pub applied: bool,
    pub file_hashes_verified: bool,
}

fn parse(bytes: &[u8], library: &LibraryDocument) -> Result<Manifest> {
    if bytes.len() > MAX_LIBRARY_DOCUMENT_BYTES {
        return Err(error("LIBRARY_REVIEW_LIMIT"));
    }
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| error("LIBRARY_REVIEW_INVALID"))?;
    if manifest.schema_version != 1
        || manifest.items.is_empty()
        || manifest.items.len() > MAX_LIBRARY_ITEMS
    {
        return Err(error("LIBRARY_REVIEW_INVALID"));
    }
    if library.root.as_ref() != Some(&manifest.root) {
        return Err(error("LIBRARY_REVIEW_ROOT_MISMATCH"));
    }
    if library.phase != LibraryPhase::Complete {
        return Err(error("LIBRARY_NOT_READY"));
    }
    let mut paths = HashSet::new();
    let mut references = HashSet::new();
    for item in &manifest.items {
        if !library_relative_path_is_valid(&item.relative_path)
            || !item.relative_path.to_ascii_lowercase().ends_with(".zip")
            || !paths.insert(&item.relative_path)
            || !library_hash_is_valid(&item.sha256)
            || item.bytes == 0
            || item.bytes > MAX_SAFE_INTEGER
            || item.references.is_empty()
        {
            return Err(error("LIBRARY_REVIEW_INVALID"));
        }
        for reference in &item.references {
            if !reference.is_valid() || !references.insert((reference.source, &reference.work_id)) {
                return Err(error("LIBRARY_REVIEW_CONFLICT"));
            }
        }
    }
    if references.len() > MAX_LIBRARY_ITEMS * 2 {
        return Err(error("LIBRARY_REVIEW_LIMIT"));
    }
    Ok(manifest)
}

fn prepare(
    store: &WorkbenchStore,
    bytes: &[u8],
    verify_hashes: bool,
) -> Result<(Document<LibraryDocument>, ReviewedLibraryImport)> {
    let mut library = store.read_library()?;
    let manifest = parse(bytes, &library.value)?;
    let root = Root::restore(&manifest.root)?;
    let digest = hash(bytes);
    let registered_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(MAX_SAFE_INTEGER as u128) as u64;
    let records = library
        .value
        .records
        .iter()
        .map(|r| (r.item.relative_path.as_str(), r))
        .collect::<BTreeMap<_, _>>();
    let mut works = library
        .value
        .reviewed_works
        .iter()
        .cloned()
        .map(|r| (key(&r.reference), r))
        .collect::<BTreeMap<_, _>>();
    let mut added = 0;
    let mut unchanged = 0;
    let mut verified = Vec::new();
    for item in &manifest.items {
        let record = records
            .get(item.relative_path.as_str())
            .ok_or(error("LIBRARY_REVIEW_NOT_INDEXED"))?;
        if record.item.format != LibraryFormat::Zip
            || record.item.state != LibraryItemState::Indexed
            || record.item.error_code.is_some()
            || record.item.page_count.is_none_or(|v| v == 0)
        {
            return Err(error("LIBRARY_REVIEW_NOT_INDEXED"));
        }
        let Node::File(mut file) = root.node(&item.relative_path)? else {
            return Err(error("LIBRARY_FILE_CHANGED"));
        };
        let identity = paths::identity(&file.file)?;
        if identity.bytes != item.bytes || record.identity.as_ref() != Some(&identity) {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        if verify_hashes {
            let mut content = Sha256::new();
            let mut buffer = [0_u8; 65536];
            loop {
                let n = file
                    .file
                    .read(&mut buffer)
                    .map_err(|_| error("LIBRARY_READ_FAILED"))?;
                if n == 0 {
                    break;
                }
                content.update(&buffer[..n]);
            }
            if format!("{:x}", content.finalize()) != item.sha256
                || paths::identity(&file.file)? != identity
            {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
        }
        for reference in &item.references {
            let next = ReviewedLibraryWork {
                reference: reference.clone(),
                relative_path: item.relative_path.clone(),
                library_entry_id: record.item.id.clone(),
                identity: identity.clone(),
                sha256: item.sha256.clone(),
                review_sha256: digest.clone(),
                registered_at,
            };
            if let Some(old) = works.get(&key(reference)) {
                if old.relative_path != next.relative_path
                    || old.library_entry_id != next.library_entry_id
                    || old.identity != next.identity
                    || old.sha256 != next.sha256
                {
                    return Err(error("LIBRARY_REVIEW_CONFLICT"));
                }
                unchanged += 1;
            } else {
                works.insert(key(reference), next);
                added += 1;
            }
        }
        verified.push((item.relative_path.clone(), identity));
    }
    if works.len() > MAX_LIBRARY_ITEMS * 2 {
        return Err(error("LIBRARY_REVIEW_LIMIT"));
    }
    // Reopen each selected path before the atomic document write; a scan or file
    // replacement cannot silently turn a previously checked candidate into owned.
    for (path, identity) in verified {
        let Node::File(file) = root.node(&path)? else {
            return Err(error("LIBRARY_FILE_CHANGED"));
        };
        if paths::identity(&file.file)? != identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
    }
    root.verify()?;
    let report = ReviewedLibraryImport {
        manifest_sha256: digest,
        library_revision: library.revision,
        files: manifest.items.len(),
        references: added + unchanged,
        added,
        unchanged,
        applied: false,
        file_hashes_verified: verify_hashes,
    };
    library.value.reviewed_works = works.into_values().collect();
    Ok((library, report))
}

/// Preview checks scope/index/file identity, but explicitly does not claim the
/// manifest's SHA-256 values were checked. It makes no persistent changes.
pub fn preview_reviewed_library(
    store: &WorkbenchStore,
    bytes: &[u8],
) -> Result<ReviewedLibraryImport> {
    prepare(store, bytes, false).map(|(_, report)| report)
}

/// The caller supplies the exact preview revision and explicitly accepted list
/// digest. Everything is checked before one CAS write; no task is completed.
pub fn import_reviewed_library(
    store: &WorkbenchStore,
    bytes: &[u8],
    expected_revision: u64,
    expected_manifest_sha256: &str,
) -> Result<ReviewedLibraryImport> {
    if hash(bytes) != expected_manifest_sha256 {
        return Err(error("LIBRARY_REVIEW_CHANGED"));
    }
    if store.read_library()?.revision != expected_revision {
        return Err(error("LIBRARY_STALE_SNAPSHOT"));
    }
    let (library, mut report) = prepare(store, bytes, true)?;
    if library.revision != expected_revision {
        return Err(error("LIBRARY_STALE_SNAPSHOT"));
    }
    if report.added > 0 {
        report.library_revision = store
            .write_library(expected_revision, library.value)?
            .revision;
    } else if store.read_library()?.revision != expected_revision {
        return Err(error("LIBRARY_STALE_SNAPSHOT"));
    }
    report.applied = true;
    Ok(report)
}

fn key(reference: &LibraryReference) -> String {
    format!("{:?}:{}", reference.source, reference.work_id)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewedFilePresence {
    Present,
    Missing,
    Changed,
    Unavailable,
}

pub struct ReviewedLibraryPresence {
    pub reference: LibraryReference,
    pub library_entry_id: String,
    pub presence: ReviewedFilePresence,
}

/// Rechecks only explicitly registered file identities, not titles, metadata
/// references, legacy links or unregistered neighboring ZIPs.
pub fn reviewed_library_presence(library: &LibraryDocument) -> Vec<ReviewedLibraryPresence> {
    let root = library.root.as_ref().and_then(|r| Root::restore(r).ok());
    let records = library
        .records
        .iter()
        .map(|r| (r.item.id.as_str(), r))
        .collect::<BTreeMap<_, _>>();
    let mut cache = BTreeMap::new();
    let mut result = Vec::new();
    for work in &library.reviewed_works {
        let relocated = library
            .relocated_path(&work.relative_path)
            .filter(|r| r.sha256 == work.sha256 && r.old_item_id == work.library_entry_id);
        let path = relocated.map_or(work.relative_path.as_str(), |r| &r.new_path);
        let id = relocated.map_or(work.library_entry_id.as_str(), |r| &r.new_item_id);
        let expected = relocated.map_or(&work.identity, |r| &r.identity);
        let state = *cache
            .entry((
                path,
                id,
                expected.file_key.as_str(),
                expected.modified.as_str(),
                expected.bytes,
            ))
            .or_insert_with(|| {
                let Some(root) = &root else {
                    return ReviewedFilePresence::Unavailable;
                };
                match root.node(path) {
                    Err(problem) if problem.code == "LIBRARY_ENTRY_MISSING" => {
                        ReviewedFilePresence::Missing
                    }
                    Err(_) => ReviewedFilePresence::Unavailable,
                    Ok(Node::File(file)) => {
                        if paths::identity(&file.file).ok().as_ref() != Some(expected) {
                            return ReviewedFilePresence::Changed;
                        }
                        match records.get(id) {
                            Some(record)
                                if record.item.relative_path == path
                                    && record.identity.as_ref() == Some(expected)
                                    && record.item.state == LibraryItemState::Indexed
                                    && record.item.error_code.is_none() =>
                            {
                                ReviewedFilePresence::Present
                            }
                            _ => ReviewedFilePresence::Unavailable,
                        }
                    }
                    Ok(_) => ReviewedFilePresence::Changed,
                }
            });
        result.push(ReviewedLibraryPresence {
            reference: work.reference.clone(),
            library_entry_id: id.into(),
            presence: state,
        });
    }
    if root.as_ref().is_none_or(|r| r.verify().is_err()) {
        for item in &mut result {
            item.presence = ReviewedFilePresence::Unavailable;
        }
    }
    result
}

//! Explicit local mapping import. It only changes the private index; media,
//! old immutable task receipts and retired phone/booklist documents stay intact.
use crate::{
    archive, error, hash,
    paths::{self, Node, Root},
    scan, LibraryService, LibrarySnapshot, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::{Component, Path},
};
use workbench_storage::{
    LibraryEvidence, LibraryFormat, LibraryItemState, LibraryPhase, LibraryRecord,
    LibraryRelocation, WorkbenchStore, MAX_LIBRARY_ITEMS,
};

#[derive(Deserialize)]
struct Mapping {
    schema_version: u64,
    library: String,
    items: Vec<MappingItem>,
}
#[derive(Deserialize)]
struct MappingItem {
    old_source: String,
    zip: String,
    output_sha256: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMigrationResult {
    pub snapshot: LibrarySnapshot,
    pub mapped: usize,
    pub associated: usize,
    pub unchanged: usize,
}

// Old paths no longer exist, so derive them lexically under the explicit mapping
// root. The current root itself is reopened and bound by OS identity below.
fn relative(root: &Path, value: &str) -> Result<String> {
    let path = Path::new(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|v| matches!(v, Component::ParentDir | Component::CurDir))
    {
        return Err(error("LIBRARY_MIGRATION_INVALID"));
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| error("LIBRARY_MIGRATION_ROOT_MISMATCH"))?;
    let value = relative
        .to_str()
        .ok_or(error("LIBRARY_MIGRATION_INVALID"))?
        .replace('\\', "/");
    if !workbench_storage::library_relative_path_is_valid(&value) {
        return Err(error("LIBRARY_MIGRATION_INVALID"));
    }
    Ok(value)
}
fn verified_zip(root: &Root, relative: &str, expected: &str) -> Result<LibraryRecord> {
    let Node::File(mut file) = root.node(relative)? else {
        return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
    };
    let identity = paths::identity(&file.file)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let size = file
            .file
            .read(&mut buffer)
            .map_err(|_| error("LIBRARY_READ_FAILED"))?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
    }
    let mut record = scan::base_record(&root.saved.id, relative, LibraryFormat::Zip);
    record.item.bytes = identity.bytes;
    record.item.modified_at = file
        .file
        .metadata()
        .ok()
        .and_then(|v| paths::modified_at(&v));
    record.identity = Some(identity.clone());
    archive::inspect(&mut file.file, &mut record)?;
    if paths::identity(&file.file)? != identity {
        return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
    }
    if record.item.state != LibraryItemState::Indexed || record.item.error_code.is_some() {
        return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
    }
    Ok(record)
}

impl LibraryService {
    pub fn import_paths(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        bytes: &[u8],
    ) -> Result<LibraryMigrationResult> {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(error("LIBRARY_MIGRATION_LIMIT"));
        }
        let mapping: Mapping =
            serde_json::from_slice(bytes).map_err(|_| error("LIBRARY_MIGRATION_INVALID"))?;
        if mapping.schema_version != 1
            || mapping.items.is_empty()
            || mapping.items.len() > MAX_LIBRARY_ITEMS
        {
            return Err(error("LIBRARY_MIGRATION_INVALID"));
        }
        let mut current = store.read_library()?;
        let saved = current
            .value
            .root
            .as_ref()
            .ok_or(error("LIBRARY_NOT_CONFIGURED"))?;
        if saved.id != root_id || current.value.generation != generation {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        if current.value.phase == LibraryPhase::Reading {
            return Err(error("LIBRARY_BUSY"));
        }
        let root = Root::restore(saved)?;
        let supplied_root = Path::new(&mapping.library);
        if Root::choose(supplied_root)?.saved != root.saved {
            return Err(error("LIBRARY_MIGRATION_ROOT_MISMATCH"));
        }
        let tasks = store.read_downloads()?;
        if tasks.value.tasks.iter().any(|task| {
            task.root == root.saved
                && matches!(
                    task.phase,
                    workbench_storage::DownloadPhase::Queued
                        | workbench_storage::DownloadPhase::Downloading
                        | workbench_storage::DownloadPhase::Verifying
                        | workbench_storage::DownloadPhase::Saving
                )
        }) {
            return Err(error("LIBRARY_BUSY"));
        }
        let mut existing: BTreeMap<_, _> = current
            .value
            .records
            .iter()
            .cloned()
            .map(|record| (record.item.relative_path.clone(), record))
            .collect();
        let mut targets = BTreeMap::<String, (String, LibraryRecord)>::new();
        let mut relocations: BTreeMap<_, _> = current
            .value
            .relocations
            .iter()
            .cloned()
            .map(|item| (item.old_path.clone(), item))
            .collect();
        let mut old_paths = HashSet::new();
        let mut entries = Vec::new();
        let mut associated = 0;
        let mut unchanged = 0;
        for item in &mapping.items {
            let old = relative(supplied_root, &item.old_source)?;
            let new = relative(supplied_root, &item.zip)?;
            if !new.to_ascii_lowercase().ends_with(".zip")
                || new.contains('/')
                || !old_paths.insert(old.clone())
                || !workbench_storage::library_hash_is_valid(&item.output_sha256)
            {
                return Err(error("LIBRARY_MIGRATION_INVALID"));
            }
            if !targets.contains_key(&new) {
                targets.insert(
                    new.clone(),
                    (
                        item.output_sha256.clone(),
                        verified_zip(&root, &new, &item.output_sha256)?,
                    ),
                );
            }
            let (digest, target) = targets
                .get_mut(&new)
                .ok_or(error("LIBRARY_MIGRATION_INVALID"))?;
            if digest != &item.output_sha256 {
                return Err(error("LIBRARY_MIGRATION_INVALID"));
            }
            for previous in [
                existing.get(&new),
                (old != new).then(|| existing.get(&old)).flatten(),
            ]
            .into_iter()
            .flatten()
            {
                target.item.added_at = match (target.item.added_at, previous.item.added_at) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                for link in &previous.item.links {
                    if target
                        .item
                        .references()
                        .any(|v| v.source == link.reference.source && v != &link.reference)
                    {
                        return Err(error("LIBRARY_MIGRATION_CONFLICT"));
                    }
                    if !target.item.references().any(|v| v == &link.reference) {
                        target.item.links.push(link.clone());
                    }
                }
                if previous.manual_override {
                    if target.manual_override && target.item.source_ref != previous.item.source_ref
                    {
                        return Err(error("LIBRARY_MIGRATION_CONFLICT"));
                    }
                    target.item.source_ref.clone_from(&previous.item.source_ref);
                    target.item.identity_evidence = previous
                        .item
                        .source_ref
                        .as_ref()
                        .map(|_| LibraryEvidence::Manual);
                    target.manual_override = true;
                    associated += 1;
                } else if !target.manual_override {
                    if let Some(reference) = &previous.item.source_ref {
                        if target
                            .item
                            .source_ref
                            .as_ref()
                            .is_some_and(|v| v != reference)
                        {
                            return Err(error("LIBRARY_MIGRATION_CONFLICT"));
                        }
                        if target.item.source_ref.is_none() {
                            target.item.source_ref = Some(reference.clone());
                            // The explicit mapping import is the new association evidence.
                            target.item.identity_evidence = Some(LibraryEvidence::Manual);
                            target.manual_override = true;
                            associated += 1;
                        }
                    }
                }
            }
            if old == new {
                unchanged += 1;
            } else {
                relocations.insert(
                    old.clone(),
                    LibraryRelocation {
                        old_item_id: existing
                            .get(&old)
                            .map(|record| record.item.id.clone())
                            .unwrap_or_else(|| {
                                hash(format!("{}\0{old}", root.saved.id).as_bytes())
                            }),
                        old_path: old.clone(),
                        new_path: new.clone(),
                        new_item_id: target.item.id.clone(),
                        identity: target
                            .identity
                            .clone()
                            .ok_or(error("LIBRARY_MIGRATION_FILE_CHANGED"))?,
                        sha256: item.output_sha256.clone(),
                    },
                );
            }
            entries.push((old, new));
        }
        // Only retire old index paths which really no longer exist. No unrelated
        // entries or media are removed, and every target is rechecked before CAS.
        for (old, new) in entries {
            if old != new {
                match root.node(&old) {
                    Err(problem) if problem.code == "LIBRARY_ENTRY_MISSING" => {
                        existing.remove(&old);
                    }
                    Ok(_) => return Err(error("LIBRARY_MIGRATION_ORIGINAL_PRESENT")),
                    Err(problem) => return Err(problem),
                }
            }
        }
        for (path, (_, record)) in targets {
            let Node::File(file) = root.node(&path)? else {
                return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
            };
            if Some(paths::identity(&file.file)?) != record.identity {
                return Err(error("LIBRARY_MIGRATION_FILE_CHANGED"));
            }
            existing.insert(path, record);
        }
        if existing.len() > MAX_LIBRARY_ITEMS || relocations.len() > MAX_LIBRARY_ITEMS {
            return Err(error("LIBRARY_MIGRATION_LIMIT"));
        }
        root.verify()?;
        if store.read_downloads()?.revision != tasks.revision {
            return Err(error("LIBRARY_STALE_SNAPSHOT"));
        }
        current.value.records = existing.into_values().collect();
        current.value.relocations = relocations.into_values().collect();
        current.value.visited = current
            .value
            .visited
            .max(current.value.records.len() as u64 + current.value.skipped);
        // The mapping covers only changed works. A subsequent ordinary scan is
        // necessary before absence across the whole collection can be claimed.
        current.value.phase = LibraryPhase::Paused;
        current.value.error_code = None;
        let saved = store.write_library(current.revision, current.value)?;
        self.job = None;
        Ok(LibraryMigrationResult {
            snapshot: crate::service::snapshot(saved, false),
            mapped: mapping.items.len(),
            associated,
            unchanged,
        })
    }
}

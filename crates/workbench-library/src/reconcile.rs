use crate::{
    error,
    paths::Root,
    service::{now, require_scope, snapshot, verify_record},
    LibraryService, LibrarySnapshot, Result,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use workbench_storage::{
    matching_title, strong_library_match, LibraryLinkEvidence, LibraryMatchWork, LibraryPhase,
    LibraryReference, LibrarySourceLink, WorkbenchStore, MAX_LIBRARY_ITEMS,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryReconcileResult {
    pub snapshot: LibrarySnapshot,
    pub linked: usize,
    pub examined: usize,
}

impl LibraryService {
    /// Only metadata associations are written. Existing files, titles and source IDs stay intact.
    pub fn reconcile(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        works: &[LibraryMatchWork],
        apply: bool,
    ) -> Result<LibraryReconcileResult> {
        if works.len() > MAX_LIBRARY_ITEMS
            || works.iter().any(|w| {
                !w.reference.is_valid()
                    || w.title.is_empty()
                    || w.title.chars().count() > 1024
                    || w.authors.len() > 50
                    || w.authors.iter().any(|a| a.chars().count() > 200)
                    || w.page_count.is_some_and(|n| n > 100_000)
            })
        {
            return Err(error("VALIDATION_FAILED"));
        }
        let mut document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        if document.value.phase != LibraryPhase::Complete {
            return Err(error("LIBRARY_BUSY"));
        }
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        let pairs = store.read_source_matches()?;
        let mut aliases = HashMap::new();
        for pair in &pairs.value.pairs {
            aliases.insert(
                (pair.jm.source, pair.jm.work_id.clone()),
                LibraryReference {
                    source: pair.pica.source,
                    work_id: pair.pica.work_id.clone(),
                },
            );
            aliases.insert(
                (pair.pica.source, pair.pica.work_id.clone()),
                LibraryReference {
                    source: pair.jm.source,
                    work_id: pair.jm.work_id.clone(),
                },
            );
        }
        let mut seen = HashSet::new();
        let mut proposals = Vec::new();
        let mut targets = HashMap::new();
        let mut titles = HashMap::<String, Vec<usize>>::new();
        let mut known = HashSet::new();
        let mut related = HashMap::<usize, Vec<LibraryReference>>::new();
        for (index, record) in document.value.records.iter().enumerate() {
            titles
                .entry(matching_title(
                    &record.item.title,
                    &record.item.authors,
                    true,
                ))
                .or_default()
                .push(index);
            let references = related.entry(index).or_default();
            for reference in record.item.references() {
                references.push(reference.clone());
                if let Some(other) = aliases.get(&(reference.source, reference.work_id.clone())) {
                    references.push(other.clone());
                }
            }
            for reference in references {
                known.insert(format!("{:?}:{}", reference.source, reference.work_id));
            }
        }
        for work in works {
            let key = format!("{:?}:{}", work.reference.source, work.reference.work_id);
            let existing = known.contains(&key);
            if !seen.insert(key) {
                return Err(error("VALIDATION_FAILED"));
            }
            if existing {
                continue;
            }
            let title = matching_title(&work.title, &work.authors, true);
            let Some(candidates) = titles.get(&title).filter(|_| !title.is_empty()) else {
                continue;
            };
            if let [index] = candidates.as_slice() {
                let record = &document.value.records[*index];
                // Explicit manual decisions, including an unlink, are never silently replaced.
                if !record.manual_override
                    && strong_library_match(&record.item, work)
                    && !related[index]
                        .iter()
                        .any(|v| v.source == work.reference.source && v != &work.reference)
                {
                    let target = (*index, work.reference.source);
                    *targets.entry(target).or_insert(0_usize) += 1;
                    proposals.push((*index, work.reference.clone()));
                }
            }
        }
        let mut linked = 0;
        for (index, reference) in proposals {
            if targets[&(index, reference.source)] != 1 {
                continue;
            }
            let record = &mut document.value.records[index];
            // A cached name or stale file is never enough to create an association.
            if verify_record(&root, record).is_err() {
                continue;
            }
            record.item.links.push(LibrarySourceLink {
                reference,
                evidence: LibraryLinkEvidence::TitleAuthorPages,
                linked_at: now(),
            });
            linked += 1;
        }
        root.verify()?;
        let result = if apply && linked > 0 {
            document.value.updated_at = Some(now());
            store.write_library(document.revision, document.value)?
        } else if !apply {
            store.read_library()?
        } else {
            document
        };
        Ok(LibraryReconcileResult {
            snapshot: snapshot(result, false),
            linked,
            examined: works.len(),
        })
    }

    /// Direct confirmation from the candidate card. A second source is added, never substituted.
    pub fn associate(
        &mut self,
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
        reference: LibraryReference,
    ) -> Result<LibrarySnapshot> {
        if !reference.is_valid() {
            return Err(error("VALIDATION_FAILED"));
        }
        let mut document = store.read_library()?;
        require_scope(&document.value, root_id, generation)?;
        if document.value.phase != LibraryPhase::Complete {
            return Err(error("LIBRARY_BUSY"));
        }
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        if document
            .value
            .records
            .iter()
            .any(|r| r.item.id != entry_id && r.item.references().any(|v| v == &reference))
        {
            return Err(error("LIBRARY_IDENTITY_CONFLICT"));
        }
        let record = document
            .value
            .records
            .iter_mut()
            .find(|r| r.item.id == entry_id)
            .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))?;
        verify_record(&root, record)?;
        if record.item.references().any(|v| v == &reference) {
            return Ok(snapshot(document, false));
        }
        if record
            .item
            .references()
            .any(|v| v.source == reference.source)
        {
            return Err(error("LIBRARY_IDENTITY_CONFLICT"));
        }
        record.item.links.push(LibrarySourceLink {
            reference,
            evidence: LibraryLinkEvidence::Manual,
            linked_at: now(),
        });
        document.value.updated_at = Some(now());
        root.verify()?;
        Ok(snapshot(
            store.write_library(document.revision, document.value)?,
            false,
        ))
    }
}

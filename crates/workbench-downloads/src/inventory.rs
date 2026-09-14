//! Source-specific ownership from completed downloads, never manga matching.
use crate::{presence, DownloadPhase, LocalFiles};
use serde::Serialize;
use std::collections::BTreeMap;
use workbench_storage::{Document, DownloadsDocument, LibraryDocument, Source};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInventoryItem {
    pub source: Source,
    pub work_id: String,
    pub library_entry_id: String,
    pub local_files: LocalFiles,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInventorySnapshot {
    pub revision: u64,
    pub library_revision: u64,
    pub root_id: Option<String>,
    pub items: Vec<DownloadInventoryItem>,
}

fn priority(state: LocalFiles) -> u8 {
    match state {
        LocalFiles::Present => 3,
        LocalFiles::Incomplete => 2,
        LocalFiles::Unavailable => 1,
        LocalFiles::Missing => 0,
    }
}

pub(crate) fn project(
    downloads: &Document<DownloadsDocument>,
    library: &Document<LibraryDocument>,
) -> DownloadInventorySnapshot {
    let mut items = BTreeMap::<String, DownloadInventoryItem>::new();
    let mut add = |item: DownloadInventoryItem| {
        let key = format!("{}:{}", crate::source_key(item.source), item.work_id);
        if items
            .get(&key)
            .is_none_or(|old| priority(item.local_files) > priority(old.local_files))
        {
            items.insert(key, item);
        }
    };
    for task in &downloads.value.tasks {
        if task.phase != DownloadPhase::Downloaded
            || library.value.root.as_ref() != Some(&task.root)
        {
            continue;
        }
        let Some(entry_id) = &task.library_entry_id else {
            continue;
        };
        add(DownloadInventoryItem {
            source: task.source,
            work_id: task.metadata.work_id.clone(),
            library_entry_id: presence::relocation(task, &library.value)
                .map_or(entry_id, |v| &v.new_item_id)
                .clone(),
            local_files: presence::check_with_library(task, &library.value),
        });
    }
    let records = library
        .value
        .records
        .iter()
        .map(|record| (record.item.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    for old in &downloads.value.history_evidence {
        if library.value.root.as_ref() != Some(&old.root) {
            continue;
        }
        let relocated = library.value.relocated_path(&old.destination);
        let entry_id = relocated.map_or(&old.library_entry_id, |v| &v.new_item_id);
        add(DownloadInventoryItem {
            source: old.source,
            work_id: old.work_id.clone(),
            library_entry_id: entry_id.clone(),
            local_files: presence::check_history(
                old,
                relocated,
                records.get(entry_id.as_str()).copied(),
            ),
        });
    }
    DownloadInventorySnapshot {
        revision: downloads.revision,
        library_revision: library.revision,
        root_id: library.value.root.as_ref().map(|root| root.id.clone()),
        items: items.into_values().collect(),
    }
}

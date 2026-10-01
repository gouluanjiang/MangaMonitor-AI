//! Minimal deliberate viewing history. No media, credentials or reading positions.
use crate::{library_hash_is_valid, model::ValidatedDocument, Document, LibraryReference,
    Result, Source, StoreError, WorkbenchStore, MAX_SAFE_INTEGER};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::{SystemTime, UNIX_EPOCH}};

const FILE: &str = "viewing-history.json";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum HistoryIdentity {
    Source { source: Source, work_id: String },
    Library { root_id: String, entry_id: String },
}
impl HistoryIdentity {
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Source { source, work_id } => LibraryReference { source: *source, work_id: work_id.clone() }.is_valid(),
            Self::Library { root_id, entry_id } => library_hash_is_valid(root_id) && library_hash_is_valid(entry_id),
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryEntry {
    pub identity: HistoryIdentity,
    pub title: String,
    pub visited_at: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewingHistory {
    pub version: u32,
    pub enabled: bool,
    pub entries: Vec<HistoryEntry>,
}
impl Default for ViewingHistory {
    fn default() -> Self { Self { version: 1, enabled: true, entries: vec![] } }
}
impl ValidatedDocument for ViewingHistory {
    fn validate(&self) -> Result<()> {
        if self.version != 1 { return Err(StoreError::new("UNSUPPORTED_SCHEMA")); }
        let mut seen = HashSet::new();
        if self.entries.len() > 100 || self.entries.iter().any(|entry| {
            !entry.identity.is_valid() || !seen.insert(&entry.identity)
                || entry.title.is_empty() || entry.title.chars().count() > 1024
                || entry.title.chars().any(char::is_control) || entry.visited_at > MAX_SAFE_INTEGER
        }) { return Err(StoreError::new("VALIDATION_FAILED")); }
        Ok(())
    }
}
impl WorkbenchStore {
    pub fn read_viewing_history(&self) -> Result<Document<ViewingHistory>> { self.read(FILE, MAX_BYTES) }
    fn edit_viewing_history(&self, edit: impl FnOnce(&mut ViewingHistory)) -> Result<Document<ViewingHistory>> {
        let _local = self.local_lock.lock().map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let mut document: Document<ViewingHistory> = self.read_unlocked(FILE, MAX_BYTES)?;
        edit(&mut document.value);
        self.write_unlocked(FILE, MAX_BYTES, document.revision, document.value)
    }
    pub fn set_viewing_history_enabled(&self, enabled: bool) -> Result<Document<ViewingHistory>> {
        self.edit_viewing_history(|value| value.enabled = enabled)
    }
    pub fn clear_viewing_history(&self) -> Result<Document<ViewingHistory>> {
        self.edit_viewing_history(|value| value.entries.clear())
    }
    pub fn record_viewing_history(&self, identity: HistoryIdentity, title: String) -> Result<Document<ViewingHistory>> {
        if !identity.is_valid() { return Err(StoreError::new("VALIDATION_FAILED")); }
        let visited_at = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?.as_millis())
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        // Display labels are optional presentation data, never identity evidence.
        let title: String = title.chars().filter(|c| !c.is_control()).take(1024).collect();
        let title = if title.trim().is_empty() { "作品".into() } else { title };
        self.edit_viewing_history(|value| {
            if !value.enabled { return; }
            value.entries.retain(|entry| entry.identity != identity);
            value.entries.insert(0, HistoryEntry { identity, title, visited_at });
            value.entries.truncate(100);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(id: usize) -> HistoryIdentity { HistoryIdentity::Source { source: Source::Jm, work_id: id.to_string() } }
    #[test]
    fn visits_are_bounded_persistent_and_independent_of_other_documents() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert!(store.read_viewing_history().unwrap().value.entries.is_empty());
        for id in 1..=105 { store.record_viewing_history(identity(id), format!("Work {id}")).unwrap(); }
        let result = store.record_viewing_history(identity(8), "Renamed".into()).unwrap();
        assert_eq!(result.value.entries.len(), 100);
        assert_eq!(result.value.entries[0].identity, identity(8));
        assert!(!result.value.entries.iter().any(|entry| entry.identity == identity(1)));
        store.set_viewing_history_enabled(false).unwrap();
        store.record_viewing_history(identity(500), "Not recorded".into()).unwrap();
        let reopened = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(reopened.read_viewing_history().unwrap().value.entries[0].identity, identity(8));
        assert!(!reopened.read_viewing_history().unwrap().value.enabled);
        reopened.clear_viewing_history().unwrap();
        assert!(store.read_viewing_history().unwrap().value.entries.is_empty());
        for file in ["library.json", "downloads.json", "following.json", "reader-progress.json"] {
            assert!(!root.path().join(crate::PRIVATE_DIRECTORY).join(file).exists());
        }
    }
    #[test]
    fn concurrent_visits_do_not_overwrite_and_unknown_versions_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            for id in 1..=10 {
                let path = root.path();
                scope.spawn(move || WorkbenchStore::open(path).unwrap().record_viewing_history(identity(id), "Work".into()).unwrap());
            }
        });
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(store.read_viewing_history().unwrap().value.entries.len(), 10);
        let path = root.path().join(crate::PRIVATE_DIRECTORY).join(FILE);
        let raw = std::fs::read_to_string(&path).unwrap().replace("\"version\":1", "\"version\":99");
        std::fs::write(&path, &raw).unwrap();
        assert!(store.clear_viewing_history().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    }
}

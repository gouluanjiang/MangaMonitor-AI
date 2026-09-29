//! Reading position only. This document is unrelated to acquisition, matching,
//! inventory and download history. Layout/zoom preferences are deliberately absent.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, Result, StoreError, WorkbenchStore,
    MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "reader-progress.json";
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_POSITIONS: usize = 20_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReaderPosition {
    pub chapter_id: String,
    pub page_index: u64,
    pub offset: f64,
}

impl ReaderPosition {
    pub fn is_valid(&self) -> bool {
        !self.chapter_id.is_empty()
            && self.chapter_id.len() <= 256
            && !self.chapter_id.chars().any(char::is_control)
            && self.page_index < 50_000
            && self.offset.is_finite()
            && (0.0..=1.0).contains(&self.offset)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedPosition {
    key: String,
    position: ReaderPosition,
    updated_at: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReaderProgress {
    version: u32,
    entries: Vec<SavedPosition>,
}

impl Default for ReaderProgress {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl ValidatedDocument for ReaderProgress {
    fn validate(&self) -> Result<()> {
        let mut keys = HashSet::new();
        if self.version != 1
            || self.entries.len() > MAX_POSITIONS
            || self.entries.iter().any(|entry| {
                !library_hash_is_valid(&entry.key)
                    || !keys.insert(&entry.key)
                    || !entry.position.is_valid()
                    || entry.updated_at > MAX_SAFE_INTEGER
            })
        {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn reader_position(&self, key: &str) -> Result<Option<ReaderPosition>> {
        if !library_hash_is_valid(key) {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let document = self.read::<ReaderProgress>(FILE, MAX_BYTES)?;
        Ok(document
            .value
            .entries
            .into_iter()
            .find(|e| e.key == key)
            .map(|e| e.position))
    }

    pub fn save_reader_position(&self, key: &str, position: ReaderPosition) -> Result<()> {
        if !library_hash_is_valid(key) || !position.is_valid() {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?
            .as_millis();
        let updated_at = u64::try_from(now)
            .ok()
            .filter(|n| *n <= MAX_SAFE_INTEGER)
            .ok_or(StoreError::new("STORE_UNAVAILABLE"))?;
        for _ in 0..3 {
            let mut document = self.read::<ReaderProgress>(FILE, MAX_BYTES)?;
            document.value.entries.retain(|e| e.key != key);
            if document.value.entries.len() == MAX_POSITIONS {
                let oldest = document
                    .value
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.updated_at)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                document.value.entries.remove(oldest);
            }
            document.value.entries.push(SavedPosition {
                key: key.into(),
                position: position.clone(),
                updated_at,
            });
            match self.write(FILE, MAX_BYTES, document.revision, document.value) {
                Ok(_) => return Ok(()),
                Err(problem) if problem.code == "REVISION_CONFLICT" => continue,
                Err(problem) => return Err(problem),
            }
        }
        Err(StoreError::new("REVISION_CONFLICT"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_is_independent_and_restores_only_book_coordinates() {
        let root = tempfile::tempdir().unwrap();
        let key = "a".repeat(64);
        let position = ReaderPosition {
            chapter_id: "chapter-2".into(),
            page_index: 12,
            offset: 0.25,
        };
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(store.reader_position(&key).unwrap(), None);
        store.save_reader_position(&key, position.clone()).unwrap();
        drop(store);
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(store.reader_position(&key).unwrap(), Some(position));
        let private = root.path().join(crate::PRIVATE_DIRECTORY);
        for name in ["library.json", "downloads.json", "preferences.json"] {
            assert!(!private.join(name).exists());
        }
        let raw = std::fs::read_to_string(private.join(FILE)).unwrap();
        assert!(!raw.contains("zoom") && !raw.contains("mode"));
    }
    #[test]
    fn invalid_position_and_corrupt_documents_do_not_reset_existing_progress() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        let key = "a".repeat(64);
        let mut position = ReaderPosition {
            chapter_id: "chapter".into(),
            page_index: 0,
            offset: 0.0,
        };
        for offset in [-1.0, 1.1, f64::NAN, f64::INFINITY] {
            position.offset = offset;
            assert!(store.save_reader_position(&key, position.clone()).is_err());
        }
        let path = root.path().join(crate::PRIVATE_DIRECTORY).join(FILE);
        std::fs::write(&path, b"{broken").unwrap();
        position.offset = 0.0;
        assert!(store.save_reader_position(&key, position).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
    }
}

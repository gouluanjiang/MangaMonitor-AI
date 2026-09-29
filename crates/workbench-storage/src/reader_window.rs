//! The last normal reader-window size, independent of book progress and accounts.
use crate::{model::ValidatedDocument, Result, StoreError, WorkbenchStore};
use serde::{Deserialize, Serialize};

const FILE: &str = "reader-window.json";
const MAX_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReaderWindowSize {
    pub width: f64,
    pub height: f64,
}

impl ReaderWindowSize {
    pub fn is_valid(&self) -> bool {
        [self.width, self.height]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0 && value <= 8192.0)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReaderWindowPreferences {
    version: u32,
    size: Option<ReaderWindowSize>,
}

impl Default for ReaderWindowPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            size: None,
        }
    }
}

impl ValidatedDocument for ReaderWindowPreferences {
    fn validate(&self) -> Result<()> {
        if self.version != 1 || self.size.is_some_and(|size| !size.is_valid()) {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn reader_window_size(&self) -> Result<Option<ReaderWindowSize>> {
        Ok(self
            .read::<ReaderWindowPreferences>(FILE, MAX_BYTES)?
            .value
            .size)
    }

    pub fn save_reader_window_size(&self, size: ReaderWindowSize) -> Result<()> {
        if !size.is_valid() {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        for _ in 0..3 {
            let document = self.read::<ReaderWindowPreferences>(FILE, MAX_BYTES)?;
            let next = ReaderWindowPreferences {
                version: 1,
                size: Some(size),
            };
            match self.write(FILE, MAX_BYTES, document.revision, next) {
                Ok(_) => return Ok(()),
                Err(error) if error.code == "REVISION_CONFLICT" => continue,
                Err(error) => return Err(error),
            }
        }
        Err(StoreError::new("REVISION_CONFLICT"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_size_survives_restart_without_pin_or_book_data() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(store.reader_window_size().unwrap(), None);
        let size = ReaderWindowSize {
            width: 402.5,
            height: 874.0,
        };
        store.save_reader_window_size(size).unwrap();
        drop(store);
        let store = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(store.reader_window_size().unwrap(), Some(size));
        let private = root.path().join(crate::PRIVATE_DIRECTORY);
        assert!(!private.join("reader-progress.json").exists());
        assert!(!private.join("preferences.json").exists());
        let raw = std::fs::read_to_string(private.join(FILE)).unwrap();
        assert!(!raw.contains("pin") && !raw.contains("root") && !raw.contains("position"));
    }

    #[test]
    fn invalid_sizes_and_damaged_settings_preserve_the_previous_document() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        let valid = ReaderWindowSize {
            width: 400.0,
            height: 870.0,
        };
        store.save_reader_window_size(valid).unwrap();
        for width in [0.0, -1.0, 8193.0, f64::NAN, f64::INFINITY] {
            assert!(store
                .save_reader_window_size(ReaderWindowSize { width, ..valid })
                .is_err());
            assert_eq!(store.reader_window_size().unwrap(), Some(valid));
        }
        let file = root.path().join(crate::PRIVATE_DIRECTORY).join(FILE);
        std::fs::write(&file, b"{damaged").unwrap();
        assert!(store.reader_window_size().is_err());
        assert!(store.save_reader_window_size(valid).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"{damaged");
    }
}

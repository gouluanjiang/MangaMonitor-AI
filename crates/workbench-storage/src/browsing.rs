//! Optional browse baselines, independent of catalogs, scan markers and unread state.
use crate::{model::ValidatedDocument, Document, Result, StoreError, WorkbenchStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_BROWSING_IDS: usize = 500_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowsingSurface {
    Recent,
    Authors,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowsingBaseline {
    pub known_ids: Vec<String>,
    /// Only a continuous recent-feed head, never an arbitrary catalog prefix.
    pub head_ids: Vec<String>,
    pub reached_end: bool,
}

impl BrowsingBaseline {
    fn validate(&self) -> Result<()> {
        let known: HashSet<_> = self.known_ids.iter().collect();
        let heads: HashSet<_> = self.head_ids.iter().collect();
        if self.known_ids.len() > MAX_BROWSING_IDS
            || self.head_ids.len() > 1000
            || known.len() != self.known_ids.len()
            || heads.len() != self.head_ids.len()
            || self.known_ids.iter().any(|id| {
                id.is_empty()
                    || id.len() > 128
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            })
            || self.head_ids.iter().any(|id| !known.contains(id))
        {
            return Err(StoreError::new("BROWSING_BASELINE_INVALID"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowsingMarkers {
    pub version: u32,
    pub baseline: Option<BrowsingBaseline>,
}

impl Default for BrowsingMarkers {
    fn default() -> Self {
        Self {
            version: 1,
            baseline: None,
        }
    }
}

impl ValidatedDocument for BrowsingMarkers {
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(StoreError::new("UNSUPPORTED_SCHEMA"));
        }
        if let Some(baseline) = &self.baseline {
            baseline.validate()?;
        }
        Ok(())
    }
}

fn filename(account_key: &str, surface: BrowsingSurface) -> Result<String> {
    // A hash derived by the authenticated account service, never a renderer path.
    if account_key.len() != 64
        || !account_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StoreError::new("BROWSING_ACCOUNT_INVALID"));
    }
    let surface = match surface {
        BrowsingSurface::Recent => "recent",
        BrowsingSurface::Authors => "authors",
    };
    Ok(format!("browsing-{account_key}-{surface}.json"))
}

impl WorkbenchStore {
    pub fn read_browsing_markers(
        &self,
        account_key: &str,
        surface: BrowsingSurface,
    ) -> Result<Document<BrowsingMarkers>> {
        self.read(&filename(account_key, surface)?, MAX_BYTES)
    }

    /// Edits exactly one native-resolved namespace. Unreadable/newer documents
    /// are preserved. The lease is checked after acquiring storage locks too.
    pub fn write_browsing_markers(
        &self,
        account_key: &str,
        surface: BrowsingSurface,
        baseline: BrowsingBaseline,
        require_current: impl FnOnce() -> Result<()>,
    ) -> Result<Document<BrowsingMarkers>> {
        baseline.validate()?;
        let name = filename(account_key, surface)?;
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let current: Document<BrowsingMarkers> = self.read_unlocked(&name, MAX_BYTES)?;
        require_current()?;
        if current.value.baseline.as_ref() == Some(&baseline) {
            return Ok(current);
        }
        self.write_unlocked(
            &name,
            MAX_BYTES,
            current.revision,
            BrowsingMarkers {
                version: 1,
                baseline: Some(baseline),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline(id: &str) -> BrowsingBaseline {
        BrowsingBaseline {
            known_ids: vec![id.into()],
            head_ids: vec![id.into()],
            reached_end: false,
        }
    }

    fn put(store: &WorkbenchStore, account: &str, surface: BrowsingSurface, id: &str) {
        store
            .write_browsing_markers(account, surface, baseline(id), || Ok(()))
            .unwrap();
    }

    fn get(store: &WorkbenchStore, account: &str, surface: BrowsingSurface) -> Option<BrowsingBaseline> {
        store
            .read_browsing_markers(account, surface)
            .unwrap()
            .value
            .baseline
    }

    #[test]
    fn account_and_surface_baselines_are_independent_and_reads_do_not_migrate() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        let account = "a".repeat(64);
        let other = "b".repeat(64);
        assert!(get(&store, &account, BrowsingSurface::Recent).is_none());
        assert!(!store
            .root
            .join(filename(&account, BrowsingSurface::Recent).unwrap())
            .exists());
        put(&store, &account, BrowsingSurface::Recent, "1");
        put(&store, &account, BrowsingSurface::Authors, "2");
        put(&store, &other, BrowsingSurface::Recent, "3");
        let reopened = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(
            get(&reopened, &account, BrowsingSurface::Recent),
            Some(baseline("1"))
        );
        assert_eq!(
            get(&reopened, &account, BrowsingSurface::Authors),
            Some(baseline("2"))
        );
        assert_eq!(
            get(&reopened, &other, BrowsingSurface::Recent),
            Some(baseline("3"))
        );
        for file in [
            "library.json",
            "downloads.json",
            "following.json",
            "special.json",
        ] {
            assert!(!store.root.join(file).exists());
        }
    }

    #[test]
    fn failed_lease_invalid_input_and_future_schema_preserve_bytes() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        let account = "a".repeat(64);
        put(&store, &account, BrowsingSurface::Recent, "1");
        let path = store
            .root
            .join(filename(&account, BrowsingSurface::Recent).unwrap());
        let original = std::fs::read(&path).unwrap();
        let error = store
            .write_browsing_markers(&account, BrowsingSurface::Recent, baseline("2"), || {
                Err(StoreError::new("SESSION_CHANGED"))
            })
            .unwrap_err();
        assert_eq!(error.code, "SESSION_CHANGED");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(store
            .write_browsing_markers("../escape", BrowsingSurface::Recent, baseline("1"), || Ok(()))
            .is_err());
        assert!(store
            .write_browsing_markers(&account, BrowsingSurface::Recent, baseline("../bad"), || Ok(()))
            .is_err());
        let future = String::from_utf8(original)
            .unwrap()
            .replace("\"version\":1", "\"version\":99");
        std::fs::write(&path, &future).unwrap();
        assert_eq!(
            store
                .write_browsing_markers(&account, BrowsingSurface::Recent, baseline("3"), || Ok(()))
                .unwrap_err()
                .code,
            "UNSUPPORTED_SCHEMA"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), future);
    }

    #[test]
    fn bounds_and_head_membership_reject_incomplete_replacements() {
        let mut value = baseline("1");
        value.head_ids.push("2".into());
        assert!(value.validate().is_err());
        value.head_ids = vec!["1".into(), "1".into()];
        assert!(value.validate().is_err());
        value.head_ids.clear();
        value.known_ids = (0..=MAX_BROWSING_IDS).map(|id| id.to_string()).collect();
        assert!(value.validate().is_err());
    }
}

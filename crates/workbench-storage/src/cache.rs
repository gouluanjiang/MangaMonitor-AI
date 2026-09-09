//! Bounded catalog storage and removal of the application's retired cover slots.
use crate::{
    store::{
        check_directory_tree, check_open_regular, check_optional_regular, read_regular_bounded,
        safe_options,
    },
    Result, StoreError, WorkbenchStore,
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

const CATALOG_LIMIT: usize = 32 * 1024 * 1024;
const INDEX_LIMIT: usize = 64 * 1024;
const GLOBAL_LIMIT: u64 = 256 * 1024 * 1024;
const ACCOUNT_LIMIT: usize = 20;
const LEGACY_COVER_RESERVATION: u64 = 64 * 1024 * 1024;
const REGISTRY: &str = "cache-registry-v1.json";
const SCRATCH: &str = ".cache-transaction.tmp";

#[derive(Clone, Copy)]
pub enum CacheEntry {
    Catalog,
}
impl CacheEntry {
    fn name(self, account: &str) -> String {
        format!("cache-{account}-catalog.json")
    }
}
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registry {
    version: u32,
    accounts: Vec<AccountBudget>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountBudget {
    key: String,
    catalog_peak: u64,
    cover_peak: u64,
    used_at: u64,
}
fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?
            .as_millis(),
    )
    .map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))
}
fn legacy_cover_names(key: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::once(format!("cache-{key}-covers.json"))
        .chain((0..255).map(move |slot| format!("cache-{key}-cover-{slot:03}.bin")))
}

pub struct AccountCache<'a> {
    store: &'a WorkbenchStore,
    key: String,
    budget: Option<u64>,
}
impl WorkbenchStore {
    /// Native-only transaction. The account key comes from a verified profile.
    pub fn with_account_cache<T>(
        &self,
        account_key: &str,
        operation: impl FnOnce(&mut AccountCache<'_>) -> Result<T>,
    ) -> Result<T> {
        if !valid_key(account_key) {
            return Err(StoreError::new("CACHE_INPUT_INVALID"));
        }
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        operation(&mut AccountCache {
            store: self,
            key: account_key.into(),
            budget: None,
        })
    }

    /// Retire only fixed cover filenames identified by the bounded old registry.
    /// No directory enumeration, recursion, or renderer-provided path is accepted.
    /// A completed account has coverPeak zero; old versions can reserve it again.
    pub fn cleanup_legacy_cover_cache(&self) -> Result<()> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let mut registry = self.read_cache_registry()?;
        // The old single scratch slot can contain an interrupted cover write.
        // It is never valid catalog state and is safe to unlink under this lock.
        self.remove_cache_file(SCRATCH)?;
        for index in 0..registry.accounts.len() {
            if registry.accounts[index].cover_peak == 0 {
                continue;
            }
            let key = &registry.accounts[index].key;
            // Preflight the complete, fixed set before deleting any of this account.
            for name in legacy_cover_names(key) {
                check_directory_tree(&self.root)?;
                check_optional_regular(&self.root.join(name))?;
            }
            for name in legacy_cover_names(key) {
                self.remove_cache_file(&name)?;
            }
            registry.accounts[index].cover_peak = 0;
            // Commit only after every slot of this account was removed. Failed
            // or interrupted cleanup keeps its reservation and can retry safely.
            self.write_cache_file(
                REGISTRY,
                &serde_json::to_vec(&registry).map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?,
            )?;
        }
        Ok(())
    }

    fn read_cache_registry(&self) -> Result<Registry> {
        let registry = match self.read_cache_file(REGISTRY, INDEX_LIMIT)? {
            None => Registry {
                version: 1,
                accounts: vec![],
            },
            Some(bytes) => serde_json::from_slice::<Registry>(&bytes)
                .map_err(|_| StoreError::new("CACHE_CORRUPT"))?,
        };
        let mut keys = HashSet::new();
        if registry.version != 1
            || registry.accounts.len() > ACCOUNT_LIMIT
            || registry.accounts.iter().any(|a| {
                !valid_key(&a.key)
                    || !keys.insert(&a.key)
                    || a.catalog_peak > CATALOG_LIMIT as u64
                    || ![0, LEGACY_COVER_RESERVATION].contains(&a.cover_peak)
                    || a.used_at > crate::MAX_SAFE_INTEGER
            })
        {
            return Err(StoreError::new("CACHE_CORRUPT"));
        }
        Ok(registry)
    }
    fn read_cache_file(&self, name: &str, limit: usize) -> Result<Option<Vec<u8>>> {
        check_directory_tree(&self.root)?;
        let path = self.root.join(name);
        if check_optional_regular(&path)?.is_none() {
            return Ok(None);
        }
        read_regular_bounded(&path, limit).map(Some)
    }
    fn remove_cache_file(&self, name: &str) -> Result<()> {
        check_directory_tree(&self.root)?;
        let path = self.root.join(name);
        if check_optional_regular(&path)?.is_some() {
            // All callers generate one fixed basename; remove_file unlinks that
            // name only and never recursively traverses another directory.
            fs::remove_file(path).map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
        }
        Ok(())
    }
    fn write_cache_file(&self, name: &str, bytes: &[u8]) -> Result<()> {
        check_directory_tree(&self.root)?;
        let destination = self.root.join(name);
        let temporary = self.root.join(SCRATCH);
        check_optional_regular(&destination)?;
        self.remove_cache_file(SCRATCH)?;
        let result = (|| {
            let mut file = safe_options()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
            check_open_regular(&file)?;
            file.write_all(bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
            drop(file);
            check_directory_tree(&self.root)?;
            check_optional_regular(&destination)?;
            check_optional_regular(&temporary)?;
            fs::rename(&temporary, &destination)
                .map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
            #[cfg(unix)]
            File::open(&self.root)
                .and_then(|file| file.sync_all())
                .map_err(|_| StoreError::new("COMMIT_UNCERTAIN"))?;
            Ok(())
        })();
        if result.is_err() && check_optional_regular(&temporary).ok().flatten().is_some() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}
impl AccountCache<'_> {
    pub fn read(&self, entry: CacheEntry) -> Result<Option<Vec<u8>>> {
        self.store
            .read_cache_file(&entry.name(&self.key), CATALOG_LIMIT)
    }
    /// Reserve the catalog high-water mark before writing. Legacy cover bytes
    /// remain accounted for until the explicit startup cleanup has succeeded.
    pub fn reserve(&mut self, catalog_bytes: u64) -> Result<()> {
        if catalog_bytes > CATALOG_LIMIT as u64 {
            return Err(StoreError::new("CACHE_TOO_LARGE"));
        }
        let mut registry = self.store.read_cache_registry()?;
        if let Some(account) = registry.accounts.iter_mut().find(|a| a.key == self.key) {
            account.catalog_peak = account.catalog_peak.max(catalog_bytes);
            account.used_at = now()?;
        } else {
            registry.accounts.push(AccountBudget {
                key: self.key.clone(),
                catalog_peak: catalog_bytes,
                cover_peak: 0,
                used_at: now()?,
            });
        }
        while registry.accounts.len() > ACCOUNT_LIMIT
            || registry
                .accounts
                .iter()
                .map(|a| a.catalog_peak + a.cover_peak)
                .sum::<u64>()
                > GLOBAL_LIMIT
        {
            // Do not orphan legacy cover files by forgetting their only known
            // account key after a failed cleanup. Such scopes remain reserved.
            let oldest = registry
                .accounts
                .iter()
                .enumerate()
                .filter(|(_, a)| a.key != self.key && a.cover_peak == 0)
                .min_by_key(|(_, a)| a.used_at)
                .map(|(i, _)| i)
                .ok_or(StoreError::new("CACHE_TOO_LARGE"))?;
            self.store
                .remove_cache_file(&CacheEntry::Catalog.name(&registry.accounts[oldest].key))?;
            registry.accounts.remove(oldest);
        }
        self.store.write_cache_file(
            REGISTRY,
            &serde_json::to_vec(&registry).map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?,
        )?;
        self.budget = Some(
            registry
                .accounts
                .iter()
                .find(|a| a.key == self.key)
                .ok_or(StoreError::new("CACHE_UNAVAILABLE"))?
                .catalog_peak,
        );
        Ok(())
    }
    pub fn write(&self, entry: CacheEntry, bytes: &[u8]) -> Result<()> {
        if bytes.len() > CATALOG_LIMIT {
            return Err(StoreError::new("CACHE_TOO_LARGE"));
        }
        if bytes.len() as u64
            > self
                .budget
                .ok_or(StoreError::new("CACHE_RESERVATION_REQUIRED"))?
        {
            return Err(StoreError::new("CACHE_RESERVATION_REQUIRED"));
        }
        self.store.write_cache_file(&entry.name(&self.key), bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn key(n: u8) -> String {
        format!("{n:064x}")
    }
    fn registry(store: &WorkbenchStore) -> Registry {
        serde_json::from_slice(&fs::read(store.root.join(REGISTRY)).unwrap()).unwrap()
    }
    fn seed_legacy(store: &WorkbenchStore) -> String {
        let key = key(1);
        let registry = Registry {
            version: 1,
            accounts: vec![AccountBudget {
                key: key.clone(),
                catalog_peak: 123,
                cover_peak: LEGACY_COVER_RESERVATION,
                used_at: 1,
            }],
        };
        fs::write(
            store.root.join(REGISTRY),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();
        key
    }
    #[test]
    fn fixed_catalog_requires_reservation_and_survives_reopen() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                assert_eq!(
                    cache.write(CacheEntry::Catalog, b"data").unwrap_err().code,
                    "CACHE_RESERVATION_REQUIRED"
                );
                cache.reserve(4)?;
                cache.write(CacheEntry::Catalog, b"data")
            })
            .unwrap();
        drop(store);
        let reopened = WorkbenchStore::open(temp.path()).unwrap();
        assert_eq!(
            reopened
                .with_account_cache(&key(1), |cache| cache.read(CacheEntry::Catalog))
                .unwrap(),
            Some(b"data".to_vec())
        );
        assert_eq!(
            reopened
                .with_account_cache("../../outside", |_| Ok(()))
                .unwrap_err()
                .code,
            "CACHE_INPUT_INVALID"
        );
    }
    #[test]
    fn catalog_high_water_reservations_keep_the_global_budget_and_documents() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        for n in 1..=9 {
            store
                .with_account_cache(&key(n), |cache| {
                    cache.reserve(CATALOG_LIMIT as u64)?;
                    cache.write(CacheEntry::Catalog, b"catalog")
                })
                .unwrap();
        }
        assert_eq!(registry(&store).accounts.len(), 8);
        assert_eq!(
            registry(&store)
                .accounts
                .iter()
                .map(|a| a.catalog_peak)
                .sum::<u64>(),
            GLOBAL_LIMIT
        );
        assert!(registry(&store).accounts.iter().all(|a| a.cover_peak == 0));
        assert!(store
            .with_account_cache(&key(1), |cache| cache.read(CacheEntry::Catalog))
            .unwrap()
            .is_none());
        assert_eq!(store.read_preferences().unwrap().revision, 0);
        assert_eq!(store.read_booklists().unwrap().revision, 0);
    }
    #[test]
    fn stale_hardlinked_scratch_never_truncates_its_other_name() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let unrelated = temp.path().join("unrelated.txt");
        fs::write(&unrelated, b"preserve-unrelated").unwrap();
        fs::hard_link(&unrelated, store.root.join(SCRATCH)).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                cache.reserve(2)?;
                cache.write(CacheEntry::Catalog, b"ok")
            })
            .unwrap();
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve-unrelated");
        assert!(!store.root.join(SCRATCH).exists());
    }
    #[test]
    fn simultaneous_instances_share_the_document_lock_without_lost_transactions() {
        let temp = TempDir::new().unwrap();
        let first = WorkbenchStore::open(temp.path()).unwrap();
        let second = WorkbenchStore::open(temp.path()).unwrap();
        first
            .with_account_cache(&key(1), |cache| {
                assert_eq!(
                    second
                        .with_account_cache(&key(2), |_| Ok(()))
                        .unwrap_err()
                        .code,
                    "BUSY"
                );
                assert_eq!(
                    second.cleanup_legacy_cover_cache().unwrap_err().code,
                    "BUSY"
                );
                cache.reserve(3)?;
                cache.write(CacheEntry::Catalog, b"one")
            })
            .unwrap();
        second
            .with_account_cache(&key(2), |cache| {
                cache.reserve(3)?;
                cache.write(CacheEntry::Catalog, b"two")
            })
            .unwrap();
        assert_eq!(registry(&first).accounts.len(), 2);
    }
    #[test]
    fn legacy_cleanup_deletes_only_registered_fixed_cover_slots_and_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = seed_legacy(&store);
        for name in legacy_cover_names(&key) {
            fs::write(store.root.join(name), b"retired").unwrap();
        }
        let preserved = [
            CacheEntry::Catalog.name(&key),
            "preferences.json".into(),
            "booklists.json".into(),
            "following.json".into(),
            format!("cache-{key}-cover-255.bin"),
            format!("cache-{:064x}-cover-000.bin", 2),
        ];
        for name in &preserved {
            fs::write(store.root.join(name), name.as_bytes()).unwrap();
        }
        fs::write(store.root.join(SCRATCH), b"interrupted-cover").unwrap();
        let outside = temp.path().join("library-sentinel");
        fs::write(&outside, b"preserve").unwrap();
        store.cleanup_legacy_cover_cache().unwrap();
        for name in legacy_cover_names(&key) {
            assert!(!store.root.join(name).exists());
        }
        for name in &preserved {
            assert_eq!(fs::read(store.root.join(name)).unwrap(), name.as_bytes());
        }
        assert_eq!(fs::read(outside).unwrap(), b"preserve");
        assert!(!store.root.join(SCRATCH).exists());
        let first = fs::read(store.root.join(REGISTRY)).unwrap();
        assert_eq!(registry(&store).accounts[0].cover_peak, 0);
        assert_eq!(registry(&store).accounts[0].catalog_peak, 123);
        store.cleanup_legacy_cover_cache().unwrap();
        assert_eq!(fs::read(store.root.join(REGISTRY)).unwrap(), first);
        // A downgraded version can reserve and write again; there is no forever marker.
        seed_legacy(&store);
        fs::write(
            store.root.join(format!("cache-{key}-cover-001.bin")),
            b"old-version",
        )
        .unwrap();
        store.cleanup_legacy_cover_cache().unwrap();
        assert!(!store
            .root
            .join(format!("cache-{key}-cover-001.bin"))
            .exists());
    }
    #[test]
    fn interrupted_or_missing_slots_retry_and_corrupt_registry_is_preserved() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = seed_legacy(&store);
        // Earlier slots are already gone after an interrupted process.
        let last = store.root.join(format!("cache-{key}-cover-254.bin"));
        fs::write(&last, b"remaining").unwrap();
        store.cleanup_legacy_cover_cache().unwrap();
        assert!(!last.exists());
        seed_legacy(&store);
        fs::write(&last, b"preserve-on-error").unwrap();
        for invalid in [
            b"{broken".as_slice(),
            br#"{"version":2,"accounts":[]}"#.as_slice(),
        ] {
            fs::write(store.root.join(REGISTRY), invalid).unwrap();
            assert_eq!(
                store.cleanup_legacy_cover_cache().unwrap_err().code,
                "CACHE_CORRUPT"
            );
            assert_eq!(fs::read(&last).unwrap(), b"preserve-on-error");
            assert_eq!(fs::read(store.root.join(REGISTRY)).unwrap(), invalid);
        }
    }
    #[test]
    fn unsafe_legacy_slot_keeps_the_account_reservation_and_prevents_partial_cleanup() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = seed_legacy(&store);
        let index = store.root.join(format!("cache-{key}-covers.json"));
        fs::write(&index, b"index").unwrap();
        let unsafe_slot = store.root.join(format!("cache-{key}-cover-254.bin"));
        fs::create_dir(&unsafe_slot).unwrap();
        assert_eq!(
            store.cleanup_legacy_cover_cache().unwrap_err().code,
            "UNSAFE_PATH"
        );
        assert_eq!(fs::read(&index).unwrap(), b"index");
        assert!(unsafe_slot.is_dir());
        assert_eq!(
            registry(&store).accounts[0].cover_peak,
            LEGACY_COVER_RESERVATION
        );
        // Later catalog pressure must not forget the failed legacy account and
        // orphan its cover slots; only clean catalog accounts can be evicted.
        for n in 2..=9 {
            store
                .with_account_cache(&format!("{n:064x}"), |cache| {
                    cache.reserve(CATALOG_LIMIT as u64)?;
                    cache.write(CacheEntry::Catalog, b"catalog")
                })
                .unwrap();
        }
        let budgets = registry(&store);
        assert!(budgets.accounts.iter().any(|account| {
            account.key == key && account.cover_peak == LEGACY_COVER_RESERVATION
        }));
        assert!(
            budgets
                .accounts
                .iter()
                .map(|a| a.catalog_peak + a.cover_peak)
                .sum::<u64>()
                <= GLOBAL_LIMIT
        );
        assert_eq!(fs::read(index).unwrap(), b"index");
        assert!(unsafe_slot.is_dir());
    }
    #[cfg(unix)]
    #[test]
    fn catalog_and_legacy_cover_symlinks_never_redirect_operations() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = seed_legacy(&store);
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, store.root.join(CacheEntry::Catalog.name(&key)))
            .unwrap();
        assert!(store
            .with_account_cache(&key, |cache| cache.read(CacheEntry::Catalog))
            .is_err());
        std::os::unix::fs::symlink(
            &outside,
            store.root.join(format!("cache-{key}-covers.json")),
        )
        .unwrap();
        assert_eq!(
            store.cleanup_legacy_cover_cache().unwrap_err().code,
            "UNSAFE_PATH"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert_eq!(
            registry(&store).accounts[0].cover_peak,
            LEGACY_COVER_RESERVATION
        );
    }
}

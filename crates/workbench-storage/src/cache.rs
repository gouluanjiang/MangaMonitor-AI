//! Disposable UI caches. Fixed filenames and bounded storage never touch user documents.
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
const COVER_LIMIT: usize = 256 * 1024 + 32;
const INDEX_LIMIT: usize = 64 * 1024;
const GLOBAL_LIMIT: u64 = 256 * 1024 * 1024;
const ACCOUNT_LIMIT: usize = 20;
const REGISTRY: &str = "cache-registry-v1.json";

#[derive(Clone, Copy)]
pub enum CacheEntry {
    Catalog,
    CoverIndex,
    Cover(u8),
}

impl CacheEntry {
    fn name(self, account: &str) -> Result<String> {
        Ok(match self {
            Self::Catalog => format!("cache-{account}-catalog.json"),
            Self::CoverIndex => format!("cache-{account}-covers.json"),
            Self::Cover(slot) if slot < 255 => format!("cache-{account}-cover-{slot:03}.bin"),
            Self::Cover(_) => return Err(StoreError::new("CACHE_INPUT_INVALID")),
        })
    }
    fn limit(self) -> usize {
        match self {
            Self::Catalog => CATALOG_LIMIT,
            Self::CoverIndex => INDEX_LIMIT,
            Self::Cover(_) => COVER_LIMIT,
        }
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

pub struct AccountCache<'a> {
    store: &'a WorkbenchStore,
    key: String,
    budget: Option<(u64, u64)>,
}

impl WorkbenchStore {
    /// Native-only transaction. The account key is derived from a verified profile.
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
}

impl AccountCache<'_> {
    pub fn read(&self, entry: CacheEntry) -> Result<Option<Vec<u8>>> {
        self.read_name(&entry.name(&self.key)?, entry.limit())
    }

    /// Reserve the account's observed high-water marks before writing. Conservative
    /// reservation also bounds interrupted writes without depending on an index commit.
    pub fn reserve(&mut self, catalog_bytes: u64, cover_bytes: u64) -> Result<()> {
        if catalog_bytes > CATALOG_LIMIT as u64 || cover_bytes > 64 * 1024 * 1024 {
            return Err(StoreError::new("CACHE_TOO_LARGE"));
        }
        let cover_bytes = if cover_bytes > 0 { 64 * 1024 * 1024 } else { 0 };
        let mut registry = match self.read_name(REGISTRY, INDEX_LIMIT)? {
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
                    || ![0, 64 * 1024 * 1024].contains(&a.cover_peak)
                    || a.used_at > crate::MAX_SAFE_INTEGER
            })
        {
            return Err(StoreError::new("CACHE_CORRUPT"));
        }
        if let Some(account) = registry.accounts.iter_mut().find(|a| a.key == self.key) {
            account.catalog_peak = account.catalog_peak.max(catalog_bytes);
            account.cover_peak = account.cover_peak.max(cover_bytes);
            account.used_at = now()?;
        } else {
            registry.accounts.push(AccountBudget {
                key: self.key.clone(),
                catalog_peak: catalog_bytes,
                cover_peak: cover_bytes,
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
            let oldest = registry
                .accounts
                .iter()
                .enumerate()
                .filter(|(_, a)| a.key != self.key)
                .min_by_key(|(_, a)| a.used_at)
                .map(|(index, _)| index)
                .ok_or(StoreError::new("CACHE_TOO_LARGE"))?;
            self.remove_account(&registry.accounts[oldest].key)?;
            registry.accounts.remove(oldest);
        }
        self.write_name(
            REGISTRY,
            &serde_json::to_vec(&registry).map_err(|_| StoreError::new("CACHE_UNAVAILABLE"))?,
        )?;
        let current = registry
            .accounts
            .iter()
            .find(|a| a.key == self.key)
            .ok_or(StoreError::new("CACHE_UNAVAILABLE"))?;
        self.budget = Some((current.catalog_peak, current.cover_peak));
        Ok(())
    }

    pub fn write(&self, entry: CacheEntry, bytes: &[u8]) -> Result<()> {
        if bytes.len() > entry.limit() {
            return Err(StoreError::new("CACHE_TOO_LARGE"));
        }
        let budget = self
            .budget
            .ok_or(StoreError::new("CACHE_RESERVATION_REQUIRED"))?;
        let reserved = match entry {
            CacheEntry::Catalog => budget.0,
            _ => budget.1,
        };
        if bytes.len() as u64 > reserved {
            return Err(StoreError::new("CACHE_RESERVATION_REQUIRED"));
        }
        self.write_name(&entry.name(&self.key)?, bytes)
    }

    fn read_name(&self, name: &str, limit: usize) -> Result<Option<Vec<u8>>> {
        check_directory_tree(&self.store.root)?;
        let path = self.store.root.join(name);
        if check_optional_regular(&path)?.is_none() {
            return Ok(None);
        }
        read_regular_bounded(&path, limit).map(Some)
    }

    fn remove_account(&self, key: &str) -> Result<()> {
        // Whole-account eviction touches only these fixed cache slots, never following,
        // preferences, booklists or any media library. No filesystem enumeration.
        for entry in [CacheEntry::Catalog, CacheEntry::CoverIndex]
            .into_iter()
            .chain((0..255).map(CacheEntry::Cover))
        {
            check_directory_tree(&self.store.root)?;
            let path = self.store.root.join(entry.name(key)?);
            if check_optional_regular(&path)?.is_some() {
                fs::remove_file(path).map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
            }
        }
        Ok(())
    }

    fn write_name(&self, name: &str, bytes: &[u8]) -> Result<()> {
        check_directory_tree(&self.store.root)?;
        let destination = self.store.root.join(name);
        // One fixed scratch slot for all cache transactions; it is never read as state.
        let temporary = self.store.root.join(".cache-transaction.tmp");
        check_optional_regular(&destination)?;
        if check_optional_regular(&temporary)?.is_some() {
            // Unlink only this stale scratch name: a pre-existing hard link must
            // never let truncation modify its other, unrelated filename.
            fs::remove_file(&temporary).map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
        }
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
            check_directory_tree(&self.store.root)?;
            check_optional_regular(&destination)?;
            check_optional_regular(&temporary)?;
            fs::rename(&temporary, &destination)
                .map_err(|_| StoreError::new("CACHE_WRITE_FAILED"))?;
            #[cfg(unix)]
            File::open(&self.store.root)
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

    #[test]
    fn fixed_cache_slots_require_reservation_and_survive_reopen() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                assert_eq!(
                    cache.write(CacheEntry::Catalog, b"data").unwrap_err().code,
                    "CACHE_RESERVATION_REQUIRED"
                );
                cache.reserve(4, 0)?;
                cache.write(CacheEntry::Catalog, b"data")
            })
            .unwrap();
        drop(store);
        let reopened = WorkbenchStore::open(temp.path()).unwrap();
        reopened
            .with_account_cache(&key(1), |cache| {
                assert_eq!(cache.read(CacheEntry::Catalog)?, Some(b"data".to_vec()));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            reopened
                .with_account_cache("../../outside", |_| Ok(()))
                .unwrap_err()
                .code,
            "CACHE_INPUT_INVALID"
        );
        assert_eq!(
            reopened
                .with_account_cache(&key(1), |cache| cache.read(CacheEntry::Cover(255)))
                .unwrap_err()
                .code,
            "CACHE_INPUT_INVALID"
        );
    }

    #[test]
    fn interrupted_cover_index_still_reserves_every_slot_and_bounds_other_accounts() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let first = key(1);
        // Simulate successful slot replacement followed by a failed index commit.
        let failed = store.with_account_cache(&first, |cache| {
            cache.reserve(0, 1)?;
            cache.write(CacheEntry::CoverIndex, b"old-index")?;
            cache.write(CacheEntry::Cover(0), &vec![42; COVER_LIMIT])?;
            Err::<(), _>(StoreError::new("CACHE_WRITE_FAILED"))
        });
        assert_eq!(failed.unwrap_err().code, "CACHE_WRITE_FAILED");
        store
            .with_account_cache(&first, |cache| {
                assert_eq!(
                    cache.read(CacheEntry::CoverIndex)?,
                    Some(b"old-index".to_vec())
                );
                cache.reserve(0, 1)?;
                cache.write(CacheEntry::Cover(1), &vec![7; COVER_LIMIT])
            })
            .unwrap();
        assert_eq!(registry(&store).accounts[0].cover_peak, 64 * 1024 * 1024);
        for n in 2..=4 {
            store
                .with_account_cache(&key(n), |cache| {
                    cache.reserve(0, 1)?;
                    cache.write(CacheEntry::Cover(0), b"small")
                })
                .unwrap();
        }
        assert_eq!(
            registry(&store)
                .accounts
                .iter()
                .map(|a| a.catalog_peak + a.cover_peak)
                .sum::<u64>(),
            GLOBAL_LIMIT
        );
        store
            .with_account_cache(&key(5), |cache| {
                cache.reserve(0, 1)?;
                cache.write(CacheEntry::Cover(0), b"fifth")
            })
            .unwrap();
        assert_eq!(registry(&store).accounts.len(), 4);
        assert!(!registry(&store).accounts.iter().any(|a| a.key == first));
        store
            .with_account_cache(&first, |cache| {
                assert!(cache.read(CacheEntry::Cover(0))?.is_none());
                assert!(cache.read(CacheEntry::Cover(1))?.is_none());
                assert!(cache.read(CacheEntry::CoverIndex)?.is_none());
                Ok(())
            })
            .unwrap();
        assert_eq!(store.read_preferences().unwrap().revision, 0);
        assert_eq!(store.read_booklists().unwrap().revision, 0);
    }

    #[test]
    fn stale_hardlinked_scratch_never_truncates_its_other_name() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let unrelated = temp.path().join("unrelated.txt");
        fs::write(&unrelated, b"preserve-unrelated").unwrap();
        fs::hard_link(&unrelated, store.root.join(".cache-transaction.tmp")).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                cache.reserve(2, 0)?;
                cache.write(CacheEntry::Catalog, b"ok")
            })
            .unwrap();
        assert_eq!(fs::read(&unrelated).unwrap(), b"preserve-unrelated");
        assert!(!store.root.join(".cache-transaction.tmp").exists());
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
                cache.reserve(3, 0)?;
                cache.write(CacheEntry::Catalog, b"one")
            })
            .unwrap();
        second
            .with_account_cache(&key(2), |cache| {
                cache.reserve(3, 0)?;
                cache.write(CacheEntry::Catalog, b"two")
            })
            .unwrap();
        assert_eq!(registry(&first).accounts.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn cache_symlinks_are_never_read_written_or_evicted() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside").unwrap();
        let linked = store.root.join(CacheEntry::Catalog.name(&key(1)).unwrap());
        std::os::unix::fs::symlink(&outside, &linked).unwrap();
        assert!(store
            .with_account_cache(&key(1), |cache| cache.read(CacheEntry::Catalog))
            .is_err());
        assert!(store
            .with_account_cache(&key(1), |cache| {
                cache.reserve(2, 0)?;
                cache.write(CacheEntry::Catalog, b"no")
            })
            .is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
    }
}

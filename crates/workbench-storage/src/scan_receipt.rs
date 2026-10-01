//! Last successful manual scan receipt; independent of unread special updates.
use crate::{library_hash_is_valid, model::ValidatedDocument, DiscoveryCheckPhase,
    DiscoveryCheckSummary, Result, StoreError, WorkbenchStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
const FILE: &str = "scan-additions.json";
const MAX_BYTES: usize = 128 * 1024;
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountReceipt { account_key: String, summary: Option<DiscoveryCheckSummary> }
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScanReceipts { version: u32, accounts: Vec<AccountReceipt> }
impl Default for ScanReceipts {
    fn default() -> Self { Self {version:1,accounts:vec![]} }
}
impl ValidatedDocument for ScanReceipts {
    fn validate(&self) -> Result<()> {
        if self.version != 1 { return Err(StoreError::new("UNSUPPORTED_SCHEMA")); }
        let mut keys = HashSet::new();
        if self.accounts.len() > 20 || self.accounts.iter().any(|row| {
            !library_hash_is_valid(&row.account_key) || !keys.insert(&row.account_key)
                || row.summary.as_ref().is_some_and(|summary| !summary.is_valid() || summary.phase != DiscoveryCheckPhase::Complete)
        }) { return Err(StoreError::new("VALIDATION_FAILED")); }
        Ok(())
    }
}
impl WorkbenchStore {
    pub fn read_successful_scan(&self, account_key: &str) -> Result<Option<DiscoveryCheckSummary>> {
        if !library_hash_is_valid(account_key) {return Err(StoreError::new("VALIDATION_FAILED"));}
        Ok(self.read::<ScanReceipts>(FILE,MAX_BYTES)?.value.accounts.into_iter()
            .find(|row| row.account_key == account_key).and_then(|row| row.summary))
    }
    pub fn save_successful_scan(&self, account_key: &str, summary: DiscoveryCheckSummary) -> Result<()> {
        if !library_hash_is_valid(account_key) || !summary.is_valid() || summary.phase != DiscoveryCheckPhase::Complete {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let _local = self.local_lock.lock().map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let mut document = self.read_unlocked::<ScanReceipts>(FILE,MAX_BYTES)?;
        if let Some(existing) = document.value.accounts.iter_mut().find(|row| row.account_key == account_key) {
            if existing.summary.as_ref().is_some_and(|saved| saved.started_at > summary.started_at) { return Ok(()); }
            existing.summary = Some(summary);
        } else {
            document.value.accounts.push(AccountReceipt {account_key:account_key.into(),summary:Some(summary)});
        }
        self.write_unlocked(FILE,MAX_BYTES,document.revision,document.value)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn summary(id: &str, time: u64) -> DiscoveryCheckSummary {
        DiscoveryCheckSummary {id:id.repeat(64),started_at:time,finished_at:Some(time+1),phase:DiscoveryCheckPhase::Complete,
            mode:crate::DiscoveryMode::Full,only_unfinished:false,first_catalog:false,all_followed:true,
            author_count:1,total_scopes:2,attempted_scopes:2,complete_scopes:2}
    }
    #[test]
    fn only_success_replaces_markers_and_late_receipts_cannot_replace_newer_success() {
        let root = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(root.path()).unwrap();
        let key = "a".repeat(64);
        let first = summary("b",100);
        store.save_successful_scan(&key,first.clone()).unwrap();
        for phase in [DiscoveryCheckPhase::Partial,DiscoveryCheckPhase::Cancelled,DiscoveryCheckPhase::Error] {
            let mut failed = summary("c",200); failed.phase = phase;
            assert!(store.save_successful_scan(&key,failed).is_err());
            assert_eq!(store.read_successful_scan(&key).unwrap(),Some(first.clone()));
        }
        let next = summary("d",300);
        store.save_successful_scan(&key,next.clone()).unwrap();
        store.save_successful_scan(&key,first).unwrap();
        let reopened = WorkbenchStore::open(root.path()).unwrap();
        assert_eq!(reopened.read_successful_scan(&key).unwrap(),Some(next));
        assert!(!root.path().join(crate::PRIVATE_DIRECTORY).join("special-follows.json").exists());
    }
}

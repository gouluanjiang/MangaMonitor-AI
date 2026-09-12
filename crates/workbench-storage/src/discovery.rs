//! Account-pair-scoped author metadata. No media, library or task authority.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, Document, LibraryReference, Result, Source,
    StoreError, WorkbenchStore, MAX_FOLLOWING_NAME_CHARACTERS, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const DISCOVERY_FILE: &str = "discovery.json";
pub const MAX_DISCOVERY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DISCOVERY_RECORDS: usize = 20_000;
pub const MAX_DISCOVERY_PAGES: u64 = 1000;
pub const MAX_DISCOVERY_AUTHORS: usize = 2000;
const MAX_DISCOVERY_ACCOUNTS: usize = 20;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryWork {
    pub source: Source,
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub favorite: Option<bool>,
    pub chapter_count: Option<u64>,
    pub page_count: Option<u64>,
    pub cover_available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryRecord {
    pub work: DiscoveryWork,
    pub matched_authors: Vec<String>,
    pub author_verified: bool,
    pub observed_at: u64,
    pub scan_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryRangeState {
    Idle,
    Checking,
    Complete,
    Partial,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryAuthorRange {
    pub author: String,
    pub source: Source,
    pub state: DiscoveryRangeState,
    pub last_attempt_at: Option<u64>,
    pub last_complete_at: Option<u64>,
    pub observed_count: usize,
    pub pages_read: u64,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryAccount {
    /// Native-derived SHA-256 of the two verified account keys, never IPC input.
    pub account_key: String,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub records: Vec<DiscoveryRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryDocument {
    pub version: u32,
    pub accounts: Vec<DiscoveryAccount>,
}

impl Default for DiscoveryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: vec![],
        }
    }
}

fn bounded(value: &str, units: usize) -> bool {
    value.encode_utf16().count() <= units && !value.chars().any(char::is_control)
}

pub fn discovery_author_is_valid(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_FOLLOWING_NAME_CHARACTERS
        && !value.chars().any(char::is_control)
}

impl DiscoveryWork {
    pub fn is_valid(&self) -> bool {
        LibraryReference {
            source: self.source,
            work_id: self.work_id.clone(),
        }
        .is_valid()
            && !self.title.trim().is_empty()
            && bounded(&self.title, 2000)
            && self.authors.len() <= 64
            && self
                .authors
                .iter()
                .all(|name| !name.trim().is_empty() && bounded(name, 2000))
            && self
                .description
                .as_ref()
                .is_none_or(|text| bounded(text, 10_000))
            && self.tags.len() <= 64
            && self
                .tags
                .iter()
                .all(|tag| !tag.trim().is_empty() && bounded(tag, 2000))
            && self
                .chapter_count
                .is_none_or(|count| count <= MAX_SAFE_INTEGER)
            && self
                .page_count
                .is_none_or(|count| count <= MAX_SAFE_INTEGER)
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= 64 * 1024)
    }
}

impl ValidatedDocument for DiscoveryDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1 || self.accounts.len() > MAX_DISCOVERY_ACCOUNTS {
            return Err(invalid());
        }
        let mut account_keys = HashSet::new();
        let mut record_count = 0;
        for account in &self.accounts {
            record_count += account.records.len();
            if !library_hash_is_valid(&account.account_key)
                || !account_keys.insert(&account.account_key)
                || account.authors.len() > MAX_DISCOVERY_AUTHORS * 2
                || record_count > MAX_DISCOVERY_RECORDS
            {
                return Err(invalid());
            }
            let mut ranges = HashSet::new();
            for range in &account.authors {
                if !discovery_author_is_valid(&range.author)
                    || !ranges.insert((range.source, &range.author))
                    || range.pages_read > MAX_DISCOVERY_PAGES
                    || range.observed_count > MAX_DISCOVERY_RECORDS
                    || range
                        .last_attempt_at
                        .is_some_and(|value| value > MAX_SAFE_INTEGER)
                    || range
                        .last_complete_at
                        .is_some_and(|value| value > MAX_SAFE_INTEGER)
                    || range.error_code.as_ref().is_some_and(|code| {
                        code.is_empty()
                            || code.len() > 80
                            || !code.bytes().all(|byte| {
                                byte.is_ascii_uppercase() || byte == b'_' || byte.is_ascii_digit()
                            })
                    })
                {
                    return Err(invalid());
                }
            }
            let mut keys = HashSet::new();
            for record in &account.records {
                let mut authors = HashSet::new();
                if !record.work.is_valid()
                    || !keys.insert((record.work.source, &record.work.work_id))
                    || record.observed_at > MAX_SAFE_INTEGER
                    || !library_hash_is_valid(&record.scan_id)
                    || record.matched_authors.is_empty()
                    || record.matched_authors.len() > MAX_DISCOVERY_AUTHORS
                    || record.matched_authors.iter().any(|author| {
                        !discovery_author_is_valid(author)
                            || !authors.insert(author)
                            || (record.author_verified
                                && !record
                                    .work
                                    .authors
                                    .iter()
                                    .any(|name| name.trim() == author.trim()))
                    })
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_discovery(&self) -> Result<Document<DiscoveryDocument>> {
        self.read(DISCOVERY_FILE, MAX_DISCOVERY_BYTES)
    }

    /// Native metadata controller only; there is deliberately no whole-document IPC.
    pub fn write_discovery(
        &self,
        expected_revision: u64,
        value: DiscoveryDocument,
    ) -> Result<Document<DiscoveryDocument>> {
        self.write(
            DISCOVERY_FILE,
            MAX_DISCOVERY_BYTES,
            expected_revision,
            value,
        )
    }

    /// Both documents share the same lock; a changed follow cannot race a scan commit.
    pub fn write_discovery_for_following(
        &self,
        expected_revision: u64,
        following_revision: u64,
        value: DiscoveryDocument,
    ) -> Result<Document<DiscoveryDocument>> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let following: Document<crate::AccountFollowing> =
            self.read_unlocked("following.json", crate::store::MAX_FOLLOWING_BYTES)?;
        if following.revision != following_revision {
            return Err(StoreError::new("DISCOVERY_FOLLOWING_CHANGED"));
        }
        self.write_unlocked(
            DISCOVERY_FILE,
            MAX_DISCOVERY_BYTES,
            expected_revision,
            value,
        )
    }
}

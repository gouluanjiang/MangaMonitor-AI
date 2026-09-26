//! Account-pair-scoped author metadata. No media, library or task authority.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, Document, LibraryReference, Result, Source,
    StoreError, WorkbenchStore, MAX_FOLLOWING_NAME_CHARACTERS, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const DISCOVERY_FILE: &str = "discovery.json";
pub const MAX_DISCOVERY_BYTES: usize = 128 * 1024 * 1024;
/// Active author results have a separate limit from retained raw query history.
pub const MAX_DISCOVERY_RECORDS: usize = 100_000;
pub const MAX_DISCOVERY_RAW_RECORDS: usize = 500_000;
pub const MAX_DISCOVERY_RAW_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_DISCOVERY_HEAD_IDS: usize = 20;
pub const MAX_DISCOVERY_ISSUE_SAMPLES: usize = 20;
pub const MAX_DISCOVERY_PAGES: u64 = 1000;
pub const MAX_DISCOVERY_AUTHORS: usize = 2000;
pub(crate) const MAX_DISCOVERY_ACCOUNTS: usize = 20;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_updated_at: Option<String>,
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
    /// Absent legacy records are the historical baseline, never dated retroactively.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_discovered_run_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryCheckPhase {
    Checking,
    Complete,
    Partial,
    Cancelled,
    Error,
    Interrupted,
}

/// The most recent explicit check; discovering a source ID is not publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryCheckSummary {
    pub id: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub phase: DiscoveryCheckPhase,
    pub mode: DiscoveryMode,
    pub only_unfinished: bool,
    pub first_catalog: bool,
    pub all_followed: bool,
    pub author_count: usize,
    pub total_scopes: usize,
    pub attempted_scopes: usize,
    pub complete_scopes: usize,
}

impl DiscoveryCheckSummary {
    pub fn is_valid(&self) -> bool {
        library_hash_is_valid(&self.id)
            && self.started_at <= MAX_SAFE_INTEGER
            && self
                .finished_at
                .is_none_or(|time| time >= self.started_at && time <= MAX_SAFE_INTEGER)
            && (1..=MAX_DISCOVERY_AUTHORS).contains(&self.author_count)
            && (self.author_count..=self.author_count * 2).contains(&self.total_scopes)
            && self.attempted_scopes <= self.total_scopes
            && self.complete_scopes <= self.attempted_scopes
            && (self.phase != DiscoveryCheckPhase::Checking || self.finished_at.is_none())
            && (self.phase != DiscoveryCheckPhase::Complete
                || (self.complete_scopes == self.total_scopes && self.finished_at.is_some()))
    }
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryMode {
    #[default]
    Incremental,
    Full,
}

/// A source-query checkpoint, never inferred from local ownership or author attribution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryBaseline {
    pub query_version: u32,
    pub head_ids: Vec<String>,
    pub total: u64,
    pub established_at: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryQueryBaseline {
    pub query: String,
    pub baseline: DiscoveryBaseline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiscoveryItemIssueCode {
    #[serde(rename = "SOURCE_ITEM_INVALID")]
    Invalid,
    #[serde(rename = "SOURCE_ITEM_METADATA_MISSING")]
    MetadataMissing,
}

/// A bounded source slot diagnostic, never a work or an author assertion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryItemIssue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub page: u64,
    pub index: u64,
    pub work_id: Option<String>,
    pub code: DiscoveryItemIssueCode,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryAuthorRange {
    pub author: String,
    pub source: Source,
    pub state: DiscoveryRangeState,
    pub last_attempt_at: Option<u64>,
    pub last_complete_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check_mode: Option<DiscoveryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<DiscoveryBaseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_baselines: Vec<DiscoveryQueryBaseline>,
    /// Queries successfully checked in this attempt, distinct from retained old checkpoints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_queries: Vec<String>,
    pub observed_count: usize,
    pub pages_read: u64,
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub issue_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issue_samples: Vec<DiscoveryItemIssue>,
    /// Pagination reached its end even though isolated records may need review.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pages_complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryAccount {
    /// Native-derived SHA-256 of the two verified account keys, never IPC input.
    pub account_key: String,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub records: Vec<DiscoveryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DiscoveryCheckSummary>,
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
            // Up to 64 original source tags plus two explicit language kinds.
            && self.tags.len() <= 66
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
            && self
                .source_updated_at
                .as_deref()
                .is_none_or(crate::work_date_is_valid)
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= 64 * 1024)
    }
}

fn baseline_valid(baseline: &DiscoveryBaseline, source: Source) -> bool {
    let mut ids = HashSet::new();
    baseline.query_version > 0
        && baseline.head_ids.len() <= MAX_DISCOVERY_HEAD_IDS
        && baseline.total <= MAX_SAFE_INTEGER
        && baseline.head_ids.len() == baseline.total.min(MAX_DISCOVERY_HEAD_IDS as u64) as usize
        && baseline.established_at <= MAX_SAFE_INTEGER
        && baseline.head_ids.iter().all(|id| {
            ids.insert(id)
                && LibraryReference {
                    source,
                    work_id: id.clone(),
                }
                .is_valid()
        })
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
                || record_count > MAX_DISCOVERY_RAW_RECORDS
                || account
                    .last_check
                    .as_ref()
                    .is_some_and(|summary| !summary.is_valid())
            {
                return Err(invalid());
            }
            let mut ranges = HashSet::new();
            for range in &account.authors {
                let mut issue_slots = HashSet::new();
                let mut previous_issue = (0, 0);
                let mut previous_query: Option<&str> = None;
                let mut issue_queries = HashSet::from([None]);
                let mut baseline_queries = HashSet::new();
                let mut completed_queries = HashSet::new();
                if !discovery_author_is_valid(&range.author)
                    || !ranges.insert((range.source, &range.author))
                    || range.pages_read > MAX_DISCOVERY_PAGES * crate::MAX_AUTHOR_QUERIES as u64
                    || range
                        .query_fingerprint
                        .as_ref()
                        .is_some_and(|value| !library_hash_is_valid(value))
                    || range.query_baselines.len() > crate::MAX_AUTHOR_QUERIES
                    || (!range.query_baselines.is_empty() && range.query_fingerprint.is_none())
                    || range.query_baselines.iter().any(|item| {
                        !discovery_author_is_valid(&item.query)
                            || !baseline_queries.insert(&item.query)
                            || !baseline_valid(&item.baseline, range.source)
                            || range.issue_samples.iter().any(|issue| {
                                issue
                                    .query
                                    .as_ref()
                                    .is_none_or(|query| query == &item.query)
                            })
                    })
                    || range.completed_queries.len() > crate::MAX_AUTHOR_QUERIES
                    || range.completed_queries.iter().any(|query| {
                        !completed_queries.insert(query)
                            || !range
                                .query_baselines
                                .iter()
                                .any(|saved| &saved.query == query)
                    })
                    || range.observed_count > MAX_DISCOVERY_RAW_RECORDS
                    || range.issue_count > MAX_DISCOVERY_RECORDS
                    || range.issue_count > range.pages_read as usize * 1000
                    || range.issue_samples.len()
                        != range.issue_count.min(MAX_DISCOVERY_ISSUE_SAMPLES)
                    || (range.issue_count > 0
                        && (range.state == DiscoveryRangeState::Complete
                            || range.baseline.is_some()))
                    || (range.pages_complete
                        && (range.pages_read == 0
                            || !matches!(
                                range.state,
                                DiscoveryRangeState::Complete | DiscoveryRangeState::Partial
                            )))
                    || range.issue_samples.iter().any(|issue| {
                        let position = (issue.page, issue.index);
                        let query = issue.query.as_deref();
                        let changed_query = query != previous_query;
                        let repeated_query = changed_query && !issue_queries.insert(query);
                        let out_of_order = !changed_query && position <= previous_issue;
                        previous_issue = position;
                        previous_query = query;
                        issue.page == 0
                            || issue.page > range.pages_read
                            || issue.page > MAX_DISCOVERY_PAGES
                            || !(1..=1000).contains(&issue.index)
                            || issue
                                .query
                                .as_ref()
                                .is_some_and(|query| !discovery_author_is_valid(query))
                            || repeated_query
                            || out_of_order
                            || !issue_slots.insert((query, issue.page, issue.index))
                            || (issue.code == DiscoveryItemIssueCode::MetadataMissing
                                && issue.work_id.is_none())
                            || issue.work_id.as_ref().is_some_and(|work_id| {
                                (range.source == Source::Jm && work_id.len() > 19)
                                    || !LibraryReference {
                                        source: range.source,
                                        work_id: work_id.clone(),
                                    }
                                    .is_valid()
                            })
                    })
                    || range
                        .last_attempt_at
                        .is_some_and(|value| value > MAX_SAFE_INTEGER)
                    || range
                        .last_complete_at
                        .is_some_and(|value| value > MAX_SAFE_INTEGER)
                    || range
                        .last_checked_at
                        .is_some_and(|value| value > MAX_SAFE_INTEGER)
                    || range
                        .baseline
                        .as_ref()
                        .is_some_and(|baseline| !baseline_valid(baseline, range.source))
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
                    || record
                        .first_discovered_run_id
                        .as_ref()
                        .is_some_and(|id| !library_hash_is_valid(id))
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
        self.read_discovery_journal()
    }

    /// Native metadata controller only; there is deliberately no whole-document IPC.
    pub fn write_discovery(
        &self,
        expected_revision: u64,
        value: DiscoveryDocument,
    ) -> Result<Document<DiscoveryDocument>> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        self.require_legacy_discovery_unlocked()?;
        self.write_unlocked(
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
        self.require_legacy_discovery_unlocked()?;
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

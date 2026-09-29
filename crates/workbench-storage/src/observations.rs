//! Source-account metadata observed outside author queries. Never a query baseline.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, DiscoveryWork, Document, Result, Source,
    StoreError, WorkbenchStore, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const FILE: &str = "observed-works.json";
const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_OBSERVED_WORKS: usize = 100_000;
pub const MAX_RECENT_HISTORY: usize = 20_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedWork {
    pub work: DiscoveryWork,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    pub observed_at: u64,
    pub metadata_detail_at: Option<u64>,
    pub via: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentCoverage {
    pub head_ids: Vec<String>,
    pub checked_at: Option<u64>,
    pub pages_read: u64,
    pub reached_end: bool,
    pub joined_previous: bool,
    #[serde(default)]
    pub initial_window: bool,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedAccount {
    pub account_key: String,
    pub source: Source,
    pub records: Vec<ObservedWork>,
    pub recent_ids: Vec<String>,
    pub coverage: RecentCoverage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedDocument {
    pub version: u32,
    pub accounts: Vec<ObservedAccount>,
}

impl Default for ObservedDocument {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: vec![],
        }
    }
}

impl ValidatedDocument for ObservedDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        let mut accounts = HashSet::new();
        let mut total = 0;
        if self.version != 1 || self.accounts.len() > 20 {
            return Err(invalid());
        }
        for account in &self.accounts {
            total += account.records.len();
            if !library_hash_is_valid(&account.account_key)
                || !accounts.insert(&account.account_key)
                || total > MAX_OBSERVED_WORKS
                || account.recent_ids.len() > MAX_RECENT_HISTORY
            {
                return Err(invalid());
            }
            let mut ids = HashSet::new();
            for record in &account.records {
                if record.work.source != account.source
                    || !record.work.is_valid()
                    || !ids.insert(&record.work.work_id)
                    || record.observed_at > MAX_SAFE_INTEGER
                    || record
                        .metadata_detail_at
                        .is_some_and(|time| time > record.observed_at)
                    || record.via.is_empty()
                    || record.via.len() > 8
                    || record.via.iter().any(|kind| {
                        !matches!(
                            kind.as_str(),
                            "recent"
                                | "favorites"
                                | "search"
                                | "author"
                                | "tag"
                                | "category"
                                | "ranking"
                                | "detail"
                        )
                    })
                    || record.categories.as_ref().is_some_and(|items| {
                        items.len() > 64
                            || items.iter().any(|item| {
                                item.trim().is_empty()
                                    || item.encode_utf16().count() > 2000
                                    || item.chars().any(char::is_control)
                            })
                    })
                {
                    return Err(invalid());
                }
            }
            let mut recent = HashSet::new();
            let mut head = HashSet::new();
            if account
                .recent_ids
                .iter()
                .any(|id| !ids.contains(id) || !recent.insert(id))
                || account.coverage.head_ids.len() > 200
                || account
                    .coverage
                    .head_ids
                    .iter()
                    .any(|id| !ids.contains(id) || !head.insert(id))
                || account
                    .coverage
                    .checked_at
                    .is_some_and(|time| time > MAX_SAFE_INTEGER)
                || account.coverage.pages_read > 1000
                || account.coverage.error_code.as_ref().is_some_and(|code| {
                    code.is_empty()
                        || code.len() > 80
                        || !code
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                })
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_observed_works(&self) -> Result<Document<ObservedDocument>> {
        self.read(FILE, MAX_BYTES)
    }

    /// Native-derived account scope. The complete page is committed or rejected;
    /// reaching a budget never evicts unread works or advances a coverage boundary.
    pub fn merge_observed_works(
        &self,
        account_key: &str,
        source: Source,
        records: Vec<ObservedWork>,
        recent_page: Option<u64>,
    ) -> Result<Document<ObservedDocument>> {
        if !library_hash_is_valid(account_key)
            || records.len() > 1000
            || recent_page.is_some_and(|page| !(1..=1000).contains(&page))
        {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let mut document: Document<ObservedDocument> = self.read_unlocked(FILE, MAX_BYTES)?;
        let account = if let Some(index) = document
            .value
            .accounts
            .iter()
            .position(|a| a.account_key == account_key)
        {
            &mut document.value.accounts[index]
        } else {
            document.value.accounts.push(ObservedAccount {
                account_key: account_key.into(),
                source,
                records: vec![],
                recent_ids: vec![],
                coverage: RecentCoverage::default(),
            });
            document
                .value
                .accounts
                .last_mut()
                .expect("inserted account")
        };
        if account.source != source {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let mut positions: HashMap<String, usize> = account
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (r.work.work_id.clone(), i))
            .collect();
        let page_ids: Vec<String> = records.iter().map(|r| r.work.work_id.clone()).collect();
        for mut incoming in records {
            if let Some(index) = positions.get(&incoming.work.work_id).copied() {
                let old = &account.records[index];
                let mut via = old.via.clone();
                for kind in &incoming.via {
                    if !via.contains(kind) {
                        via.push(kind.clone());
                    }
                }
                if incoming.observed_at < old.observed_at
                    && incoming
                        .metadata_detail_at
                        .is_none_or(|at| old.metadata_detail_at.is_some_and(|saved| saved >= at))
                {
                    account.records[index].via = via;
                    continue;
                }
                let preserve_detail = old
                    .metadata_detail_at
                    .is_some_and(|at| incoming.metadata_detail_at.is_none_or(|fresh| fresh < at));
                let incoming_is_detail = incoming
                    .metadata_detail_at
                    .is_some_and(|at| old.metadata_detail_at.is_none_or(|saved| at >= saved));
                if preserve_detail
                    || (incoming.work.authors.is_empty() && !old.work.authors.is_empty())
                {
                    let fresh_tags = incoming.work.tags.clone();
                    let fresh_date = incoming.work.source_updated_at.clone();
                    incoming.work = old.work.clone();
                    for tag in fresh_tags {
                        if !incoming.work.tags.contains(&tag) {
                            incoming.work.tags.push(tag);
                        }
                    }
                    if incoming.observed_at >= old.observed_at && fresh_date.is_some() {
                        incoming.work.source_updated_at = fresh_date;
                    }
                } else {
                    if incoming.work.description.is_none() {
                        incoming.work.description = old.work.description.clone();
                    }
                    if incoming.work.chapter_count.is_none() {
                        incoming.work.chapter_count = old.work.chapter_count;
                    }
                    if incoming.work.page_count.is_none() {
                        incoming.work.page_count = old.work.page_count;
                    }
                }
                incoming.observed_at = incoming.observed_at.max(old.observed_at);
                incoming.metadata_detail_at =
                    match (old.metadata_detail_at, incoming.metadata_detail_at) {
                        (Some(saved), Some(fresh)) => Some(saved.max(fresh)),
                        (saved, fresh) => saved.or(fresh),
                    };
                if incoming.work.source_updated_at.is_none() {
                    incoming.work.source_updated_at = old.work.source_updated_at.clone();
                }
                if incoming.categories.as_ref().is_none_or(Vec::is_empty) {
                    incoming.categories = old.categories.clone();
                }
                // An abbreviated list must not discard known explicit labels.
                if !incoming_is_detail {
                    for tag in &old.work.tags {
                        if !incoming.work.tags.contains(tag) {
                            incoming.work.tags.push(tag.clone());
                        }
                    }
                }
                incoming.via = via;
                account.records[index] = incoming;
            } else {
                positions.insert(incoming.work.work_id.clone(), account.records.len());
                account.records.push(incoming);
            }
        }
        if let Some(page) = recent_page {
            let mut seen = HashSet::new();
            let next: Vec<String> = if page == 1 {
                page_ids
                    .into_iter()
                    .chain(account.recent_ids.iter().cloned())
                    .filter(|id| seen.insert(id.clone()))
                    .collect()
            } else {
                account
                    .recent_ids
                    .iter()
                    .cloned()
                    .chain(page_ids)
                    .filter(|id| seen.insert(id.clone()))
                    .collect()
            };
            if next.len() > MAX_RECENT_HISTORY {
                return Err(StoreError::new("RECENT_LIMIT"));
            }
            account.recent_ids = next;
        }
        if document
            .value
            .accounts
            .iter()
            .map(|a| a.records.len())
            .sum::<usize>()
            > MAX_OBSERVED_WORKS
        {
            return Err(StoreError::new("OBSERVATION_LIMIT"));
        }
        self.write_unlocked(FILE, MAX_BYTES, document.revision, document.value)
    }

    /// A recent traversal checkpoint is separate from every author query baseline.
    pub fn set_recent_coverage(&self, account_key: &str, coverage: RecentCoverage) -> Result<()> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let mut document: Document<ObservedDocument> = self.read_unlocked(FILE, MAX_BYTES)?;
        let account = document
            .value
            .accounts
            .iter_mut()
            .find(|a| a.account_key == account_key)
            .ok_or(StoreError::new("VALIDATION_FAILED"))?;
        account.coverage = coverage;
        self.write_unlocked(FILE, MAX_BYTES, document.revision, document.value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: &str, author: &str, detail: bool, at: u64) -> ObservedWork {
        ObservedWork {
            work: DiscoveryWork {
                source: Source::Jm,
                work_id: id.into(),
                title: "Synthetic work".into(),
                authors: vec![author.into()],
                description: None,
                tags: vec![],
                favorite: None,
                chapter_count: None,
                page_count: None,
                source_updated_at: None,
                cover_available: false,
            },
            categories: None,
            observed_at: at,
            metadata_detail_at: detail.then_some(at),
            via: vec![if detail { "detail" } else { "recent" }.into()],
        }
    }
    #[test]
    fn observed_metadata_retains_detail_and_does_not_advance_any_baseline() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = "a".repeat(64);
        store
            .merge_observed_works(
                &key,
                Source::Jm,
                vec![record("42", "Creator", true, 10)],
                None,
            )
            .unwrap();
        store
            .merge_observed_works(&key, Source::Jm, vec![record("42", "", false, 11)], Some(1))
            .unwrap();
        let document = WorkbenchStore::open(temp.path())
            .unwrap()
            .read_observed_works()
            .unwrap();
        assert_eq!(
            document.value.accounts[0].records[0].work.authors,
            vec!["Creator"]
        );
        assert_eq!(document.value.accounts[0].recent_ids, vec!["42"]);
        assert_eq!(
            document.value.accounts[0].coverage,
            RecentCoverage::default()
        );
        assert_eq!(store.read_discovery().unwrap().revision, 0);
    }
    #[test]
    fn older_reply_cannot_erase_newer_credit_and_account_scopes_stay_separate() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = "a".repeat(64);
        store
            .merge_observed_works(&key, Source::Jm, vec![record("42", "New", true, 20)], None)
            .unwrap();
        store
            .merge_observed_works(&key, Source::Jm, vec![record("42", "Old", true, 10)], None)
            .unwrap();
        store
            .merge_observed_works(
                &"b".repeat(64),
                Source::Jm,
                vec![record("42", "Other", true, 30)],
                None,
            )
            .unwrap();
        let document = store.read_observed_works().unwrap();
        assert_eq!(
            document.value.accounts[0].records[0].work.authors,
            vec!["New"]
        );
        assert_eq!(
            document.value.accounts[1].records[0].work.authors,
            vec!["Other"]
        );
    }

    #[test]
    fn merged_detail_time_and_rich_metadata_survive_newer_abbreviated_lists() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = "a".repeat(64);
        let mut detail = record("42", "Verified", true, 20);
        detail.work.description = Some("Full metadata".into());
        detail.work.page_count = Some(60);
        store
            .merge_observed_works(&key, Source::Jm, vec![detail.clone()], None)
            .unwrap();
        let mut weak = record("42", "List-only credit", false, 30);
        weak.work.tags = vec!["AI作画".into()];
        store
            .merge_observed_works(&key, Source::Jm, vec![weak], Some(1))
            .unwrap();
        let mut replay = record("42", "Old detail with newer observation", true, 40);
        replay.metadata_detail_at = Some(10);
        store
            .merge_observed_works(&key, Source::Jm, vec![replay], None)
            .unwrap();
        let saved = store.read_observed_works().unwrap();
        let work = &saved.value.accounts[0].records[0];
        assert_eq!(work.work.authors, ["Verified"]);
        assert_eq!(work.work.description, detail.work.description);
        assert_eq!(work.work.page_count, Some(60));
        assert!(work.work.tags.contains(&"AI作画".into()));
        assert_eq!(work.metadata_detail_at, Some(20));
        assert_eq!(work.observed_at, 40);
        let mut corrected = detail;
        corrected.observed_at = 50;
        corrected.metadata_detail_at = Some(50);
        corrected.work.authors = vec!["Corrected".into()];
        store
            .merge_observed_works(&key, Source::Jm, vec![corrected], None)
            .unwrap();
        assert_eq!(
            store.read_observed_works().unwrap().value.accounts[0].records[0]
                .work
                .authors,
            ["Corrected"]
        );
    }

    #[test]
    fn invalid_page_is_atomic_and_cannot_advance_recent_coverage_or_lose_existing_works() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = "a".repeat(64);
        store
            .merge_observed_works(
                &key,
                Source::Jm,
                vec![record("42", "Creator", false, 20)],
                Some(1),
            )
            .unwrap();
        let coverage = RecentCoverage {
            head_ids: vec!["42".into()],
            checked_at: Some(20),
            pages_read: 1,
            reached_end: true,
            ..RecentCoverage::default()
        };
        store.set_recent_coverage(&key, coverage.clone()).unwrap();
        let before = store.read_observed_works().unwrap();
        let mut invalid = record("43", "Creator", true, 30);
        invalid.metadata_detail_at = Some(31);
        assert!(store
            .merge_observed_works(
                &key,
                Source::Jm,
                vec![record("44", "Creator", false, 30), invalid],
                Some(1)
            )
            .is_err());
        assert_eq!(store.read_observed_works().unwrap(), before);
        let invalid_coverage = RecentCoverage {
            head_ids: vec!["42".into(), "42".into()],
            ..coverage
        };
        assert!(store.set_recent_coverage(&key, invalid_coverage).is_err());
        assert_eq!(store.read_observed_works().unwrap(), before);
    }

    #[test]
    fn category_limits_match_source_work_utf16_contract() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let key = "a".repeat(64);
        let mut valid = record("42", "Creator", false, 20);
        valid.categories = Some(vec!["字".repeat(2000)]);
        store
            .merge_observed_works(&key, Source::Jm, vec![valid.clone()], None)
            .unwrap();
        let before = store.read_observed_works().unwrap();
        valid.categories = Some(vec!["字".repeat(2001)]);
        assert!(store
            .merge_observed_works(&key, Source::Jm, vec![valid.clone()], None)
            .is_err());
        valid.categories = Some(vec!["Category".into(); 65]);
        assert!(store
            .merge_observed_works(&key, Source::Jm, vec![valid], None)
            .is_err());
        assert_eq!(store.read_observed_works().unwrap(), before);
    }
}

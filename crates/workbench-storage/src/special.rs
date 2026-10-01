//! Optional special follows; independent of ordinary follows and scan receipts.
use crate::{
    discovery_author_is_valid, library_hash_is_valid, model::ValidatedDocument, DiscoveryWork,
    Document, LibraryReference, Result, Source, StoreError, WorkbenchStore, MAX_DISCOVERY_AUTHORS,
    MAX_DISCOVERY_RECORDS, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const FILE: &str = "special-follows.json";
const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecialRange {
    pub source: Source,
    pub baseline_complete: bool,
    pub query_fingerprint: Option<String>,
    pub known_ids: Vec<String>,
    pub checked_at: Option<u64>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecialAuthor {
    pub author: String,
    pub enabled: bool,
    pub ranges: Vec<SpecialRange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecialUpdate {
    pub work: DiscoveryWork,
    pub authors: Vec<String>,
    pub discovered_at: u64,
    pub read_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecialAccount {
    pub account_key: String,
    pub authors: Vec<SpecialAuthor>,
    pub updates: Vec<SpecialUpdate>,
}

impl SpecialAccount {
    pub fn set_author(&mut self, author: &str, enabled: bool) {
        if let Some(row) = self.authors.iter_mut().find(|row| row.author == author) {
            row.enabled = enabled;
        } else if enabled {
            self.authors.push(SpecialAuthor {
                author: author.into(),
                enabled: true,
                ranges: [Source::Jm, Source::Pica]
                    .into_iter()
                    .map(|source| SpecialRange {
                        source,
                        baseline_complete: false,
                        query_fingerprint: None,
                        known_ids: vec![],
                        checked_at: None,
                        error_code: None,
                    })
                    .collect(),
            });
        }
    }

    /// Caller supplies the already attributed local projection after a shared scan.
    /// Incomplete initial baselines never create unread entries. A query-rule change
    /// also needs a new baseline; existing unread entries survive that operation.
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        author: &str,
        source: Source,
        fingerprint: &str,
        works: &[DiscoveryWork],
        complete: bool,
        checked_at: u64,
        error: Option<String>,
    ) {
        let Some(row) = self.authors.iter_mut().find(|row| row.author == author && row.enabled) else {
            return;
        };
        let Some(range) = row.ranges.iter_mut().find(|range| range.source == source) else {
            return;
        };
        let ready = range.baseline_complete
            && range.query_fingerprint.as_deref() == Some(fingerprint);
        let known: HashSet<_> = range.known_ids.iter().cloned().collect();
        let incoming: HashSet<_> = works.iter().map(|work| work.work_id.clone()).collect();
        if ready {
            for work in works {
                if let Some(update) = self.updates.iter_mut().find(|update| {
                    update.work.source == source && update.work.work_id == work.work_id
                }) {
                    update.work = work.clone();
                    if !update.authors.iter().any(|name| name == author) {
                        update.authors.push(author.into());
                    }
                } else if !known.contains(&work.work_id) {
                    self.updates.push(SpecialUpdate {
                        work: work.clone(),
                        authors: vec![author.into()],
                        discovered_at: checked_at,
                        read_at: None,
                    });
                }
            }
        }
        // Keep observed identities through failed checks and source removals.
        range.known_ids = known.union(&incoming).cloned().collect();
        range.known_ids.sort();
        range.baseline_complete = ready || complete;
        range.query_fingerprint = Some(fingerprint.into());
        range.checked_at = Some(checked_at);
        range.error_code = error;
    }

    pub fn mark_read(&mut self, identity: Option<&LibraryReference>, now: u64) {
        let enabled: HashSet<_> = self.authors.iter().filter(|row| row.enabled).map(|row| row.author.as_str()).collect();
        for update in &mut self.updates {
            if identity.is_none_or(|key| {
                key.source == update.work.source && key.work_id == update.work.work_id
            }) && update.read_at.is_none() && update.authors.iter().any(|author| enabled.contains(author.as_str()))
            {
                update.read_at = Some(now);
            }
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecialDocument {
    pub version: u32,
    pub accounts: Vec<SpecialAccount>,
}

impl Default for SpecialDocument {
    fn default() -> Self {
        Self { version: 1, accounts: vec![] }
    }
}

impl ValidatedDocument for SpecialDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        let mut accounts = HashSet::new();
        if self.version != 1 || self.accounts.len() > 20 {
            return Err(invalid());
        }
        for account in &self.accounts {
            if !library_hash_is_valid(&account.account_key)
                || !accounts.insert(&account.account_key)
                || account.authors.len() > MAX_DISCOVERY_AUTHORS
                || account.updates.len() > MAX_DISCOVERY_RECORDS
            {
                return Err(invalid());
            }
            let mut names = HashSet::new();
            let mut identities = 0;
            for author in &account.authors {
                if !discovery_author_is_valid(&author.author) || !names.insert(&author.author)
                    || author.ranges.len() != 2
                    || author.ranges[0].source == author.ranges[1].source
                {
                    return Err(invalid());
                }
                for range in &author.ranges {
                    let mut ids = HashSet::new();
                    identities += range.known_ids.len();
                    if identities > crate::MAX_DISCOVERY_RAW_RECORDS
                        || range.query_fingerprint.as_ref().is_some_and(|id| !library_hash_is_valid(id))
                        || (range.baseline_complete && range.query_fingerprint.is_none())
                        || range.checked_at.is_some_and(|time| time > MAX_SAFE_INTEGER)
                        || range.error_code.as_ref().is_some_and(|code| code.is_empty() || code.len() > 100 || !code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
                        || range.known_ids.iter().any(|id| !ids.insert(id) || !LibraryReference { source: range.source, work_id: id.clone() }.is_valid())
                    {
                        return Err(invalid());
                    }
                }
            }
            let mut keys = HashSet::new();
            for update in &account.updates {
                let mut authors = HashSet::new();
                if !update.work.is_valid()
                    || !keys.insert((update.work.source, &update.work.work_id))
                    || update.discovered_at > MAX_SAFE_INTEGER
                    || update.read_at.is_some_and(|time| time > MAX_SAFE_INTEGER)
                    || update.authors.is_empty()
                    || update.authors.iter().any(|name| !names.contains(name) || !authors.insert(name))
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_special_follows(&self) -> Result<Document<SpecialDocument>> {
        self.read(FILE, MAX_BYTES)
    }

    /// Fixed native document; renderer cannot supply a whole document or filename.
    pub fn edit_special_follows<F>(&self, account_key: &str, revisions: (u64, u64), operation: F) -> Result<SpecialAccount>
    where F: FnOnce(&mut SpecialAccount),
    {
        let _local = self.local_lock.lock().map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let following: Document<crate::AccountFollowing> = self.read_unlocked("following.json", crate::store::MAX_FOLLOWING_BYTES)?;
        let policies: Document<crate::AuthorQueryDocument> = self.read_unlocked(crate::author_query::AUTHOR_QUERY_FILE, crate::author_query::MAX_AUTHOR_QUERY_BYTES)?;
        if (following.revision, policies.revision) != revisions {
            return Err(StoreError::new("REVISION_CONFLICT"));
        }
        let mut document: Document<SpecialDocument> = self.read_unlocked(FILE, MAX_BYTES)?;
        if !document.value.accounts.iter().any(|account| account.account_key == account_key) {
            document.value.accounts.push(SpecialAccount { account_key: account_key.into(), ..Default::default() });
        }
        let account = document.value.accounts.iter_mut().find(|account| account.account_key == account_key).expect("inserted account");
        operation(account);
        let result = account.clone();
        self.write_unlocked(FILE, MAX_BYTES, document.revision, document.value)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work(id: &str) -> DiscoveryWork {
        DiscoveryWork { source: Source::Jm, work_id: id.into(), title: "Synthetic work".into(), authors: vec!["Author A".into()], description: None, tags: vec![], favorite: None, chapter_count: None, page_count: None, source_updated_at: None, cover_available: false }
    }
    #[test]
    fn incomplete_baseline_never_marks_old_works_new_and_metadata_does_not_unread() {
        let mut account = SpecialAccount::default();
        account.set_author("Author A", true);
        let fingerprint = "b".repeat(64);
        account.observe("Author A", Source::Jm, &fingerprint, &[work("1")], false, 1, Some("SOURCE_TIMEOUT".into()));
        account.observe("Author A", Source::Jm, &fingerprint, &[work("1"), work("2")], true, 2, None);
        assert!(account.updates.is_empty());
        account.observe("Author A", Source::Jm, &fingerprint, &[work("3")], false, 3, Some("SOURCE_TIMEOUT".into()));
        assert_eq!(account.updates.len(), 1);
        account.mark_read(Some(&LibraryReference {source: Source::Jm, work_id: "3".into()}), 4);
        let mut changed = work("3"); changed.title = "New title".into();
        account.observe("Author A", Source::Jm, &fingerprint, &[changed], true, 5, None);
        assert_eq!(account.updates[0].read_at, Some(4));
        account.observe("Author A", Source::Jm, &"c".repeat(64), &[work("4")], true, 6, None);
        assert_eq!(account.updates.len(), 1, "changed queries establish a fresh baseline");
    }
    #[test]
    fn coauthor_updates_deduplicate_and_state_survives_restart_without_following_writes() {
        let temp = tempfile::TempDir::new().unwrap();
        let key = "a".repeat(64);
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let fingerprint = "b".repeat(64);
        store.edit_special_follows(&key, (0, 0), |account| {
            for author in ["Author A", "Author B"] {
                account.set_author(author, true);
                account.observe(author, Source::Jm, &fingerprint, &[work("1")], true, 1, None);
                account.observe(author, Source::Jm, &fingerprint, &[work("2")], true, 2, None);
            }
        }).unwrap();
        let reopened = WorkbenchStore::open(temp.path()).unwrap();
        let saved = reopened.read_special_follows().unwrap();
        assert_eq!(saved.value.accounts[0].updates.len(), 1);
        assert_eq!(saved.value.accounts[0].updates[0].authors.len(), 2);
        assert_eq!(reopened.read_following().unwrap().revision, 0);
        reopened.edit_special_follows(&key, (0, 0), |account| { account.set_author("Author A", false); account.mark_read(None, 3); }).unwrap();
        let saved = reopened.read_special_follows().unwrap();
        assert!(!saved.value.accounts[0].authors[0].enabled);
        assert_eq!(saved.value.accounts[0].updates[0].read_at, Some(3));
        assert_eq!(reopened.edit_special_follows(&key, (1, 0), |_| {}).err().unwrap().code, "REVISION_CONFLICT");
    }
}

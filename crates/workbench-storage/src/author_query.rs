//! Private, explicitly reviewed author query spellings. Never inferred from titles.
use crate::{
    discovery_author_is_valid, library_hash_is_valid, model::ValidatedDocument, Document, Result,
    Source, StoreError, WorkbenchStore, MAX_FOLLOWED_ACCOUNTS, MAX_FOLLOWED_AUTHORS_PER_ACCOUNT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(crate) const AUTHOR_QUERY_FILE: &str = "author-query-policies.json";
pub(crate) const MAX_AUTHOR_QUERY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_AUTHOR_QUERIES: usize = 4;
pub const MAX_AUTHOR_ALIASES: usize = 16;
pub const MAX_AUTHOR_WORK_CREDITS: usize = 500;
pub const MAX_WORK_CREDIT_AUTHORS: usize = 64;

/// Explicit evidence for one source work. Raw website credits stay immutable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorWorkCredit {
    pub work_id: String,
    pub expected_authors: Vec<String>,
    /// Reviewed complete credit sets returned by other listings of this work.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_author_variants: Vec<Vec<String>>,
    pub corrected_authors: Vec<String>,
}

impl AuthorWorkCredit {
    pub fn matches_expected(&self, credits: &[String]) -> bool {
        let normalize = crate::author_evidence::normalized_author_credit;
        let actual: HashSet<_> = credits.iter().map(|credit| normalize(credit)).collect();
        std::iter::once(&self.expected_authors)
            .chain(&self.expected_author_variants)
            .any(|names| {
                names
                    .iter()
                    .map(|credit| normalize(credit))
                    .collect::<HashSet<_>>()
                    == actual
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorQueryProfile {
    /// Stable following/display key, not replaced with a website's spelling.
    pub author: String,
    pub queries: Vec<String>,
    pub verified_aliases: Vec<String>,
    /// Reviewed complete source-author fields; never split into personal aliases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exact_credits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorQueryAccount {
    pub source: Source,
    pub account_key: String,
    pub profiles: Vec<AuthorQueryProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_credits: Vec<AuthorWorkCredit>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorQueryDocument {
    pub version: u32,
    pub accounts: Vec<AuthorQueryAccount>,
}

impl Default for AuthorQueryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: vec![],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorQueryPolicy {
    pub source: Source,
    pub author: String,
    pub queries: Vec<String>,
    pub verified_aliases: Vec<String>,
    pub exact_credits: Vec<String>,
    pub query_fingerprint: String,
    /// Only rules relevant to this author, including arbitrary unfollowed searches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_credits: Vec<AuthorWorkCredit>,
}

/// Exact UTF-8 query order/spelling and fixed source ordering semantics; aliases
/// deliberately do not invalidate a remote-query checkpoint.
pub fn author_query_fingerprint(source: Source, queries: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"author-query-v1:newest:jm-default-categories:pica-default\0");
    digest.update(match source {
        Source::Jm => b"jm".as_slice(),
        Source::Pica => b"pica".as_slice(),
    });
    for query in queries {
        digest.update((query.len() as u64).to_be_bytes());
        digest.update(query.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl AuthorQueryDocument {
    pub fn resolve(&self, source: Source, account_key: &str, author: &str) -> AuthorQueryPolicy {
        let account = self
            .accounts
            .iter()
            .find(|account| account.source == source && account.account_key == account_key);
        let profile = account.and_then(|account| {
            account
                .profiles
                .iter()
                .find(|profile| profile.author == author)
        });
        let queries = profile.map_or_else(
            || vec![author.to_owned()],
            |profile| profile.queries.clone(),
        );
        let mut policy = AuthorQueryPolicy {
            source,
            author: author.to_owned(),
            query_fingerprint: author_query_fingerprint(source, &queries),
            queries,
            verified_aliases: profile
                .map_or_else(Vec::new, |profile| profile.verified_aliases.clone()),
            exact_credits: profile.map_or_else(Vec::new, |profile| profile.exact_credits.clone()),
            work_credits: vec![],
        };
        if let Some(account) = account {
            policy.work_credits = account
                .work_credits
                .iter()
                .filter(|rule| {
                    policy.matches_credits(&rule.expected_authors)
                        || rule
                            .expected_author_variants
                            .iter()
                            .any(|names| policy.matches_credits(names))
                        || policy.matches_credits(&rule.corrected_authors)
                })
                .cloned()
                .collect();
        }
        policy
    }

    /// Merge only explicitly supplied profiles. Existing scopes and unrelated
    /// profiles remain intact; display/following records are never rewritten.
    pub fn merged_import(
        &self,
        incoming: &Self,
        following: &crate::AccountFollowing,
    ) -> Result<Self> {
        incoming.validate()?;
        let mut result = self.clone();
        for account in &incoming.accounts {
            let followed = following
                .accounts
                .iter()
                .find(|item| {
                    item.source == account.source && item.account_key == account.account_key
                })
                .ok_or(StoreError::new("AUTHOR_POLICY_SCOPE_UNKNOWN"))?;
            if account
                .profiles
                .iter()
                .any(|profile| !followed.authors.contains(&profile.author))
            {
                return Err(StoreError::new("AUTHOR_POLICY_AUTHOR_UNKNOWN"));
            }
            let position = result.accounts.iter().position(|item| {
                item.source == account.source && item.account_key == account.account_key
            });
            let position = position.unwrap_or_else(|| {
                result.accounts.push(AuthorQueryAccount {
                    source: account.source,
                    account_key: account.account_key.clone(),
                    profiles: vec![],
                    work_credits: vec![],
                });
                result.accounts.len() - 1
            });
            for profile in &account.profiles {
                if let Some(existing) = result.accounts[position]
                    .profiles
                    .iter_mut()
                    .find(|item| item.author == profile.author)
                {
                    *existing = profile.clone();
                } else {
                    result.accounts[position].profiles.push(profile.clone());
                }
            }
            for rule in &account.work_credits {
                if let Some(existing) = result.accounts[position]
                    .work_credits
                    .iter_mut()
                    .find(|item| item.work_id == rule.work_id)
                {
                    *existing = rule.clone();
                } else {
                    result.accounts[position].work_credits.push(rule.clone());
                }
            }
        }
        result.validate()?;
        Ok(result)
    }
}

impl AuthorQueryPolicy {
    /// Apply only an exact work-ID and normalized full-credit-set guard. This is
    /// a view over reviewed evidence, never a rewrite of saved source metadata.
    pub fn effective_work_credits<'a>(
        &'a self,
        work_id: &str,
        credits: &'a [String],
    ) -> &'a [String] {
        let Some(rule) = self
            .work_credits
            .iter()
            .find(|rule| rule.work_id == work_id)
        else {
            return credits;
        };
        if rule.matches_expected(credits) {
            &rule.corrected_authors
        } else {
            credits
        }
    }

    pub fn matches_work_credits(&self, work_id: &str, credits: &[String]) -> bool {
        self.matches_credits(self.effective_work_credits(work_id, credits))
    }

    pub fn matches_credits(&self, credits: &[String]) -> bool {
        std::iter::once(&self.author)
            .chain(&self.verified_aliases)
            .any(|author| {
                credits
                    .iter()
                    .any(|credit| crate::author_credit_matches(author, credit))
            })
            || self.exact_credits.iter().any(|expected| {
                credits.iter().any(|credit| {
                    crate::author_evidence::normalized_author_credit(expected)
                        == crate::author_evidence::normalized_author_credit(credit)
                })
            })
    }

    pub fn is_original_query(&self) -> bool {
        self.queries.len() == 1 && self.queries[0] == self.author
    }
}

impl ValidatedDocument for AuthorQueryDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1 || self.accounts.len() > MAX_FOLLOWED_ACCOUNTS {
            return Err(invalid());
        }
        let mut accounts = HashSet::new();
        for account in &self.accounts {
            if !library_hash_is_valid(&account.account_key)
                || !accounts.insert((account.source, &account.account_key))
                || account.profiles.len() > MAX_FOLLOWED_AUTHORS_PER_ACCOUNT
                || account.work_credits.len() > MAX_AUTHOR_WORK_CREDITS
            {
                return Err(invalid());
            }
            let mut authors = HashSet::new();
            let mut work_ids = HashSet::new();
            for rule in &account.work_credits {
                if !(crate::LibraryReference {
                    source: account.source,
                    work_id: rule.work_id.clone(),
                })
                .is_valid()
                    || !work_ids.insert(&rule.work_id)
                    || rule.expected_author_variants.len() > 4
                {
                    return Err(invalid());
                }
                let mut expected_sets = HashSet::new();
                for names in
                    std::iter::once(&rule.expected_authors).chain(&rule.expected_author_variants)
                {
                    let mut normalized: Vec<_> = names
                        .iter()
                        .map(|name| crate::author_evidence::normalized_author_credit(name))
                        .collect();
                    normalized.sort();
                    if !expected_sets.insert(normalized) {
                        return Err(invalid());
                    }
                }
                for names in [&rule.expected_authors, &rule.corrected_authors]
                    .into_iter()
                    .chain(&rule.expected_author_variants)
                {
                    let mut unique = HashSet::new();
                    if names.is_empty()
                        || names.len() > MAX_WORK_CREDIT_AUTHORS
                        || names.iter().any(|name| {
                            let normalized = crate::author_evidence::normalized_author_credit(name);
                            name.chars().count() > 2000
                                || name.chars().any(char::is_control)
                                || normalized.is_empty()
                                || !unique.insert(normalized)
                        })
                    {
                        return Err(invalid());
                    }
                }
            }
            for profile in &account.profiles {
                if !discovery_author_is_valid(&profile.author)
                    || profile.author.trim() != profile.author
                    || !authors.insert(&profile.author)
                    || profile.queries.is_empty()
                    || profile.queries.len() > MAX_AUTHOR_QUERIES
                    || profile.verified_aliases.len() > MAX_AUTHOR_ALIASES
                    || profile.exact_credits.len() > MAX_AUTHOR_ALIASES
                {
                    return Err(invalid());
                }
                for names in [&profile.queries, &profile.verified_aliases] {
                    let mut unique = HashSet::new();
                    if names.iter().any(|name| {
                        !discovery_author_is_valid(name)
                            || name.trim() != name
                            || !unique.insert(name)
                    }) {
                        return Err(invalid());
                    }
                }
                let mut credits = HashSet::new();
                if profile.exact_credits.iter().any(|credit| {
                    credit.trim().is_empty()
                        || credit.chars().count() > 2000
                        || credit.chars().any(char::is_control)
                        || !credits.insert(credit)
                }) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_author_query_policies(&self) -> Result<Document<AuthorQueryDocument>> {
        self.read(AUTHOR_QUERY_FILE, MAX_AUTHOR_QUERY_BYTES)
    }

    /// Maintenance import only: no renderer-selected account scope or whole-file
    /// mutation command. Scans atomically compare this revision on every commit.
    pub fn write_author_query_policies(
        &self,
        expected_revision: u64,
        value: AuthorQueryDocument,
    ) -> Result<Document<AuthorQueryDocument>> {
        self.write(
            AUTHOR_QUERY_FILE,
            MAX_AUTHOR_QUERY_BYTES,
            expected_revision,
            value,
        )
    }

    pub(crate) fn require_author_query_revision_unlocked(
        &self,
        expected_revision: Option<u64>,
    ) -> Result<()> {
        if let Some(expected) = expected_revision {
            let current: Document<AuthorQueryDocument> =
                self.read_unlocked(AUTHOR_QUERY_FILE, MAX_AUTHOR_QUERY_BYTES)?;
            if current.revision != expected {
                return Err(StoreError::new("DISCOVERY_POLICY_CHANGED"));
            }
        }
        Ok(())
    }
}

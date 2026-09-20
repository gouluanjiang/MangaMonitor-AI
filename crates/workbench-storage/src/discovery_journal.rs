//! Small, immutable author-discovery page commits over an unchanged legacy base.
//!
//! The manifest is the only commit point. A page written before a failed manifest
//! replacement is uncommitted and retained; every referenced page is hash checked
//! during loading. The writer cache contains keys and encoded sizes, never a second
//! copy of the catalog's cold metadata.
use crate::{
    discovery::{DISCOVERY_FILE, MAX_DISCOVERY_ACCOUNTS},
    model::ValidatedDocument,
    store::{
        check_directory_tree, check_optional_regular, read_regular_bounded, MAX_FOLLOWING_BYTES,
    },
    AccountFollowing, DiscoveryAccount, DiscoveryAuthorRange, DiscoveryDocument, DiscoveryRecord,
    Document, Result, Source, StoreError, WorkbenchStore, MAX_DISCOVERY_AUTHORS,
    MAX_DISCOVERY_BYTES, MAX_DISCOVERY_RAW_BYTES, MAX_DISCOVERY_RAW_RECORDS, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, collections::HashSet, time::SystemTime};

const MANIFEST: &str = "discovery-journal.json";
const JOURNAL_REQUIRED: &str = "discovery-journal-required.json";
const JOURNAL_REQUIRED_BYTES: &[u8] = b"{\"version\":1}";
const MANIFEST_BYTES: usize = 4096;
const MAX_PATCH_BYTES: usize = 64 * 1024 * 1024;
const MAX_PATCH_RECORDS: usize = 1000;
// Uncompacted pages are independently bounded. Hitting this limit is an explicit
// incomplete result; records are never discarded to make an incoming page fit.
const MAX_JOURNAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_JOURNAL_PATCHES: u64 = 100_000;
const CATALOG_ENVELOPE_ALLOWANCE: usize = 64 * 1024;

/// A native page transaction, never a renderer-supplied whole-document replacement.
/// Records are complete upserts already merged by the caller. No operation deletes
/// saved records. `retain_authors` reconciles only the account's visible range rows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryPagePatch {
    pub account_key: String,
    pub authors: Vec<DiscoveryAuthorRange>,
    pub records: Vec<DiscoveryRecord>,
    pub retain_authors: Option<Vec<String>>,
}

impl DiscoveryPagePatch {
    fn validate(&self) -> Result<()> {
        if self.records.len() > MAX_PATCH_RECORDS {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        if let Some(retained) = &self.retain_authors {
            let names: HashSet<_> = retained.iter().collect();
            if retained.len() > MAX_DISCOVERY_AUTHORS
                || retained.len() != names.len()
                || retained
                    .iter()
                    .any(|name| !crate::discovery_author_is_valid(name))
                || self
                    .authors
                    .iter()
                    .any(|range| !names.contains(&range.author))
            {
                return Err(StoreError::new("VALIDATION_FAILED"));
            }
        }
        // This validation is bounded by the incoming page, not the saved history.
        DiscoveryDocument {
            version: 1,
            accounts: vec![DiscoveryAccount {
                account_key: self.account_key.clone(),
                authors: self.authors.clone(),
                records: self.records.clone(),
            }],
        }
        .validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    base_revision: u64,
    base_sha256: Option<String>,
    revision: u64,
    head_sha256: Option<String>,
    checkpoint: Option<Checkpoint>,
    patch_count: u64,
    journal_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    revision: u64,
    sha256: String,
    bytes: u64,
}

impl Manifest {
    fn validate(&self) -> Result<()> {
        if self.version > 1 {
            return Err(StoreError::new("UNSUPPORTED_SCHEMA"));
        }
        if self.version != 1
            || self.base_revision > MAX_SAFE_INTEGER
            || self.revision > MAX_SAFE_INTEGER
            || self.patch_count > MAX_JOURNAL_PATCHES
            || self.chain_base_revision().checked_add(self.patch_count) != Some(self.revision)
            || self
                .head_sha256
                .as_ref()
                .is_some_and(|hash| !crate::library_hash_is_valid(hash))
            || (self.patch_count == 0) != self.head_sha256.is_none()
            || (self.patch_count == 0) != (self.journal_bytes == 0)
            || (self.patch_count == 0 && self.checkpoint.is_none())
            || self.checkpoint.as_ref().is_some_and(|checkpoint| {
                checkpoint.revision < self.base_revision
                    || checkpoint.revision > self.revision
                    || !crate::library_hash_is_valid(&checkpoint.sha256)
                    || checkpoint.bytes == 0
                    || checkpoint.bytes > MAX_DISCOVERY_RAW_BYTES as u64
            })
            || self
                .base_sha256
                .as_ref()
                .is_some_and(|hash| !crate::library_hash_is_valid(hash))
            || (self.base_revision == 0) != self.base_sha256.is_none()
            || self.journal_bytes > MAX_JOURNAL_BYTES
        {
            return Err(corrupt());
        }
        Ok(())
    }

    fn chain_base_revision(&self) -> u64 {
        self.checkpoint
            .as_ref()
            .map_or(self.base_revision, |checkpoint| checkpoint.revision)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PatchEnvelope {
    version: u32,
    previous_revision: u64,
    previous_sha256: Option<String>,
    revision: u64,
    patch: DiscoveryPagePatch,
}

// Traversing the chain first collects only references; cold page bodies are not
// accumulated in memory alongside the caller's complete document.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchHeader {
    version: u32,
    previous_revision: u64,
    previous_sha256: Option<String>,
    revision: u64,
}

#[derive(Eq, PartialEq)]
struct BaseStamp {
    bytes: u64,
    modified: SystemTime,
}

#[derive(Default)]
struct AccountIndex {
    records: HashMap<(Source, String), usize>,
    ranges: HashMap<(Source, String), usize>,
}

#[derive(Default)]
struct RawIndex {
    accounts: HashMap<String, AccountIndex>,
    record_count: usize,
    encoded_bytes: usize,
}

pub(crate) struct DiscoveryIndexCache {
    manifest: Option<Manifest>,
    base_revision: u64,
    base_sha256: Option<String>,
    base_stamp: Option<BaseStamp>,
    index: RawIndex,
}

struct PreparedPatch {
    record_count: usize,
    encoded_bytes: usize,
    record_sizes: Vec<usize>,
    range_sizes: Vec<usize>,
}

type RecordPositions = HashMap<String, HashMap<(Source, String), usize>>;

fn corrupt() -> StoreError {
    StoreError::new("DOCUMENT_CORRUPT")
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn patch_name(hash: &str) -> String {
    format!("discovery-page-{hash}.json")
}

fn checkpoint_name(hash: &str) -> String {
    format!("discovery-checkpoint-{hash}.json")
}

fn encoded_size(value: &impl Serialize) -> Result<usize> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() + 1)
        .map_err(|_| StoreError::new("VALIDATION_FAILED"))
}

impl RawIndex {
    fn from_document(value: &DiscoveryDocument) -> Result<Self> {
        let mut index = Self {
            encoded_bytes: CATALOG_ENVELOPE_ALLOWANCE,
            ..Self::default()
        };
        for account in &value.accounts {
            let mut entries = AccountIndex::default();
            for record in &account.records {
                let bytes = encoded_size(record)?;
                entries
                    .records
                    .insert((record.work.source, record.work.work_id.clone()), bytes);
                index.record_count += 1;
                index.encoded_bytes += bytes;
            }
            for range in &account.authors {
                let bytes = encoded_size(range)?;
                entries
                    .ranges
                    .insert((range.source, range.author.clone()), bytes);
                index.encoded_bytes += bytes;
            }
            index.accounts.insert(account.account_key.clone(), entries);
        }
        if index.record_count > MAX_DISCOVERY_RAW_RECORDS
            || index.encoded_bytes > MAX_DISCOVERY_RAW_BYTES
        {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        Ok(index)
    }

    fn prepare(&self, patch: &DiscoveryPagePatch) -> Result<PreparedPatch> {
        let account = self.accounts.get(&patch.account_key);
        if account.is_none() && self.accounts.len() >= MAX_DISCOVERY_ACCOUNTS {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let retained = patch
            .retain_authors
            .as_ref()
            .map(|names| names.iter().map(String::as_str).collect::<HashSet<_>>());
        let mut ranges: HashMap<_, _> = account
            .into_iter()
            .flat_map(|account| account.ranges.iter())
            .filter(|((_, author), _)| {
                retained
                    .as_ref()
                    .is_none_or(|names| names.contains(author.as_str()))
            })
            .map(|(key, bytes)| (key.clone(), *bytes))
            .collect();
        let old_range_bytes: usize = account
            .into_iter()
            .flat_map(|account| account.ranges.values())
            .sum();
        let mut encoded_bytes = self.encoded_bytes - old_range_bytes;
        let mut range_sizes = Vec::with_capacity(patch.authors.len());
        for range in &patch.authors {
            let bytes = encoded_size(range)?;
            ranges.insert((range.source, range.author.clone()), bytes);
            range_sizes.push(bytes);
        }
        if ranges.len() > MAX_DISCOVERY_AUTHORS * 2 {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        encoded_bytes += ranges.values().sum::<usize>();
        let mut record_count = self.record_count;
        let mut record_sizes = Vec::with_capacity(patch.records.len());
        for record in &patch.records {
            let key = (record.work.source, record.work.work_id.clone());
            if let Some(previous) = account.and_then(|account| account.records.get(&key)) {
                encoded_bytes -= previous;
            } else {
                record_count += 1;
            }
            let bytes = encoded_size(record)?;
            encoded_bytes += bytes;
            record_sizes.push(bytes);
        }
        if record_count > MAX_DISCOVERY_RAW_RECORDS || encoded_bytes > MAX_DISCOVERY_RAW_BYTES {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        Ok(PreparedPatch {
            record_count,
            encoded_bytes,
            record_sizes,
            range_sizes,
        })
    }

    fn apply(&mut self, patch: &DiscoveryPagePatch, prepared: PreparedPatch) {
        let account = self.accounts.entry(patch.account_key.clone()).or_default();
        if let Some(retained) = &patch.retain_authors {
            let names: HashSet<_> = retained.iter().map(String::as_str).collect();
            account
                .ranges
                .retain(|(_, author), _| names.contains(author.as_str()));
        }
        for (record, bytes) in patch.records.iter().zip(prepared.record_sizes) {
            account
                .records
                .insert((record.work.source, record.work.work_id.clone()), bytes);
        }
        for (range, bytes) in patch.authors.iter().zip(prepared.range_sizes) {
            account
                .ranges
                .insert((range.source, range.author.clone()), bytes);
        }
        self.record_count = prepared.record_count;
        self.encoded_bytes = prepared.encoded_bytes;
    }
}

impl WorkbenchStore {
    fn discovery_manifest_unlocked(&self) -> Result<Option<Manifest>> {
        let marker = self.root.join(JOURNAL_REQUIRED);
        let journal_required = check_optional_regular(&marker)?.is_some();
        if journal_required && read_regular_bounded(&marker, 64)? != JOURNAL_REQUIRED_BYTES {
            return Err(corrupt());
        }
        let path = self.root.join(MANIFEST);
        if check_optional_regular(&path)?.is_none() {
            // Once a successful transaction adopted the journal, a missing
            // manifest cannot silently revert to an older legacy catalog.
            if journal_required {
                return Err(corrupt());
            }
            return Ok(None);
        }
        let bytes = read_regular_bounded(&path, MANIFEST_BYTES)?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
        manifest.validate()?;
        Ok(Some(manifest))
    }

    fn discovery_base_stamp_unlocked(&self) -> Result<Option<BaseStamp>> {
        check_optional_regular(&self.root.join(DISCOVERY_FILE))?
            .map(|metadata| {
                Ok(BaseStamp {
                    bytes: metadata.len(),
                    modified: metadata
                        .modified()
                        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?,
                })
            })
            .transpose()
    }

    pub(crate) fn require_legacy_discovery_unlocked(&self) -> Result<()> {
        if self.discovery_manifest_unlocked()?.is_some() {
            return Err(StoreError::new("DISCOVERY_JOURNAL_ACTIVE"));
        }
        Ok(())
    }

    fn read_discovery_patch_unlocked(&self, expected_hash: &str) -> Result<Vec<u8>> {
        if !crate::library_hash_is_valid(expected_hash) {
            return Err(corrupt());
        }
        let bytes =
            read_regular_bounded(&self.root.join(patch_name(expected_hash)), MAX_PATCH_BYTES)?;
        if hash(&bytes) != expected_hash {
            return Err(corrupt());
        }
        Ok(bytes)
    }

    fn discovery_chain_unlocked(&self, manifest: &Manifest) -> Result<Vec<String>> {
        let mut expected = manifest.head_sha256.clone();
        let mut revision = manifest.revision;
        let mut bytes_read = 0u64;
        let mut chain = Vec::with_capacity(manifest.patch_count as usize);
        for _ in 0..manifest.patch_count {
            let current = expected.take().ok_or_else(corrupt)?;
            let bytes = self.read_discovery_patch_unlocked(&current)?;
            bytes_read += bytes.len() as u64;
            if bytes_read > manifest.journal_bytes {
                return Err(corrupt());
            }
            let header: PatchHeader = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
            if header.version != 1
                || header.revision != revision
                || header.previous_revision.checked_add(1) != Some(revision)
                || header.previous_revision < manifest.chain_base_revision()
            {
                return Err(corrupt());
            }
            revision = header.previous_revision;
            expected = header.previous_sha256;
            chain.push(current);
        }
        if expected.is_some()
            || revision != manifest.chain_base_revision()
            || bytes_read != manifest.journal_bytes
        {
            return Err(corrupt());
        }
        chain.reverse();
        Ok(chain)
    }

    fn load_discovery_unlocked(
        &self,
        manifest: Option<Manifest>,
    ) -> Result<(Document<DiscoveryDocument>, DiscoveryIndexCache)> {
        let mut document: Document<DiscoveryDocument> =
            self.read_unlocked(DISCOVERY_FILE, MAX_DISCOVERY_BYTES)?;
        let base_stamp = self.discovery_base_stamp_unlocked()?;
        let base_sha256 = if base_stamp.is_some() {
            Some(hash(&read_regular_bounded(
                &self.root.join(DISCOVERY_FILE),
                MAX_DISCOVERY_BYTES,
            )?))
        } else {
            None
        };
        let base_revision = document.revision;
        if manifest.as_ref().is_some_and(|manifest| {
            manifest.base_revision != base_revision || manifest.base_sha256 != base_sha256
        }) {
            return Err(corrupt());
        }
        if let Some(checkpoint) = manifest
            .as_ref()
            .and_then(|manifest| manifest.checkpoint.as_ref())
        {
            let bytes = read_regular_bounded(
                &self.root.join(checkpoint_name(&checkpoint.sha256)),
                MAX_DISCOVERY_RAW_BYTES,
            )?;
            if bytes.len() as u64 != checkpoint.bytes || hash(&bytes) != checkpoint.sha256 {
                return Err(corrupt());
            }
            document = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
            if document.revision != checkpoint.revision {
                return Err(corrupt());
            }
            document.value.validate().map_err(|_| corrupt())?;
        }
        let mut index = RawIndex::from_document(&document.value)?;
        let mut positions: RecordPositions = document
            .value
            .accounts
            .iter()
            .map(|account| {
                (
                    account.account_key.clone(),
                    account
                        .records
                        .iter()
                        .enumerate()
                        .map(|(position, record)| {
                            ((record.work.source, record.work.work_id.clone()), position)
                        })
                        .collect(),
                )
            })
            .collect();
        if let Some(manifest) = &manifest {
            for reference in self.discovery_chain_unlocked(manifest)? {
                let bytes = self.read_discovery_patch_unlocked(&reference)?;
                let envelope: PatchEnvelope =
                    serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
                envelope.patch.validate().map_err(|_| corrupt())?;
                let prepared = index.prepare(&envelope.patch).map_err(|_| corrupt())?;
                index.apply(&envelope.patch, prepared);
                apply_to_document(&mut document.value, &mut positions, envelope.patch);
                document.revision = envelope.revision;
            }
            if document.revision != manifest.revision {
                return Err(corrupt());
            }
        }
        Ok((
            document,
            DiscoveryIndexCache {
                manifest,
                base_revision,
                base_sha256,
                base_stamp,
                index,
            },
        ))
    }

    pub(crate) fn read_discovery_journal(&self) -> Result<Document<DiscoveryDocument>> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let loaded = self.load_discovery_unlocked(self.discovery_manifest_unlocked()?);
        let mut cache = self
            .discovery_index
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        match loaded {
            Ok((document, index)) => {
                *cache = Some(index);
                Ok(document)
            }
            Err(error) => {
                *cache = None;
                Err(error)
            }
        }
    }

    /// Commits one page while sharing the following document's cross-process lock.
    /// Reuse this store instance through a scan so only the first access loads the
    /// base and prior pages; later writes inspect the manifest and page-sized delta.
    pub fn apply_discovery_patch_for_following(
        &self,
        expected_revision: u64,
        following_revision: u64,
        patch: DiscoveryPagePatch,
    ) -> Result<u64> {
        if expected_revision >= MAX_SAFE_INTEGER {
            return Err(StoreError::new("REVISION_EXHAUSTED"));
        }
        patch.validate()?;
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let following: Document<AccountFollowing> =
            self.read_unlocked("following.json", MAX_FOLLOWING_BYTES)?;
        if following.revision != following_revision {
            return Err(StoreError::new("DISCOVERY_FOLLOWING_CHANGED"));
        }
        let manifest = self.discovery_manifest_unlocked()?;
        let stamp = self.discovery_base_stamp_unlocked()?;
        let mut guard = self
            .discovery_index
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        if guard
            .as_ref()
            .is_none_or(|cache| cache.manifest != manifest || cache.base_stamp != stamp)
        {
            let (_, cache) = self.load_discovery_unlocked(manifest)?;
            *guard = Some(cache);
        }
        let cache = guard.as_mut().expect("discovery index was loaded");
        let revision = cache
            .manifest
            .as_ref()
            .map_or(cache.base_revision, |manifest| manifest.revision);
        if revision != expected_revision {
            return Err(StoreError::new("REVISION_CONFLICT"));
        }
        let prepared = cache.index.prepare(&patch)?;
        let retained_bytes = patch
            .retain_authors
            .as_ref()
            .map(encoded_size)
            .transpose()?
            .unwrap_or(0);
        if prepared.record_sizes.iter().sum::<usize>()
            + prepared.range_sizes.iter().sum::<usize>()
            + retained_bytes
            + CATALOG_ENVELOPE_ALLOWANCE
            > MAX_PATCH_BYTES
        {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        let next_revision = revision + 1;
        let envelope = PatchEnvelope {
            version: 1,
            previous_revision: revision,
            previous_sha256: cache
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.head_sha256.clone()),
            revision: next_revision,
            patch,
        };
        let bytes =
            serde_json::to_vec(&envelope).map_err(|_| StoreError::new("VALIDATION_FAILED"))?;
        if bytes.len() > MAX_PATCH_BYTES {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        let next = Manifest {
            version: 1,
            base_revision: cache.base_revision,
            base_sha256: cache.base_sha256.clone(),
            revision: next_revision,
            head_sha256: Some(hash(&bytes)),
            checkpoint: cache
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.checkpoint.clone()),
            patch_count: cache
                .manifest
                .as_ref()
                .map_or(1, |manifest| manifest.patch_count + 1),
            journal_bytes: cache
                .manifest
                .as_ref()
                .map_or(0, |manifest| manifest.journal_bytes)
                + bytes.len() as u64,
        };
        if next.patch_count > MAX_JOURNAL_PATCHES || next.journal_bytes > MAX_JOURNAL_BYTES {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        let manifest_bytes =
            serde_json::to_vec(&next).map_err(|_| StoreError::new("VALIDATION_FAILED"))?;
        let written = (|| {
            let content_hash = next
                .head_sha256
                .as_ref()
                .expect("a committed page has a hash");
            let name = patch_name(content_hash);
            if check_optional_regular(&self.root.join(&name))?.is_some() {
                // A retry can reuse its uncommitted page, but never replace a
                // corrupt file at a content-addressed location.
                if self.read_discovery_patch_unlocked(content_hash)? != bytes {
                    return Err(corrupt());
                }
            } else {
                self.atomic_replace(&name, &bytes)?;
            }
            self.atomic_replace(MANIFEST, &manifest_bytes)?;
            if check_optional_regular(&self.root.join(JOURNAL_REQUIRED))?.is_none() {
                // A failure here follows a committed manifest. A retry must read
                // the current revision instead of resubmitting the same page.
                self.atomic_replace(JOURNAL_REQUIRED, JOURNAL_REQUIRED_BYTES)
                    .map_err(|_| StoreError::new("COMMIT_UNCERTAIN"))?;
            }
            Ok(())
        })();
        if let Err(error) = written {
            *guard = None;
            return Err(error);
        }
        cache.index.apply(&envelope.patch, prepared);
        cache.manifest = Some(next);
        Ok(next_revision)
    }

    /// Explicit end-of-run compaction. This performs one complete catalog read and
    /// write, preserves its logical revision, and never modifies the legacy base.
    /// Only after the new manifest is durable can the retired application pages be
    /// reclaimed. An interrupted compaction leaves the old or new complete view.
    pub fn checkpoint_discovery_for_following(
        &self,
        expected_revision: u64,
        following_revision: u64,
    ) -> Result<()> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        let following: Document<AccountFollowing> =
            self.read_unlocked("following.json", MAX_FOLLOWING_BYTES)?;
        if following.revision != following_revision {
            return Err(StoreError::new("DISCOVERY_FOLLOWING_CHANGED"));
        }
        let previous = self.discovery_manifest_unlocked()?;
        let current_revision = if let Some(manifest) = &previous {
            manifest.revision
        } else {
            self.read_unlocked::<DiscoveryDocument>(DISCOVERY_FILE, MAX_DISCOVERY_BYTES)?
                .revision
        };
        if current_revision != expected_revision {
            return Err(StoreError::new("REVISION_CONFLICT"));
        }
        let Some(previous) = previous else {
            return Ok(());
        };
        if previous.patch_count == 0 {
            return Ok(());
        }
        let (document, mut cache) = self.load_discovery_unlocked(Some(previous.clone()))?;
        let retired = self.discovery_chain_unlocked(&previous)?;
        let bytes =
            serde_json::to_vec(&document).map_err(|_| StoreError::new("VALIDATION_FAILED"))?;
        if bytes.len() > MAX_DISCOVERY_RAW_BYTES {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        let checkpoint = Checkpoint {
            revision: document.revision,
            sha256: hash(&bytes),
            bytes: bytes.len() as u64,
        };
        let next = Manifest {
            checkpoint: Some(checkpoint.clone()),
            head_sha256: None,
            patch_count: 0,
            journal_bytes: 0,
            ..previous.clone()
        };
        let manifest_bytes =
            serde_json::to_vec(&next).map_err(|_| StoreError::new("VALIDATION_FAILED"))?;
        let mut guard = self
            .discovery_index
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let written = (|| {
            let name = checkpoint_name(&checkpoint.sha256);
            if check_optional_regular(&self.root.join(&name))?.is_some() {
                if read_regular_bounded(&self.root.join(&name), MAX_DISCOVERY_RAW_BYTES)? != bytes {
                    return Err(corrupt());
                }
            } else {
                self.atomic_replace(&name, &bytes)?;
            }
            self.atomic_replace(MANIFEST, &manifest_bytes)
        })();
        if let Err(error) = written {
            *guard = None;
            return Err(error);
        }
        cache.manifest = Some(next);
        *guard = Some(cache);
        // Reclamation is best effort after a successful commit. Failure retains
        // unused application files; it never makes an incomplete catalog current.
        for reference in retired {
            self.remove_retired_discovery_file(&patch_name(&reference));
        }
        if let Some(old) = previous.checkpoint {
            if old.sha256 != checkpoint.sha256 {
                self.remove_retired_discovery_file(&checkpoint_name(&old.sha256));
            }
        }
        Ok(())
    }

    fn remove_retired_discovery_file(&self, name: &str) {
        let path = self.root.join(name);
        if check_directory_tree(&self.root).is_ok()
            && check_optional_regular(&path).is_ok_and(|metadata| metadata.is_some())
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn apply_to_document(
    value: &mut DiscoveryDocument,
    positions: &mut RecordPositions,
    patch: DiscoveryPagePatch,
) {
    let account_position = value
        .accounts
        .iter()
        .position(|account| account.account_key == patch.account_key)
        .unwrap_or_else(|| {
            value.accounts.push(DiscoveryAccount {
                account_key: patch.account_key.clone(),
                authors: vec![],
                records: vec![],
            });
            value.accounts.len() - 1
        });
    let account = &mut value.accounts[account_position];
    if let Some(retained) = patch.retain_authors {
        let names: HashSet<_> = retained.into_iter().collect();
        account
            .authors
            .retain(|range| names.contains(&range.author));
    }
    for range in patch.authors {
        if let Some(previous) = account
            .authors
            .iter_mut()
            .find(|previous| previous.source == range.source && previous.author == range.author)
        {
            *previous = range;
        } else {
            account.authors.push(range);
        }
    }
    let records = positions.entry(patch.account_key).or_default();
    for record in patch.records {
        let key = (record.work.source, record.work.work_id.clone());
        if let Some(position) = records.get(&key) {
            account.records[*position] = record;
        } else {
            records.insert(key, account.records.len());
            account.records.push(record);
        }
    }
}

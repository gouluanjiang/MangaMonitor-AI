//! User-confirmed cross-source identities. These never create library or phone records.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, Document, LibraryReference, Result, Source,
    StoreError, WorkbenchStore, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, time::SystemTime};

const MATCHES_FILE: &str = "source-matches.json";
const MAX_MATCHES_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SOURCE_MATCHES: usize = 10_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMatchWork {
    pub source: Source,
    pub work_id: String,
    pub title: String,
}

impl SourceMatchWork {
    fn is_valid(&self, source: Source) -> bool {
        self.source == source
            && LibraryReference {
                source: self.source,
                work_id: self.work_id.clone(),
            }
            .is_valid()
            && !self.title.trim().is_empty()
            && self.title.chars().count() <= 1024
            && !self.title.chars().any(char::is_control)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceMatchEvidence {
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMatchPair {
    pub id: String,
    pub jm: SourceMatchWork,
    pub pica: SourceMatchWork,
    pub confirmed_at: u64,
    pub evidence: SourceMatchEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMatchesDocument {
    pub version: u32,
    pub pairs: Vec<SourceMatchPair>,
}

impl Default for SourceMatchesDocument {
    fn default() -> Self {
        Self {
            version: 1,
            pairs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMatchesSnapshot {
    pub revision: u64,
    pub pairs: Vec<SourceMatchPair>,
}

impl From<Document<SourceMatchesDocument>> for SourceMatchesSnapshot {
    fn from(document: Document<SourceMatchesDocument>) -> Self {
        Self {
            revision: document.revision,
            pairs: document.value.pairs,
        }
    }
}

fn pair_id(jm: &str, pica: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"source-match-v1\0JM\0");
    digest.update(jm.as_bytes());
    digest.update(b"\0Pica\0");
    digest.update(pica.as_bytes());
    format!("{:x}", digest.finalize())
}

impl ValidatedDocument for SourceMatchesDocument {
    fn validate(&self) -> Result<()> {
        if self.version != 1 || self.pairs.len() > MAX_SOURCE_MATCHES {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let mut jm = HashSet::new();
        let mut pica = HashSet::new();
        for pair in &self.pairs {
            if !pair.jm.is_valid(Source::Jm)
                || !pair.pica.is_valid(Source::Pica)
                || pair.confirmed_at > MAX_SAFE_INTEGER
                || pair.id != pair_id(&pair.jm.work_id, &pair.pica.work_id)
                || !jm.insert(&pair.jm.work_id)
                || !pica.insert(&pair.pica.work_id)
            {
                return Err(StoreError::new("VALIDATION_FAILED"));
            }
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_source_matches(&self) -> Result<Document<SourceMatchesDocument>> {
        self.read(MATCHES_FILE, MAX_MATCHES_BYTES)
    }
}

pub fn source_matches_read(store: &WorkbenchStore) -> Result<SourceMatchesSnapshot> {
    store.read_source_matches().map(Into::into)
}

/// Only this explicit command establishes an alias; titles are display data, never proof.
pub fn source_matches_confirm(
    store: &WorkbenchStore,
    revision: u64,
    jm: SourceMatchWork,
    pica: SourceMatchWork,
) -> Result<SourceMatchesSnapshot> {
    if !jm.is_valid(Source::Jm) || !pica.is_valid(Source::Pica) {
        return Err(StoreError::new("SOURCE_MATCH_INVALID"));
    }
    let mut current = store.read_source_matches()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    let id = pair_id(&jm.work_id, &pica.work_id);
    if current.value.pairs.iter().any(|pair| pair.id == id) {
        return Ok(current.into());
    }
    if current
        .value
        .pairs
        .iter()
        .any(|pair| pair.jm.work_id == jm.work_id || pair.pica.work_id == pica.work_id)
    {
        return Err(StoreError::new("SOURCE_MATCH_CONFLICT"));
    }
    if current.value.pairs.len() >= MAX_SOURCE_MATCHES {
        return Err(StoreError::new("SOURCE_MATCH_LIMIT"));
    }
    let confirmed_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or(StoreError::new("SOURCE_MATCH_UNAVAILABLE"))?;
    current.value.pairs.push(SourceMatchPair {
        id,
        jm,
        pica,
        confirmed_at,
        evidence: SourceMatchEvidence::Manual,
    });
    store
        .write(MATCHES_FILE, MAX_MATCHES_BYTES, revision, current.value)
        .map(Into::into)
}

/// Removes identity metadata only. Download history, both libraries and media are untouched.
pub fn source_matches_unlink(
    store: &WorkbenchStore,
    revision: u64,
    pair_id: &str,
) -> Result<SourceMatchesSnapshot> {
    if !library_hash_is_valid(pair_id) {
        return Err(StoreError::new("SOURCE_MATCH_INVALID"));
    }
    let mut current = store.read_source_matches()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    let Some(position) = current
        .value
        .pairs
        .iter()
        .position(|pair| pair.id == pair_id)
    else {
        return Err(StoreError::new("SOURCE_MATCH_NOT_FOUND"));
    };
    current.value.pairs.remove(position);
    store
        .write(MATCHES_FILE, MAX_MATCHES_BYTES, revision, current.value)
        .map(Into::into)
}

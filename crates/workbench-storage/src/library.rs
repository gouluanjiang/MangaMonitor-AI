//! Private metadata for one explicitly selected, read-only existing library.
use crate::{model::ValidatedDocument, Result, Source, StoreError, MAX_SAFE_INTEGER};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Component, Path},
};

pub const MAX_LIBRARY_ITEMS: usize = 20_000;
pub const MAX_LIBRARY_VISITED: u64 = 5_000_000;
pub const MAX_LIBRARY_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryReference {
    pub source: Source,
    pub work_id: String,
}

impl LibraryReference {
    pub fn is_valid(&self) -> bool {
        match self.source {
            Source::Jm => {
                self.work_id.starts_with(|c: char| ('1'..='9').contains(&c))
                    && self.work_id.len() <= 20
                    && self.work_id.bytes().all(|b| b.is_ascii_digit())
            }
            Source::Pica => {
                self.work_id.len() == 24
                    && self
                        .work_id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryFormat {
    Zip,
    Cbz,
    Rar,
    Directory,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryItemState {
    Indexed,
    Unreadable,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryEvidence {
    Metadata,
    Filename,
    Manual,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryPhase {
    Idle,
    Reading,
    Paused,
    Complete,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryItem {
    pub id: String,
    pub relative_path: String,
    pub file_name: String,
    pub format: LibraryFormat,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub bytes: u64,
    pub modified_at: Option<u64>,
    /// First successful registration. Legacy entries retain unknown dates.
    #[serde(default)]
    pub added_at: Option<u64>,
    /// Source update time captured for this local version, never the scan time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_updated_at: Option<String>,
    pub page_count: Option<u64>,
    pub cover_available: bool,
    pub state: LibraryItemState,
    pub error_code: Option<String>,
    pub source_ref: Option<LibraryReference>,
    pub identity_evidence: Option<LibraryEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LibrarySourceLink>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryLinkEvidence {
    TitleAuthorPages,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibrarySourceLink {
    pub reference: LibraryReference,
    pub evidence: LibraryLinkEvidence,
    pub linked_at: u64,
}

impl LibraryItem {
    pub fn references(&self) -> impl Iterator<Item = &LibraryReference> {
        self.source_ref
            .iter()
            .chain(self.links.iter().map(|link| &link.reference))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryFileIdentity {
    pub file_key: String,
    pub bytes: u64,
    // Exact OS timestamp text avoids precision loss through a JSON number.
    pub modified: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryRecord {
    pub item: LibraryItem,
    pub identity: Option<LibraryFileIdentity>,
    // Explicit unlink remains authoritative through refresh of the same file.
    pub manual_override: bool,
    pub cover: Option<LibraryCoverFile>,
}

/// User-imported, hash-verified container relocation. Kept separately from old
/// task receipts: renaming a library file never rewrites download authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryRelocation {
    pub old_path: String,
    pub old_item_id: String,
    pub new_path: String,
    pub new_item_id: String,
    pub identity: LibraryFileIdentity,
    pub sha256: String,
}

/// Explicitly reviewed old-library evidence. This is neither a download receipt
/// nor a title/author inference; only the bounded local review importer writes it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedLibraryWork {
    pub reference: LibraryReference,
    pub relative_path: String,
    pub library_entry_id: String,
    pub identity: LibraryFileIdentity,
    pub sha256: String,
    pub review_sha256: String,
    pub registered_at: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryCoverFile {
    // Relative to the selected root for a directory work; archive entry name for ZIP.
    pub relative_path: String,
    pub identity: Option<LibraryFileIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryRoot {
    pub id: String,
    pub path: String,
    pub file_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryDocument {
    pub version: u32,
    pub root: Option<LibraryRoot>,
    pub generation: u64,
    pub phase: LibraryPhase,
    pub records: Vec<LibraryRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relocations: Vec<LibraryRelocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_works: Vec<ReviewedLibraryWork>,
    pub visited: u64,
    pub skipped: u64,
    pub updated_at: Option<u64>,
    pub error_code: Option<String>,
}

impl Default for LibraryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            root: None,
            generation: 0,
            phase: LibraryPhase::Idle,
            records: Vec::new(),
            relocations: Vec::new(),
            reviewed_works: Vec::new(),
            visited: 0,
            skipped: 0,
            updated_at: None,
            error_code: None,
        }
    }
}

impl LibraryDocument {
    pub fn relocated_path(&self, old_path: &str) -> Option<&LibraryRelocation> {
        self.relocations
            .iter()
            .find(|item| item.old_path == old_path)
    }
}

/// Only native-picked mapping files reach this bounded reader.
pub fn library_path_mapping_bytes(path: &Path) -> Result<Vec<u8>> {
    crate::store::read_regular_bounded(path, MAX_LIBRARY_DOCUMENT_BYTES)
}

pub fn library_hash_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn library_relative_path_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.contains('\\')
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.len() <= 1024
                && !part.contains(':')
                && !part.chars().any(char::is_control)
        })
}

fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}

fn error_code(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|v| {
        !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
    })
}

impl ValidatedDocument for LibraryDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1
            || self.generation > MAX_SAFE_INTEGER
            || self.visited > MAX_LIBRARY_VISITED
            || self.skipped > self.visited
            || self.records.len() > MAX_LIBRARY_ITEMS
            || self.relocations.len() > MAX_LIBRARY_ITEMS
            || self.reviewed_works.len() > MAX_LIBRARY_ITEMS * 2
            || self.records.len() as u64 > self.visited
            || self.updated_at.is_some_and(|v| v > MAX_SAFE_INTEGER)
            || !error_code(&self.error_code)
        {
            return Err(invalid());
        }
        if let Some(root) = &self.root {
            let path = Path::new(&root.path);
            if !library_hash_is_valid(&root.id)
                || !library_hash_is_valid(&root.file_key)
                || root.path.len() > 32768
                || root.path.chars().any(char::is_control)
                || !path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
                || self.generation == 0
                || self.phase == LibraryPhase::Idle
            {
                return Err(invalid());
            }
        } else if self.generation != 0
            || self.phase != LibraryPhase::Idle
            || !self.records.is_empty()
            || !self.relocations.is_empty()
            || !self.reviewed_works.is_empty()
            || self.visited != 0
            || self.skipped != 0
            || self.updated_at.is_some()
            || self.error_code.is_some()
        {
            return Err(invalid());
        }
        if (self.phase == LibraryPhase::Error) != self.error_code.is_some() {
            return Err(invalid());
        }
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        let mut old_paths = HashSet::new();
        let mut reviewed_references = HashSet::new();
        for reviewed in &self.reviewed_works {
            if !reviewed.reference.is_valid()
                || !reviewed_references
                    .insert((reviewed.reference.source, &reviewed.reference.work_id))
                || !library_relative_path_is_valid(&reviewed.relative_path)
                || !reviewed
                    .relative_path
                    .to_ascii_lowercase()
                    .ends_with(".zip")
                || !library_hash_is_valid(&reviewed.library_entry_id)
                || !library_hash_is_valid(&reviewed.sha256)
                || !library_hash_is_valid(&reviewed.review_sha256)
                || !library_hash_is_valid(&reviewed.identity.file_key)
                || reviewed.identity.bytes == 0
                || reviewed.identity.bytes > MAX_SAFE_INTEGER
                || reviewed.identity.modified.is_empty()
                || reviewed.identity.modified.len() > 64
                || !reviewed
                    .identity
                    .modified
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b':' || b == b'-')
                || reviewed.registered_at > MAX_SAFE_INTEGER
            {
                return Err(invalid());
            }
        }
        for relocation in &self.relocations {
            if !library_relative_path_is_valid(&relocation.old_path)
                || !library_relative_path_is_valid(&relocation.new_path)
                || relocation.old_path == relocation.new_path
                || !relocation.new_path.to_ascii_lowercase().ends_with(".zip")
                || !old_paths.insert(&relocation.old_path)
                || !library_hash_is_valid(&relocation.old_item_id)
                || !library_hash_is_valid(&relocation.new_item_id)
                || !library_hash_is_valid(&relocation.sha256)
                || !library_hash_is_valid(&relocation.identity.file_key)
                || relocation.identity.bytes == 0
                || relocation.identity.bytes > MAX_SAFE_INTEGER
                || relocation.identity.modified.is_empty()
                || relocation.identity.modified.len() > 64
                || !relocation
                    .identity
                    .modified
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b':' || b == b'-')
            {
                return Err(invalid());
            }
        }
        for record in &self.records {
            let item = &record.item;
            if !library_hash_is_valid(&item.id)
                || !ids.insert(&item.id)
                || !paths.insert(&item.relative_path)
                || !library_relative_path_is_valid(&item.relative_path)
                || item.relative_path.rsplit('/').next() != Some(item.file_name.as_str())
                || !text(&item.title, 1024)
                || item.authors.len() > 50
                || item.tags.len() > 100
                || item.authors.iter().any(|v| !text(v, 200))
                || item.tags.iter().any(|v| !text(v, 200))
                || item.description.as_ref().is_some_and(|v| !text(v, 4096))
                || item.bytes > MAX_SAFE_INTEGER
                || item.modified_at.is_some_and(|v| v > MAX_SAFE_INTEGER)
                || item.added_at.is_some_and(|v| v > MAX_SAFE_INTEGER)
                || item
                    .version_updated_at
                    .as_deref()
                    .is_some_and(|v| !crate::work_date_is_valid(v))
                || item.page_count.is_some_and(|v| v > 60_000)
                || !error_code(&item.error_code)
                || item.source_ref.as_ref().is_some_and(|v| !v.is_valid())
                || item.source_ref.is_some() != item.identity_evidence.is_some()
                || item.links.len() > 2
                || item
                    .links
                    .iter()
                    .any(|link| !link.reference.is_valid() || link.linked_at > MAX_SAFE_INTEGER)
                || item
                    .references()
                    .map(|reference| reference.source)
                    .collect::<HashSet<_>>()
                    .len()
                    != item.references().count()
                || (item.identity_evidence == Some(LibraryEvidence::Manual)
                    && !record.manual_override)
                || (record.manual_override
                    && item.source_ref.is_some()
                    && item.identity_evidence != Some(LibraryEvidence::Manual))
                || (item.state != LibraryItemState::Indexed
                    && (item.cover_available || item.error_code.is_none()))
                || (item.cover_available
                    && item
                        .page_count
                        .is_none_or(|v| v == 0 && item.format != LibraryFormat::Directory))
            {
                return Err(invalid());
            }
            let suffix = item
                .file_name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if item.format != LibraryFormat::Directory
                && !matches!(
                    (item.format, suffix.as_str()),
                    (LibraryFormat::Zip, "zip")
                        | (LibraryFormat::Cbz, "cbz")
                        | (LibraryFormat::Rar, "rar")
                )
            {
                return Err(invalid());
            }
            if let Some(identity) = &record.identity {
                if !library_hash_is_valid(&identity.file_key)
                    || (item.format != LibraryFormat::Directory && identity.bytes != item.bytes)
                    || identity.modified.is_empty()
                    || identity.modified.len() > 64
                    || !identity
                        .modified
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b == b':' || b == b'-')
                {
                    return Err(invalid());
                }
            } else if item.state == LibraryItemState::Indexed || record.manual_override {
                return Err(invalid());
            }
            if item.cover_available != record.cover.is_some() {
                return Err(invalid());
            }
            if let Some(cover) = &record.cover {
                if !library_relative_path_is_valid(&cover.relative_path)
                    || (item.format == LibraryFormat::Directory
                        && (!cover
                            .relative_path
                            .starts_with(&format!("{}/", item.relative_path))
                            || cover.identity.is_none()))
                    || (item.format != LibraryFormat::Directory && cover.identity.is_some())
                {
                    return Err(invalid());
                }
                if let Some(identity) = &cover.identity {
                    if !library_hash_is_valid(&identity.file_key)
                        || identity.bytes > MAX_SAFE_INTEGER
                        || identity.modified.is_empty()
                        || identity.modified.len() > 64
                        || !identity
                            .modified
                            .bytes()
                            .all(|b| b.is_ascii_digit() || b == b':' || b == b'-')
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        Ok(())
    }
}

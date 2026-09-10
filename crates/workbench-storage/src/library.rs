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
    pub page_count: Option<u64>,
    pub cover_available: bool,
    pub state: LibraryItemState,
    pub error_code: Option<String>,
    pub source_ref: Option<LibraryReference>,
    pub identity_evidence: Option<LibraryEvidence>,
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
            visited: 0,
            skipped: 0,
            updated_at: None,
            error_code: None,
        }
    }
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
                || item.page_count.is_some_and(|v| v > 10_000)
                || !error_code(&item.error_code)
                || item.source_ref.as_ref().is_some_and(|v| !v.is_valid())
                || item.source_ref.is_some() != item.identity_evidence.is_some()
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
                        .all(|b| b.is_ascii_digit() || b == ':' || b == '-')
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
                            .all(|b| b.is_ascii_digit() || b == ':' || b == '-')
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        Ok(())
    }
}

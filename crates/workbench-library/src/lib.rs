//! Read-only indexing of an explicitly selected PC library. This crate does not
//! download, extract, modify, move or delete user media, or claim phone presence.
mod archive;
mod cover;
mod metadata;
mod paths;
mod scan;
mod service;

use serde::{Deserialize, Serialize};
pub use service::LibraryService;
use sha2::{Digest, Sha256};
use workbench_storage::StoreError;
pub use workbench_storage::{
    LibraryEvidence, LibraryFormat, LibraryItem, LibraryItemState, LibraryPhase, LibraryReference,
};

pub type Result<T> = std::result::Result<T, StoreError>;
pub(crate) const fn error(code: &'static str) -> StoreError {
    StoreError { code }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanAction {
    Start,
    Next,
    Pause,
    Resume,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryFreshness {
    None,
    Cached,
    Live,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub revision: u64,
    pub root_id: Option<String>,
    pub root_path: Option<String>,
    pub generation: u64,
    pub phase: LibraryPhase,
    pub freshness: LibraryFreshness,
    pub items: Vec<LibraryItem>,
    pub visited: u64,
    pub skipped: u64,
    pub updated_at: Option<u64>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCover {
    pub root_id: String,
    pub generation: u64,
    pub entry_id: String,
    pub data_url: Option<String>,
}

//! Bounded, revisioned storage for the desktop preview's private UI documents.
//!
//! No library paths, credentials, download commands, or media operations are exposed.
mod background;
mod model;
mod store;

pub use background::{background_from_path, BackgroundSelection, MAX_BACKGROUND_BYTES};
pub use model::{
    AppearancePreferences, BackgroundMode, Booklist, Booklists, ResourcePreferences,
    ResourceProfile, Source, WorkIdentity, WorkbenchPreferences, MAX_SAFE_INTEGER,
};
pub use store::{Document, WorkbenchStore, PRIVATE_DIRECTORY};

use serde::Serialize;

/// IPC errors contain only a stable code; filesystem paths and OS messages stay private.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct StoreError {
    pub code: &'static str,
}

impl StoreError {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for StoreError {}

pub(crate) type Result<T> = std::result::Result<T, StoreError>;

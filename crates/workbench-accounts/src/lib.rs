//! Desktop account state. No library, task, downloader or cloud-publication authority.
mod backend;
mod service;

pub use backend::{Authenticated, SourceBackend};
pub use service::AccountService;
pub use workbench_credentials::Source;
pub use workbench_sources::{SourceAccount, SourceFolder, SourcePage, SourceWork};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AccountError {
    pub code: &'static str,
}

pub type Result<T> = std::result::Result<T, AccountError>;

impl AccountError {
    pub fn new(code: &'static str) -> Self {
        Self { code }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Disconnected,
    Connected,
    Expired,
    Unavailable,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub source: Source,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub display_name: Option<String>,
    pub state: AccountState,
    pub remembered: bool,
    pub error_code: Option<&'static str>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryKind {
    Favorites,
    Search,
    Detail,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub source: Source,
    pub session_id: String,
    #[serde(flatten)]
    pub page: SourcePage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteResult {
    pub source: Source,
    pub session_id: String,
    pub work_id: String,
    pub favorite: bool,
    pub changed: bool,
    pub verified: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverResult {
    pub source: Source,
    pub session_id: String,
    pub work_id: String,
    pub data_url: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FollowKind {
    Work,
    Author,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowedWork {
    pub work_id: String,
    pub title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowingSnapshot {
    pub source: Source,
    pub session_id: String,
    pub revision: u64,
    pub works: Vec<FollowedWork>,
    pub authors: Vec<String>,
}

#[cfg(test)]
mod tests;

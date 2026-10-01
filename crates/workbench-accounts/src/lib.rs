//! Desktop account state. No library, task, downloader or cloud-publication authority.
mod backend;
mod cache;
mod discovery;
mod observations;
mod service;
mod special;

pub use backend::{Authenticated, SourceBackend};
pub use cache::{CatalogAction, CatalogResult, CatalogSnapshot};
pub use cloud_monitor::online_reader::{
    Chapter, ChapterInfo, ChapterPage, OnlineReader, ReaderError, ReaderImage, ReaderSource,
};
pub use discovery::{
    discovery_work_from_source, DiscoveryMode, DiscoveryPhase, DiscoveryProgress, DiscoveryRun,
    DiscoveryScope, DiscoverySnapshot, DiscoveryStart,
};
pub use observations::{
    KnownAuthorWorksResult, RecentCheckResult, RecentCheckRun, RecentHistoryResult,
};
pub use service::{AccountService, DownloadSession, SessionLease};
pub use special::{SpecialRun, SpecialSnapshot};
pub use workbench_credentials::Source;
pub use workbench_sources::{
    JmSearchBoundary, JmSearchBoundaryItem, SourceAccount, SourceFolder, SourceItemIssue,
    SourceItemIssueCode, SourcePage, SourceWork,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AccountError {
    pub code: &'static str,
    #[serde(rename = "retryAfterMs", skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

pub type Result<T> = std::result::Result<T, AccountError>;

impl AccountError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            retry_after_ms: None,
        }
    }
    pub fn with_retry_after(mut self, delay: Option<u64>) -> Self {
        self.retry_after_ms = delay;
        self
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
    #[serde(rename = "rememberLogin")]
    pub remember_login: bool,
    pub error_code: Option<&'static str>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryKind {
    Favorites,
    Search,
    Author,
    Tag,
    Category,
    Ranking,
    Recent,
    Detail,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub timing: SourceQueryTiming,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub content_verified_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_verified_until: Option<u64>,
    pub source: Source,
    pub session_id: String,
    #[serde(flatten)]
    pub page: SourcePage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_error_code: Option<String>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceQueryTiming {
    pub queue_ms: u64,
    pub source_operation_ms: u64,
    pub local_commit_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorQueryPolicyResult {
    pub session_id: String,
    pub revision: u64,
    #[serde(flatten)]
    pub policy: workbench_storage::AuthorQueryPolicy,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankOptionsResult {
    pub source: Source,
    pub session_id: String,
    pub options: workbench_sources::RankOptions,
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

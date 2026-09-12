use serde::{Deserialize, Serialize};
pub use workbench_credentials::Source;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SourceError {
    pub code: &'static str,
}

impl SourceError {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for SourceError {}

pub type SourceResult<T> = Result<T, SourceError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAccount {
    pub source: Source,
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceWork {
    pub source: Source,
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub favorite: Option<bool>,
    pub chapter_count: Option<u64>,
    pub page_count: Option<u64>,
    /// A validated descriptor exists; this does not claim the cover was fetched.
    pub cover_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFolder {
    pub id: String,
    pub name: String,
    pub count: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePage {
    pub page: u64,
    pub total: Option<u64>,
    pub pages: Option<u64>,
    pub has_more: Option<bool>,
    pub folders: Vec<SourceFolder>,
    pub items: Vec<SourceWork>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FavoritePageRequest {
    pub page: u64,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteUpdate {
    pub work_id: String,
    pub favorite: bool,
    pub changed: bool,
    pub verified: bool,
}

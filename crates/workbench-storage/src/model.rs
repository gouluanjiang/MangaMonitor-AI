use crate::{background::validate_data_url, Result, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub(crate) trait ValidatedDocument:
    Clone + Default + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static
{
    fn validate(&self) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackgroundMode {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceProfile {
    Economy,
    Balanced,
    Custom,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppearancePreferences {
    pub background_mode: BackgroundMode,
    pub density: u8,
    #[serde(deserialize_with = "required_nullable")]
    pub background_image: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub background_name: Option<String>,
}

fn required_nullable<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePreferences {
    pub profile: ResourceProfile,
    pub simultaneous_works: u8,
    pub image_requests: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchPreferences {
    pub version: u32,
    pub appearance: AppearancePreferences,
    pub resources: ResourcePreferences,
}

impl Default for WorkbenchPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            appearance: AppearancePreferences {
                background_mode: BackgroundMode::B,
                density: 7,
                background_image: None,
                background_name: None,
            },
            resources: ResourcePreferences {
                profile: ResourceProfile::Balanced,
                simultaneous_works: 2,
                image_requests: 4,
            },
        }
    }
}

impl ValidatedDocument for WorkbenchPreferences {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || ![5, 7, 9].contains(&self.appearance.density)
            || self.appearance.background_image.is_some()
                != self.appearance.background_name.is_some()
            || !(1..=4).contains(&self.resources.simultaneous_works)
            || !(1..=8).contains(&self.resources.image_requests)
        {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        let limits = (
            self.resources.simultaneous_works,
            self.resources.image_requests,
        );
        if matches!(self.resources.profile, ResourceProfile::Economy) && limits != (1, 2)
            || matches!(self.resources.profile, ResourceProfile::Balanced) && limits != (2, 4)
        {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        if let Some(name) = &self.appearance.background_name {
            if name.is_empty()
                || name.trim() != name
                || name.encode_utf16().count() > 180
                || name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\')
            {
                return Err(StoreError::new("VALIDATION_FAILED"));
            }
        }
        if let Some(data) = &self.appearance.background_image {
            validate_data_url(data)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Source {
    #[serde(rename = "JM")]
    Jm,
    Pica,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkIdentity {
    pub source: Source,
    pub work_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Booklist {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub archived: bool,
    pub members: Vec<WorkIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Booklists {
    pub version: u32,
    pub lists: Vec<Booklist>,
}

impl Default for Booklists {
    fn default() -> Self {
        Self {
            version: 1,
            lists: Vec::new(),
        }
    }
}

fn valid_id(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

impl ValidatedDocument for Booklists {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1 || self.lists.len() > 100 {
            return Err(invalid());
        }
        let mut ids = HashSet::new();
        let mut active_names = HashSet::new();
        let mut total_members = 0usize;
        for list in &self.lists {
            total_members += list.members.len();
            if !valid_id(&list.id, 80)
                || !ids.insert(&list.id)
                || list.name.trim() != list.name
                || list.name.is_empty()
                || list.name.chars().count() > 80
                || list.name.chars().any(char::is_control)
                || (!list.archived && !active_names.insert(&list.name))
                || list.created_at > MAX_SAFE_INTEGER
                || list.updated_at > MAX_SAFE_INTEGER
                || list.updated_at < list.created_at
                || list.members.len() > 2_000
                || total_members > 20_000
            {
                return Err(invalid());
            }
            let mut members = HashSet::new();
            for member in &list.members {
                if !valid_id(&member.work_id, 160) || !members.insert(member) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

pub const MAX_FOLLOWED_ACCOUNTS: usize = 20;
pub const MAX_FOLLOWED_WORKS_PER_ACCOUNT: usize = 500;
pub const MAX_FOLLOWED_AUTHORS_PER_ACCOUNT: usize = 200;
pub const MAX_FOLLOWING_NAME_CHARACTERS: usize = 200;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FollowedWork {
    pub work_id: String,
    pub title: String,
}

/// A local scope derived by the native service from a verified remote identity.
/// account_key is a lowercase SHA-256 hex digest, never a renderer-selected key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FollowedAccount {
    pub source: Source,
    pub account_key: String,
    pub works: Vec<FollowedWork>,
    pub authors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountFollowing {
    pub version: u32,
    pub accounts: Vec<FollowedAccount>,
}

impl Default for AccountFollowing {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
        }
    }
}

fn valid_following_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_FOLLOWING_NAME_CHARACTERS
        && !value.chars().any(char::is_control)
}

impl ValidatedDocument for AccountFollowing {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1 || self.accounts.len() > MAX_FOLLOWED_ACCOUNTS {
            return Err(invalid());
        }
        let mut scopes = HashSet::new();
        for account in &self.accounts {
            if account.account_key.len() != 64
                || !account
                    .account_key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || !scopes.insert((account.source, &account.account_key))
                || account.works.len() > MAX_FOLLOWED_WORKS_PER_ACCOUNT
                || account.authors.len() > MAX_FOLLOWED_AUTHORS_PER_ACCOUNT
            {
                return Err(invalid());
            }
            let mut works = HashSet::new();
            for work in &account.works {
                if !valid_id(&work.work_id, 160)
                    || !valid_following_name(&work.title)
                    || !works.insert(&work.work_id)
                {
                    return Err(invalid());
                }
            }
            let mut authors = HashSet::new();
            for author in &account.authors {
                if !valid_following_name(author) || !authors.insert(author) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

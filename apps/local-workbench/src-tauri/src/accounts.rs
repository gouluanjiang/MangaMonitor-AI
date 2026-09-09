use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{
    AccountError, AccountService, AccountSummary, CatalogAction, CatalogResult, CatalogSnapshot,
    CoverResult, FavoriteResult, FollowKind, FollowingSnapshot, QueryKind, QueryResult, Source,
};
use workbench_sources::WorkbenchSources;
use zeroize::Zeroizing;

// A unit-test application must never restore credentials from the developer's OS.
#[cfg(test)]
type PlatformVault = workbench_credentials::test_support::MemoryVault;
#[cfg(all(windows, not(test)))]
type PlatformVault = workbench_credentials::WindowsVault;

#[cfg(all(not(windows), not(test)))]
struct PlatformVault;

#[cfg(all(not(windows), not(test)))]
impl PlatformVault {
    fn new() -> Self {
        Self
    }
}

#[cfg(all(not(windows), not(test)))]
impl workbench_credentials::Vault for PlatformVault {
    fn compare_exchange(
        &self,
        _source: Source,
        _expected: Option<[u8; 32]>,
        _next: Option<&workbench_credentials::StoredCredential>,
    ) -> workbench_credentials::Result<()> {
        Err(workbench_credentials::VaultError::UNAVAILABLE)
    }

    fn load(
        &self,
        _source: Source,
    ) -> workbench_credentials::Result<Option<workbench_credentials::StoredCredential>> {
        Err(workbench_credentials::VaultError::UNAVAILABLE)
    }

    fn save(
        &self,
        _source: Source,
        _credential: &workbench_credentials::StoredCredential,
    ) -> workbench_credentials::Result<()> {
        Err(workbench_credentials::VaultError::UNAVAILABLE)
    }

    fn delete(&self, _source: Source) -> workbench_credentials::Result<()> {
        Err(workbench_credentials::VaultError::UNAVAILABLE)
    }
}

type Service = AccountService<WorkbenchSources, PlatformVault>;

pub(super) struct DesktopAccounts {
    root: Result<PathBuf, AccountError>,
    cached: Mutex<Option<Arc<Service>>>,
}

impl DesktopAccounts {
    pub(super) fn new(root: Result<PathBuf, workbench_storage::StoreError>) -> Self {
        Self {
            root: root.map_err(|error| AccountError::new(error.code)),
            cached: Mutex::new(None),
        }
    }

    // Called only by a blocking worker. Failed construction is never cached.
    fn open(&self) -> Result<Arc<Service>, AccountError> {
        let root = self.root.as_ref().map_err(|error| *error)?;
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| AccountError::new("ACCOUNT_SERVICE_UNAVAILABLE"))?;
        if let Some(service) = cached.as_ref() {
            return Ok(Arc::clone(service));
        }
        let sources = WorkbenchSources::new().map_err(|error| AccountError::new(error.code))?;
        let service = Arc::new(AccountService::new(
            sources,
            PlatformVault::new(),
            root.clone(),
        ));
        *cached = Some(Arc::clone(&service));
        Ok(service)
    }
}

async fn service(state: Arc<DesktopAccounts>) -> Result<Arc<Service>, AccountError> {
    tauri::async_runtime::spawn_blocking(move || state.open())
        .await
        .map_err(|_| AccountError::new("ACCOUNT_SERVICE_UNAVAILABLE"))?
}

fn require_main(label: &str) -> Result<(), AccountError> {
    crate::require_main(label).map_err(|error| AccountError::new(error.code))
}

#[tauri::command]
pub(super) async fn source_accounts<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    refresh: Option<bool>,
) -> Result<Vec<AccountSummary>, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    Ok(service.accounts(refresh.unwrap_or(false)).await)
}

#[tauri::command]
pub(super) async fn source_login<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    username: String,
    password: String,
    remember: bool,
) -> Result<AccountSummary, AccountError> {
    // Also clear inputs if initialization fails or the waiting command is dropped.
    let mut username = Zeroizing::new(username);
    let mut password = Zeroizing::new(password);
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    // Only the native service receives these owned values. Its login operation
    // zeroizes them; neither command diagnostics nor the response contain them.
    service
        .login(
            source,
            std::mem::take(&mut *username),
            std::mem::take(&mut *password),
            remember,
        )
        .await
}

#[tauri::command]
pub(super) async fn source_logout<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: Option<String>,
) -> Result<AccountSummary, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service.logout(source, session_id.as_deref()).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Preserve the public flat IPC argument contract.
pub(super) async fn source_query<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
    kind: QueryKind,
    query: String,
    folder_id: Option<String>,
    page: u64,
    reverse: Option<bool>,
) -> Result<QueryResult, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service
        .query_ordered(
            source,
            &session_id,
            kind,
            &query,
            folder_id,
            page,
            reverse.unwrap_or(false),
        )
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Preserve the public flat IPC argument contract.
pub(super) async fn source_catalog<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
    folder_id: Option<String>,
    reverse: bool,
    action: CatalogAction,
    snapshot: Option<CatalogSnapshot>,
) -> Result<CatalogResult, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service
        .catalog(source, &session_id, folder_id, reverse, action, snapshot)
        .await
}

#[tauri::command]
pub(super) async fn source_favorite<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
    work_id: String,
    desired: bool,
) -> Result<FavoriteResult, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service
        .favorite(source, &session_id, &work_id, desired)
        .await
}

#[tauri::command]
pub(super) async fn source_cover<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
    work_id: String,
) -> Result<CoverResult, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service.cover(source, &session_id, &work_id).await
}

#[tauri::command]
pub(super) async fn source_following<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
) -> Result<FollowingSnapshot, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service.following(source, &session_id).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Preserve the public flat IPC argument contract.
pub(super) async fn source_follow<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    source: Source,
    session_id: String,
    kind: FollowKind,
    value: String,
    desired: bool,
    expected_revision: u64,
) -> Result<FollowingSnapshot, AccountError> {
    require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner())).await?;
    service
        .follow(
            source,
            &session_id,
            kind,
            &value,
            desired,
            expected_revision,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_guard_rejects_every_non_main_label() {
        assert_eq!(require_main("main"), Ok(()));
        for label in ["", "secondary", "main-child", "Main"] {
            assert_eq!(require_main(label), Err(AccountError::new("FORBIDDEN")));
        }
    }
}

//! Manual author catalog reads only; this module has no download authority.
use crate::accounts::{service, DesktopAccounts};
use std::sync::Arc;
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{
    AccountError, DiscoveryMode, DiscoveryProgress, DiscoveryRun, DiscoveryScope,
    DiscoverySnapshot, DiscoveryStart,
};

fn require_main(label: &str) -> Result<(), AccountError> {
    crate::require_main(label).map_err(|error| AccountError::new(error.code))
}

#[tauri::command]
pub(crate) async fn special_read<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>, scopes: Vec<DiscoveryScope>) -> Result<workbench_accounts::SpecialSnapshot, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_read(scopes).await
}

#[tauri::command]
pub(crate) async fn special_set<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>, scopes: Vec<DiscoveryScope>, author: String, enabled: bool) -> Result<workbench_accounts::SpecialSnapshot, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_set(scopes, author, enabled).await
}

#[tauri::command]
pub(crate) async fn special_mark_read<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>, scopes: Vec<DiscoveryScope>, identity: Option<workbench_storage::LibraryReference>) -> Result<workbench_accounts::SpecialSnapshot, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_mark_read(scopes, identity).await
}

#[tauri::command]
pub(crate) async fn special_progress<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>) -> Result<workbench_accounts::SpecialRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_run()
}

#[tauri::command]
pub(crate) async fn special_start<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>) -> Result<workbench_accounts::SpecialRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_start()
}

#[tauri::command]
pub(crate) async fn special_cancel<R: Runtime>(window: WebviewWindow<R>, accounts: State<'_, Arc<DesktopAccounts>>) -> Result<workbench_accounts::SpecialRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner())).await?.special_cancel()
}

#[tauri::command]
pub(crate) async fn discovery_read<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
    include_other: Option<bool>,
) -> Result<DiscoverySnapshot, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_read_view(scopes, include_other.unwrap_or(false))
        .await
}

#[tauri::command]
pub(crate) async fn discovery_progress<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
) -> Result<DiscoveryProgress, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_progress(scopes)
        .await
}

#[tauri::command]
pub(crate) async fn discovery_start_unfinished<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
    authors: Vec<String>,
) -> Result<DiscoveryStart, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_start_unfinished(scopes, authors)
        .await
}

#[tauri::command]
pub(crate) async fn discovery_start<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
    authors: Vec<String>,
    mode: Option<DiscoveryMode>,
) -> Result<DiscoveryStart, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_start_with_mode(scopes, authors, mode.unwrap_or_default())
        .await
}

#[tauri::command]
pub(crate) async fn discovery_cancel<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    run_id: String,
) -> Result<DiscoveryRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_cancel(&run_id)
}

#[tauri::command]
pub(crate) async fn recent_check_start<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
    max_pages: Option<u64>,
) -> Result<workbench_accounts::RecentCheckRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .recent_check_start(scopes, max_pages)
        .await
}

#[tauri::command]
pub(crate) async fn recent_check_progress<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
) -> Result<Option<workbench_accounts::RecentCheckRun>, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .recent_check_progress()
}

#[tauri::command]
pub(crate) async fn recent_check_cancel<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    run_id: String,
) -> Result<workbench_accounts::RecentCheckRun, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .recent_check_cancel(&run_id)
}

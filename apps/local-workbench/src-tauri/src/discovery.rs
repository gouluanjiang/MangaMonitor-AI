//! Manual author catalog reads only; this module has no download authority.
use crate::accounts::{service, DesktopAccounts};
use std::sync::Arc;
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{
    AccountError, DiscoveryRun, DiscoveryScope, DiscoverySnapshot, DiscoveryStart,
};

fn require_main(label: &str) -> Result<(), AccountError> {
    crate::require_main(label).map_err(|error| AccountError::new(error.code))
}

#[tauri::command]
pub(crate) async fn discovery_read<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
) -> Result<DiscoverySnapshot, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_read(scopes)
        .await
}

#[tauri::command]
pub(crate) async fn discovery_start<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    scopes: Vec<DiscoveryScope>,
    authors: Vec<String>,
) -> Result<DiscoveryStart, AccountError> {
    require_main(window.label())?;
    service(Arc::clone(accounts.inner()))
        .await?
        .discovery_start(scopes, authors)
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

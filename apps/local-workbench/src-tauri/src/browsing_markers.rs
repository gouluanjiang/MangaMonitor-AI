//! Local browse baselines only: no source requests, catalog edits or unread mutations.
use crate::{
    accounts::{service, DesktopAccounts},
    with_store, DesktopStore,
};
use std::sync::Arc;
use tauri::{Runtime, State, WebviewWindow};
use workbench_accounts::{AccountError, Source};
use workbench_storage::{BrowsingBaseline, BrowsingMarkers, BrowsingSurface, Document, StoreError};

fn store_error(error: AccountError) -> StoreError {
    StoreError { code: error.code }
}

#[tauri::command]
pub(crate) async fn browsing_markers_read<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    store: State<'_, Arc<DesktopStore>>,
    source: Source,
    session_id: String,
    surface: BrowsingSurface,
) -> Result<Document<BrowsingMarkers>, StoreError> {
    crate::require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner()))
        .await
        .map_err(store_error)?;
    let (account_key, _, lease) = service
        .observation_identity(source, &session_id)
        .await
        .map_err(store_error)?;
    with_store(Arc::clone(store.inner()), move |store| {
        lease.require_current().map_err(store_error)?;
        let result = store.read_browsing_markers(&account_key, surface)?;
        lease.require_current().map_err(store_error)?;
        Ok(result)
    })
    .await
}

#[tauri::command]
pub(crate) async fn browsing_markers_write<R: Runtime>(
    window: WebviewWindow<R>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    store: State<'_, Arc<DesktopStore>>,
    source: Source,
    session_id: String,
    surface: BrowsingSurface,
    baseline: BrowsingBaseline,
) -> Result<Document<BrowsingMarkers>, StoreError> {
    crate::require_main(window.label())?;
    let service = service(Arc::clone(accounts.inner()))
        .await
        .map_err(store_error)?;
    let (account_key, _, lease) = service
        .observation_identity(source, &session_id)
        .await
        .map_err(store_error)?;
    with_store(Arc::clone(store.inner()), move |store| {
        store.write_browsing_markers(&account_key, surface, baseline, || {
            lease.require_current().map_err(store_error)
        })
    })
    .await
}

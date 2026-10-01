use crate::{with_store, DesktopStore};
use std::sync::Arc;
use tauri::{Runtime, State, WebviewWindow};
use workbench_storage::{Document, HistoryIdentity, StoreError, ViewingHistory, WorkbenchStore};

#[tauri::command]
pub(crate) async fn history_read<R: Runtime>(window: WebviewWindow<R>, store: State<'_, Arc<DesktopStore>>) -> Result<Document<ViewingHistory>, StoreError> {
    crate::require_main(window.label())?;
    with_store(Arc::clone(store.inner()), WorkbenchStore::read_viewing_history).await
}
#[tauri::command]
pub(crate) async fn history_record<R: Runtime>(window: WebviewWindow<R>, store: State<'_, Arc<DesktopStore>>, identity: HistoryIdentity, title: String) -> Result<Document<ViewingHistory>, StoreError> {
    crate::require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| store.record_viewing_history(identity, title)).await
}
#[tauri::command]
pub(crate) async fn history_clear<R: Runtime>(window: WebviewWindow<R>, store: State<'_, Arc<DesktopStore>>) -> Result<Document<ViewingHistory>, StoreError> {
    crate::require_main(window.label())?;
    with_store(Arc::clone(store.inner()), WorkbenchStore::clear_viewing_history).await
}
#[tauri::command]
pub(crate) async fn history_set_enabled<R: Runtime>(window: WebviewWindow<R>, store: State<'_, Arc<DesktopStore>>, enabled: bool) -> Result<Document<ViewingHistory>, StoreError> {
    crate::require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| store.set_viewing_history_enabled(enabled)).await
}

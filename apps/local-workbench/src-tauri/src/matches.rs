use crate::{require_main, with_store, DesktopStore};
use std::sync::Arc;
use tauri::{Runtime, State, WebviewWindow};
use workbench_storage::{SourceMatchWork, SourceMatchesSnapshot, StoreError};

#[tauri::command]
pub(crate) async fn source_matches_read<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<SourceMatchesSnapshot, StoreError> {
    require_main(window.label())?;
    with_store(
        Arc::clone(store.inner()),
        workbench_storage::source_matches_read,
    )
    .await
}

#[tauri::command]
pub(crate) async fn source_matches_confirm<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    jm: SourceMatchWork,
    pica: SourceMatchWork,
) -> Result<SourceMatchesSnapshot, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::source_matches_confirm(store, revision, jm, pica)
    })
    .await
}

#[tauri::command]
pub(crate) async fn source_matches_unlink<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    pair_id: String,
) -> Result<SourceMatchesSnapshot, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::source_matches_unlink(store, revision, &pair_id)
    })
    .await
}

use crate::{require_main, with_store, DesktopStore};
use std::sync::Arc;
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use workbench_storage::{LibraryReference, PhoneLibrarySnapshot, StoreError};

#[tauri::command]
pub(crate) async fn phone_library_read<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<PhoneLibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_store(
        Arc::clone(store.inner()),
        workbench_storage::phone_library_read,
    )
    .await
}

#[tauri::command]
pub(crate) async fn phone_library_import<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
) -> Result<Option<PhoneLibrarySnapshot>, StoreError> {
    require_main(window.label())?;
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("导入手机漫画 TXT 名单")
            .add_filter("TXT 名单", &["txt"])
            .set_parent(&window)
            .blocking_pick_file()
            .map(|selected| {
                selected.into_path().map_err(|_| StoreError {
                    code: "PHONE_LIBRARY_INVALID_TXT",
                })
            })
            .transpose()
    })
    .await
    .map_err(|_| StoreError {
        code: "PHONE_LIBRARY_PICKER_FAILED",
    })??;
    let Some(path) = selected else {
        return Ok(None);
    };
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::phone_library_from_path(store, &path, revision).map(Some)
    })
    .await
}

#[tauri::command]
pub(crate) async fn phone_library_mark<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    name: String,
    reference: Option<LibraryReference>,
) -> Result<PhoneLibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::phone_library_mark(store, revision, name, reference)
    })
    .await
}

#[tauri::command]
pub(crate) async fn phone_library_unmark<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    revision: u64,
    entry_id: String,
) -> Result<PhoneLibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        workbench_storage::phone_library_unmark(store, revision, &entry_id)
    })
    .await
}

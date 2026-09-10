use crate::{require_main, DesktopStore};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use workbench_library::{
    LibraryCover, LibraryReference, LibraryService, LibrarySnapshot, ScanAction,
};
use workbench_storage::{StoreError, WorkbenchStore};

#[derive(Default)]
pub(crate) struct DesktopLibrary {
    service: Mutex<LibraryService>,
}

pub(super) async fn with_library<T, F>(
    library: Arc<DesktopLibrary>,
    store: Arc<DesktopStore>,
    operation: F,
) -> Result<T, StoreError>
where
    T: Send + 'static,
    F: FnOnce(&mut LibraryService, &WorkbenchStore) -> Result<T, StoreError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let store = store.open()?;
        let mut service = library.service.lock().map_err(|_| StoreError {
            code: "LIBRARY_UNAVAILABLE",
        })?;
        operation(&mut service, &store)
    })
    .await
    .map_err(|_| StoreError {
        code: "LIBRARY_UNAVAILABLE",
    })?
}

#[tauri::command]
pub(crate) async fn library_read<R: Runtime>(
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<LibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        |service, store| service.read(store),
    )
    .await
}

#[tauri::command]
pub(crate) async fn library_choose<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<Option<LibrarySnapshot>, StoreError> {
    require_main(window.label())?;
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择漫画库目录")
            .set_parent(&window)
            .blocking_pick_folder()
            .map(|selected| {
                selected.into_path().map_err(|_| StoreError {
                    code: "LIBRARY_PATH_INVALID",
                })
            })
            .transpose()
    })
    .await
    .map_err(|_| StoreError {
        code: "LIBRARY_PICKER_FAILED",
    })??;
    let Some(path) = selected else {
        return Ok(None);
    };
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| service.choose(store, &path).map(Some),
    )
    .await
}

#[tauri::command]
pub(crate) async fn library_scan<R: Runtime>(
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    root_id: String,
    generation: u64,
    action: ScanAction,
) -> Result<LibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| service.scan(store, &root_id, generation, action),
    )
    .await
}

#[tauri::command]
pub(crate) async fn library_cover<R: Runtime>(
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    root_id: String,
    generation: u64,
    entry_id: String,
) -> Result<LibraryCover, StoreError> {
    require_main(window.label())?;
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| service.cover(store, &root_id, generation, &entry_id),
    )
    .await
}

#[tauri::command]
pub(crate) async fn library_link<R: Runtime>(
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    root_id: String,
    generation: u64,
    entry_id: String,
    reference: Option<LibraryReference>,
) -> Result<LibrarySnapshot, StoreError> {
    require_main(window.label())?;
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| service.link(store, &root_id, generation, &entry_id, reference),
    )
    .await
}

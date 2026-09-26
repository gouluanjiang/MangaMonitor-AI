use crate::{require_main, DesktopStore};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use workbench_library::{LibraryCover, LibraryService, LibrarySnapshot, ScanAction};
use workbench_storage::{StoreError, WorkbenchStore};

#[tauri::command]
pub(crate) async fn library_reveal<R: Runtime>(
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    root_id: String,
    generation: u64,
    entry_id: String,
) -> Result<(), StoreError> {
    require_main(window.label())?;
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| {
            service.with_verified_location(store, &root_id, generation, &entry_id, reveal_location)
        },
    )
    .await
}

#[cfg(windows)]
fn reveal_location(path: &std::path::Path) -> Result<(), StoreError> {
    with_shell_item(path, |item| unsafe {
        // cidl=0 opens the containing folder and selects the single item.
        // https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shopenfolderandselectitems
        windows_sys::Win32::UI::Shell::SHOpenFolderAndSelectItems(item, 0, std::ptr::null(), 0)
    })
}

#[cfg(windows)]
fn with_shell_item(
    path: &std::path::Path,
    action: impl FnOnce(*const windows_sys::Win32::UI::Shell::Common::ITEMIDLIST) -> i32,
) -> Result<(), StoreError> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Shell::SHParseDisplayName,
    };
    // Shell parsing rejects the canonical verbatim prefix. Simplify only when
    // that preserves the filesystem meaning (not reserved or trailing-dot names).
    let wide: Vec<u16> = dunce::simplified(path)
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Pass a filesystem item ID, never a command line or a file to execute.
    let result = unsafe {
        let initialized = CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32);
        if initialized < 0 && initialized != RPC_E_CHANGED_MODE {
            return Err(StoreError {
                code: "LIBRARY_REVEAL_FAILED",
            });
        }
        let mut item = ptr::null_mut();
        let parsed = SHParseDisplayName(
            wide.as_ptr(),
            ptr::null_mut(),
            &mut item,
            0,
            ptr::null_mut(),
        );
        let result = if parsed >= 0 && !item.is_null() {
            action(item)
        } else {
            parsed.min(-1)
        };
        if !item.is_null() {
            CoTaskMemFree(item.cast());
        }
        if initialized >= 0 {
            CoUninitialize();
        }
        result
    };
    if result < 0 {
        Err(StoreError {
            code: "LIBRARY_REVEAL_FAILED",
        })
    } else {
        Ok(())
    }
}

#[cfg(all(test, windows))]
#[test]
fn shell_parses_canonical_unicode_file_and_directory_without_opening_explorer() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("[合成作者] 日本語, 目录");
    std::fs::create_dir(&directory).unwrap();
    let file = directory.join("合成作品, (C107).zip");
    std::fs::write(&file, b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    for path in [&file, &directory] {
        let mut parsed = false;
        with_shell_item(&std::fs::canonicalize(path).unwrap(), |item| {
            assert!(!item.is_null());
            parsed = true;
            0
        })
        .unwrap();
        assert!(parsed);
    }
}

#[cfg(not(windows))]
fn reveal_location(_path: &std::path::Path) -> Result<(), StoreError> {
    Err(StoreError {
        code: "LIBRARY_REVEAL_UNSUPPORTED",
    })
}

pub(crate) struct DesktopLibrary {
    service: Mutex<LibraryService>,
    covers: Arc<tokio::sync::Semaphore>,
}

impl Default for DesktopLibrary {
    fn default() -> Self {
        Self {
            service: Mutex::new(LibraryService::default()),
            covers: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
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
        |service, store| service.read_current_files(store),
    )
    .await
}

#[tauri::command]
pub(crate) async fn library_import_paths<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    root_id: String,
    generation: u64,
) -> Result<Option<workbench_library::LibraryMigrationResult>, StoreError> {
    require_main(window.label())?;
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("导入 ZIP 整理对应表")
            .add_filter("整理对应表", &["json"])
            .set_parent(&window)
            .blocking_pick_file()
            .map(|file| {
                file.into_path().map_err(|_| StoreError {
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
        move |service, store| {
            let bytes = workbench_storage::library_path_mapping_bytes(&path)?;
            service
                .import_paths(store, &root_id, generation, &bytes)
                .map(Some)
        },
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
    with_library_cover(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |store| LibraryService::read_cover(store, &root_id, generation, &entry_id),
    )
    .await
}

async fn with_library_cover<T, F>(
    library: Arc<DesktopLibrary>,
    store: Arc<DesktopStore>,
    operation: F,
) -> Result<T, StoreError>
where
    T: Send + 'static,
    F: FnOnce(&WorkbenchStore) -> Result<T, StoreError> + Send + 'static,
{
    let permit = Arc::clone(&library.covers)
        .acquire_owned()
        .await
        .map_err(|_| StoreError {
            code: "LIBRARY_UNAVAILABLE",
        })?;
    // ZIP reads/decoding no longer hold the scan/registration mutex. read_cover
    // revalidates the stored revision and the actual file before returning bytes.
    tauri::async_runtime::spawn_blocking(move || {
        // Keep the permit inside the blocking task even if its IPC waiter closes.
        let _permit = permit;
        let store = store.open()?;
        operation(&store)
    })
    .await
    .map_err(|_| StoreError {
        code: "LIBRARY_UNAVAILABLE",
    })?
}

#[cfg(test)]
#[test]
fn cover_workers_are_bounded_and_do_not_hold_the_scan_mutex() {
    use std::sync::mpsc;
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(DesktopStore::new(Ok(root.path().to_owned())));
    let library = Arc::new(DesktopLibrary::default());
    let scan = library.service.lock().unwrap();
    let (entered, started) = mpsc::channel();
    let mut releases = vec![];
    let mut tasks = vec![];
    for index in 0..3 {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        let entered = entered.clone();
        tasks.push(tauri::async_runtime::spawn(with_library_cover(
            Arc::clone(&library),
            Arc::clone(&store),
            move |_| {
                entered.send(index).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(index)
            },
        )));
    }
    let first = started.recv_timeout(Duration::from_secs(3)).unwrap();
    let second = started.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_ne!(first, second);
    assert!(started.recv_timeout(Duration::from_millis(100)).is_err());
    // Both workers entered while the scan mutex was still held. A third waits.
    assert_eq!(library.covers.available_permits(), 0);
    drop(scan);
    releases[first].send(()).unwrap();
    let third = started.recv_timeout(Duration::from_secs(3)).unwrap();
    releases[second].send(()).unwrap();
    releases[third].send(()).unwrap();
    for task in tasks {
        tauri::async_runtime::block_on(task).unwrap().unwrap();
    }
    assert_eq!(library.covers.available_permits(), 2);
}

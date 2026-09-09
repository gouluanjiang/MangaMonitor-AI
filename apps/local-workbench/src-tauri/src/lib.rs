mod accounts;

use accounts::DesktopAccounts;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Builder, Manager, Runtime, State, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use workbench_storage::{
    background_from_path, BackgroundSelection, Booklists, Document, StoreError,
    WorkbenchPreferences, WorkbenchStore,
};

struct DesktopStore {
    root: Result<PathBuf, StoreError>,
    cached: Mutex<Option<Arc<WorkbenchStore>>>,
}

impl DesktopStore {
    fn new(root: Result<PathBuf, StoreError>) -> Self {
        Self {
            root,
            cached: Mutex::new(None),
        }
    }

    // Only called from a blocking worker. Failed opens remain retryable.
    fn open(&self) -> Result<Arc<WorkbenchStore>, StoreError> {
        let root = self.root.as_ref().map_err(|error| *error)?;
        let mut cached = self.cached.lock().map_err(|_| StoreError {
            code: "STORE_UNAVAILABLE",
        })?;
        if let Some(store) = cached.as_ref() {
            return Ok(Arc::clone(store));
        }
        let store = Arc::new(WorkbenchStore::open(root)?);
        *cached = Some(Arc::clone(&store));
        Ok(store)
    }
}

async fn with_store<T, F>(state: Arc<DesktopStore>, operation: F) -> Result<T, StoreError>
where
    T: Send + 'static,
    F: FnOnce(&WorkbenchStore) -> Result<T, StoreError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        // Own both Arcs until the operation has actually finished, even if its
        // waiting IPC future is dropped when a window closes.
        let store = state.open()?;
        operation(&store)
    })
    .await
    .map_err(|_| StoreError {
        code: "STORE_UNAVAILABLE",
    })?
}

fn require_main(label: &str) -> Result<(), StoreError> {
    if label == "main" {
        Ok(())
    } else {
        Err(StoreError { code: "FORBIDDEN" })
    }
}

#[tauri::command]
async fn read_preferences<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<Document<WorkbenchPreferences>, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), WorkbenchStore::read_preferences).await
}

#[tauri::command]
async fn write_preferences<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    expected_revision: u64,
    value: WorkbenchPreferences,
) -> Result<Document<WorkbenchPreferences>, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        store.write_preferences(expected_revision, value)
    })
    .await
}

#[tauri::command]
async fn read_booklists<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<Document<Booklists>, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), WorkbenchStore::read_booklists).await
}

#[tauri::command]
async fn write_booklists<R: Runtime>(
    window: WebviewWindow<R>,
    store: State<'_, Arc<DesktopStore>>,
    expected_revision: u64,
    value: Booklists,
) -> Result<Document<Booklists>, StoreError> {
    require_main(window.label())?;
    with_store(Arc::clone(store.inner()), move |store| {
        store.write_booklists(expected_revision, value)
    })
    .await
}

#[tauri::command]
async fn choose_background<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<Option<BackgroundSelection>, StoreError> {
    require_main(window.label())?;
    // The blocking OS dialog runs off the UI thread; JS never supplies a path.
    tauri::async_runtime::spawn_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title("选择工作台背景")
            .add_filter("背景图片", &["png", "jpg", "jpeg", "webp"])
            .set_parent(&window)
            .blocking_pick_file();
        selection
            .map(|file| {
                let path = file.into_path().map_err(|_| StoreError {
                    code: "BACKGROUND_PATH_INVALID",
                })?;
                background_from_path(&path)
            })
            .transpose()
    })
    .await
    .map_err(|_| StoreError {
        code: "BACKGROUND_PICKER_FAILED",
    })?
}

fn app_builder<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            read_preferences,
            write_preferences,
            read_booklists,
            write_booklists,
            choose_background,
            accounts::source_accounts,
            accounts::source_login,
            accounts::source_logout,
            accounts::source_query,
            accounts::source_favorite,
            accounts::source_cover,
            accounts::source_following,
            accounts::source_follow,
        ])
}

fn trusted_navigation(url: &tauri::Url, development: bool) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let bundled = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost") && url.port().is_none(),
        "http" | "https" => url.host_str() == Some("tauri.localhost") && url.port().is_none(),
        _ => false,
    };
    bundled
        || (development
            && url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port() == Some(4173))
}

pub fn run() {
    app_builder(Builder::default())
        .setup(|app| {
            // Keep the window available when storage is inaccessible. The first
            // IPC read initializes it on a worker and returns a sanitized error.
            let root = app.path().app_data_dir().map_err(|_| StoreError {
                code: "APP_DATA_UNAVAILABLE",
            });
            app.manage(Arc::new(DesktopStore::new(root.clone())));
            app.manage(Arc::new(DesktopAccounts::new(root)));
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or_else(|| std::io::Error::other("MAIN_WINDOW_MISSING"))?;
            WebviewWindowBuilder::from_config(app, window_config)?
                .on_navigation(|url| trusted_navigation(url, cfg!(dev)))
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .on_download(|_, _| false)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run the development workbench");
}

#[cfg(test)]
mod tests;

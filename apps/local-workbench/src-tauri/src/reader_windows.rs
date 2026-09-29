//! Native-owned reader windows. Renderer labels and book/session IDs never grant
//! access to another window; only registered children receive a bound reader.
mod geometry;
use crate::{
    reader::{DesktopReader, ReaderRequest},
    require_main, trusted_navigation, with_store, DesktopStore,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    Window, WindowEvent,
};
use workbench_storage::{LibraryReference, ReaderWindowSize, StoreError, WorkbenchStore};

type Result<T> = std::result::Result<T, StoreError>;
fn error(code: &'static str) -> StoreError {
    StoreError { code }
}

struct Child {
    request: ReaderRequest,
    key: String,
    reader: Arc<DesktopReader>,
    ready: bool,
    normal_size: Option<ReaderWindowSize>,
}
#[derive(Default)]
struct Registry {
    children: HashMap<String, Child>,
    next: u64,
    main_closed: bool,
    main_ready: bool,
}
pub(crate) struct ReaderWindows {
    main: Arc<DesktopReader>,
    registry: Mutex<Registry>,
    launch: tokio::sync::Mutex<()>,
}
impl Default for ReaderWindows {
    fn default() -> Self {
        Self {
            main: Arc::new(DesktopReader::default()),
            registry: Mutex::new(Registry::default()),
            launch: tokio::sync::Mutex::new(()),
        }
    }
}

pub(crate) struct Reservation {
    pub label: String,
    pub created: bool,
    pub changed: bool,
    ordinal: usize,
}
impl ReaderWindows {
    pub(crate) fn scope(&self, label: &str) -> Result<Arc<DesktopReader>> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if label == "main" {
            return if registry.main_closed {
                Err(error("FORBIDDEN"))
            } else {
                Ok(Arc::clone(&self.main))
            };
        }
        registry
            .children
            .get(label)
            .map(|child| Arc::clone(&child.reader))
            .ok_or(error("FORBIDDEN"))
    }
    pub(crate) fn reserve(&self, request: ReaderRequest) -> Result<Reservation> {
        let key = identity(&request)?;
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if let Some((label, child)) = registry
            .children
            .iter_mut()
            .find(|(_, child)| child.key == key)
        {
            let changed = child.request != request;
            if changed {
                child.reader.bind_request(request.clone())?;
                child.request = request;
            }
            return Ok(Reservation {
                label: label.clone(),
                created: false,
                changed,
                ordinal: 0,
            });
        }
        registry.next = registry
            .next
            .checked_add(1)
            .ok_or(error("READER_WINDOW_LIMIT"))?;
        let label = format!("reader-window-{}", registry.next);
        let reader = Arc::new(DesktopReader::scoped(
            &label,
            Arc::clone(&self.main.pages),
            Arc::clone(&self.main.progress),
        ));
        reader.bind_request(request.clone())?;
        let ordinal = usize::try_from(registry.next - 1).unwrap_or(0);
        registry.children.insert(
            label.clone(),
            Child {
                request,
                key,
                reader,
                ready: false,
                normal_size: None,
            },
        );
        Ok(Reservation {
            label,
            created: true,
            changed: false,
            ordinal,
        })
    }
    fn context(&self, label: &str) -> Result<WindowContext> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let child = registry.children.get_mut(label).ok_or(error("FORBIDDEN"))?;
        child.ready = true;
        Ok(WindowContext {
            request: child.request.clone(),
        })
    }
    fn ready(&self, label: &str) -> Result<bool> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if label == "main" {
            Ok(registry.main_ready)
        } else {
            registry
                .children
                .get(label)
                .map(|child| child.ready)
                .ok_or(error("FORBIDDEN"))
        }
    }
    fn remove(&self, label: &str) -> Result<bool> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if let Some(child) = registry.children.remove(label) {
            child.reader.retire()?;
        }
        Ok(registry.main_closed && registry.children.is_empty())
    }
    fn close_main(&self) -> Result<bool> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        self.main.retire()?;
        registry.main_closed = true;
        Ok(registry.children.is_empty())
    }
    fn restore_main(&self) -> Result<()> {
        self.registry
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?
            .main_closed = false;
        self.main.resume();
        Ok(())
    }
    fn normal_size(&self, label: &str) -> Option<ReaderWindowSize> {
        self.registry.lock().ok()?.children.get(label)?.normal_size
    }
    fn record_size(&self, label: &str, size: ReaderWindowSize) {
        if !size.is_valid() {
            return;
        }
        if let Ok(mut registry) = self.registry.lock() {
            if let Some(child) = registry.children.get_mut(label) {
                child.normal_size = Some(size);
            }
        }
    }
}

fn identity(request: &ReaderRequest) -> Result<String> {
    match request {
        ReaderRequest::Library {
            root_id,
            generation,
            entry_id,
        } => {
            let valid_hash =
                |value: &str| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
            if !valid_hash(root_id) || !valid_hash(entry_id) || *generation == 0 {
                return Err(error("VALIDATION_FAILED"));
            }
            Ok(format!("local:{root_id}:{entry_id}"))
        }
        ReaderRequest::Source {
            source,
            session_id,
            work_id,
        } => {
            let reference = LibraryReference {
                source: *source,
                work_id: work_id.clone(),
            };
            if !reference.is_valid()
                || session_id.is_empty()
                || session_id.len() > 256
                || session_id.chars().any(char::is_control)
            {
                return Err(error("VALIDATION_FAILED"));
            }
            Ok(format!("source:{source:?}:{work_id}"))
        }
    }
}

#[derive(Serialize)]
pub(crate) struct WindowOpened {
    label: String,
}
#[derive(Serialize)]
pub(crate) struct WindowContext {
    request: ReaderRequest,
}

fn focus<R: Runtime>(window: &WebviewWindow<R>) -> Result<()> {
    window
        .show()
        .and_then(|_| window.unminimize())
        .and_then(|_| window.set_focus())
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))
}
fn show_main<R: Runtime>(app: &AppHandle<R>, readers: &ReaderWindows) -> Result<()> {
    let main = app
        .get_webview_window("main")
        .ok_or(error("READER_WINDOW_UNAVAILABLE"))?;
    main.show()
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    readers.restore_main()?;
    focus(&main)
}
fn child_scope(readers: &ReaderWindows, label: &str) -> Result<Arc<DesktopReader>> {
    if label == "main" {
        return Err(error("FORBIDDEN"));
    }
    readers.scope(label)
}

#[tauri::command]
pub(crate) async fn reader_window_open<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
    store: State<'_, Arc<DesktopStore>>,
    request: ReaderRequest,
) -> Result<WindowOpened> {
    require_main(window.label())?;
    let _launch = readers.launch.lock().await;
    let reservation = readers.reserve(request)?;
    if !reservation.created {
        if let Some(child) = app.get_webview_window(&reservation.label) {
            if reservation.changed {
                app.emit_to(
                    reservation.label.as_str(),
                    "reader-window-context-changed",
                    (),
                )
                .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
            }
            focus(&child)?;
            return Ok(WindowOpened {
                label: reservation.label,
            });
        }
        if readers.remove(&reservation.label)? {
            app.exit(0);
        }
        return Err(error("READER_WINDOW_UNAVAILABLE"));
    }
    // Preference failure does not prevent opening a read-only viewer.
    let saved = with_store(
        Arc::clone(store.inner()),
        WorkbenchStore::reader_window_size,
    )
    .await
    .unwrap_or(None);
    let built = create_child(&app, &window, &reservation, saved);
    if let Err(problem) = built {
        if let Some(child) = app.get_webview_window(&reservation.label) {
            let _ = child.destroy();
        }
        if readers.remove(&reservation.label)? {
            app.exit(0);
        }
        return Err(problem);
    }
    Ok(WindowOpened {
        label: reservation.label,
    })
}

fn create_child<R: Runtime>(
    app: &AppHandle<R>,
    main: &WebviewWindow<R>,
    reservation: &Reservation,
    saved: Option<ReaderWindowSize>,
) -> Result<()> {
    let monitor = main
        .current_monitor()
        .map_err(|_| error("READER_WINDOW_MONITOR_UNAVAILABLE"))?
        .or(main
            .primary_monitor()
            .map_err(|_| error("READER_WINDOW_MONITOR_UNAVAILABLE"))?)
        .ok_or(error("READER_WINDOW_MONITOR_UNAVAILABLE"))?;
    let work = monitor.work_area();
    let area = geometry::Area {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
        scale: monitor.scale_factor(),
    };
    let placement = geometry::placement(area, saved, reservation.ordinal)?;
    let window = WebviewWindowBuilder::new(
        app,
        &reservation.label,
        WebviewUrl::App("index.html#reader-window".into()),
    )
    .title("MangaMonitor · 手机小框阅读")
    .inner_size(placement.width, placement.height)
    .position(
        f64::from(placement.x) / area.scale,
        f64::from(placement.y) / area.scale,
    )
    .visible(false)
    .resizable(true)
    .always_on_top(false)
    .disable_drag_drop_handler()
    .on_navigation(|url| trusted_navigation(url, cfg!(dev)))
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .build()
    .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    // Initial position is physical: negative monitor origins and non-primary DPI
    // must not be interpreted in the primary monitor's logical coordinate space.
    window
        .set_position(tauri::PhysicalPosition::new(placement.x, placement.y))
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    let outer = window
        .outer_size()
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    if outer.width > area.width || outer.height > area.height {
        let inner = window
            .inner_size()
            .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
        let width = area
            .width
            .saturating_sub(outer.width.saturating_sub(inner.width));
        let height = area
            .height
            .saturating_sub(outer.height.saturating_sub(inner.height));
        if width == 0 || height == 0 {
            return Err(error("READER_WINDOW_SIZE_INVALID"));
        }
        let ratio = (f64::from(width) / f64::from(inner.width.max(1)))
            .min(f64::from(height) / f64::from(inner.height.max(1)))
            .min(1.0);
        window
            .set_size(tauri::PhysicalSize::new(
                (f64::from(inner.width) * ratio).floor().max(1.0) as u32,
                (f64::from(inner.height) * ratio).floor().max(1.0) as u32,
            ))
            .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    }
    let outer = window
        .outer_size()
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    let (x, y) =
        geometry::contained_position(area, outer.width, outer.height, reservation.ordinal)?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    focus(&window)
}

#[tauri::command]
pub(crate) fn reader_window_context<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
) -> Result<WindowContext> {
    readers.context(window.label())
}
#[tauri::command]
pub(crate) fn reader_main_ready<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
) -> Result<()> {
    require_main(window.label())?;
    readers
        .registry
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?
        .main_ready = true;
    Ok(())
}
#[tauri::command]
pub(crate) fn reader_window_pin<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
    pinned: bool,
) -> Result<()> {
    child_scope(&readers, window.label())?;
    window
        .set_always_on_top(pinned)
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))
}
#[tauri::command]
pub(crate) fn reader_window_show_main<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
) -> Result<()> {
    child_scope(&readers, window.label())?;
    show_main(&app, &readers)
}
#[tauri::command]
pub(crate) fn reader_window_download<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
    reader_id: String,
) -> Result<()> {
    let scope = child_scope(&readers, window.label())?;
    let reference = scope.online_reference(&reader_id)?;
    show_main(&app, &readers)?;
    app.emit_to("main", "reader-window-download", reference)
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))
}
#[tauri::command]
pub(crate) async fn reader_main_close<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
) -> Result<()> {
    require_main(window.label())?;
    let _launch = readers.launch.lock().await;
    if readers.close_main()? {
        app.exit(0);
    } else if window.hide().is_err() {
        readers.restore_main()?;
        return Err(error("READER_WINDOW_UNAVAILABLE"));
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn reader_window_close<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    readers: State<'_, Arc<ReaderWindows>>,
    store: State<'_, Arc<DesktopStore>>,
) -> Result<()> {
    child_scope(&readers, window.label())?;
    // Frontend already flushed the position and cancelled pending opens. Never
    // persist a fullscreen/maximized size as the user's normal small-window size.
    if !window.is_fullscreen().unwrap_or(true) && !window.is_maximized().unwrap_or(true) {
        if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
            let logical = size.to_logical::<f64>(scale);
            let size = ReaderWindowSize {
                width: logical.width,
                height: logical.height,
            };
            readers.record_size(window.label(), size);
        }
    }
    if let Some(size) = readers.normal_size(window.label()) {
        let _ = with_store(Arc::clone(store.inner()), move |store| {
            store.save_reader_window_size(size)
        })
        .await;
    }
    window
        .destroy()
        .map_err(|_| error("READER_WINDOW_UNAVAILABLE"))?;
    if readers.remove(window.label())? {
        app.exit(0);
    }
    Ok(())
}

pub(crate) fn window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let app = window.app_handle();
    let Some(readers) = app.try_state::<Arc<ReaderWindows>>() else {
        return;
    };
    let label = window.label();
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            let Ok(ready) = readers.ready(label) else {
                return;
            };
            api.prevent_close();
            if ready {
                let name = if label == "main" {
                    "reader-main-close-requested"
                } else {
                    "reader-window-close-requested"
                };
                let _ = app.emit_to(label, name, ());
            } else if label == "main" {
                if readers.close_main().unwrap_or(false) {
                    app.exit(0);
                } else {
                    let _ = window.hide();
                }
            } else if let Some(child) = app.get_webview_window(label) {
                let _ = child.destroy();
                if readers.remove(label).unwrap_or(false) {
                    app.exit(0);
                }
            }
        }
        WindowEvent::Destroyed if label != "main" => {
            if readers.remove(label).unwrap_or(false) {
                app.exit(0);
            }
        }
        WindowEvent::Resized(size)
            if label != "main"
                && !window.is_fullscreen().unwrap_or(true)
                && !window.is_maximized().unwrap_or(true) =>
        {
            if let Ok(scale) = window.scale_factor() {
                let logical = size.to_logical::<f64>(scale);
                readers.record_size(
                    label,
                    ReaderWindowSize {
                        width: logical.width,
                        height: logical.height,
                    },
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;

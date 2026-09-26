//! Native reader bridge: scoped, read-only media sessions and a separate position
//! document. No download, registration or arbitrary path/URL command is exposed.
mod state;
use crate::{
    accounts::{self, DesktopAccounts},
    require_main, DesktopStore,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use state::{error, Backend, Chapter, ChapterResult, PageResult, ReaderBook, Result, Session};
pub(crate) use state::{DesktopReader, ReaderRequest};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};
use tauri::{Runtime, State, WebviewWindow};
use workbench_library::{source_reader_key, LocalReader, ReaderImage};
use workbench_storage::{LibraryReference, ReaderPosition, Source};

#[tauri::command]
pub(crate) async fn reader_open<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    store: State<'_, Arc<DesktopStore>>,
    accounts: State<'_, Arc<DesktopAccounts>>,
    request: ReaderRequest,
    request_id: String,
) -> Result<ReaderBook> {
    require_main(window.label())?;
    let ticket = reader.begin(&request_id)?;
    let generation = ticket.generation;
    let result = ticket
        .run(open(
            Arc::clone(reader.inner()),
            Arc::clone(store.inner()),
            Arc::clone(accounts.inner()),
            request,
            request_id,
            generation,
        ))
        .await;
    reader.finish_open(generation)?;
    if result.is_err() {
        restore_fullscreen_if_idle(&window, &reader)?;
    }
    result
}

async fn open(
    reader: Arc<DesktopReader>,
    store: Arc<DesktopStore>,
    accounts: Arc<DesktopAccounts>,
    request: ReaderRequest,
    request_id: String,
    generation: u64,
) -> Result<ReaderBook> {
    let id = format!("reader-{generation}");
    let (backend, title, origin, source_ref, progress_key, chapters) = match request {
        ReaderRequest::Library {
            root_id,
            generation: library_generation,
            entry_id,
        } => {
            let store = Arc::clone(&store);
            let local = tauri::async_runtime::spawn_blocking(move || {
                let store = store.open()?;
                LocalReader::open(&store, &root_id, library_generation, &entry_id)
            })
            .await
            .map_err(|_| error("READER_UNAVAILABLE"))??;
            local_book(local)
        }
        ReaderRequest::Source {
            source,
            session_id,
            work_id,
        } => {
            let reference = LibraryReference {
                source,
                work_id: work_id.clone(),
            };
            if !reference.is_valid() || session_id.is_empty() || session_id.len() > 256 {
                return Err(error("VALIDATION_FAILED"));
            }
            let native_store = Arc::clone(&store);
            let local_ref = reference.clone();
            let local = tauri::async_runtime::spawn_blocking(move || {
                let store = native_store.open()?;
                LocalReader::for_source(&store, &local_ref)
            })
            .await
            .map_err(|_| error("READER_UNAVAILABLE"))??;
            reader.require_generation(generation)?;
            if let Some(local) = local {
                local_book(local)
            } else {
                let service = accounts::service(accounts)
                    .await
                    .map_err(|e| error(e.code))?;
                let account_source = match source {
                    Source::Jm => workbench_accounts::Source::Jm,
                    Source::Pica => workbench_accounts::Source::Pica,
                };
                let online = service
                    .online_reader(account_source, &session_id, &work_id)
                    .await
                    .map_err(|e| error(e.code))?;
                let title = online.title().to_owned();
                let backend = Backend::Online(Arc::new(online));
                let Backend::Online(online) = &backend else {
                    unreachable!()
                };
                let mut chapters = Vec::new();
                let mut seen = HashSet::new();
                let mut complete = false;
                // Enumerate chapter titles only. Image-address pages are fetched
                // later for the chapter/page the reader actually requests.
                for page in 1..=5000 {
                    reader.require_generation(generation)?;
                    let result = online.chapters(page).await.map_err(|e| error(e.code))?;
                    reader.require_generation(generation)?;
                    if result.page != page || (result.has_more && result.items.is_empty()) {
                        return Err(error("READER_CHAPTER_INVALID"));
                    }
                    for chapter in result.items {
                        if !seen.insert(chapter.id.clone()) || chapters.len() == 5000 {
                            return Err(error("READER_CHAPTER_LIMIT"));
                        }
                        chapters.push((
                            chapter.order,
                            Chapter {
                                id: chapter.id,
                                title: chapter.title,
                                page_count: None,
                            },
                        ));
                    }
                    if !result.has_more {
                        complete = true;
                        break;
                    }
                }
                if !complete {
                    return Err(error("READER_CHAPTER_LIMIT"));
                }
                if chapters.is_empty() {
                    return Err(error("READER_NO_PAGES"));
                }
                chapters.sort_by_key(|(order, _)| *order);
                let chapters = chapters.into_iter().map(|(_, chapter)| chapter).collect();
                let origin = match source {
                    Source::Jm => "JM",
                    Source::Pica => "Pica",
                };
                (
                    backend,
                    title,
                    origin,
                    Some(reference.clone()),
                    source_reader_key(&reference)?,
                    chapters,
                )
            }
        }
    };
    reader.require_generation(generation)?;
    let session = Arc::new(Session::new(
        id.clone(),
        request_id,
        generation,
        progress_key,
        backend,
        &chapters,
    ));
    let native_store = Arc::clone(&store);
    let key = session.progress_key.clone();
    let position =
        tauri::async_runtime::spawn_blocking(move || native_store.open()?.reader_position(&key))
            .await
            .map_err(|_| error("READER_UNAVAILABLE"))??;
    // Opening the book should not depend on one remembered online chapter's
    // availability. The ordinary chapter command reports its error inside the
    // reader, where another chapter can be selected. Clamp unknown online page
    // counts only after that chapter has actually loaded.
    let position = position.filter(|saved| {
        session
            .page_count(&saved.chapter_id)
            .ok()
            .is_some_and(|count| count.is_none_or(|count| saved.page_index < count))
    });
    reader.publish(session)?;
    Ok(ReaderBook {
        reader_id: id,
        title,
        origin,
        source_ref,
        chapters,
        position,
    })
}

type OpenedBook = (
    Backend,
    String,
    &'static str,
    Option<LibraryReference>,
    String,
    Vec<Chapter>,
);
fn local_book(local: LocalReader) -> OpenedBook {
    let chapters = local
        .chapters()
        .into_iter()
        .map(|c| Chapter {
            id: c.id,
            title: c.title,
            page_count: Some(c.page_count),
        })
        .collect();
    let title = local.title.clone();
    let source_ref = local.source_ref.clone();
    let key = local.progress_key.clone();
    (
        Backend::Local(Arc::new(local)),
        title,
        "library",
        source_ref,
        key,
        chapters,
    )
}

async fn chapter_info(
    session: Arc<Session>,
    store: Arc<DesktopStore>,
    chapter_id: String,
) -> Result<u64> {
    session.require_current()?;
    session.page_count(&chapter_id)?;
    let count = match &session.backend {
        Backend::Local(local) => {
            let local = Arc::clone(local);
            let chapter_id = chapter_id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let store = store.open()?;
                local.verify(&store)?;
                local.page_count(&chapter_id)
            })
            .await
            .map_err(|_| error("READER_UNAVAILABLE"))??
        }
        Backend::Online(online) => {
            online
                .chapter(&chapter_id)
                .await
                .map_err(|e| error(e.code))?
                .page_count
        }
    };
    session.require_current()?;
    if count == 0 || count > 50_000 {
        return Err(error("READER_PAGE_LIMIT"));
    }
    session
        .page_counts
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?
        .insert(chapter_id, Some(count));
    Ok(count)
}

#[tauri::command]
pub(crate) async fn reader_chapter<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    store: State<'_, Arc<DesktopStore>>,
    reader_id: String,
    chapter_id: String,
) -> Result<ChapterResult> {
    require_main(window.label())?;
    let session = reader.session(&reader_id)?;
    let page_count = chapter_info(session, Arc::clone(store.inner()), chapter_id.clone()).await?;
    Ok(ChapterResult {
        reader_id,
        chapter_id,
        page_count,
    })
}

#[tauri::command]
pub(crate) async fn reader_page<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    store: State<'_, Arc<DesktopStore>>,
    reader_id: String,
    chapter_id: String,
    page_index: u64,
) -> Result<PageResult> {
    require_main(window.label())?;
    let session = reader.session(&reader_id)?;
    if session
        .page_count(&chapter_id)?
        .is_none_or(|count| page_index >= count)
    {
        return Err(error("READER_PAGE_UNKNOWN"));
    }
    let permit = Arc::clone(&reader.pages)
        .acquire_owned()
        .await
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    session.require_current()?;
    let image = match &session.backend {
        Backend::Local(local) => {
            let local = Arc::clone(local);
            let store = Arc::clone(store.inner());
            let chapter_id = chapter_id.clone();
            let current = Arc::clone(&session);
            tauri::async_runtime::spawn_blocking(move || {
                let _permit = permit;
                current.require_current()?;
                let store = store.open()?;
                local.page(&store, &chapter_id, page_index)
            })
            .await
            .map_err(|_| error("READER_UNAVAILABLE"))??
        }
        Backend::Online(online) => {
            let _permit = permit;
            let image = online
                .page(&chapter_id, page_index)
                .await
                .map_err(|e| error(e.code))?;
            ReaderImage {
                bytes: image.bytes,
                mime: image.mime,
                width: image.width,
                height: image.height,
            }
        }
    };
    session.require_current()?;
    if image.bytes.len() > 32 * 1024 * 1024
        || image.width == 0
        || image.height == 0
        || image.width > 20_000
        || image.height > 20_000
        || u64::from(image.width) * u64::from(image.height) > 32_000_000
        || !["image/jpeg", "image/png", "image/webp", "image/gif"].contains(&image.mime)
    {
        return Err(error("READER_IMAGE_LIMIT"));
    }
    let data_url = format!(
        "data:{};base64,{}",
        image.mime,
        STANDARD.encode(&image.bytes)
    );
    session.require_current()?;
    Ok(PageResult {
        reader_id,
        chapter_id,
        page_index,
        data_url,
        width: image.width,
        height: image.height,
    })
}

#[tauri::command]
pub(crate) async fn reader_save_position<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    store: State<'_, Arc<DesktopStore>>,
    reader_id: String,
    position: ReaderPosition,
) -> Result<()> {
    require_main(window.label())?;
    let session = reader.session(&reader_id)?;
    if !position.is_valid()
        || session
            .page_count(&position.chapter_id)?
            .is_none_or(|n| position.page_index >= n)
    {
        return Err(error("READER_POSITION_INVALID"));
    }
    let store = Arc::clone(store.inner());
    let sequence = session.position_sequence.fetch_add(1, Ordering::AcqRel) + 1;
    let progress = Arc::clone(&reader.progress);
    tauri::async_runtime::spawn_blocking(move || {
        let _progress = progress.lock().map_err(|_| error("READER_UNAVAILABLE"))?;
        session.require_current()?;
        if session.position_sequence.load(Ordering::Acquire) != sequence {
            return Ok(());
        }
        let store = store.open()?;
        if let Backend::Local(local) = &session.backend {
            local.verify(&store)?;
        }
        session.require_current()?;
        store.save_reader_position(&session.progress_key, position)
    })
    .await
    .map_err(|_| error("READER_UNAVAILABLE"))?
}

fn restore_fullscreen<R: Runtime>(window: &WebviewWindow<R>, reader: &DesktopReader) -> Result<()> {
    let mut previous = reader
        .fullscreen_before
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    if let Some(fullscreen) = *previous {
        window
            .set_fullscreen(fullscreen)
            .map_err(|_| error("READER_FULLSCREEN_FAILED"))?;
        *previous = None;
    }
    Ok(())
}

fn restore_fullscreen_if_idle<R: Runtime>(
    window: &WebviewWindow<R>,
    reader: &DesktopReader,
) -> Result<()> {
    let current = reader
        .current
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    let pending = reader
        .pending
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    if current.is_none() && pending.is_none() {
        restore_fullscreen(window, reader)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn reader_cancel_open<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    request_id: String,
) -> Result<()> {
    require_main(window.label())?;
    if reader.cancel_open(&request_id)? {
        restore_fullscreen_if_idle(&window, &reader)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn reader_close<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    reader_id: String,
) -> Result<()> {
    require_main(window.label())?;
    if reader.close(&reader_id)? {
        restore_fullscreen_if_idle(&window, &reader)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn reader_fullscreen<R: Runtime>(
    window: WebviewWindow<R>,
    reader: State<'_, Arc<DesktopReader>>,
    fullscreen: bool,
) -> Result<()> {
    require_main(window.label())?;
    if !fullscreen {
        return restore_fullscreen(&window, &reader);
    }
    let current = reader
        .current
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    current
        .as_ref()
        .ok_or(error("READER_CLOSED"))?
        .require_current()?;
    let mut previous = reader
        .fullscreen_before
        .lock()
        .map_err(|_| error("READER_UNAVAILABLE"))?;
    if previous.is_none() {
        *previous = Some(
            window
                .is_fullscreen()
                .map_err(|_| error("READER_FULLSCREEN_FAILED"))?,
        );
    }
    window
        .set_fullscreen(true)
        .map_err(|_| error("READER_FULLSCREEN_FAILED"))
}

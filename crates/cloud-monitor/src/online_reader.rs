//! Explicit, revocable online reading. No download task, staging, filesystem,
//! inventory or completion API is invoked by this module.
//!
//! Protocol parsers and pixel transforms reuse the audited MIT-licensed JM/Pica
//! pins in the adapters. Media transport remains private, credential-free and
//! host constrained. A reader may fetch only chapter IDs it previously listed.
mod image;
#[cfg(test)]
mod tests;

use crate::live_media_transport::{JmTransport, PicaTransport};
use jm_adapter::{media_descriptors::JmMediaItem, JmClient};
use pica_adapter::{media_descriptors::PicaMediaItem, reader::ReaderPageScope, PicaClient};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};
use tokio::sync::{Mutex, Notify, Semaphore};

pub const MAX_READER_CHAPTERS: usize = 5_000;
pub const MAX_READER_IMAGES: u64 = 50_000;
const MAX_CACHED_CHAPTERS: usize = 4;
const MAX_CACHED_IMAGE_PAGES: usize = 8;
static MEDIA_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderSource {
    Jm,
    Pica,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderError {
    pub code: &'static str,
}
pub type ReaderResult<T> = Result<T, ReaderError>;

impl std::fmt::Display for ReaderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}
impl std::error::Error for ReaderError {}

fn error(code: &'static str) -> ReaderError {
    ReaderError { code }
}

fn runtime_is_allowed(github_actions: Option<&str>) -> ReaderResult<()> {
    if github_actions.is_some_and(|value| value.eq_ignore_ascii_case("true")) {
        return Err(error("READER_GITHUB_ACTIONS_FORBIDDEN"));
    }
    Ok(())
}

/// Real reader traffic is desktop-only. Pure parsing, construction and fixture
/// tests do not require this gate; all actual reader network entry points do.
pub fn require_local_runtime() -> ReaderResult<()> {
    runtime_is_allowed(std::env::var("GITHUB_ACTIONS").ok().as_deref())
}
fn public_error(problem: String) -> ReaderError {
    let code = match problem.as_str() {
        "READER_CLOSED" => "READER_CLOSED",
        "READER_GITHUB_ACTIONS_FORBIDDEN" => "READER_GITHUB_ACTIONS_FORBIDDEN",
        "SESSION_CHANGED" => "SESSION_CHANGED",
        "HTTP_401" | "API_CODE_401" => "READER_SESSION_EXPIRED",
        "READER_REQUEST_INVALID" | "INVALID_JM_ID" | "INVALID_PICA_ID" => "READER_REQUEST_INVALID",
        "READER_CHAPTER_UNKNOWN" => "READER_CHAPTER_UNKNOWN",
        "READER_PAGE_OUT_OF_RANGE" => "READER_PAGE_OUT_OF_RANGE",
        "READER_SOURCE_CHANGED" | "PICA_PAGINATION_CHANGED" => "READER_SOURCE_CHANGED",
        "READER_CATALOG_INCOMPLETE" => "READER_CATALOG_INCOMPLETE",
        "READER_METADATA_LIMIT"
        | "READER_CHAPTER_LIMIT"
        | "READER_IMAGE_COUNT_INVALID"
        | "JM_MEDIA_RESPONSE_TOO_LARGE"
        | "PICA_MEDIA_RESPONSE_TOO_LARGE"
        | "READER_IMAGE_LIMIT" => "READER_LIMIT",
        "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED" | "READER_IMAGE_INVALID" => "READER_IMAGE_INVALID",
        value
            if value.contains("TIMEOUT")
                || value.contains("CONNECT_ERROR")
                || value.contains("TRANSPORT") =>
        {
            "READER_NETWORK_FAILED"
        }
        _ => "READER_SOURCE_UNAVAILABLE",
    };
    error(code)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub order: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterPage {
    pub items: Vec<Chapter>,
    pub page: u64,
    pub has_more: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterInfo {
    pub id: String,
    pub title: String,
    pub order: u64,
    pub page_count: u64,
}

/// Original validated encoding for Pica/GIF; JM WebP uses the existing restored
/// JPEG transform. Byte buffers are not retained or written by the service.
pub struct ReaderImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

type AccountGuard = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
struct Control {
    closed: AtomicBool,
    notify: Notify,
    account: AccountGuard,
}
impl Control {
    fn require(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("READER_CLOSED".into());
        }
        (self.account)()
    }
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    async fn run<T>(&self, future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
        // Register the notification before testing closed, so a close between
        // the check and the first poll cannot strand a pending request.
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        self.require()?;
        tokio::select! {
            biased;
            _ = &mut notified => Err("READER_CLOSED".into()),
            result = future => { self.require()?; result }
        }
    }
}

enum Client {
    Jm(JmClient),
    Pica(PicaClient),
}
struct PageCatalog {
    info: ChapterInfo,
    jm: Vec<JmMediaItem>,
    pica_scope: Option<ReaderPageScope>,
    pica_pages: BTreeMap<u64, Vec<PicaMediaItem>>,
    // Keep compact identities after URL-page eviction: a source must not move
    // an already observed image to another page or replace its position.
    pica_positions: BTreeMap<u64, String>,
    pica_ids: BTreeMap<String, u64>,
}
struct Metadata {
    client: Client,
    chapters: BTreeMap<String, Chapter>,
    directory: BTreeMap<u64, ChapterPage>,
    directory_scope: Option<ReaderPageScope>,
    images: BTreeMap<String, PageCatalog>,
}

#[derive(Clone)]
enum ImageDescriptor {
    Jm { chapter: String, item: JmMediaItem },
    Pica(PicaMediaItem),
}

/// Native-only handle. It intentionally has no Serialize/Debug implementation:
/// paths, source URLs and credentials never cross the renderer boundary.
pub struct OnlineReader {
    source: ReaderSource,
    work_id: String,
    title: String,
    control: Arc<Control>,
    metadata: Mutex<Metadata>,
    media_slots: Arc<Semaphore>,
    jm: Option<JmTransport>,
    pica: Option<PicaTransport>,
}

impl OnlineReader {
    /// No source request is made here. The account owner supplies a revocable
    /// generation guard; tokens are accepted only by the Pica metadata client.
    pub fn new(
        source: ReaderSource,
        work_id: &str,
        title: &str,
        pica_token: Option<String>,
        require_account: impl Fn() -> Result<(), String> + Send + Sync + 'static,
    ) -> ReaderResult<Self> {
        let valid_id = match source {
            ReaderSource::Jm => {
                !work_id.is_empty()
                    && work_id.len() <= 20
                    && work_id.bytes().all(|b| b.is_ascii_digit())
            }
            ReaderSource::Pica => {
                work_id.len() == 24 && work_id.bytes().all(|b| b.is_ascii_hexdigit())
            }
        };
        if !valid_id || title.len() > 16_384 || title.chars().any(char::is_control) {
            return Err(error("READER_REQUEST_INVALID"));
        }
        require_account().map_err(public_error)?;
        let (client, jm, pica) = match (source, pica_token) {
            (ReaderSource::Jm, None) => (
                Client::Jm(JmClient::new_for_reader().map_err(public_error)?),
                Some(JmTransport::for_reader().map_err(public_error)?),
                None,
            ),
            (ReaderSource::Pica, Some(token))
                if !token.is_empty()
                    && token.len() <= 8192
                    && !token.chars().any(char::is_control)
                    && token.trim() == token =>
            {
                (
                    Client::Pica(PicaClient::new_for_reader(token).map_err(public_error)?),
                    None,
                    Some(PicaTransport::for_reader().map_err(public_error)?),
                )
            }
            _ => return Err(error("READER_REQUEST_INVALID")),
        };
        Ok(Self {
            source,
            work_id: work_id.into(),
            title: title.into(),
            control: Arc::new(Control {
                closed: AtomicBool::new(false),
                notify: Notify::new(),
                account: Arc::new(require_account),
            }),
            metadata: Mutex::new(Metadata {
                client,
                chapters: BTreeMap::new(),
                directory: BTreeMap::new(),
                directory_scope: None,
                images: BTreeMap::new(),
            }),
            media_slots: Arc::clone(MEDIA_SLOTS.get_or_init(|| Arc::new(Semaphore::new(2)))),
            jm,
            pica,
        })
    }

    pub fn source(&self) -> ReaderSource {
        self.source
    }
    pub fn work_id(&self) -> &str {
        &self.work_id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn require_current(&self) -> ReaderResult<()> {
        self.control.require().map_err(public_error)
    }
    pub fn cancel(&self) {
        self.control.close();
    }

    /// Pages refer to chapter metadata, not comic images. Callers may enumerate
    /// this small directory, but no image address/bytes are fetched here.
    pub async fn chapters(&self, page: u64) -> ReaderResult<ChapterPage> {
        self.control
            .run(async {
                if !(1..=pica_adapter::reader::MAX_READER_PAGES).contains(&page) {
                    return Err("READER_REQUEST_INVALID".into());
                }
                let mut state = self.metadata.lock().await;
                self.control.require()?;
                if let Some(cached) = state.directory.get(&page) {
                    return Ok(cached.clone());
                }
                // Sequential enumeration binds each page to the same catalog and
                // prevents callers from presenting a truncated middle page as all.
                if page != state.directory.len() as u64 + 1
                    || state
                        .directory
                        .last_key_value()
                        .is_some_and(|(_, last)| !last.has_more)
                {
                    return Err("READER_REQUEST_INVALID".into());
                }
                require_local_runtime().map_err(|problem| problem.code.to_owned())?;
                let (items, scope, more) = match &mut state.client {
                    Client::Jm(client) => {
                        if page != 1 {
                            return Err("READER_REQUEST_INVALID".into());
                        }
                        let rows = client
                            .reader_chapters(&self.work_id, || self.control.require())
                            .await;
                        client.traces.clear();
                        (
                            rows?
                                .into_iter()
                                .map(|r| Chapter {
                                    id: r.id,
                                    title: r.title,
                                    order: r.order,
                                })
                                .collect(),
                            None,
                            false,
                        )
                    }
                    Client::Pica(client) => {
                        let rows = client
                            .reader_chapters(&self.work_id, page, || self.control.require())
                            .await;
                        client.traces.clear();
                        let rows = rows?;
                        let more = page < rows.scope.pages;
                        (
                            rows.items
                                .into_iter()
                                .map(|r| Chapter {
                                    id: r.id,
                                    title: r.title,
                                    order: r.order,
                                })
                                .collect(),
                            Some(rows.scope),
                            more,
                        )
                    }
                };
                self.control.require()?;
                commit_directory(
                    &mut state,
                    ChapterPage {
                        items,
                        page,
                        has_more: more,
                    },
                    scope,
                )
            })
            .await
            .map_err(public_error)
    }

    pub async fn chapter(&self, chapter_id: &str) -> ReaderResult<ChapterInfo> {
        self.control
            .run(async {
                let mut state = self.metadata.lock().await;
                self.load_chapter(&mut state, chapter_id).await
            })
            .await
            .map_err(public_error)
    }

    async fn load_chapter(
        &self,
        state: &mut Metadata,
        chapter_id: &str,
    ) -> Result<ChapterInfo, String> {
        self.control.require()?;
        let chapter = state
            .chapters
            .get(chapter_id)
            .cloned()
            .ok_or("READER_CHAPTER_UNKNOWN")?;
        if let Some(catalog) = state.images.get(chapter_id) {
            return Ok(catalog.info.clone());
        }
        require_local_runtime().map_err(|problem| problem.code.to_owned())?;
        let (jm, first_pica, count) = match &mut state.client {
            Client::Jm(client) => {
                let result = client
                    .reader_chapter_media(chapter_id, || self.control.require())
                    .await;
                client.traces.clear();
                let result = result?;
                let count = result.media.len() as u64;
                (result.media, None, count)
            }
            Client::Pica(client) => {
                let result = client
                    .reader_images(&self.work_id, chapter.order, 1, || self.control.require())
                    .await;
                client.traces.clear();
                let result = result?;
                let count = result.scope.total;
                (Vec::new(), Some(result), count)
            }
        };
        self.control.require()?;
        if count == 0 || count > MAX_READER_IMAGES {
            return Err("READER_IMAGE_COUNT_INVALID".into());
        }
        let info = ChapterInfo {
            id: chapter.id,
            title: chapter.title,
            order: chapter.order,
            page_count: count,
        };
        let mut catalog = PageCatalog {
            info: info.clone(),
            jm,
            pica_scope: first_pica.as_ref().map(|page| page.scope),
            pica_pages: BTreeMap::new(),
            pica_positions: BTreeMap::new(),
            pica_ids: BTreeMap::new(),
        };
        if let Some(first_page) = first_pica {
            commit_image_page(&mut catalog, 1, first_page)?;
        }
        if state.images.len() >= MAX_CACHED_CHAPTERS {
            if let Some(key) = state.images.keys().next().cloned() {
                state.images.remove(&key);
            }
        }
        state.images.insert(chapter_id.into(), catalog);
        Ok(info)
    }

    async fn descriptor(&self, chapter_id: &str, index: u64) -> Result<ImageDescriptor, String> {
        let mut state = self.metadata.lock().await;
        let info = self.load_chapter(&mut state, chapter_id).await?;
        if index >= info.page_count {
            return Err("READER_PAGE_OUT_OF_RANGE".into());
        }
        let catalog = state
            .images
            .get(chapter_id)
            .ok_or("READER_CHAPTER_UNKNOWN")?;
        if self.source == ReaderSource::Jm {
            return catalog
                .jm
                .get(index as usize)
                .cloned()
                .map(|item| ImageDescriptor::Jm {
                    chapter: chapter_id.into(),
                    item,
                })
                .ok_or_else(|| "READER_PAGE_OUT_OF_RANGE".into());
        }
        let scope = catalog.pica_scope.ok_or("READER_SOURCE_CHANGED")?;
        let page = index / scope.limit + 1;
        let offset = (index % scope.limit) as usize;
        if !catalog.pica_pages.contains_key(&page) {
            require_local_runtime().map_err(|problem| problem.code.to_owned())?;
            let Client::Pica(client) = &mut state.client else {
                return Err("READER_SOURCE_CHANGED".into());
            };
            let result = client
                .reader_images(&self.work_id, info.order, page, || self.control.require())
                .await;
            client.traces.clear();
            let result = result?;
            self.control.require()?;
            let catalog = state
                .images
                .get_mut(chapter_id)
                .ok_or("READER_CHAPTER_UNKNOWN")?;
            commit_image_page(catalog, page, result)?;
        }
        state
            .images
            .get(chapter_id)
            .and_then(|catalog| catalog.pica_pages.get(&page))
            .and_then(|items| items.get(offset))
            .cloned()
            .map(ImageDescriptor::Pica)
            .ok_or_else(|| "READER_PAGE_OUT_OF_RANGE".into())
    }

    /// Exactly the requested image, plus only its necessary address-metadata
    /// page. Failed images are never cached: retrying this call is independent.
    pub async fn page(&self, chapter_id: &str, page_index: u64) -> ReaderResult<ReaderImage> {
        self.control
            .run(async {
                let permit = Arc::clone(&self.media_slots)
                    .acquire_owned()
                    .await
                    .map_err(|_| "READER_CLOSED")?;
                self.control.require()?;
                let descriptor = self.descriptor(chapter_id, page_index).await?;
                self.control.require()?;
                require_local_runtime().map_err(|problem| problem.code.to_owned())?;
                let bytes = match &descriptor {
                    ImageDescriptor::Jm { chapter, item } => {
                        let url = format!(
                            "https://{}/media/photos/{chapter}/{}",
                            jm_adapter::media_descriptors::IMAGE_DOMAIN,
                            item.filename
                        );
                        self.jm
                            .as_ref()
                            .ok_or("READER_REQUEST_INVALID")?
                            .fetch_exact(&url)
                            .await?
                    }
                    ImageDescriptor::Pica(item) => {
                        let url = format!("{}/static/{}", item.file_server, item.path);
                        self.pica
                            .as_ref()
                            .ok_or("READER_REQUEST_INVALID")?
                            .fetch_with_redirects(&url, || {
                                std::future::ready(self.control.require())
                            })
                            .await?
                    }
                };
                self.control.require()?;
                let control = Arc::clone(&self.control);
                // Moving the permit into the CPU task prevents rapid cancellation
                // and retry from accumulating detached decoding jobs.
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    control.require()?;
                    let image = image::decode(descriptor, bytes)?;
                    control.require()?;
                    Ok(image)
                })
                .await
                .map_err(|_| "READER_IMAGE_INVALID")?;
                self.control.require()?;
                result
            })
            .await
            .map_err(public_error)
    }
}

impl Drop for OnlineReader {
    fn drop(&mut self) {
        self.control.close();
    }
}

fn commit_directory(
    state: &mut Metadata,
    page: ChapterPage,
    scope: Option<ReaderPageScope>,
) -> Result<ChapterPage, String> {
    if state
        .directory_scope
        .is_some_and(|previous| Some(previous) != scope)
    {
        return Err("READER_SOURCE_CHANGED".into());
    }
    if state.chapters.len().saturating_add(page.items.len()) > MAX_READER_CHAPTERS {
        return Err("READER_CHAPTER_LIMIT".into());
    }
    for item in &page.items {
        if state.chapters.contains_key(&item.id)
            || state.chapters.values().any(|old| old.order == item.order)
        {
            return Err("READER_SOURCE_CHANGED".into());
        }
    }
    if !page.has_more
        && scope
            .is_some_and(|scope| scope.total != (state.chapters.len() + page.items.len()) as u64)
    {
        return Err("READER_CATALOG_INCOMPLETE".into());
    }
    for item in &page.items {
        state.chapters.insert(item.id.clone(), item.clone());
    }
    state.directory_scope = scope;
    state.directory.insert(page.page, page.clone());
    Ok(page)
}

fn commit_image_page(
    catalog: &mut PageCatalog,
    page: u64,
    incoming: pica_adapter::reader::ReaderMediaPage,
) -> Result<(), String> {
    if catalog.pica_scope != Some(incoming.scope)
        || page == 0
        || page > incoming.scope.pages
        || incoming.scope.total > MAX_READER_IMAGES
        || incoming.scope.limit == 0
    {
        return Err("READER_SOURCE_CHANGED".into());
    }
    let start = (page - 1)
        .checked_mul(incoming.scope.limit)
        .ok_or("READER_SOURCE_CHANGED")?;
    let mut positions = BTreeMap::new();
    for (offset, item) in incoming.items.iter().enumerate() {
        let index = start
            .checked_add(offset as u64)
            .ok_or("READER_SOURCE_CHANGED")?;
        if index >= incoming.scope.total
            || catalog
                .pica_positions
                .get(&index)
                .is_some_and(|old| old != &item.media_id)
            || catalog
                .pica_ids
                .get(&item.media_id)
                .is_some_and(|old| *old != index)
            || positions.insert(item.media_id.clone(), index).is_some()
        {
            return Err("READER_SOURCE_CHANGED".into());
        }
    }
    if !catalog.pica_pages.contains_key(&page) && catalog.pica_pages.len() >= MAX_CACHED_IMAGE_PAGES
    {
        if let Some(key) = catalog.pica_pages.keys().next().copied() {
            catalog.pica_pages.remove(&key);
        }
    }
    for (id, index) in positions {
        catalog.pica_positions.insert(index, id.clone());
        catalog.pica_ids.insert(id, index);
    }
    catalog.pica_pages.insert(page, incoming.items);
    Ok(())
}

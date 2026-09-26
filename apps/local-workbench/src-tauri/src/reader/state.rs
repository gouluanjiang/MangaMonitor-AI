use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use workbench_accounts::OnlineReader;
use workbench_library::LocalReader;
use workbench_storage::{LibraryReference, ReaderPosition, Source, StoreError};

pub(super) type Result<T> = std::result::Result<T, StoreError>;
pub(super) const fn error(code: &'static str) -> StoreError {
    StoreError { code }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ReaderRequest {
    Library {
        root_id: String,
        generation: u64,
        entry_id: String,
    },
    Source {
        source: Source,
        session_id: String,
        work_id: String,
    },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Chapter {
    pub id: String,
    pub title: String,
    pub page_count: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReaderBook {
    pub reader_id: String,
    pub title: String,
    pub origin: &'static str,
    pub source_ref: Option<LibraryReference>,
    pub chapters: Vec<Chapter>,
    pub position: Option<ReaderPosition>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChapterResult {
    pub reader_id: String,
    pub chapter_id: String,
    pub page_count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageResult {
    pub reader_id: String,
    pub chapter_id: String,
    pub page_index: u64,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

pub(super) enum Backend {
    Local(Arc<LocalReader>),
    Online(Arc<OnlineReader>),
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Self::Online(reader) = self {
            reader.cancel();
        }
    }
}

pub(super) struct Session {
    pub id: String,
    pub request_id: String,
    pub generation: u64,
    pub progress_key: String,
    pub backend: Backend,
    pub page_counts: Mutex<HashMap<String, Option<u64>>>,
    pub position_sequence: AtomicU64,
    alive: AtomicBool,
}

impl Session {
    pub fn new(
        id: String,
        request_id: String,
        generation: u64,
        progress_key: String,
        backend: Backend,
        chapters: &[Chapter],
    ) -> Self {
        Self {
            id,
            request_id,
            generation,
            progress_key,
            backend,
            page_counts: Mutex::new(
                chapters
                    .iter()
                    .map(|c| (c.id.clone(), c.page_count))
                    .collect(),
            ),
            position_sequence: AtomicU64::new(0),
            alive: AtomicBool::new(true),
        }
    }
    pub fn require_current(&self) -> Result<()> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(error("READER_CLOSED"));
        }
        if let Backend::Online(reader) = &self.backend {
            reader.require_current().map_err(|e| error(e.code))?;
        }
        Ok(())
    }
    pub fn cancel(&self) {
        self.alive.store(false, Ordering::Release);
        if let Backend::Online(reader) = &self.backend {
            reader.cancel();
        }
    }
    pub fn page_count(&self, chapter_id: &str) -> Result<Option<u64>> {
        self.page_counts
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?
            .get(chapter_id)
            .copied()
            .ok_or(error("READER_CHAPTER_UNKNOWN"))
    }
}

pub(super) struct PendingOpen {
    pub request_id: String,
    pub generation: u64,
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}
impl PendingOpen {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    pub async fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        // Register before inspecting cancellation. Dropping the losing future
        // closes direct HTTP awaits and its Online backend RAII guard.
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.cancelled.load(Ordering::Acquire) {
            return Err(error("READER_CLOSED"));
        }
        tokio::select! {
            biased;
            _=&mut notified=>Err(error("READER_CLOSED")),
            result=future=>result,
        }
    }
}

pub(crate) struct DesktopReader {
    pub sequence: AtomicU64,
    pub(super) current: Mutex<Option<Arc<Session>>>,
    pub(super) pending: Mutex<Option<Arc<PendingOpen>>>,
    cancelled_requests: Mutex<VecDeque<String>>,
    pub pages: Arc<tokio::sync::Semaphore>,
    pub fullscreen_before: Mutex<Option<bool>>,
    pub progress: Arc<Mutex<()>>,
}

impl Default for DesktopReader {
    fn default() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            current: Mutex::new(None),
            pending: Mutex::new(None),
            cancelled_requests: Mutex::new(VecDeque::new()),
            pages: Arc::new(tokio::sync::Semaphore::new(2)),
            fullscreen_before: Mutex::new(None),
            progress: Arc::new(Mutex::new(())),
        }
    }
}

impl DesktopReader {
    pub(super) fn begin(&self, request_id: &str) -> Result<Arc<PendingOpen>> {
        validate_request_id(request_id)?;
        let mut current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if self
            .cancelled_requests
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?
            .iter()
            .any(|id| id == request_id)
        {
            return Err(error("READER_CLOSED"));
        }
        if current
            .as_ref()
            .is_some_and(|session| session.request_id == request_id)
            || pending
                .as_ref()
                .is_some_and(|ticket| ticket.request_id == request_id)
        {
            return Err(error("READER_REQUEST_INVALID"));
        }
        let generation = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        if let Some(previous) = current.take() {
            previous.cancel();
        }
        if let Some(previous) = pending.take() {
            previous.cancel();
        }
        let ticket = Arc::new(PendingOpen {
            request_id: request_id.into(),
            generation,
            cancelled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        });
        *pending = Some(Arc::clone(&ticket));
        Ok(ticket)
    }
    pub fn require_generation(&self, generation: u64) -> Result<()> {
        if self.sequence.load(Ordering::Acquire) != generation {
            Err(error("READER_CLOSED"))
        } else {
            Ok(())
        }
    }
    pub(super) fn session(&self, id: &str) -> Result<Arc<Session>> {
        let current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let session = current
            .as_ref()
            .filter(|s| s.id == id)
            .cloned()
            .ok_or(error("READER_CLOSED"))?;
        session.require_current()?;
        Ok(session)
    }
    pub(super) fn publish(&self, session: Arc<Session>) -> Result<()> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        self.require_generation(session.generation)?;
        if !pending.as_ref().is_some_and(|ticket| {
            ticket.generation == session.generation
                && ticket.request_id == session.request_id
                && !ticket.cancelled.load(Ordering::Acquire)
        }) {
            return Err(error("READER_CLOSED"));
        }
        session.require_current()?;
        *current = Some(session);
        *pending = None;
        Ok(())
    }
    pub fn cancel_open(&self, request_id: &str) -> Result<bool> {
        validate_request_id(request_id)?;
        let mut current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        // A cancel IPC can win scheduling before its open IPC. Keep a small
        // tombstone set so that late open cannot resurrect a closed overlay.
        let mut cancelled = self
            .cancelled_requests
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if !cancelled.iter().any(|id| id == request_id) {
            if cancelled.len() == 128 {
                cancelled.pop_front();
            }
            cancelled.push_back(request_id.into());
        }
        let matches_pending = pending
            .as_ref()
            .is_some_and(|ticket| ticket.request_id == request_id);
        let matches_current = current
            .as_ref()
            .is_some_and(|session| session.request_id == request_id);
        if matches_pending {
            if let Some(ticket) = pending.take() {
                ticket.cancel();
            }
        }
        // Publication can precede delivery of reader_open's IPC response.
        if matches_current {
            if let Some(session) = current.take() {
                session.cancel();
            }
        }
        if matches_pending || matches_current {
            self.sequence.fetch_add(1, Ordering::AcqRel);
        }
        Ok(matches_pending || matches_current)
    }
    pub fn finish_open(&self, generation: u64) -> Result<()> {
        let _current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if pending
            .as_ref()
            .is_some_and(|ticket| ticket.generation == generation)
        {
            *pending = None;
        }
        Ok(())
    }
    pub fn close(&self, id: &str) -> Result<bool> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| error("READER_UNAVAILABLE"))?;
        if current.as_ref().is_some_and(|s| s.id == id) {
            if let Some(session) = current.take() {
                session.cancel();
            }
            self.sequence.fetch_add(1, Ordering::AcqRel);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn validate_request_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        Err(error("READER_REQUEST_INVALID"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_cancel_is_token_scoped_and_remembered_before_open_starts() {
        let state = DesktopReader::default();
        let old = state.begin("strict-old").unwrap();
        let new = state.begin("strict-new").unwrap();
        assert!(old.cancelled.load(Ordering::Acquire));
        assert!(!state.cancel_open("strict-old").unwrap());
        assert!(!new.cancelled.load(Ordering::Acquire));
        assert!(state.require_generation(new.generation).is_ok());
        assert!(state.cancel_open("strict-new").unwrap());
        assert!(new.cancelled.load(Ordering::Acquire));
        assert!(state.require_generation(new.generation).is_err());
        assert!(!state.cancel_open("not-started").unwrap());
        assert!(state.begin("not-started").is_err());
    }
    #[test]
    fn cancelling_open_drops_the_inflight_directory_future_before_another_page() {
        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let state = DesktopReader::default();
        let ticket = state.begin("pending-test").unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let visited = AtomicU64::new(0);
        let result: Result<()> = tauri::async_runtime::block_on(ticket.run(async {
            let _guard = DropFlag(Arc::clone(&dropped));
            visited.fetch_add(1, Ordering::AcqRel);
            state.cancel_open("pending-test")?;
            std::future::pending::<()>().await;
            visited.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }));
        assert_eq!(result.unwrap_err().code, "READER_CLOSED");
        assert_eq!(visited.load(Ordering::Acquire), 1);
        assert!(dropped.load(Ordering::Acquire));
    }
}

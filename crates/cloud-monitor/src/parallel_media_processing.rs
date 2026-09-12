//! CPU-only media processing. The coordinator retains all network, filesystem,
//! authorization, ordering, and completion responsibilities.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

const MAX_PARALLEL_PROCESSING: usize = 20;
const WORKER_FAILED: &str = "media processing worker failed";

pub(crate) struct MediaProcessor {
    state: Arc<ProcessingState>,
}

struct ProcessingState {
    permits: Arc<Semaphore>,
    active: AtomicUsize,
    idle: Notify,
}

impl MediaProcessor {
    pub(crate) fn new() -> Self {
        let parallelism = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            .min(MAX_PARALLEL_PROCESSING);
        Self::with_parallelism(parallelism)
    }

    fn with_parallelism(parallelism: usize) -> Self {
        Self {
            state: Arc::new(ProcessingState {
                permits: Arc::new(Semaphore::new(parallelism)),
                active: AtomicUsize::new(0),
                idle: Notify::new(),
            }),
        }
    }

    /// Register an outer async job before spawning it. Keep this guard in that
    /// job through its GET and CPU wait so drain cannot observe a gap before
    /// process registers the independently owned blocking closure.
    pub(crate) fn track(&self) -> MediaWorkGuard {
        self.state.active.fetch_add(1, Ordering::AcqRel);
        MediaWorkGuard {
            state: self.state.clone(),
            permit: None,
        }
    }

    pub(crate) async fn process<T, F>(&self, operation: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let permit = self
            .state
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| WORKER_FAILED.to_owned())?;
        // Register before spawn so work queued in Tokio's blocking pool already
        // counts toward drain. The receiving future never owns this guard again.
        let mut guard = self.track();
        guard.permit = Some(permit);
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            operation()
        })
        .await
        .map_err(|_| WORKER_FAILED.to_owned())?
    }

    /// Wait for tracked async jobs and dispatched closures, including queued
    /// work and work whose receiving future was cancelled. Stop creating jobs
    /// first; this does not close the pool. Use track before spawning async jobs
    /// to include their GET and semaphore-wait phases as well as CPU processing.
    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.state.idle.notified();
            tokio::pin!(notified);
            // Register before observing zero to avoid losing the final drop's
            // wakeup between the active check and the first await poll.
            let _ = notified.as_mut().enable();
            if self.state.active.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

pub(crate) struct MediaWorkGuard {
    state: Arc<ProcessingState>,
    permit: Option<OwnedSemaphorePermit>,
}

impl Drop for MediaWorkGuard {
    fn drop(&mut self) {
        // Release capacity before advertising idle; drain must not finish while
        // a completed closure still retains a permit. Unwinding uses this path.
        drop(self.permit.take());
        if self.state.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.state.idle.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{atomic::AtomicBool, mpsc},
        time::Duration,
    };
    use tokio::{sync::oneshot, time::timeout};

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn held_job(
        processor: Arc<MediaProcessor>,
        value: usize,
        entered: oneshot::Sender<()>,
        release: mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<Result<usize, String>> {
        tokio::spawn(async move {
            processor
                .process(move || {
                    let _ = entered.send(());
                    release
                        .recv_timeout(TEST_TIMEOUT)
                        .map_err(|_| "synthetic release missing".to_owned())?;
                    Ok(value)
                })
                .await
        })
    }

    #[tokio::test]
    async fn independent_cpu_jobs_actually_overlap() {
        let processor = Arc::new(MediaProcessor::with_parallelism(2));
        let (entered_a, started_a) = oneshot::channel();
        let (entered_b, started_b) = oneshot::channel();
        let (release_a, held_a) = mpsc::channel();
        let (release_b, held_b) = mpsc::channel();
        let first = held_job(processor.clone(), 7, entered_a, held_a);
        let second = held_job(processor.clone(), 8, entered_b, held_b);

        timeout(TEST_TIMEOUT, async {
            started_a.await.unwrap();
            started_b.await.unwrap();
        })
        .await
        .expect("both closures must enter before either is released");
        assert_eq!(processor.state.active.load(Ordering::Acquire), 2);
        release_a.send(()).unwrap();
        release_b.send(()).unwrap();
        assert_eq!(first.await.unwrap().unwrap(), 7);
        assert_eq!(second.await.unwrap().unwrap(), 8);
        timeout(TEST_TIMEOUT, processor.drain()).await.unwrap();
        assert_eq!(processor.state.permits.available_permits(), 2);
    }

    #[tokio::test]
    async fn cancelling_receiver_keeps_real_cpu_work_in_drain() {
        let processor = Arc::new(MediaProcessor::with_parallelism(1));
        let (entered, started) = oneshot::channel();
        let (release, held) = mpsc::channel();
        let receiver = held_job(processor.clone(), 7, entered, held);
        timeout(TEST_TIMEOUT, started).await.unwrap().unwrap();
        receiver.abort();
        assert!(receiver.await.unwrap_err().is_cancelled());

        assert_eq!(processor.state.active.load(Ordering::Acquire), 1);
        assert_eq!(processor.state.permits.available_permits(), 0);
        assert!(timeout(Duration::from_millis(25), processor.drain())
            .await
            .is_err());
        release.send(()).unwrap();
        timeout(TEST_TIMEOUT, processor.drain()).await.unwrap();
        assert_eq!(processor.state.active.load(Ordering::Acquire), 0);
        assert_eq!(processor.state.permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn tracked_async_job_covers_the_handoff_to_independent_cpu_work() {
        let processor = Arc::new(MediaProcessor::with_parallelism(1));
        let guard = processor.track();
        let (dispatch, may_dispatch) = oneshot::channel();
        let (entered, started) = oneshot::channel();
        let (release, held) = mpsc::channel();
        let job = {
            let processor = processor.clone();
            tokio::spawn(async move {
                let _guard = guard;
                may_dispatch.await.unwrap();
                processor
                    .process(move || {
                        let _ = entered.send(());
                        held.recv_timeout(TEST_TIMEOUT)
                            .map_err(|_| "synthetic release missing".to_owned())?;
                        Ok(())
                    })
                    .await
            })
        };
        assert_eq!(processor.state.active.load(Ordering::Acquire), 1);
        assert!(timeout(Duration::from_millis(25), processor.drain())
            .await
            .is_err());
        dispatch.send(()).unwrap();
        timeout(TEST_TIMEOUT, started).await.unwrap().unwrap();
        assert_eq!(processor.state.active.load(Ordering::Acquire), 2);

        job.abort();
        assert!(job.await.unwrap_err().is_cancelled());
        assert_eq!(processor.state.active.load(Ordering::Acquire), 1);
        assert_eq!(processor.state.permits.available_permits(), 0);
        assert!(timeout(Duration::from_millis(25), processor.drain())
            .await
            .is_err());
        release.send(()).unwrap();
        timeout(TEST_TIMEOUT, processor.drain()).await.unwrap();
        assert_eq!(processor.state.active.load(Ordering::Acquire), 0);
        assert_eq!(processor.state.permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn panic_releases_count_and_permit_and_pool_remains_usable() {
        let processor = MediaProcessor::with_parallelism(1);
        let result = processor
            .process(|| -> Result<(), String> { panic!("synthetic CPU panic") })
            .await;
        assert_eq!(result.unwrap_err(), WORKER_FAILED);
        timeout(TEST_TIMEOUT, processor.drain()).await.unwrap();
        assert_eq!(processor.state.active.load(Ordering::Acquire), 0);
        assert_eq!(processor.state.permits.available_permits(), 1);
        assert_eq!(processor.process(|| Ok(9)).await.unwrap(), 9);
    }

    #[test]
    fn cancelled_receiver_does_not_forget_work_queued_in_blocking_pool() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let (entered, started) = oneshot::channel();
            let (release, held) = mpsc::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                let _ = entered.send(());
                held.recv_timeout(TEST_TIMEOUT).unwrap();
            });
            timeout(TEST_TIMEOUT, started).await.unwrap().unwrap();

            let processor = Arc::new(MediaProcessor::with_parallelism(1));
            let ran = Arc::new(AtomicBool::new(false));
            let receiver = {
                let processor = processor.clone();
                let ran = ran.clone();
                tokio::spawn(async move {
                    processor
                        .process(move || {
                            ran.store(true, Ordering::Release);
                            Ok(())
                        })
                        .await
                })
            };
            timeout(TEST_TIMEOUT, async {
                while processor.state.active.load(Ordering::Acquire) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(!ran.load(Ordering::Acquire));
            receiver.abort();
            assert!(receiver.await.unwrap_err().is_cancelled());
            assert!(timeout(Duration::from_millis(25), processor.drain())
                .await
                .is_err());

            release.send(()).unwrap();
            blocker.await.unwrap();
            timeout(TEST_TIMEOUT, processor.drain()).await.unwrap();
            assert!(ran.load(Ordering::Acquire));
            assert_eq!(processor.state.active.load(Ordering::Acquire), 0);
            assert_eq!(processor.state.permits.available_permits(), 1);
        });
    }
}

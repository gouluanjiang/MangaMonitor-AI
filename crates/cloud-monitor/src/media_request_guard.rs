//! Per-request authorization rendezvous for independently spawned media GETs.
//!
//! The HTTP task owns only a channel sender. The staging coordinator retains
//! the single borrowed current-state callback and grants one request at a time;
//! no cached authorization value or source credential crosses this channel.

use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};

const CLOSED: &str = "MEDIA_REQUEST_GUARD_CLOSED";
type Reply = oneshot::Sender<Result<(), String>>;

#[derive(Clone)]
pub(crate) struct RequestGuard {
    sender: mpsc::Sender<Reply>,
}

pub(crate) struct GrantRequests {
    receiver: mpsc::Receiver<Reply>,
}

pub(crate) fn channel(capacity: usize) -> (RequestGuard, GrantRequests) {
    let (sender, receiver) = mpsc::channel(capacity);
    (RequestGuard { sender }, GrantRequests { receiver })
}

impl RequestGuard {
    /// A fresh rendezvous is required immediately before each physical GET,
    /// including the initial URL and every explicitly validated redirect hop.
    pub(crate) async fn require_current(&self) -> Result<(), String> {
        let (reply, result) = oneshot::channel();
        self.sender
            .send(reply)
            .await
            .map_err(|_| CLOSED.to_owned())?;
        result.await.map_err(|_| CLOSED.to_owned())?
    }
}

impl GrantRequests {
    /// Called on every coordinator poll, before polling the media wrappers.
    /// poll_recv also registers the coordinator waker when the queue is empty;
    /// try_recv alone would deadlock if every HTTP task awaited its next grant.
    pub(crate) fn poll_current(
        &mut self,
        context: &mut Context<'_>,
        mut current: impl FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        while let Poll::Ready(Some(reply)) = self.receiver.poll_recv(context) {
            if reply.is_closed() {
                // The request was cancelled before it could consume a grant.
                // It cannot start another GET through this ticket.
                continue;
            }
            let result = current();
            // Cancellation may race with delivery. An authorization failure
            // still terminates the owning staging coordinator.
            let _ = reply.send(result.clone());
            result?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, future::poll_fn, future::Future, task::Poll, time::Duration};
    use tokio::time::timeout;

    const LIMIT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn initial_get_and_redirect_each_wait_for_a_fresh_coordinator_grant() {
        let (guard, mut requests) = channel(1);
        let calls = Cell::new(0);
        let granted = Cell::new(0);
        let operation = async {
            for _ in 0..2 {
                guard.require_current().await?;
                calls.set(calls.get() + 1);
            }
            Ok::<_, String>(())
        };
        tokio::pin!(operation);
        timeout(
            LIMIT,
            poll_fn(|cx| {
                requests.poll_current(cx, || {
                    granted.set(granted.get() + 1);
                    assert_eq!(granted.get(), calls.get() + 1);
                    Ok(())
                })?;
                operation.as_mut().poll(cx)
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(granted.get(), 2);
    }

    #[tokio::test]
    async fn denied_next_request_preserves_the_reason_on_both_sides() {
        let (guard, mut requests) = channel(1);
        let calls = Cell::new(0);
        let operation = async {
            guard.require_current().await?;
            calls.set(1);
            guard.require_current().await?;
            calls.set(2);
            Ok::<_, String>(())
        };
        tokio::pin!(operation);
        let error = timeout(
            LIMIT,
            poll_fn(|cx| {
                requests.poll_current(cx, || {
                    if calls.get() == 0 {
                        Ok(())
                    } else {
                        Err("DOWNLOAD_PAUSED".into())
                    }
                })?;
                operation.as_mut().poll(cx)
            }),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error, "DOWNLOAD_PAUSED");
        assert_eq!(
            timeout(LIMIT, operation).await.unwrap().unwrap_err(),
            "DOWNLOAD_PAUSED"
        );
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn closing_coordinator_releases_queued_and_capacity_waiting_requests() {
        let (guard, requests) = channel(1);
        let first = guard.require_current();
        let second = guard.require_current();
        tokio::pin!(first, second);
        poll_fn(|cx| {
            assert!(first.as_mut().poll(cx).is_pending());
            assert!(second.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(requests);
        assert_eq!(timeout(LIMIT, first).await.unwrap().unwrap_err(), CLOSED);
        assert_eq!(timeout(LIMIT, second).await.unwrap().unwrap_err(), CLOSED);
    }

    #[tokio::test]
    async fn cancelled_request_cannot_receive_a_late_grant() {
        let (guard, mut requests) = channel(1);
        let mut request = Box::pin(guard.require_current());
        poll_fn(|cx| {
            assert!(request.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(request);
        poll_fn(|cx| {
            requests
                .poll_current(cx, || panic!("cancelled ticket must not be granted"))
                .unwrap();
            Poll::Ready(())
        })
        .await;
    }
}

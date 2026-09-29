# Cover loading: visible-first, bounded runtime reuse

Scope: the first approved backlog item only. The user will perform final desktop acceptance. Download queue cleanup, A6, recent updates, reader and UI redesign remain later work.

## Diagnosis and change

The previous renderer source queue was FIFO with two active jobs; nearby/offscreen requests could remain ahead of a newly visible row. Local covers used a single renderer worker, and native library decoding held the same service mutex as scan/registration operations. Raising renderer concurrency alone would not remove that native serialization. Existing Blob caches already survived card unmounts; this work does not claim to introduce session caching.

Both cover paths now distinguish the actual `main` viewport from a 400 px adjacent preload margin. Visible cards precede queued nearby cards; a prefetched card is promoted on entering the viewport. There is at most one speculative nearby job. Leaving the margin immediately removes a queued request when it has no remaining consumers. Started work can finish into the bounded runtime cache, so returning does not unnecessarily restart it. Shared consumers preserve the highest requested priority and one in-flight load per identity.

Source requests allow four active jobs in both the renderer and account service. Local covers allow two in the renderer and native command. The local native permit is held by the blocking worker even if its IPC waiter is dropped. A state-independent library cover reader no longer holds the scan service mutex while reading/decoding ZIP images; root, generation, revision and actual file validation remain unchanged before/after the read. Metadata authorization, source origin allowlists, redirects, byte/dimension limits, credentials, retries and download concurrency are unchanged.

Ready Blob images use the explicit visibility scheduler instead of a second browser lazy-loading delay. Decoding is asynchronous. Existing bounds remain: network compressed cover cache 256 MiB / 4096 entries; local compressed cache 64 MiB. These are compressed-data budgets, not a total process-RAM limit. No persistent cover files or settings are added. Closing the application clears runtime cover data. Late local image errors cannot evict a replacement URL, and cancellation/temporary queue pressure is not cached as a permanent failure.

Concurrent directory refresh or successful registration can advance the stored revision during a cover read. A `LIBRARY_STALE_SNAPSHOT` response is deferred and retried by the existing observed-card retry timer rather than cached as a permanent cover failure. Leaving the observed region cancels this retry; changed/missing files and other errors keep their normal explicit failure behavior.

## Verification

Formal checks/builds run only in CI. Focused regressions cover visible-first priority, promotion, bounded speculative work, queue cancellation/overflow, exception recovery, shared cache requests, stale library generations, stale image errors, source-session logout, retained thumbnails after scrolling/detail/settings, and two independent native workers without holding the scan mutex. Existing file/scope/identity tests continue to apply to the shared reader.

A controlled 21-cover equal-latency diagnostic compares scheduling waves: two source slots require 11 waves versus 6 with four; one local slot requires 21 versus 11 with two. It uses synthetic promises and the same request count, not source traffic or a real ZIP/network timing measurement. It must not be reported as a real-world speedup factor. Actual network/ZIP latency, first-screen paint, scrolling and return-to-view behavior remain user acceptance items.

Final revision `82048feadcd19681669bd91fe43c56c51fde1d8e` passed [frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35947947927) (192 logic / 159 Chromium), [baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35947947801) and [desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35947947880) (including 39 native IPC tests, Clippy and actual isolated Windows WebView startup/restart). The verified executable and updated Dev shortcuts are recorded in the handoff. Real user experience acceptance remains pending. No all-author scan, real media download, account change, library mutation, installer, merge or production enablement is part of this batch.

## User acceptance

1. Open the delivered Dev executable after exiting the previous version. Inspect the first visible covers in the PC library, JM/Pica favorites and author results.
2. Scroll down several screens, then scroll back. Loaded covers should be reused within the session; rapidly scrolling past unseen rows should not leave the visible row behind a long obsolete queue.
3. Open a loaded work's detail and return, then switch to another app page and return. The same session thumbnail should remain reusable.
4. Check that a genuine source/invalid-image failure remains visible and retryable. Faster scheduling cannot repair a source that does not supply a usable image.
5. Exit and reopen. Loading thumbnails again is expected; covers are not stored long term on disk.

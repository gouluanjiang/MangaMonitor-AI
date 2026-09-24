# Cover loading, second optimization round

The user reported that the first delivery is faster and authorized another optimization round, followed by their own experience acceptance and an assessment of remaining worthwhile work. This batch does not advance the other backlog items.

## Changes

- Native source sessions retain up to 8192 cover descriptors under a 4 MiB accounting budget (URL/key strings plus estimated entry overhead), replacing the 128-address limit. A tree-based LRU order avoids scanning the entire retained list on every access. The existing 20000-known-work bound, session isolation, source URL/redirect validation and missing/unknown distinctions remain. These are addresses in process memory, not downloaded cover files, and the budget is not a total process RAM limit.
- A retained descriptor remains usable when the larger work-metadata entry was evicted. Only unavailable metadata needs a detail request; a race that loses the descriptor retains the existing one-time recovery. This never grants favorite/follow/download authority or populates the action-authorizing metadata cache.
- All mounted covers in a scroll container share an observer pair and one passive scroll listener. The preload margin leads the scroll direction by one viewport (bounded to 400–1200 px), retains 200 px behind, and turns after a 48 px direction change to avoid jitter. Retired observer callbacks are ignored; last unmount releases all observers/listeners. Virtualization still bounds mounted rows.
- Source preloading can use two otherwise free slots within the unchanged four-request total. Visible tasks always lead the waiting queue, leaving capacity for newly visible work. Local cover loading retains two total slots and one speculative slot. Old queued work cancels when it leaves the observed range. Already-started work may finish into the runtime cache.

Existing image quality, dimensions, byte limits, validation, source/session checks and runtime-only Blob cache limits remain. No source endpoint, authentication flow, cover persistence, account action, download behavior, ZIP/library record or author policy changes.

## Verification and further assessment

Formal suites and builds run only in CI. New regressions cover the descriptor byte budget and LRU replacement; early covers in a 1500-item catalog without detail fanout; retained descriptors without action authority and race recovery; shared observer cleanup, direction changes and stale callbacks; two bounded prefetch slots with visible arrivals; and next-screen loading before scrolling with no duplicate image request on entry.

A separate ignored synthetic diagnostic runs once in the Windows release profile after the executable build, using already-built dependencies. It measures source-image decode, resize/JPEG encode, Base64 roundtrip, normalized-thumbnail decode and total thumbnail processing for two generated image sizes. It excludes network, queue waiting, ZIP I/O, WebView IPC and browser decode; results are not live latency claims. No real account traffic or user-library performance probe runs for this batch. Final timings, CI links, delivery and the decision on further optimization will be recorded after CI.

The application still needs source metadata after a fresh session when it has never obtained an address. An in-memory descriptor budget cannot remove that first lookup or source network latency. Binary IPC, thumbnail decode/resize alternatives and further concurrency changes are candidates to evaluate against measured cost, not automatically accepted follow-up work. Persistent cover files remain excluded.

## User acceptance

Open the delivered Dev version after exiting the previous executable. Compare first-screen covers, normal continuous scrolling, changing direction, jumping far down a large list and returning, and detail/settings return. Check both JM/Pica pages and the PC library. A larger preload window can fetch a small nearby region before it is viewed; it must not fetch the whole catalog. Closing/reopening reloads covers as intended. Genuine source failures retain the existing retry behavior.

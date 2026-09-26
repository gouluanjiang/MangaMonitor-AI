# Website recent updates

The user deferred the A6 all-followed-author acceptance and authorized the next scheduled development batch on 2026-09-26. Existing A6 single-author and repeated-single-author evidence remains valid within its documented scope; do not restart the full author scan for this batch.

## Product behavior

- Add 最近更新 within Discover, alongside the existing JM weekly recommendations and Pica rankings. Preserve the accepted navigation, card grid, density and detail/return behavior.
- Select JM or Pica and read only its first page when opening this view. Retain the source's latest ordering; do not re-sort the loaded subset and imply a different website-wide order.
- Continue one page at a time on a fresh downward scroll near the list end. Filtering the loaded list must not start an unattended crawl to fill an empty screen; provide explicit single-page continuation in filtered/empty/error cases.
- Refresh starts from the current first page. Keep the previously displayed list if that request fails; replace its sequence only on success, and clear old batch selection. Retry an unsuccessful continuation at that same page.
- Latest feeds are mutable: additions and removals can change reported totals or repeat an item across adjacent pages. De-duplicate source IDs while preserving first-seen order, show the loaded scope and pagination warnings honestly, and do not claim an exhaustive stable snapshot of the website.
- Reuse the existing source title/author, runtime-only cover cache, explicit language badge, website-provided date, inventory filters and detail/manual download confirmation. No automatic author attribution filter applies to this website-wide feed. Download registration continues through the existing executor unchanged.
- Selection applies only to the current loaded and filtered records, with the existing 500-work confirmation limit. Source, session, filter and refresh changes clear obsolete selection. No single click selects an unqueried website catalog.
- Returning from details or switching Discover tabs retains the visited view's runtime list. Hidden panels must not start or continue source requests; expired/replaced sessions cannot reveal or append stale account results.

## Source semantics and boundaries

Use the existing pinned protocols, clients, source/session identity checks and normal source error handling. The request kind is `recent`, with an empty query, no folder, no reverse flag and an explicit positive page. SourcePage is the response shape; no new persistence document or credentials destination is introduced.

The existing JMComic-Crawler-Python pin establishes latest category browsing with `/categories/filter`, `c=0`, `o=mr`. The existing lanyeeee Pica pin permits empty-keyword/category advanced search with `sort=dd` (newest first). Preserve these source orders. Neither reference alone proves that every chapter added to an old comic appears at the top, or that a website user's language/category preferences produce an identical list. Describe this as source-provided latest order; never manufacture an update date, fetch every detail to fill dates, or promise an exhaustive chapter-change feed. Source-specific date gaps remain visible as unknown.

No global scan, background watcher, cross-source identity inference, new follows, real download, local-library cleanup, installer, production enablement or formal release belongs to this batch. Built-in reader and overall UI work remain later batches.

## Verification

Formal suites/builds run once in CI under AGENTS.md. Add targeted protocol and renderer tests for request routing and validation, item isolation, session changes, mutable pagination, duplicates, partial/error retry, refresh preservation, filtered-list continuation, current-selection bounds and navigation retention. Capture the affected running synthetic UI in CI for comparison with the accepted Discover reference. Keep synthetic/engineering evidence separate from real-source and user experience acceptance; the latter stays pending until actually performed.

## Bottom-continuation correction

During real use the user found that continued downward input at the bottom required an upward detour before another page would load. The old handler only checked on an increasing scroll position and expired drag intent after 1.5 seconds. The authorized correction uses a stable listener with direct positive-input checks near the end, including a clamped position, and a held-scrollbar gesture independent of a short timer. Input is consumed before a request; completing it or reflowing the virtual grid does not create another request. Non-downward/editor/modal inputs and inactive/filtered/error/complete states cannot continue a feed. Real Chromium mouse dragging and the bottom/no-movement case are added to the existing browser suite. Code and tests are being reviewed; final evidence follows actual CI and delivery.

## Initial delivery evidence

Implementation, independent static review, CI, visual review and verified Dev delivery are complete. A6 all-author acceptance remains deferred at the user's request; recent-updates real-source and user experience acceptance remain pending. Formal suites/builds ran only in CI. No real source query, author scan, manga download, profile/library/history edit, or user app start/stop ran in this batch.

Final head `20269015d1c067db0249697475d5c4a05737e6da`, test merge `2c6e2306ce4bb1d1d4f309ea2dec4a1c029d74f8`. [UI CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36242003395) passed formatting, type-check/build, 212 logic and 182 Chromium tests. [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36242003411) and [Windows desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36242003393) passed source/account/storage/library/download checks, 39 native IPC tests, Clippy, the EXE build and actual isolated WebView startup/restart. Two affected running synthetic UI captures were reviewed against the accepted layout. The first revision's CI exposed an omitted Playwright test registration when the test counts were audited; the second revision ran all four new browser cases successfully. Screenshot review then caught an unstyled field. Both were corrected, and the final revision passed all required checks.

Dev delivery: `Documents/Codex/MangaMonitor-Dev-20260926-2026901/mangamonitor-workbench-preview.exe`, SHA-256 `06b3c2cd3ac7adb011ef268dced0dcba40ec297c99978c835b5e2128c6fc4885`. Artifact digest, CRC, x64 PE and embedded revision passed. Both existing shortcuts point to this version; previous links and executable remain. Exit the current app and reopen Dev to use the new build. Private report and receipts: `Documents/Codex/MangaMonitor-recent-updates-20260926`.

The new view is Discover → 最近更新. It defaults to Pica, supports JM, and reuses loaded-scope inventory/language/date/details/download controls. Source-provided latest order is preserved; it does not guarantee that every new chapter of an old work moves it to the top, and missing source dates remain unknown. No new persistent document is introduced. Dev remains 0.3.4, PR #19 draft/unmerged, production disabled, no installer/formal release. Final evidence-only notes remain local until the next necessary push.

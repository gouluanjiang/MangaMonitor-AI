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

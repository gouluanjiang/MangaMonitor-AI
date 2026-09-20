# Author catalog storage and progress optimization

## Problem and accepted scope

The all-followed-author check can receive thousands of general keyword hits for a single author. The previous implementation retained all of their metadata in a single JSON document, rewrote that complete document on every page, and sent complete records on every progress poll. A shared 100,000-record limit could terminate the run before later authors were attempted. A failed source range could only be restarted by selecting its author, which also checked the other, already completed source.

The user authorized the proposed optimization after diagnosis. This batch changes local catalog storage, progress delivery and the selection of unfinished source ranges. It preserves complete-pagination claims, exact author-credit attribution, prior catalog history, manually triggered checks and manual downloads. It does not change query keywords, introduce background monitoring, or add source request concurrency. More selective source queries and source-level concurrency remain later work requiring evidence.

## Data and read contract

- Preserve the original discovery JSON and import its complete records, query associations, range states and checkpoints. Never infer local ownership from author/title similarity.
- Commit a page as a small revisioned transaction. Check the current following revision and native account/session generations before accepting it. A failed write must not advance the committed manifest or published progress.
- Keep keyword-only results outside the default result payload. They remain inspectable on demand and retain the raw IDs needed for pagination and incremental checkpoints. A work whose author metadata is missing or uncertain is not silently promoted into the author list.
- Separate the 100,000 confirmed-author-record budget from a bounded raw catalog. This is a storage and payload redesign, not an assertion that source pagination is reduced by hiding other results.
- Poll only progress, revision and range metadata while checking. Results remain the last explicitly read snapshot until refresh or terminal completion. A progress update must not claim that the displayed work list has already received newly observed records.
- Keep session/epoch guards, serialized frontend reads, stale-progress labels, separate action/read errors and bounded local BUSY recovery from the previous repair.

## Unfinished checks and transient source failures

The new action chooses individual `(author, source)` ranges whose state is not complete. A complete JM range is left alone when only that author's Pica range needs work. No pending ranges means no network request. Ordinary incremental checks and explicit full rechecks keep their existing semantics.

Retry only a short allowlist of transient request/connection/timeout errors, on the same page with a bounded retry budget. Recheck cancellation and account identity around requests. Authentication, access restriction, rate limiting, invalid responses and pagination inconsistencies are not interpreted as empty results or retried indefinitely.

Cross-run continuation at an old page number is not introduced by this batch: source pages may have shifted. Remaining failed ranges rebuild their own pagination, while successfully completed ranges retain their valid incremental baselines.

## Verification and delivery

Formal tests and builds run in CI only. Required synthetic coverage includes lossless legacy import, raw catalogs larger than the old shared limit, small-transaction write size, revision/following conflicts, damaged committed transaction detection, bounded source retries and cancellation, exact unfinished-source selection, cold-result preservation, metadata-only progress, terminal refresh and stale-session UI rejection. Native IPC must retain main-window/origin boundaries for the new commands.

Real acceptance must preserve the user's library, following list, download history and all committed discovery records. Backups and real-data evidence stay outside Git. Do not replay all followed authors for acceptance, download media or delete authors. Engineering checks, native verification and user acceptance are reported separately. Performance improvement is measured rather than presented as an unverified multiple.

Implementation and verification are in progress. Version remains 0.3.4; no new installer, release, merge or production enablement is included.

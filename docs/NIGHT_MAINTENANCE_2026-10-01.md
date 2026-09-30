# Overnight maintenance follow-up

## Confirmed problems

- A desktop shortcut launched a pre-v2 candidate against an author catalog already saved using the authorized v2 protocol. The old application returned `UNSUPPORTED_SCHEMA`. A compatible candidate loaded the existing catalog and ownership counters; this was not evidence of lost authors.
- Proactive recent-feed tag checks issued detail queries before showing unknown works. The user rejected the waiting and failures and selected passive filtering using already available metadata.
- `query_ordered_unobserved` held the account-state mutex during source network I/O. Cover access and account changes needed the same mutex, creating unnecessary head-of-line waiting.
- The recent-feed reader waited for retained history before requesting the live first page.
- Native JM recent pages included valid raw boundary metadata, but the frontend allowed that field only for search/author/tag queries. The successful backend response was rejected as `INVALID_RESPONSE`. Synthetic recent IPC fixtures had omitted this field and therefore missed the contract mismatch.

## Changes

1. Remove the proactive tag-check pool, gated card, waiting UI and verified-only selection. Keep explicit tag/category filtering and normal cover retry. Unknown works load normally; naturally acquired metadata updates lists without closing open readers.
2. Clone the authenticated source session before the query, release account state during network I/O, then recheck the generation before applying errors or results. A separate per-source operation mutex keeps queries and favorite writes ordered. This does not fan out source requests or bypass source rate limits.
3. Request live recent results and local retained history concurrently. A failed/slow supplementary history does not hide successful live results; either can be shown first.
4. Queue independent in-process document handles on one canonical-root mutex before acquiring the cross-process file lock. Large local catalog transactions previously competed with the application's own history/observation reads against a two-second OS wait. Keep the bounded wait for other processes, atomic commits, CAS revisions and all identity/path checks. Account-cache callbacks and explicit cache cleanup retain one combined two-second budget across both local and OS contention, including nested callback rejection; their existing regressions are preserved.
5. Release both account-state guards before discovery-context local I/O and revalidate the captured leases afterward. A queued metadata read must not hold up covers or account replacement. Retain sanitized history/observation error codes so a future failure can be diagnosed without private error text.
6. Deliver a distinct rc.3 candidate and repoint the development shortcut only after verifying that payload. Confirm the actual launched path and a complete exit/restart.
7. Accept the existing JM boundary contract for recent queries, retaining its source, exact shape, identity and failed-slot checks. Include the native boundary fields in every synthetic JM recent UI response, and exercise live-page merging and malformed-next-page retention through the real frontend adapter.

## Validation requirements

- Synthetic races: a blocked detail does not block a known cover or account replacement; late success and session-expiry errors cannot affect the new session.
- Browser: unknown cards immediately allow covers and actions with zero filtering detail queries; explicit blocked tags remain hidden; a deliberately opened detail updates filtering naturally; background metadata refresh does not interrupt an open reader.
- Recent history: either result may finish first, failure preserves live data, stale-session/disposed readers still reject late results.
- JM recent IPC: valid native boundaries allow first and subsequent pages, overlapping works merge without hiding earlier works, and malformed boundaries still fail while retaining the loaded list. Favorites/details/rankings and Pica cannot use the JM-only exception.
- Local transaction contention: a read through an independently opened handle waits for a longer local commit and sees its final revision; stale CAS is still rejected and external lock timeout remains bounded. A blocked local context read does not block known covers or use a replaced account.
- Existing scrolling anchors, loading-next-page behavior, selection, downloaded identity, file authority and session separation remain covered by existing CI.
- Native: preserved catalog/library/download records, compatible startup, both sources, covers/detail/reader responsiveness and shortcut cold start. No extra manga download, deletion or all-author rescan is needed.

Formal builds and suites run in CI only. Engineering results, native observations and user acceptance are reported separately. Performance observations must distinguish site variability and coarse UI sampling from measured application waits; no fabricated speed percentage.

## Status

The rc.2 source `2158e1a` passed UI/baseline/native CI (the Windows lifecycle job required one retry after a window-inspector process timeout). Native checks restored the existing author catalog, both sessions and passive-filter browsing. Follow-up native history/observation warnings prompted the root-queue/context changes above. Source `8e81cb7` passed UI CI (307 logic and 242 browser tests), baseline CI and desktop CI; the desktop job required one unchanged retry for a five-second synthetic cancellation-gate timeout. Its native checks restored the catalog and Pica live/history/detail browsing without verification placeholders or supplementary-history warnings, but exposed the JM boundary mismatch described above. The additional frontend correction and final shortcut checks require fresh validation.

An isolated release-mode diagnostic over roughly 104,000 saved metadata records measured a 2,126 ms initial read, 15–17 ms small patch commits and a 2,590 ms checkpoint; round-trip content and the original legacy file were unchanged. This is one local metadata measurement, not an end-to-end website timing or a speedup percentage. Debug-helper timings are not release-app performance evidence. No corrupt author catalog or invalid saved recent-work DTOs were found in the isolated checks.

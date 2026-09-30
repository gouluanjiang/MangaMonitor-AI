# Overnight maintenance follow-up

## Confirmed problems

- A desktop shortcut launched a pre-v2 candidate against an author catalog already saved using the authorized v2 protocol. The old application returned `UNSUPPORTED_SCHEMA`. A compatible candidate loaded the existing catalog and ownership counters; this was not evidence of lost authors.
- Proactive recent-feed tag checks issued detail queries before showing unknown works. The user rejected the waiting and failures and selected passive filtering using already available metadata.
- `query_ordered_unobserved` held the account-state mutex during source network I/O. Cover access and account changes needed the same mutex, creating unnecessary head-of-line waiting.
- The recent-feed reader waited for retained history before requesting the live first page.

## Changes

1. Remove the proactive tag-check pool, gated card, waiting UI and verified-only selection. Keep explicit tag/category filtering and normal cover retry. Unknown works load normally; naturally acquired metadata updates lists without closing open readers.
2. Clone the authenticated source session before the query, release account state during network I/O, then recheck the generation before applying errors or results. A separate per-source operation mutex keeps queries and favorite writes ordered. This does not fan out source requests or bypass source rate limits.
3. Request live recent results and local retained history concurrently. A failed/slow supplementary history does not hide successful live results; either can be shown first.
4. Deliver a distinct rc.2 candidate and repoint the development shortcut only after verifying that payload. Confirm the actual launched path and a complete exit/restart.

## Validation requirements

- Synthetic races: a blocked detail does not block a known cover or account replacement; late success and session-expiry errors cannot affect the new session.
- Browser: unknown cards immediately allow covers and actions with zero filtering detail queries; explicit blocked tags remain hidden; a deliberately opened detail updates filtering naturally; background metadata refresh does not interrupt an open reader.
- Recent history: either result may finish first, failure preserves live data, stale-session/disposed readers still reject late results.
- Existing scrolling anchors, loading-next-page behavior, selection, downloaded identity, file authority and session separation remain covered by existing CI.
- Native: preserved catalog/library/download records, compatible startup, both sources, covers/detail/reader responsiveness and shortcut cold start. No extra manga download, deletion or all-author rescan is needed.

Formal builds and suites run in CI only. Engineering results, native observations and user acceptance are reported separately. Performance observations must distinguish site variability and coarse UI sampling from measured application waits; no fabricated speed percentage.

## Status

Implementation prepared; CI and new-candidate native verification pending. The original strict-gating candidate's passing tests do not count as acceptance of this replacement.

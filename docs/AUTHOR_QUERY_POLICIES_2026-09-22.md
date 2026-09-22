# Source-specific author queries and reviewed credits

## Accepted behavior

An imported comparison spelling can be a poor website query, and an explicit source credit may use a different reviewed spelling. Following display names remain stable. Each account/source/author may have up to four exact website queries, sixteen verified individual aliases, and sixteen reviewed complete credit fields. These private policies are maintained outside Git. Inner spaces and punctuation are preserved in requests; no global character conversion, fuzzy identity, title-based attribution or cross-source work matching is introduced.

An alias affects author-field attribution only. A complete cooperation field matches that complete field only; it does not make the collaborators interchangeable. Query words themselves do not establish authorship. Uncertain identities remain unresolved. Default author results, inventory counts and batch selection continue to exclude other keyword hits, which remain available separately.

## Shared execution

- `source_author_policy` resolves a session-scoped policy without returning account keys or credentials. Source author searches, following-row searches, ad-hoc dual-source search and saved author updates use the same policy contract.
- Each approved query traverses and validates its own pagination. Results are unioned by source/work ID. An ID repeated across different queries is expected; repetition inside one traversal still prevents a false completion.
- Multi-query progress identifies the query position and its real page. Combined totals do not pretend that overlapping source totals are a unique bibliography. A failed or cancelled query keeps the whole author/source range incomplete while preserving already-read results.
- Malformed-item diagnostics retain query/page/position. A later overlapping response with missing author metadata cannot erase previously reliable authorship.

## Saved catalog compatibility

Query fingerprints include exact UTF-8 query spelling/order and fixed source query semantics, not author aliases. Default single-query legacy baselines remain compatible. Only changed query ranges require a new complete read; history, existing dates and saved works remain available. Alias-only changes reclassify saved metadata offline and do not invalidate a remote checkpoint.

Multiple query baselines are independent. Historical checkpoints survive failed/cancelled work, while `completedQueries` distinguishes clean queries completed in the current attempt from merely retained history. Only eligible completed subranges may resume incremental checking. Source-item issues cannot establish a checkpoint for their query, and incomplete ranges cannot claim a complete author bibliography.

Discovery caches and atomic page/checkpoint commits compare the policy revision. A policy change during a scan stops stale commits. Snapshot/progress DTOs carry the reviewed credit policy so frontend classification agrees with native filtering.

## Local maintenance

The `author_query_policy` helper previews a bounded private value document and merges explicitly supplied profiles only. Apply requires expected policy/following/discovery revisions and an idle persisted scan, checked under the existing store lock. The application is closed before the final import. Backups and field/hash preservation checks stay in the private acceptance directory. The helper does not mutate following names, library/ownership, manga files or download history.

## Validation and delivery boundary

Targeted synthetic tests cover query spelling, source isolation, credit boundaries, old baselines, scoped invalidation, offline reclassification, overlapping IDs, missing metadata, partial/cancelled multi-query scans, policy revision conflicts and all author UI entries. Formal suites and builds run in CI only. Local formatting, private read-only evidence review and bounded live diagnostics are separate activities.

This is a post-V1 maintenance batch, not a new installer or release. Version remains Dev 0.3.4, PR #19 remains draft/unmerged, and production remains disabled. Real author names, query payloads, source records and account identifiers are excluded from Git. Engineering checks, real-source validation, delivery and user acceptance must be reported separately.

Status: implementation and private evidence review in progress; no final CI or live acceptance claimed yet.

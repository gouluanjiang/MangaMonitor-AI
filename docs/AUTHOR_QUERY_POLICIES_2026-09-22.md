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

Status: implementation, final CI, reviewed policy import, delivery and bounded authenticated native verification are complete at head `33d0b74a8d57fc7aa6838cc1e180d1e2912c55e9` (test merge `86162ddf6fa10e3f97ad1eba8921407560e0209a`). Frontend run `35715828925` passed 179 logic and 153 Chromium tests; desktop `35715829032` and baseline `35715828863` passed, including native IPC, Clippy, executable build and actual Windows WebView startup/restart. The initial new UI fixture lacked its synthetic JM follow; correcting that fixture resolved the sole first-run failure without changing product behavior or weakening assertions.

The final private rules were imported with the reviewed helper after normal app closure, backups and matching revisions. Existing profile files remained byte-identical, with the new policy file verified against the exact reviewed value. Native saved-result reclassification matched the independent offline prediction. Unchanged following/library displays, known ownership, local refresh, page reentry, scoped query invalidation and preservation of an existing malformed-source warning were verified after the user reconnected both accounts. The application remains open and idle; no full-author scan or manga download was started. Bounded authorized JM homepage queries are separate real-source evidence; planned Pica homepage probes were not executed and their query words were not changed. User acceptance and exhaustive live-source coverage are not claimed.

The executable and both Dev shortcuts now use the verified final delivery. Private evidence remains outside Git. A recoverable initial local-following BUSY was observed and remains a separate maintenance concern. Final evidence-only documents are kept local until the next necessary code push; see the active handoff for artifact and preservation references.

## Supplemental acceptance, 2026-09-22

After the initial delivery, the user explicitly requested the remaining targeted checks before discussion of unresolved identities. The unchanged delivered executable completed the sole changed-query rebuild. Only that scope and the two sources of one deliberately selected regression author were refreshed; every other saved scope was preserved. The changed scope completed both approved query traversals and retained its historical records.

The original real wrong-attribution sample was freshly checked on the current executable. Source-author search, following-row search, ad-hoc dual-source search and the saved author-update view displayed the same confirmed titles and credit fields for the selected source. The wrong sample remained only in other keyword results and was visibly unowned. Select-all included eligible unowned author works only; changing to other keyword results cleared selection and removed bulk controls. The comparison records visible title/credit sets, not an unobserved full cross-entry ID capture.

Four small live Pica alias samples covered punctuation, internal-space differences, circle/member credits and explicit collaboration membership, including retained negative keyword hits. A separate live JM sample verified an approved complete collaboration field. Each selected live scope finished its returned pagination. These are authenticated source queries through the application; they do not claim browser-page comparison or a fresh query of every imported Pica policy. The prior full-list offline regression and bounded authorized JM probes remain the broader regression evidence.

Final read-only journal replay verified that all old source IDs and author associations survived, with no saved-range changes outside the deliberate scopes. Policies, follows and unaffected cache documents remained byte-identical. The user independently downloaded one work while window automation was paused: precisely one completed task and one library entry were added, the ZIP exists with the registered size, and every prior library/task record is unchanged. Preserve that explicit exception rather than claiming all profile bytes stayed identical.

The seven agreed acceptance criteria now have completed evidence at their stated offline-plus-targeted-live coverage. This is agent verification, not a new user-experience acceptance or an exhaustive source-bibliography guarantee. Initial local-following BUSY recovered by ordinary retry and a transient progress BUSY recovered automatically; their broader cause remains outside this repair. The existing malformed-source uncertainty remains unchanged. No product code, query policy or follow was changed in this supplement, no agent download was initiated, and no formal suite/build/release was repeated. Private supplemental evidence and the identity discussion dossier remain outside Git.

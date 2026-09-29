# JM search page-boundary compatibility

## Problem and evidence

A bounded read of a previously incomplete JM search found that the last row of one page was returned again as the first row of the next page. Both the source ID and the complete raw JSON record were identical. The reported total stayed unchanged. Reading to the end produced exactly the reported number of unique IDs, while the number of raw returned rows was larger. The old strict cross-page duplicate check stopped at the first overlap.

A separate incomplete search still contains a record without a title. Its current source detail response also lacks usable metadata. The existing item-isolation contract remains correct: valid records are retained, the bad row remains visible as an unresolved issue, and no fabricated work or complete incremental baseline is created. This correction does not invent missing source data.

Real query names, source IDs, response bodies and counts are private evidence outside Git. No manga, account credentials or library contents are used in synthetic tests.

## Bounded compatibility

- Only the JM search adapter emits optional first/last raw-slot evidence. Each successful boundary slot contains a validated work ID and the SHA-256 of its recursively key-sorted raw JSON record. A malformed boundary slot supplies no evidence. Favorites, recent updates, ranking, Pica and redirected single-work lookups do not gain this exception.
- Each independent query traversal retains the immediately previous accepted page's last boundary. One repeated first work may be skipped only when the adjacent boundary IDs, fingerprints and decoded work agree, the source total is known and stable, and the page contains another previously unseen valid work.
- Same-page duplicates, internal/nonadjacent duplicates, changed records, repeated whole pages, unknown totals and contradictory paging remain errors. Missing evidence retains the strict behavior.
- Raw rows still count against resource limits. Effective rows (valid unique works plus unresolved source issues) count against the reported total. An overlap must never make a short catalog look complete. Issue positions retain their original source page and row numbers.
- Native author discovery and foreground search use the same rule. Resume retains the previous accepted boundary; separate aliases and new queries start independent traversals. Incremental prefix checks and saved head IDs use the effective work sequence.

Existing good baselines, source identities, first-discovery markers, ownership, downloads and favorites are preserved. No full-list recheck or migration is required merely because this compatibility rule is available.

## Validation and delivery status

The source adapter, native discovery and foreground search are implemented, with targeted synthetic coverage for exact overlap, raw versus effective counts, conflicts, short totals, resume, alias isolation, incremental baselines and retained issue/first-discovery evidence. Independent code review found no blocking issue. A private replay of the saved complete real response sequence through the edited foreground traversal reconciled the exact unique total and completed without an extra page request. That replay uses a diagnostic projection and does not claim to execute the native decoder.

Formal checks/builds run in CI only. Corrected native acceptance is not yet claimed. The published v1.0.0 assets remain unchanged.

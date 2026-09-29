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

Final source head `01d70a9399b071e9140fae13800778dc5acdba23` is in [draft PR #20](https://github.com/gouluanjiang/MangaMonitor-AI/pull/20), tested at merge `86dfe95142f2a4ca39072f84a14fa70387f199e4`. [Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36535236677) passed 247 logic tests and 213 Chromium cases. [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36535236672) passed 965 Rust tests (one existing ignored profiling test) and 63 Windows executor cases. [Windows CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36535236678) passed 462 offline Rust tests, 49 native tests, 23 license/NSIS cases, Clippy, installed WebView lifecycle and installation/data-preservation checks. All workflows passed their first revision without reruns. Formal checks/builds ran in CI only.

The downloaded CI artifact passed its independently supplied SHA-256, ZIP CRC, exact manifest/file hashes, embedded revision, x64 application PE, license inventory and installer evidence checks. Corrected native acceptance passed for the intended compatibility behavior: the previously failing adjacent-boundary range read every page and established a complete baseline matching the unique source total. The other range read all pages but correctly retained its single missing-metadata issue. This does not claim full source coverage or repair of unavailable website data.

Only the two incomplete scopes were retried. The native summary returned to idle automatically and recorded one completed scope. Its zero-new filter, refresh persistence, restored historical ownership counts and precise bad-row details were observed. Independent read-only comparison confirmed that all old IDs and first-discovery markers, all five critical documents and every other range summary were preserved. Metadata refreshes were confined to the two queried JM ranges. Real names, identifiers and detailed evidence remain private.

The app was verified as `1.0.0 / 01d70a9` in native diagnostics and left idle. Both existing Dev shortcuts were backed up and verified pointing to the complete corrected payload in `Documents/Codex/MangaMonitor-JM-fix-20260929-01d70a9`; the prior installed payload and public v1.0.0 release assets remain unchanged. PR #20 remains draft/unmerged. This is an agent-verified local correction, not user acceptance of a newly published release. Final evidence-only updates stay local until the next necessary code push.

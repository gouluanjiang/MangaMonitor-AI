# Phase 3A completion report

Phase 1A remained unchanged and accepted. Phase 3A used only five confirmed authors and stopped before Phase 3B. No manga or image was downloaded; no local manga file or production state was changed; no updater or deletion capability exists.

## Commits and Actions

- Phase 3A initial state-machine commit: `a125a28bc6375043fc7c23026a43f3c03b7e3100`
- Resume checkpoint commit: `499ba4dd42d804420371c8d8084574dd4bbc59cf`
- First real Linux dry-run: <https://github.com/gouluanjiang/MangaMonitor/actions/runs/33983528241>
- Final replay/Linux-test run: <https://github.com/gouluanjiang/MangaMonitor/actions/runs/33997566400>, tested commit `a1260120138f2ef11bdf08673544a68b4a30db3f`.

## Tests

Local Rust: 79 passed, 0 failed. This includes all pre-existing 49 Phase 1A tests and 30 Phase 3A tests. `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` passes.

Phase 3A tests cover first discovery/event deduplication, pending candidate revision, stale completion rejection, fingerprint reanalysis, source failure, early-stop non-removal, three distinct explicit unavailable checks, full sweep behavior, six-month recovery, decision persistence, ambiguity, UNKNOWN, coverage/deletion boundary, inactive recovery, state round-trip, five-author CLI replay and checkpoint resume.

The final Linux workflow passed all 79 tests, built `phase3a`, replayed the captured five-author input twice, checked incremental early-stop and simulated a source failure. All steps completed successfully.

## Five-author real dry-run

Authors: `10驛`, `2-G`, `3104`, `Bee導師`, `haruhisky`.

Run `33983528241` performed a read-only full scan: 228 API requests in 490,824 ms. JM made 145 requests; Pica made 83 including login. All ten author/source searches reached a complete boundary. It produced 215 unique catalog records: JM 140 and Pica 75. Input state was not modified; image requests were zero.

All 215 records went to conservative human review and none became pending. This is expected for this sample: every selected author already has local works, while source titles or author fields did not provide sufficient deterministic identity evidence. The matcher was not relaxed to reduce review volume.

Second pass used exactly the captured input: 0 source requests, `business_state_unchanged=true`, 0 reanalysis, 0 new events. A separate incremental replay used the scan-start historical-ID snapshot and threshold 5: two multi-page Pica searches stopped at `EARLY_STOP_HEURISTIC`; eight searches whose fetched page was already the reliable last page remained COMPLETE. The incremental run was correctly marked partial. The full run had no early stop. Final run `33997566400` obtained these results without new source requests by reading private run `33983528241`'s sanitized observations.

The source-error replay was partial, recorded `SOURCE_ERROR`, and left all 215 records active with unavailable streak 0. Pica explicit not-found classification requires the observed conjunction HTTP 404 + JSON code 404 + error 1007 + exact `not found`; generic HTTP/auth/network/parse errors remain source errors. Three distinct successful explicit checks are required before inactive. JM did not return equally reliable evidence in the bounded probe, so JM errors cannot currently accumulate unavailable.

## Review export

Reason totals:

| Reason | JM | Pica | Total |
|---|---:|---:|---:|
| `UNCONFIRMED_AUTHOR` | 45 | 69 | 114 |
| `UNRESOLVED_TITLE_IDENTITY` | 95 | 6 | 101 |
| Total | 140 | 75 | 215 |

All 215 entries are exported to JSON and CSV under `evidence/phase3a-run-33983528241/`. CSV contains 216 physical rows including the header. No entry is missing its search query. Each entry preserves enough sanitized context for later batch analysis without treating the query as confirmed author evidence.

## Actual bug fixes made during completion

- Inventory uses `black_white`; version parsing previously only recognized `monochrome`. The matcher now recognizes the real seed value.
- Explicit `未汉化`/`未漢化` was previously susceptible to the positive substring `汉化`/`漢化`. It now resolves to false; contradictory positive and negative evidence remains UNKNOWN.
- Source search query was not retained in catalog/review evidence. It is now recorded only as provenance, never identity proof.
- Reanalysis could re-notify an unchanged active review. Stable active review reasons no longer create duplicate latest events.
- Content type was not checked during title identity matching. Manga, CG set, artbook, novel and setting book now remain separate when explicit metadata identifies the type.
- Direct source errors are tracked as partial scan failures; a later confirmed available detail resets the unavailable streak and restores an inactive candidate.

The review count was not used as a target and remains 215 after these fixes.

## Outputs and state behavior

The program writes eight readable JSON state files plus one atomic `checkpoint.json`. Resume reads the checkpoint, retaining the original historical-ID snapshot and per-author/source page cursor. It also writes `review-export.json`, `review-export.csv`, reason summaries, `sanitized-state-sample.json`, `observations.json`, `scan-report.json` and `state-diff.json`. The input directory is protected from being used as output.

Decisions SAME/NOT_SAME/IGNORE affect reanalysis at the start of a new scan, before network availability matters. Conflicting mappings go to review. Search results never extend the author list. Fingerprint changes trigger detail/reanalysis; unchanged fingerprints skip heavy analysis. `source_error`, heuristic early-stop and unseen records never imply unavailable.

## Deliberately not implemented

- Phase 3B author canonicalization/freeze or all-author production full scan.
- Monthly cron, production state commits, distributed job coordination or a 200-author production batch.
- Automatic download, image download, `漫画更新.exe`, local manga repository writes or deletion.
- Human review UI or automatic processing of these 215 review items.
- AI/LLM runtime matching, semantic translation matching or author aliases.
- Automatic free-text collection coverage, final production title parser, or deletion authorization.
- Reliable JM automatic inactive classification; Pica token lifetime/concurrent-session policy; Pica chapter download-path validation.

Phase 3A ends here pending acceptance.

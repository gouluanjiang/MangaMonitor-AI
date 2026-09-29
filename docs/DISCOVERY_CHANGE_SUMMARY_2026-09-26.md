# Author-check change summaries (A6)

The user accepted the language badges and authorized the next scheduled batch, A6. Add a compact change summary and a temporary newly-discovered view within Author Updates. Keep existing manual check actions, all retained omissions, attribution and ownership rules. Author Search remains separate. No new side navigation, source query, scheduled check or automatic download is introduced.

## Product behavior

- “本次新发现” means a source ID first saved by the latest accepted manual check in the current JM/Pica account pair. It does not mean newly published, newly downloaded, or every result read again. JM and Pica IDs remain separate.
- Existing records form the historical baseline. Their mutable `observedAt` and `scanId` are not first-discovery evidence, and no historical first-seen date is invented. An old raw keyword result later confirmed as an author work is still an already-known source ID.
- A first check on an empty catalog is labeled as first catalog collection. Its found works are newly discovered to this catalog, not claimed to be newly published by the website.
- Each successfully accepted incremental check, complete recheck or unfinished-only check is a new batch. Busy/invalid/no-unfinished requests do not replace the previous summary. Unfinished-only summaries describe only that attempt; previous discoveries remain in the normal retained list.
- The summary shows the attempt's time, type and scope, plus newly discovered results and their current owned/missing/unknown counts. Historical missing results remain visible by default. Counts use the current author/source/text context before the ownership tab, so all ownership breakdowns remain meaningful.
- “仅看本次新发现” is temporary and combines with the existing ownership filter, text filter and date sort. It neither removes old records nor grants batch-download authority. Other keyword results never enter the default summary or new-result batch selection. Changing this view, account or batch clears selection.
- During a check, the lightweight progress response does not claim the retained old catalog is a final zero-new result. The summary is computed after the full result read. Failed, cancelled, interrupted and partially read checks retain saved discoveries and explicitly state that coverage is incomplete; zero found is not a claim that there are no website updates.
- Refresh and restart retain the latest saved summary and its new subset. Starting a new check replaces the summary, while previous discoveries become historical retained results. Current ownership counts update through the existing inventory projection after downloads.

## Data contract

`DiscoveryRecord.firstDiscoveredRunId` is optional, defaults to absent for old records, and is written only for a previously unknown `(source, workId)` within an account pair. Merging metadata preserves the existing value, including an absent legacy value. Multiple queries/authors cannot make a known ID new again.

`DiscoveryAccount.lastCheck` is an optional compact summary. Fields: `id`, `startedAt`, `finishedAt`, `phase`, `mode`, `onlyUnfinished`, `firstCatalog`, `allFollowed`, `authorCount`, `totalScopes`, `attemptedScopes`, `completeScopes`. The summary ID is a 64-character lowercase hexadecimal run ID. `authorCount` is the number of authors actually touched after unfinished-range filtering; `allFollowed` describes the selection before that filtering. Attempted scopes include failed attempts and are distinct from fully checked scopes.

The summary is exposed by full snapshots and progress responses. New fields use default/omit-none compatibility; old documents remain readable without rewriting every record. Newly written metadata is not guaranteed readable by older executables that reject unknown fields; keeping an old EXE does not itself promise backward data compatibility.

Use the existing journal/manifest transaction: persist accepted-check metadata, include the summary with page record changes, and store a normal terminal result under the existing identity/revision guards. Update journal indexing and exact metadata byte accounting. Do not duplicate an ever-growing new-ID array or rewrite the full catalog per poll. Failed page commits are not counted as persisted discoveries.

If cancellation or an identity/following change prevents safe final persistence, keep already committed results and project a saved checking state as interrupted after restart. Never turn a partial/checking catalog into complete or automatically restart it. Existing session, following, policy, generation and storage gates remain authoritative.

## Validation and delivery

Formal suites and builds run only in CI. Added synthetic cases cover legacy documents, first catalog, duplicate/multi-author hits, fresh observations of existing works, source/account isolation, partial and interrupted results, unfinished-only batches, journal replay/checkpoint and summary byte budgets, IPC validation, default historical visibility, subset selection and current ownership updates. UI evidence must show the summary and affected result cards against the accepted layout.

Final head `eabc2e71584a481a46c2de39cb4539eed3935f1f`, test merge `e9bfadc3b858d6c6b3b9d1af503252bf1330fed8`. [UI CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36236907373) passed formatting, type-check/build, 207 logic tests and 178 Chromium tests. [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36236907384) and [Windows desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36236907375) passed, including storage/library/accounts/sources/downloads, 39 native IPC tests, Clippy, the EXE build and actual isolated WebView startup/restart. Nine added Rust regressions cover first discovery, partial/unfinished batches, replay/checkpoint and budget, account/cancel/restart isolation and idempotent late stop. The first pushed revision passed; no local duplicate suite/build ran.

Two running synthetic Author Updates captures (complete and partial) were reviewed against the accepted layout. The summary preserves the established sidebar, controls and grid; the wider page information hierarchy remains for the later overall UI batch. This is synthetic UI/engine validation, not a real author scan or user experience acceptance.

Verified Dev delivery: `Documents/Codex/MangaMonitor-Dev-20260926-eabc2e7/mangamonitor-workbench-preview.exe`, SHA-256 `ca754904592311b2250ca37c86615629fdf66d9f0573d530e5a223b4c352a22b`. Artifact digest, CRC, x64 PE and embedded revision passed. Both existing Dev shortcuts were verified and updated, prior links and executables preserved; no user app was launched or closed. Exit the old process and reopen Dev. Private report, visual captures and receipts are in `Documents/Codex/MangaMonitor-discovery-summary-20260926`. No real account query, manga download, author scan or profile/library mutation ran. The user confirmed single-author acceptance passed; all-followed-author acceptance has not started. Dev is 0.3.4, PR #19 draft/unmerged, production disabled, with no installer or formal release. Final evidence notes stay local until the next necessary push.

After this batch: website recent updates, built-in reader, overall UI/interaction, formal release preparation. Their detailed scopes remain separate.

## User acceptance update

Single-author acceptance passed according to the user. All-followed-author acceptance has not started. This is a partial acceptance update, not evidence of a completed all-author check or authorization to start one.

## Agent multi-run single-author acceptance, 2026-09-26

At the user's request, the delivered native Dev executable was checked with eight distinct followed authors, one at a time, followed by two repeat checks: ten real manual runs, each covering JM and Pica. Nine runs completed both scopes; one retained an already-known malformed-source warning and honestly reported one of two scopes complete. Its retained works remained available. This is an A6 behavior pass within the sampled scope, not a claim that every source response succeeded.

Each run's selected-author scope, persisted independent run ID, times and counts agreed with the native summary. The newly-discovered filter returned zero confirmed author works in these samples, and turning it off restored the retained results. Both repeats preserved known IDs and did not mark them new. One new raw keyword hit was independently verified as not attributable to the queried author and correctly excluded from the author summary. Refresh and page navigation retained the latest summary.

Read-only saved-evidence audits passed for all ten runs: only the selected author's scope summaries changed, old source IDs and first-discovery markers were preserved, and library, downloads, following, author-query policies and phone-library document hashes remained unchanged. Final refresh preserved the tenth run's summary and storage revision. Normal app checks updated only their expected discovery records; no real download, follow edit, ownership edit, code/build/CI rerun or all-author scan was performed.

Coverage limits: these samples yielded no positive count of newly discovered, correctly attributed author works; that positive-result path remains covered by the existing synthetic tests, not by a new real positive sample in this supplement. This supplement did not restart the user's app. All-followed-author acceptance remains not started. Private report and evidence: `Documents/Codex/MangaMonitor-discovery-summary-20260926/real-acceptance/多批次单作者验收报告.md`. Author identities and source records stay outside Git. No new release or acceptance claim for untested scope follows from this supplement.

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

At implementation checkpoint, CI, final visual review and verified Dev delivery are pending. User experience acceptance stays separate. No real author scan, account query, manga download or profile/library mutation is needed for this batch. Dev stays 0.3.4, PR #19 draft/unmerged, production disabled, with no installer or formal release.

After this batch: website recent updates, built-in reader, overall UI/interaction, formal release preparation. Their detailed scopes remain separate.

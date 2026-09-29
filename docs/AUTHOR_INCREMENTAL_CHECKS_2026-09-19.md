# Manual incremental author checks

## User-facing behavior

The author-updates page has a single action for all followed names, defaults to unowned works, and retains previously observed works that the user has not downloaded. A selected author can be checked separately. First-time or untrusted scopes read complete JM and Pica query pagination. Later checks prefer an incremental newest-first prefix. A separate **完整复核** action explicitly reads all pages again. Ordinary ad-hoc author search continues to read complete query results.

The source does not know the PC library. Both modes read source metadata before applying the existing local ownership filter. Owned items can therefore still appear in metadata requests. No scan starts on launch, after closing the app, or on a schedule. No scan starts a download. Existing download selection, verified receipts, reviewed registrations and actual-file ownership rules remain unchanged.

## Checkpoint and fallback

Each verified account-pair/source/author/query-version range records the first 20 raw query IDs (or the whole smaller catalog), its source count and the time of the full scan that established it. Raw query IDs include other keyword hits: author attribution and ownership are never stopping conditions. The pinned queries remain JM `o=mr` and Pica `sort=dd`. These client sort values do not establish a guarantee about backend timestamps or stable ordering of ties.

A later incremental scan can finish early only when a new-ID prefix is followed by the complete consecutive old head in the same order, the rest of the received page contains known IDs, and the stable reported total equals the previous count plus the new prefix. Empty checkpoints, legacy documents, interrupted/failed scopes, changed query versions, unknown totals, reorderings or count discrepancies do not authorize early completion. Normal pagination then continues until a valid end, or records an explicit partial/error result. Actual pagination failures are not silently retried or treated as absence.

Only a successful scope advances its checkpoint. Cancellation or a failed page retains prior committed records and the last successful checkpoint. Historical records are not deleted when a later source query omits them. `lastCompleteAt` remains the last full pagination time; `lastCheckedAt` and `lastCheckMode` describe the later check. Incremental completion does not enable a new full-range/all-owned claim.

This is a head check with retained history. A same-total replacement deep in the old tail, edited old metadata or historical reclassification can escape it. Manual full recheck is the supported way to inspect those changes. The implementation does not guess cross-source identity, language/version equivalence or publication dates from IDs.

## Bounds and preservation

Queries remain sequential and paced; only the metadata needed for the stopping decision is fetched. Record merges use source/ID hash indexes. Saved metadata is bounded at 100,000 records across account scopes and 128 MiB; exceeding capacity leaves the check incomplete and preserves prior committed data. These bounds are not a claim of unlimited catalog capacity or measured full-library performance. Covers use the existing session-only lazy loading and the visible list retains virtualization.

Existing version-1 discovery documents load without an incremental baseline and receive one only after complete pagination. Their old complete flags alone are insufficient. Optional new checkpoint fields are persisted in the existing document; an older executable cannot be assumed to understand a newly written discovery document. Keep the private pre-upgrade backup if rolling back. No other private document migration is required.

## Verification status

Final head `3b936a79fffe57581ae2e17f8a73a010bfb6d7aa`, tested merge `0962ccb4969691561fc1a9c50772fa5a7423cf5a`, passed [frontend CI 35446655889](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35446655889), [desktop CI 35446655877](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35446655877) and [baseline CI 35446655883](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/35446655883). Frontend checks include 152 logic and 117 Chromium tests. Desktop checks include 66 account-module tests, 8 discovery-storage tests, remaining module/credential checks, 34 native IPC tests, Clippy, executable build and Windows WebView startup/restart. Formal suites and builds ran only in GitHub Actions.

Coverage includes first/full scans, incremental request reduction, retained omissions, same-page and cross-page boundaries, drift and unknown totals, old documents, cancellation, partial failures, limits, IPC mode/origin validation, UI modes and unchanged complete ad-hoc search. Two preexisting UI tests were synchronized with their actual scroll-restoration and mock-download creation boundaries; their functional assertions were retained.

The exact-head Dev executable passed bounded real-source acceptance for one existing followed author on both sources. First checks read complete pagination, repeat incremental checks used fewer pages with the same stored record identities and unchanged last-full timestamps, and explicit full rechecks again read every page. Historical records remained available. Restart restored the saved results, default unowned filter and truthful unchecked coverage for the rest of the followed list. Library, following and download documents remained byte-identical; saved discovery remained byte-identical across restart. No all-followed-author scan or media download was performed. These bounded native checks do not claim a full-list performance benchmark or new user acceptance.

The verified executable is in `Documents/Codex/MangaMonitor-Dev-20260919-3b936a7`; SHA-256 is `cb5999f2a3f2ba671c2d4e179069e1787b2cc0abf24040be719d4d267373f9dd`. Both existing Dev shortcuts now target it. Original data, old executables and shortcut backups remain available. Private source counts, snapshots and the Chinese development report are outside Git under `Documents/Codex/MangaMonitor-author-incremental-20260919`. Version stays 0.3.4 with no new installer, release, merge or production enablement. Final evidence-only documentation is retained locally for the next necessary code push rather than triggering another documentation-only build.

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

Implementation and synthetic coverage are prepared for CI: first/full scan, incremental request reduction, retained omissions, same-page and cross-page boundaries, drift and unknown totals, old documents, cancellation, partial failures, limits, IPC mode/origin validation, UI modes and unchanged complete ad-hoc search. Formal suites and builds run only in GitHub Actions. Native delivery and acceptance results will be recorded after exact-head CI; code presence is not live-source acceptance. No all-followed-author real scan is authorized solely to reproduce these checks.

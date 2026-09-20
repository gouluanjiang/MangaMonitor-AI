# Author progress refresh recovery

## Observed problem

During a user-started followed-author scan, the UI kept an old author/page counter and the stop button while displaying a generic incomplete-check error. Read-only inspection confirmed that the backend continued committing later scopes and that the displayed author had already completed on both sources. The page had stopped observing the job rather than stopped the job itself.

`CompletionPanel` preserved its last successful snapshot after a read error but disabled its polling effect whenever the shared error string was nonempty. The same error text was used for failed actions and failed progress reads. Thus one temporary read failure could leave an indefinitely stale checking state. An offline check with the real frontend validators found no persistent metadata/snapshot-format rejection in the inspected saved records; the in-memory run was not part of that offline check.

The original failure code was not retained, so the trigger cannot be conclusively assigned. Shared document-lock contention is plausible: reading following requires the same lock used by whole-document discovery commits, and the native bounded retry waits only 80 ms in total. This repair does not claim to reproduce that original contention or change storage-lock rules, source requests, catalog pagination, or author identity rules.

## Recovery contract

Progress-read failures are separate from action failures and source-range outcomes. Saved results and the stop control remain usable, while the counter is labeled as the last observed progress rather than current progress. A short safe error code is shown for diagnosis; raw error objects, paths and account data are not displayed.

Only exact local store contention (`BUSY`) receives three delayed read retries, at 1.5, 3 and 6 seconds. This also applies to an initial read without a prior snapshot. Exhaustion pauses progress refresh with a manual refresh route, without implying that the backend job was paused or restarted. Session, scope, validation, unavailable-store/worker and other errors do not auto-loop: their codes do not prove temporary contention. A successful read restores current state and ordinary polling while the job is running; a terminal snapshot stops polling. Manual refresh resets the read-retry budget.

Reads are serialized. Epoch and active/scope guards prevent stale results, overlapping polls or retries after navigation/unmount/session changes. Retrying a read never calls the author-start command, performs an automatic download, or creates a new source scan. A failed action cannot permanently disable observation of an already-running job.

## Verification and delivery

Implementation review, fault-injection UI regressions, CI and final native delivery are pending at this checkpoint. Formal suites/builds run only in CI. Native acceptance should read the existing saved results, refresh inventory/progress, verify interruption semantics and restart persistence on the exact delivered EXE, without an all-author scan or media download. Synthetic fault-injection and actual native acceptance must be reported separately.

Previous uncommitted query-guard delivery evidence and the current four private documents were copied and hash-verified outside Git under `Documents/Codex/MangaMonitor-progress-refresh-20260920`. Do not restore an older discovery backup over newer user results. Version remains 0.3.4, with no new installer, release, merge or production enablement planned for this repair.

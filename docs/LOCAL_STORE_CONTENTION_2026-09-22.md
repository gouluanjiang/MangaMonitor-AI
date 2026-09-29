# Local document contention and bounded acceptance

## Authorized scope

The user requested completion of roadmap items 1–3: additional evidenced per-work credits, the remaining targeted native checks, and intermittent local-read BUSY recovery. Reader/UI implementation, full-scan performance work and formal release remain outside this batch. No all-followed scan or manga download is needed.

## Diagnosis and change

The desktop document service retains one `WorkbenchStore`, while account following, policy, cache and discovery operations open independent handles to the same private directory. Each handle has its own in-process mutex. The common OS file lock is therefore the actual exclusion mechanism between these operations. Previously one unsuccessful `try_lock` immediately returned BUSY, even when another ordinary local read was about to finish. Some callers retried briefly, while the following entry did not; this explains a reproducible path to a page requiring manual refresh. The exact owner of every historical native BUSY was not logged and is not asserted.

Storage now waits for that file lock for at most two seconds, using sleeping workers. Only `WouldBlock` is retried; unsafe paths, unavailable files and other lock failures retain their errors. This wait happens before any document read, revision comparison or write. The transaction and source requests are never replayed by this change. Once the lock is acquired, existing path rechecks, revision checks, atomic replacement and automatic unlock remain in force. A persistent holder still yields BUSY rather than an empty/default document or false success. The old whole-operation retries in discovery, inventory and explicit cleanup are removed so they do not multiply this wait or replay compound operations; frontend stale-progress handling remains intact.

## Verification

Add deterministic contention regressions: a first following read waits for an independent cache transaction without requiring a second read; a stale write sees the revision committed while it waited and cannot overwrite it; another private directory remains usable. Retain external-lock BUSY, process-exit release, corrupt-document, path-safety and concurrent-CAS coverage. Native inventory recovery now verifies one invocation waiting for a short lock holder, and persistent BUSY/corruption remain visible. Explicit cleanup's test timeout allows its single storage wait without weakening its required BUSY outcome. Formal suites/builds run in CI only.

Additional per-work corrections use the delivered guarded policy-import mechanism. Private original-book evidence, exact original author arrays and plans stay outside Git; no global alias, follow-list change, source-ID change, ZIP change or download-history change is implied. Validate exact import deltas and preserve unrelated profile documents and query baselines.

After a verified artifact is delivered, native checks cover the known malformed-listing author, retained normal records and pagination/issue accounting, other-keyword viewing and absence of bulk controls, corrected credits, and local page entry/refresh. Record real-source limitations honestly; CI results alone do not close native acceptance. Do not repeat already accepted whole-author or performance checks.

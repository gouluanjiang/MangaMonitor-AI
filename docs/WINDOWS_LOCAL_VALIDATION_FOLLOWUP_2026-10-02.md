# Windows local validation follow-up

Base application: `1cba4b81eaf439de9d00c47a42aeb11027b1e1fa`.
Base handoff: `82e88a70df8477187b732e1331aac55fdc8af580`.

The initial 37-case local run reported 10 passes, 1 failure, 25 blocked
cases and 1 untested natural-update case. Those results belong to that
candidate, not a later build. The user subsequently authorized the metadata
fix, normal-user use of the existing application profile and credentials,
and adding special follows for bounded validation. Formal manga files are
excluded from destructive and fault-injection tests. No release or merge is
authorized by this validation work.

## LOCAL-01: recent cards retain stale detail metadata

Opening a detail obtains a newer title/date, but returning to Recent Updates
previously changed only labels. Its independently retained reader did not
receive the successful detail. The same failure was reproduced against the
earlier `19b50a4` frontend, so it is not a new cloud-change regression.

Successful explicit detail reads now notify the recent reader for the exact
source and session. Only identities already in that reader are enriched;
this does not fetch details, append works, change pagination or resort cards.
The compact enrichment shares objects with the display projection and is
removed when its identity leaves the reader. Repeated identical details do
not publish again. Other accounts/sources and disposed readers ignore it.

History and sparse pagination cannot undo an opened detail. A subsequent
successful explicit head refresh may replace its returned identities, while
inheriting known tags/dates omitted by lightweight records. A refresh already
in flight when the detail arrived cannot roll it back. Failure preserves the
last valid metadata. Single and combined source views share the same reader.

Targeted regressions cover this propagation, source/session isolation, late
responses, disposal/unsubscribe, no extra requests, failed refresh, combined
ordering and browser return-position stability. Formal tests/builds run in
the existing CI pipelines. Local retests must record the delivered revision,
manifest and executable hash; pending results must not be reported as passed.

## Windows validation evidence

The private local progress/report records the exact candidate and test times.
Only synthetic fixtures and sanitized diagnostics belong in the repository.
Windows-specific fault helpers must be test-only binaries built by CI;
they must not add a production bypass for source URLs, authentication,
recycling, file identity or download approval. No dependency installation,
ACL modification, permanent-delete fallback or disk-filling test is allowed.

Adding a special follow and establishing its baseline tests that workflow.
An actual future source update is still required to verify natural-update
notifications; rewriting a baseline does not satisfy that requirement.

## LOCAL-02: refused recycle invalidates the displayed inventory

Windows validation of `1613486` opened a generated ZIP in a small reader, then
requested its recycle action. The backend correctly refused the occupied file;
file bytes, registrations, history and reading progress did not change. The
controller nevertheless treated this action error as a failed inventory read,
so the card said pending verification while its filter still counted it as
available. The user authorized the narrow follow-up on 2026-10-03.

Only explicitly pre-operation busy/unsupported rejections are kept local to the
file action. They do not clear or replace a pre-existing inventory failure, and
they do not transiently clear it while the operation is running. Cancellation
preserves the old snapshot and failure. Successful verified responses replace
the snapshot as before. Unknown errors, changed files, malformed responses and
uncertain recycle outcomes continue to require an inventory recheck. Actual
inventory errors project consistent pending-verification counts and filtering.
The existing backend file identity, download/reader exclusion, confirmation,
recycle-only operation and partial-result handling are unchanged.

Controller regressions cover repeated calls, refusal, preserved read failure,
cancellation, successful retry, uncertain results and late disposal. Browser
regressions check the card, detail and filter counts after refusal, successful
retry feedback, and actual scan-error filtering/recovery. No real files or
credentials are involved in these regressions; formal tests and Windows builds
run through the existing CI.

## LOCAL-02 delivery and exact-candidate retest (2026-10-03)

Application/test candidate: `068b66b9ee0a9684472318af10bb60b08384e443`.
The repair is in `5f021ae`; the next commit corrects a test-only assumption to
use the existing right-click detail action. Later documentation commits do not
replace this candidate. Draft [PR #27](https://github.com/gouluanjiang/MangaMonitor-AI/pull/27)
remains stacked on the cloud handoff branch and unmerged.

- [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37099576526): passed.
- [Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37099576517):
  362 logic and 269 browser tests passed on attempt 2, including the new refusal,
  cancellation, actual inventory failure, filter and recovery checks.
- [Windows desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37099576518):
  passed, including native storage/IPC, credential and window boundaries,
  Clippy, WebView startup/restart and isolated installer/data-retention checks.
- [Windows candidate artifact](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37099576518/artifacts/11265574475):
  archive SHA-256 `778406c10ac103157f654867159340a2ea2734c2d8953a6c93db885d8dbcdcdc`;
  manifest SHA-256 `57464359575df5add8754521e41541cb4e94d26dd915f06eb490400824d01325`;
  delivered EXE SHA-256 `4b650ec7990074bfc39cea25d271aab53acaab778ed55c6e91ffc4117efd4ea7`.

The local run started at 13:46:32 UTC+08; the last targeted UI observation was
14:03:29. The exact source/checkout SHA, manifest, all eight payload hashes,
documented installer patch and running-process identity were verified. Testing
used the authorized existing profile with separate WebView cache, not a wholly
isolated account profile. Only a generated three-page ZIP was opened for the
fault case. Two recycle requests while its small reader was open were safely
refused; card, detail and available-filter state stayed consistent. Closing the
reader permitted the normal confirmation; selecting Cancel preserved the file
and cleared the previous operation message. No deletion was confirmed.

Seven synthetic file hashes, all library registrations, downloads and ordinary
following were preserved. History and reading changes affected only the accessed
synthetic record. Existing special-follow startup checks refreshed observation
fields in their already authorized scope; catalog identities and work metadata
were preserved. The formal installed executable and shortcut were unchanged.
Private local evidence includes `LOCAL-02-result.json` and the revision-qualified
native/protected-data snapshots. No private title list or credentials are in Git.

The first frontend attempt passed the new LOCAL-02 cases but the existing
recent-feed cold-reload test observed scrollTop 15 instead of 0. Source/section
return anchors passed. The preceding candidate passed this test, and the only
intervening change was in the LOCAL-02 test. An unchanged-SHA retry passed all
269 browser cases. This independent LOCAL-03 observation remains unresolved;
its trace was saved and no assertion was weakened or unrelated source repaired.

The 37-case cumulative matrix remains 28 passed / 8 incompletely covered /
1 natural-update experience untested, with actual revisions/layers retained.
The remaining gaps need controlled native transport/fault/performance fixtures;
current account permission is not the blocker. Native injection of a genuine
inventory read failure was not repeated on the formal profile; synthetic CI
covers that changed behavior. No performance improvement is claimed by this
correctness fix. User acceptance, formal installation, merge and release remain
separate from the targeted fix validation.

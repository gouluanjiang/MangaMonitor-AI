# Author workspace and local reading experience

## Baseline and authority

Start from delivered recent-feed correction `2f39585` on an independent successor branch. PR #23 remains unchanged and unmerged. Its engineering checks and candidate integrity passed; user acceptance and real-profile cold restart were still pending at handoff. The working tree was clean and no other agent was editing it. Preserve the stable recent-feed merge and virtual-grid stride regressions.

The user authorized six improvements in five staged commits: (1) search latency and independent author tabs, (2) special follows, (3) bounded automatic cover retries, (4) last-100 viewing history, (5) successful-scan additions. Do not merge or publish. Existing tag-only filtering, source pagination/attribution, download authority and disabled cloud monitoring remain unchanged. No full author scan, manga download or deletion is validation for this batch.

The author identity question was clarified on 2026-10-01: the user wants the same complete displayed author name from JM and Pica to reuse a search tab. This is a query workspace, not an assertion that two same-named people are identical. Preserve both source scopes and existing reviewed attribution policies; do not create aliases or ownership links. Different authenticated account scopes must not share results. Tabs and selection disappear on application restart.

## Stages

1. Instrument cached-directory reads, scheduling, native query waiting/source operations, first visible records, rendered batches and terminal completion. Retain existing records on refresh/failure; display batches while paging. Run the two different sources concurrently with one in-flight query per source, prioritizing the visible tab and at most one background request. Keep inactive grids unmounted. Reopening/closing tabs must not invalidate another tab or revive a closed one.
2. Reuse the existing manual discovery service and run identity for special follows. Establish a full successful baseline before announcing new unread works. Cold-start checks occur only in the main application process, once per launch, after account restoration and behind interactive work. Store source/account-scoped baselines and unread IDs separately from normal follows and scan additions. Unfollowing a special item has to preserve the ordinary follow unless the user explicitly removes it.
3. Retry transient cover failures at most three additional attempts, retain attempt state across card mounts, deduplicate by authenticated source/work and obey global cover concurrency. Authentication and permanent resource failures do not auto-loop. Manual retry is an explicit new attempt cycle. Respect a supported server retry deadline.
4. Persist only the last 100 deliberately opened work references and minimal labels, recording detail/reader opening, not previews or background fetches. Clearing/disabling history never changes files, downloads, follows or reader progress.
5. Keep last successful scan additions separate from a pending/partial/cancelled scan and from unread special works. Use stable `(source, work ID)` differences, deduplicate multi-author records, and expose partial coverage explicitly.

## Persistence and compatibility plan

Prefer independent, versioned documents for new special-follow/history state and the successful-scan receipt. Existing library, following, discovery, downloads, credentials and reader-progress documents remain authoritative. New documents use the same per-root lock, bounded deserialization, revision/CAS and atomic replacement facilities. Missing new documents mean empty defaults; unsupported versions produce explicit errors, never silent resets. Older candidates ignore these additional files. Do not rewrite existing author/library documents just to initialize these features.

Before any real-profile candidate switch, save a private byte-for-byte registration backup and hash receipt, record the executable/manifest source revision, and verify protected records after bounded native checks. No migration of existing records is currently planned. If implementation requires changing an existing on-disk schema or resolving an ambiguous identity, stop that dependent change and obtain the user's decision first.

## Verification and delivery

Run targeted synthetic state/concurrency/browser regressions and Windows builds in existing CI only. Record performance baseline and changed timings using the same fixture; label synthetic and real-network measurements separately. Wall-clock completion and overlapping component sums are different metrics. Never describe source-operation timing (including parsing/retries) as pure wire latency. Native verification uses a manifest-verified artifact for the exact tested commit, not just matching version text.

For each stage report code changes, CI/candidate evidence, measured timings or explicit unmeasured scope, failure/cancel/restart coverage, and user acceptance steps. Keep account/catalog metadata, benchmark author names and real app backups outside Git. A passing CI or draft PR is not user acceptance or a formal release.

## Stage 2 implementation notes

`special-follows.json` is a new private version-1 document. Existing following,
author catalogs, ownership, downloads, sessions and reader positions keep their
formats. Enabling a special author writes only this document and then asks the
existing discovery executor for the required author/source pairs. An active
manual scan is joined; cancelling the waiting special check never cancels that
manual scan. Repeat clicks share the active special run. A process-owned
cold-start gate survives main-window focus/reloads and reader-window creation.

Baselines are separate per source and query fingerprint. Initial partial reads
remain pending; completing the initial baseline does not announce its old works.
Subsequent observations deduplicate by source/work ID and retain unread entries
through errors and cancellation. Query changes establish a fresh baseline without
erasing earlier unread works. Disabling a special author does not write ordinary
following. Special state is scoped to the existing authenticated account pair.

The main-window-only commands validate sessions and use the shared storage lock,
following/policy revision checks and atomic writes. Reader windows cannot start
checks or mutate special state directly. Deliberate main detail/reader navigation
marks a known unread work; cover loading and ordinary listing never do. The
sidebar count includes visible unread work identities rather than author count.
Initial baseline omissions and errors remain explicit in the special column.

Stage 2 requires its own CI and exact-candidate verification; these notes do not
declare delivery. Synthetic cases cover pending baseline completion, changed
queries, coauthor deduplication, retained read state, restart persistence, joining
manual checks, repeated starts, cancellation and native window boundaries.

## Stage 3 implementation notes

Remote cover retries share the existing thumbnail cache and four-slot network
scheduler. Only identified transient transport, rate-limit and server failures
retry, with at most three additional attempts and increasing jittered waits.
Waiting consumes no network slot. Offscreen work stops waiting, and returning
to the card resumes the remaining budget rather than creating a new cycle.
Authentication, rejected, missing or undecodable resources require manual action.
Failure state survives card mounts for the authenticated session; successful
compressed covers remain memory-only. Explicit retries refresh metadata once.

The credential-free cover transport forwards a validated Retry-After delay,
including HTTP dates, without forwarding headers or credentials. This deadline
also prevents immediate mirror failover and explicit retries from bypassing a
server wait. Synthetic tests exercise shared consumers, exhausted remounts,
offscreen cancellation, queue availability, scoped manual retries and deadlines.
Local ZIP decode/missing-file failures remain explicit manual retries, since
they are not transient network failures. CI and exact-candidate acceptance remain
required; no source scans or real media downloads are part of these checks.

## Stage 4 implementation notes

`viewing-history.json` is a separate version-1, atomic, bounded document. It holds
at most 100 stable work references, titles and visit times plus its recording
switch. No tokens, arbitrary paths, images or reading positions are stored.
Repeated visits move the same source/work or local root/entry to the front.
Disabling recording preserves existing history; clearing changes only history.
Unknown versions and corrupt files are reported, never silently reset.

Source details and local details notify deliberate navigation; source queries,
covers and hovering do not. Reader visits are recorded by the native successful
open path for both embedded and small-window readers. Failed/cancelled opens do
not mark special updates read or add history. Reader windows cannot call history
clear, enable, record or read commands themselves. Missing local history targets
report an unavailable file instead of guessing by title or selecting a replacement.

The sidebar history view has recording and clear controls and reuses the current
detail/reader entry paths. Tests cover bounds, duplicates, concurrency, restart,
disable/clear isolation, deliberate UI visits and reader command authorization.
CI and exact-candidate checks remain required for this stage.

## Stage 5 implementation notes

`scan-additions.json` stores the last completely successful manual scan summary
per authenticated account pair. It is independent of the special-follow unread
document. Existing catalog records already carry their stable first-discovery
run ID; this stage does not infer dates or migrate those records. The receipt is
saved after the successful catalog commit with the same cancellation/account
ordering. A receipt-read race against an older catalog retries the read rather
than reporting newer markers with older works.

Pending, partial, failed or cancelled scans preserve the previous successful
markers. Their current progress and incomplete counts remain visible separately.
If no successful receipt exists, a terminal partial scan may show explicitly
incomplete new-work previews. Automatic special-follow subset checks do not
replace manual scan markers. Cards and the quick filter use the receipt's stable
source/work identities; multiple author associations count the same source work
once. A later successful manual scan replaces the previous receipt. Native and
browser regressions cover failure/cancel retention, restart, metadata-only
changes, multi-author counts and independence from special-follow updates.

## Engineering checkpoint

- `3cbe278` corrected the audit example's missing query timing field and passed
  Windows desktop CI `36821829795`.
- Special-follow native changes passed Windows CI `36824949432`; cover retry
  native changes passed `36826674242`.
- Combined UI CI `36829513681` passed 249 browser cases, including the corrected
  tab-scroll and special-unread fixtures and the new successful-scan retention
  cases. Its remaining history-switch failure exposed delayed controlled-input
  feedback; the switch now updates immediately, displays saving and restores the
  saved state on failure, with a dedicated delayed-failure regression.
- Windows CI `36829513780` passed storage/account checks but caught a production
  reader event using a test-only JSON dependency. The event now uses a typed
  serializable payload without adding dependencies. These final corrections
  require fresh combined UI/native CI and exact-candidate verification.
- No real scans, downloads, deletions, merge or release have run.

The next combined UI run (`36831187347`) passed 250 cases including history,
but exposed a pre-existing virtual-grid anchor boundary: when the viewport starts
inside one row, pinning that clipped row moves the next fully visible row by the
stride increase (48 px in the trace). The grid now prefers the first complete
visible row when it fits, falling back to the clipped row for tall cards. The
existing growth regression now deliberately starts inside a row rather than
depending on a font-sensitive fixed scroll offset. Its position tolerance stays
unchanged. This narrow follow-up preserves the accepted merge/order behavior.

## Controlled search latency comparison

Compared existing CI-built browser artifacts for `2f39585` and `b85f450`, using
three alternating fresh Edge contexts on the same computer. This is a separate
performance diagnostic, not a duplicate execution of the formal test suite.
The synthetic fixture has two sources, two pages of 20 works each, 80 ms local
catalog reads, 30 ms request queue delay, 350/550 ms source operations and 15 ms
local commits. All external network access is blocked. A DOM/paint observer
records when the first card intersects the viewport and when completion appears,
instead of counting test-runner polling overhead.

| Visible wall-clock measurement | Before, median | After, median |
| --- | ---: | ---: |
| First visible result | 3,057.5 ms | 185.2 ms |
| Complete results/status visible | 3,057.5 ms | 1,481.7 ms |
| Source page requests | 4 | 4 |

All three trials completed the same range without browser exceptions. This
demonstrates earlier cached-result publication and overlapping the two sources;
it is not a measured website speedup or a promise about real network latency.
The first visible result uses a saved directory, rather than waiting for a new
network result. With no saved directory, first visibility still requires the
first source page. This remains the evidence for the initial scheduling change;
the later native catalog optimization is measured separately below.

The instrumented first trial reports 333 ms cumulative local catalog work,
34 ms query-rule reads, 120 ms queued requests, 1,800 ms source operations,
60 ms local registration and 42 ms interface updates. These are overlapping
operation totals, not sequential wall-clock components. Source operations include
network/protocol parsing/retries; they are not raw wire latency. The internal
terminal event was 1,396 ms; its status painted at 1,481.7 ms. The old artifact
does not expose native timing, so a numeric before/after comparison of each
native subcomponent is not claimed.

## Native follow-up: large catalog reads

The exact `e49c1ed` candidate passed all three workflows: baseline `36832880185`,
UI `36832880193` (323 logic and 251 browser cases), and Windows `36832880288`.
Its eight manifested files were hash/size verified before launch. A private
registration backup was verified while the application was closed. Bounded
native checks confirmed restored sessions, two independent completed author
tabs, position retention through page changes, deliberate history recording and
an empty special-follow view. Protected library, download, following, author
policy and reader documents remained byte-identical. No manga was downloaded or
deleted and no full author scan was started.

This native observation exposed a different bottleneck from the small synthetic
fixture: repeated parsing, validation and index encoding of the saved catalog.
For one dual-source query, saved records became available at 13,897 ms and the
query completed at 58,023 ms, while cumulative source operations were 2,169 ms.
Catalog reads totalled 62,912 ms and local commits 39,780 ms across overlapping
tasks; these totals must not be added to infer elapsed time. The old 13,999 ms
UI metric measured mounted records, not viewport intersection, and is explicitly
excluded from true first-visible comparisons.

The follow-up shares one immutable, validated checkpoint parse/index per open
storage root. Reuse still reads and hashes the legacy base, checkpoint and every
referenced journal patch, checking revisions and applying subsequent writes.
Same-length corruption and writes through another store handle have regression
coverage. There is no disk format change or migration. Reading one author's
known works now merges saved observations directly without first replaying the
whole observation pool into the author catalog. Author-update reconciliation,
source attribution and paging completeness retain their existing contracts.

The visible-result metric now requires a card to intersect the scroll viewport;
received records, render work and completion remain separate. A low-height
viewport regression prevents mounted-but-offscreen cards from counting as seen.
The exact final application `4b18417caf7250f8bedf23b266bfbece1f8c9ec3` passed
baseline `36839431821`, UI `36839431796` (323 logic / 251 browser cases), and
Windows `36839431921`, including installed WebView and restart tests. The eight
manifested candidate files were verified again before native launch.

The first dual-source query after cold startup, using the same author and real
profile as the earlier observation, returned the same 70 attributed records and
unchanged ownership counts. Saved records were available at 6,182 ms and the
complete query ended at 20,572 ms. Cumulative catalog reads were 20,416 ms,
query-rule reads 231 ms, queueing 4 ms, source operations 2,496 ms, local commits
13,736 ms, rendering 174 ms and other transport 49 ms, with no failed requests or
missing native timing. These are sequential live observations rather than a
controlled network benchmark. The original mounted-record metric and the new
viewport metric are not equivalent, so this native comparison uses record
availability and completion only. The new visible metric remained unset while
results were below the viewport and recorded when the examiner scrolled to them;
that value includes deliberate inspection delay, not just software latency.

Cold native startup restored both accounts and the deliberate history entry;
runtime author tabs correctly started empty. Search counts, page-switch scroll
restoration and history's exclusion of background queries passed. Protected
library, download, following, author-policy and reading-progress hashes still
matched the initial backup. The candidate is open for user acceptance; the
formal installation and shortcuts are unchanged. No merge or release occurred.

Natural future special-follow updates, a real website cover outage, user-operated
history clearing/disable and a later successful manual author scan remain live
acceptance boundaries. Their state/failure/concurrency paths are covered by the
synthetic suites; this batch did not trigger a full author scan, deliberately
break authentication/network access, or download/delete manga to manufacture
those cases. Stages 2–5 add behaviors without a comparable old performance metric;
no separate numeric speedup is claimed for them.

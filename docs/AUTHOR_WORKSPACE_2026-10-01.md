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
- UI CI `36826674241` passed 246 browser cases with two newly registered cases
  exposing test setup mistakes: a top control click after restored navigation,
  and a one-record fixture expecting two unread records. `7aaac6e` corrects them;
  revalidation is pending. Its new history compile check also identified a
  `null`/`undefined` reset mismatch, corrected in the following revision.
- Stages 4/5 and the final combined candidate still require CI and native
  acceptance. No real scans, downloads, deletions, merge or release have run.

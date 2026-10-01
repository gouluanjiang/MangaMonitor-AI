# Current development handoff

## Download tag whitespace compatibility (2026-10-01)

After the diagnostic-only candidate a0e868a was installed on the user's
explicit request, the user approved a generic fix for tag-control rejection.
New download preparation normalizes whitespace controls in tags to word
separators, checks the original character bounds before trimming, and keeps
non-whitespace controls rejected. Strict stored-document validation, existing
record bytes and task bindings remain unchanged; no schema migration is needed.
Both single and batch preparation use the same service boundary. Tests use
synthetic metadata and isolated ledgers only, without source requests or media
execution. Fresh CI/candidate evidence is required before user acceptance.
The details are in [the metadata contract](DOWNLOAD_METADATA_DIAGNOSTICS_2026-10-01.md).
Keep draft PR #24 unmerged and do not perform a real manga download for verification.

## Synthetic download metadata diagnostics (2026-10-01)

The user approved generic validation, error classification and diagnostic work
with synthetic data only. See [the scoped change](DOWNLOAD_METADATA_DIAGNOSTICS_2026-10-01.md).
The implementation preserves the existing admission rules and all persisted
formats, adds redacted field-specific preparation errors, and retains at most
20 preparation diagnostics for the current process. It does not normalize
rejected metadata, request real source data or recover a particular work.
The exact checked revision and current CI evidence for this follow-up are
tracked in [draft PR #24](https://github.com/gouluanjiang/MangaMonitor-AI/pull/24).
Browser discovery explicitly includes the diagnostic-panel regressions. Keep
the draft unmerged; synthetic verification does not establish live acceptance
or authorize installing against the real profile.

## Active author workspace batch (2026-10-01)

The user approved six improvements in five sequential stages; see
[the implementation contract](AUTHOR_WORKSPACE_2026-10-01.md). Work is on
`codex/author-workspace-improvements`, based on the already CI-verified PR #23
head `2f39585`. Do not replace the outstanding recent-list repair or merge/release
this batch. Cross-source identical full author names reuse a runtime search tab
by explicit user choice; this is not author identity or ownership consolidation.

All five implementation stages are now present as separate commits: retained
author tabs/progressive search/timing, special baselines and unread updates,
bounded cover retries, deliberate viewing history, and successful manual scan
markers. See the batch contract's engineering checkpoint for exact CI evidence.
Early native stages passed Windows CI. The controlled, already-built-artifact
comparison measured median first visible results of 3,057.5 → 185.2 ms and
complete visible results of 3,057.5 → 1,481.7 ms with the same four synthetic
source requests; this is not a real-website measurement. Combined revision
`e49c1ed` passed baseline, 323 logic / 251 browser cases and Windows checks,
including the corrected clipped-row anchor boundary. Exact-candidate native
checks preserved sessions, library state, independent tabs, history semantics,
and recent-feed positions; protected data hashes remained unchanged.

Native timings exposed repeated large-catalog parsing as the remaining search
bottleneck. The follow-up shares a verified immutable checkpoint parse per root,
still checking disk bytes and revisions on every reuse, and avoids whole-pool
replay before returning an author's known observations. It also measures actual
card viewport intersection, including virtual row replacement. No stored format
or author-attribution rule changes. Final application revision `4b18417` passed
baseline `36839431821`, UI `36839431796` (323 logic / 251 browser cases), and
Windows `36839431921`. Its hash-verified candidate passed bounded native cold
startup/history/session/position checks. The same real-profile author query
improved from 13,897 to 6,182 ms for first records and from 58,023 to 20,572 ms
for completion, retaining result and ownership counts; live network conditions
were not controlled. Protected records stayed byte-identical. Formal install
and shortcuts remain unchanged. Draft PR #24 is unmerged; user acceptance and
public release are separate. Future special updates, natural cover outages and
later manual scan markers have synthetic evidence and still need real usage.

Updated 2026-10-01. This is the continuation entry point. Read [project status](PROJECT_STATUS.md) for current product scope and [the documentation index](README.md) for supporting contracts. Historical plans do not restore cancelled features or grant execution authority.

## Current maintenance batch

The user authorized the [independently reviewed maintenance fixes](MAINTENANCE_AUDIT_2026-09-30.md) and subsequently rejected the latency/failures of proactive recent-feed label verification. The accepted replacement uses existing explicit tags only: unknown works display and remain operable immediately, no filtering detail requests or verification placeholders, natural metadata updates filter lists without interrupting an open reader. JM female-category filtering remains. Version `1.0.2-rc.2` carries this correction; it is a candidate, not a release declaration.

Library, download and author-catalog format changes were explicitly approved: old data migrates in memory only on read; ordinary CAS/atomic writes use version 2; old programs reject unsupported formats. The user later removed the source/isolated-validation-only restriction and authorized overnight fixes, native verification and routine useful optimizations. Do not launch an older app against newly saved data. Live inspection confirmed an outdated shortcut caused `UNSUPPORTED_SCHEMA`; a compatible candidate reads the existing catalog correctly. Preserve backed-up records and verify the final shortcut target and cold restart. Application revision `d50e91204f4c38827d8427600aec5950633aaff8` passed its engineering checks, but its strict label-gating experience was rejected. New rc.2 changes require fresh CI and native evidence. No public release, manga deletion or unrequested download is part of the overnight work.

Overnight scope and validation are tracked in [the performance follow-up](NIGHT_MAINTENANCE_2026-10-01.md). The follow-up `1.0.2-rc.3` adds per-root in-process storage queuing and releases account guards during local context reads after rc.2 native browsing exposed intermittent supplement/history warnings. Source `d12056c` passed UI CI (308 logic and 242 browser tests), baseline and desktop CI. Native JM/Pica/catalog checks, manifest verification, protected-record comparison and actual desktop-shortcut cold startup passed; both Dev launchers select the compatible candidate. The overnight heartbeat is paused. This is agent verification, not user acceptance or public release.

The user then reported recent-feed flicker and authorized Computer Use diagnosis and repair. Native observation reproduced cards alternating between two positions with no scroll input. The uniform virtual grid repeatedly resampled the first mounted row as its global stride; unequal row heights could change which row was measured and cause a feedback loop. The current correction observes all mounted rows, keeps a stable maximum stride for the current width and preserves the visible anchor when measurements grow. A real resize may shrink/recompute the stride. New deep-history stability/growth/resize regressions and the shared-grid suites require CI and native verification. Do not start a full source scan for this layout fix. The user explicitly requested no further quota checks during this follow-up.

Flicker correction `b3262f1` passed baseline/desktop/UI CI (308 logic and 244 browser tests), candidate integrity, protected-record comparison, bounded native JM/Pica browsing and shortcut cold-start checks. User acceptance remains separate. A subsequent read-only investigation reproduced another issue: merging later live pages ahead of retained history moved a saved work to an earlier location without any filtering change. The user authorized its repair on 2026-10-01, keeping AI filtering unchanged. The reader now owns the displayed order separately from pagination and metadata. Automatic pages/history enrich records in place and append new identities; a successful explicit refresh may rebuild the known source order. Failures preserve the previous list, and late history must not cause a second reorder. The affected synthetic regressions and candidate delivery require fresh verification; do not repeat the full author campaign or claim the user's unrecorded disappearance was conclusively caused by this reproduced movement.

The earlier author coverage campaign has completed its private checks: all 28 known works, both sources for the 826 followed authors, continuous recent-feed supplementation and residual evidence review. This records that bounded campaign, not a guarantee about every work on either website. It does not require another full scan for unrelated maintenance.

## Delivered version

The agreed local V1 is complete and [1.0.1 is formally released](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.1). User experience acceptance, project-review corrections, release preparation, agent-operated all-followed-author acceptance, the bounded JM pagination correction, publication and local upgrade are complete. User acceptance and agent verification remain separate evidence.

- Reviewed application source: `6446e1ebdaac7f3275ef8492cbe3c36aa2fedb04`.
- PR test-merge revision: `dbe5c140ffdb729e87e4bb4dfa795d76db32225f`.
- Actual [PR #20](https://github.com/gouluanjiang/MangaMonitor-AI/pull/20) merge and `v1.0.1` tag: `641d2edb6f9e6e362259ebe48da7e4cade346a19`.
- Final PR and automatically triggered main CI passed. The reviewed artifact, four public assets and installed resources were verified. Version `1.0.1 / 6446e1e`, normal startup, complete exit/restart, session restoration and preserved library/download/author records passed native checks.
- Original v1.0.0 release assets remain intact. Later documentation commits do not replace the application revision or the reviewed release payload.

See the [1.0.1 release record](RELEASE_1.0.1_2026-09-29.md) for CI links, hashes, publication and installation evidence. The [1.0.0 release record](RELEASE_1.0.0_2026-09-29.md) and [JM boundary contract](JM_SEARCH_BOUNDARY_2026-09-29.md) retain the earlier all-author and correction boundaries.

## Known boundary and future work

2026-09-29 acceptance follow-up: the user accepted most of the browsing/download candidate and requested five targeted corrections: an opaque selection dock outside the scrolling canvas, full context-menu titles, Pica's explicit BL category variants, explicit AI-label filtering, and direct failed-cover retries including stale metadata. See [the batch contract](BROWSING_DOWNLOAD_EXPERIENCE_2026-09-29.md). These corrections require their own CI and candidate acceptance. General typography/button/UI redesign is deferred for discussion after this batch; do not expand this fix into that redesign.

One source record still lacks usable metadata. Its scope remains explicitly partial while valid results remain available; this is not an unfinished product feature or a promise to recover missing website data. Source availability, unencountered failures and future website changes are not guaranteed by the completed acceptance.

No agreed V1 development item remains open. Continue with actual maintenance issues or newly approved requirements. The in-app updater is for later discussion; cloud monitoring stays disabled and `production_enabled=false` is unchanged. Do not restart cancelled matching, phone-list, classification-booklist or other historical proposals. The reader and integrated UI have already shipped.

## Implemented experience batch awaiting acceptance

The user approved the [browsing and download experience plan](BROWSING_DOWNLOAD_EXPERIENCE_2026-09-29.md). Implementation is on `codex/browsing-download-experience` from `01274ab1`; [draft PR #23](https://github.com/gouluanjiang/MangaMonitor-AI/pull/23) records the current CI result and candidate revision. User acceptance remains pending. All manga browsing views preserve position for this run, including details and downloaded lists. This is a new maintenance batch, not part of the already accepted 1.0.1 payload. Do not merge, publish or replace the installed formal payload merely because the synthetic checks pass.

For this batch, the user explicitly removed the second confirmation for both single and batch downloads. Clicking download authorizes those selected works; source/session/root/revision checks, no-overwrite behavior, and download history remain. This current instruction supersedes historical requirements to show another download confirmation. No release or replacement of installed 1.0.1 has occurred.

## Continuing development

The user additionally authorized the [author catalog coverage repair](AUTHOR_CATALOG_COVERAGE_2026-09-29.md), followed by individual known-gap retests, saved-evidence cross-checks, both-source full checks of the current following list, continuous recent-feed supplementation and investigation of newly found discrepancies. The privately maintained acceptance list contains 28 works (the user's initial “8” was explicitly corrected to 28). That bounded verification campaign is complete as recorded above; user acceptance remains pending. Metadata observations may supplement the author catalog, but must not change ownership, download history or source pagination baselines. Real plans, account metadata and results stay outside Git.

- Use the canonical `MangaMonitor-AI` checkout and refresh current `main`, status and applicable instructions before editing. Retired dated worktrees and historical EXE paths are not continuation targets. The currently installed app remains the reviewed formal version.
- Read [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md). Formal suites/builds run in CI; do not duplicate them locally or trigger full source scans for documentation maintenance.
- Preserve local data identity, verified ownership, source uncertainty and file-operation authority; apply the current download-click authorization described above. Apply the [download thaw gate](DOWNLOAD_EXECUTOR_THAW_GATE.md) when changing download behavior or authority.
- Private account, book, library, installation and cleanup evidence stays outside Git. Local cleanup receipts are not product payloads or public documentation.
- Repository organization changes documentation and navigation only. Do not rebuild, republish or replace 1.0.1 assets for these text changes.

## Historical evidence

- [Complete handoff through 2026-09-29](archive/DEVELOPMENT_HANDOFF_2026-09-29.md), including the 2026-09-15–29 development and acceptance trail.
- [Earlier handoff through 2026-09-15](archive/DEVELOPMENT_HANDOFF_ARCHIVE_2026-09-15.md).
- [Historical document index](archive/README.md), including the original cloud, assistant and matcher plans.

Historical draft/unmerged, pending-acceptance, temporary-path and waiting-for-push statements describe their original milestones and are superseded by this current entry.

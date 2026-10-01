# Library and browsing follow-ups — 2026-10-01

## Approved scope

The user approved continuing the interrupted batch and added naturally wrapping
author search tabs. Keep the current visual design; do not merge or publish a
release. Work is based on accepted download metadata fix `0fdc8fc` on
`codex/library-browse-followups`, stacked on draft PR #24.

| Previous experience | This batch | Purpose |
| --- | --- | --- |
| Author tabs scroll horizontally | All open tabs wrap into rows; long names wrap and each has a close control | Show all authors without sideways scrolling |
| History shows titles only | Visible rows reuse scoped source/local covers and existing retry caches | Recognize previously viewed works without recording background loads as visits |
| Local cover menu has three actions | Add open file location and confirmed move to Recycle Bin | Manage the selected archive from its card |
| Detail author row only follows normally | Add a separate special-follow action; enabling first establishes ordinary follow when needed | Keep ordinary and special-follow choices together |
| Recent updates requires one source | Add JM + Pica using the same per-source readers, pagination and caches | Browse both sources with per-source failure/retry reporting |
| New markers depend on manual scan | Add automatic previous-browsing comparison to recent and author updates | Preserve new badges during a launch without conflating scan discoveries or unread updates |

## Identity, browsing and compatibility

- Source + work ID is the work identity. Cross-site copies remain separate.
- Existing filters use explicit tags only; unknown tags remain visible. This
  batch adds no per-work detail queries for filtering or whole-author scans.
- History remains capped at 100. Covers do not create visits, alter ordering,
  or save URLs/credentials. Unavailable local items retain their history row.
- Author tabs keep their existing independent results, filters, errors, scroll
  position and temporary selection. This batch changes layout and close focus,
  not cross-source author identity semantics or restart persistence.
- Ordinary follow precedes first special enable. Cancelling special follow
  leaves ordinary follow; failures do not erase baselines/unread records.
- Combined recent updates reuses exactly the single-source readers. Initial
  heads sort by available website update time; paginated and metadata updates
  preserve displayed identities and the visible anchor. Each source can fail
  and retry independently. No incomplete source is described as complete.
- Automatic browse markers have optional, version 1, per-account/source/surface
  sidecars. Existing library/download/catalog/history/special formats are not
  migrated. Native resolves account namespace from its session; no renderer path
  or credential is stored. Invalid/future sidecars are preserved, not reset.
- The first use seeds known records without marking the entire old catalog new.
  Within one launch the opening baseline and badges remain fixed across section
  and source changes. Saved baselines are for the next launch, not a live reset.
- Recent badges require a continuous head joined to the prior head; historical
  tail pagination alone cannot mark old works new. Incomplete/missing joins are
  explicit and do not advance a trusted head. Author badges compare confirmed
  catalog identities. Neither changes special unread or manual scan markers.

## Recycle-only boundary

The user chose confirmation followed by Windows Recycle Bin, recoverable.
The local menu sends only root/generation/entry/revision. Native resolves the
stored path, refuses non-ZIP/CBZ, links, changed identities, stale revisions,
active directory reads and active download admission/execution. Idle temporary
download previews are retired; durable download tasks/history are preserved.
Cancellation does not mutate library registration or files.

Root/parent leases and repeated file-identity checks bind the selected archive
through confirmation and the Shell pre-delete callback. `IFileOperation` runs
on a dedicated STA thread with recycle-only flags, vetoes non-recycle operations
and validates completion/returned recycled identity. There is no recursive,
permanent-delete or shell-script fallback. This protects against this app's
concurrent tasks; it is not isolation against a hostile same-user process
swapping a leaf path between the Shell callback and operation.

After confirmed movement, a tombstone preserves source links, original admission
metadata and reading/history identity while current ownership becomes absent.
Restore from the Recycle Bin and rescan can reinstate the same identity. OS or
registration uncertainty is reported rather than presented as clean success;
a corrected current projection prevents stale owned status when saving fails.

Windows API references: [operation flags](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags),
[pre-delete veto](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-predeleteitem),
[post-delete recycled item](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-postdeleteitem).
Reuse existing pinned Windows bindings; no copied third-party implementation.

## Verification and delivery status

Implementation and synthetic regressions are being integrated. Formal checks and
builds run in the established CI, not locally. Record exact commits, run links,
counts, artifact hashes, visual review and untested boundaries here after they
finish. Prior accepted builds do not establish this batch's acceptance.

Required regressions include multi-row tabs and focus at narrow widths; visible
history covers, failure/retry and scroll preservation; ordinary/special follow
ordering; cancel/busy/stale/late-response recycle and corrected ownership;
per-source combined pagination/late heads; first baseline, failure, account
renewal, section switches and cold restart markers; existing recent list order
and flicker checks. Real manga files are not downloaded or deleted in testing.

## User acceptance steps after exact-candidate delivery

1. Open enough authors to span several rows, resize, switch tabs, and close a
   middle tab. Names/close controls must stay visible and results stay separate.
2. Open history: recognizable covers should load as rows enter view; returning
   from a work preserves position and background cover requests add no visits.
3. In the library, right-click for all five actions. Check open location and
   cancel deletion. Recycle/restore validation uses a synthetic archive only.
4. From a source detail author row, enable special follow, then cancel it;
   ordinary follow must remain. Existing special baseline/unread rules apply.
5. In recent updates choose JM + Pica, browse multiple pages, retry one failed
   source, and return from other sections without jumping or losing records.
6. First browse establishes a baseline. On a later launch, newly seen head/catalog
   identities get a badge; keep it while switching sections/sources. Old tail
   pages should not get new badges, and special unread must remain independent.

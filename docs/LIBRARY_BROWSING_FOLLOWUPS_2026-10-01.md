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

The final application/test revision is `19b50a40d3453b7b35d7c2f6171c155df2bd1f46`.
Formal tests and builds use the established CI; no local suite or Rust build was
run for this batch.

- [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36878440436) passed.
- [UI CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36878440336) passed formatting, 355 logic cases, TypeScript/build and all 268 Chromium cases.
- [Windows CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36878440424) passed storage/account/library tests, all 57 native IPC cases, strict Clippy, packaging and isolated installation/startup/restart/data preservation.
- Windows native tests exercise a generated temporary ZIP through the actual
  Shell Recycle Bin adapter, verify the returned recycled identity, retain an
  unrelated file and reject permanent fallback signals. Library/IPC regressions
  cover cancellation, stale identities, active readers/downloads and failures.
- Browser regressions cover history covers and deliberate visits, special-follow
  ordering, independent source failures, launch baselines, old-tail pagination,
  persistence errors, wrapped tabs and unchanged recent-list/flicker behavior.
  Two initial scroll tests incorrectly assigned scrollTop while restoration was
  settling; they now use real wheel input. Anchor and stability assertions remain.
- Final CI screenshots were inspected at narrow widths: author tabs wrap into
  visible rows with close controls; the five-action local menu stays in bounds.

The downloaded candidate manifest has identical source/checkout revisions of
`19b50a40d3453b7b35d7c2f6171c155df2bd1f46`. All eight manifest files match their
recorded size and SHA-256; the installed-form executable hash is
`964918a8c18df107e4348e36eaa5541734366ba19128bc2c3e5b988bdeccc008`.
After current user approval, the exact candidate was installed through the
existing Dev desktop entry. The installed executable matches the hash above.
The installer preserved all snapshotted registration documents byte-for-byte.
Bounded native checks confirmed the five-action menu and cancelling its native
confirmation, visible history covers, a combined JM/Pica first page, navigation
position retention, and first-use recent/author browsing baselines. Existing
incomplete author ranges remain explicit; no new full scan was started.
Library, downloads, follows, reading progress, history and author-query rules
remained byte-identical after these checks. Ordinary cover/list reads and four
new per-source/surface baseline sidecars are expected local cache activity.
Special-follow real usage, future natural updates and final user acceptance remain
separate from these synthetic checks. No real manga download/deletion or full
source/author scan was performed for verification. The user approved closing current windows for this update; the new main window
is open for acceptance. No special-follow toggle or media execution was used
against the real profile.

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

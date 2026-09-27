# Project review corrections

The user authorized correction of the four findings in
[the project review](PROJECT_REVIEW_2026-09-27.md), after accepting the integrated
Dev UI. This batch changes the affected behavior and its synthetic regressions;
it does not reopen canceled identity-matching features or authorize real media
execution, user-data rewrites, replacement, deletion or production release.

## Corrected contracts

1. A compact receipt created when clearing completed download history retains
   the original task's verified output file identity. A later library scan
   cannot supply that missing historical proof. Old receipts remain readable,
   but without original identity they report unknown rather than adopting a
   replacement file at the same path. Independently reviewed library entries
   and verified ZIP relocation evidence retain their existing authority.
2. Each temporary search query keeps its first accepted page's total/page-count
   baseline, including unknown values. Continuation retains that baseline;
   changed pagination rejects the changed page and keeps previously accepted
   results visibly incomplete. Explicit fresh search resets the baseline, and
   separate approved query aliases have separate baselines. This does not add
   requests, automatic rescans or snapshot guarantees for a changing website.
3. Vertical reading distinguishes the saved viewport-top anchor from displayed
   reading progress. At the full chapter's actual bottom, a short final image
   reports the final page and exposes the chapter-end action. An internal
   virtual segment's bottom is not the chapter's end. Zoom, resize and saved
   position keep the original anchor; single-page mode remains index-based.
   Very short images at extreme zoom-out retain their actual image size and
   aspect ratio, with a page-slot height of at least one tenth of the viewport.
   This keeps every visible page inside the existing 12-page render/cache bound;
   only these short slots gain centered blank space, with no concurrency increase.
4. Download preparation checks destinations reserved by the current explicit
   batch, including retained preparation chunks. Conflicting rows become batch
   issues while other plans remain selectable for confirmation. Unrelated
   canceled previews do not reserve names. Confirmation also checks the
   existing queue atomically; execution checks the destination before fetching
   media. Original-output retries are allowed only with the recorded identity.
   Windows filename comparison is ordinal case-insensitive. No automatic
   renaming or overwrite is introduced, and exclusive final creation remains.

The optional compact-receipt field is omitted when absent. Existing documents
are read without eager migration; new evidence is written through the normal
explicit history-removal action. Older strict binaries are not promised to read
documents after this new field is saved. Keeping a prior EXE is not a data-format
downgrade guarantee. The frontend search baseline and displayed reader progress
do not change the persistent search/reader document formats.

The receipt preserves the existing native output-file identity contract. This
fix covers replacement by a different filesystem object, including after a
rescan; it does not add full-archive hashing to detect arbitrary in-place content
rewrites that preserve that native identity.

## Download gate review

`DOWNLOAD_EXECUTOR_THAW_GATE.md` was applied to local receipt/queue behavior.
The accepted ZIP, file-presence and rescan-finalization contracts were reread.
The pinned upstream protocol evidence in `upstream-source-reference.md` remains
applicable: JM `f0cdd724af6892002f2fb7be883b88832cebe7e9`, Pica
`77c8b62ede42b3afc074506d092313816af8092d`, and Python reference
`9fddb0494caf0cdc812ac6cbfc1c62f4f845b058` are unchanged. There are no source,
authentication, host, image-transform, concurrency or network-retry changes.

The affected gates are stricter local ownership evidence and earlier destination
conflict rejection. Approval generation, task/source/target binding, isolated
staging, media proofs, original-output identity, complete ZIP validation and
separate registration remain mandatory. No real manga download or library
mutation is part of development verification. `production_enabled=false` stays
in effect and PR #19 remains a draft.

## Verification and delivery

Targeted regressions cover same-path file replacement followed by a completed
library rescan, legacy receipts, unchanged files and explicit relocation;
same-name plans, mixed sources and pre-execution destination changes; pagination
drift with exact-count coincidence, unknown baselines, retry and independent
aliases; and short final pages, narrow windows, zoom, reopen and long-chapter
segments. Existing source, queue, reader and native lifecycle checks stay enabled.

Formal checks/builds run only in the existing CI. Local work consists of editing,
formatter edits and independent code review. CI execution, built-artifact visual
review and verified Dev delivery are pending. No user acceptance is claimed for
the corrections until the delivered build is checked by the user. No installer,
merge or formal release is part of this batch.

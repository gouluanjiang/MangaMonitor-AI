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

Formal checks/builds ran only in the existing CI. Local work consisted of editing,
formatter edits, independent review and verifying the CI artifacts. Final head
`2493fb51e89641daaa132cb81bde1712d99949e9` (test merge
`d4d99a57eb889aa64d03683c0e10720bfeda9fa4`) passed:

- [Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36323489129):
  formatting, type-check/build, 238 logic tests and 213 Chromium cases.
- [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36323489127):
  Linux workspace tests/Clippy and Windows local-executor contracts.
- [Desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36323489124):
  offline suites, all 12 new receipt/destination cases, isolated credentials,
  49 native tests, Clippy, executable build and isolated actual WebView
  startup/restart/multiple-reader lifecycle verification.

The two new synthetic reader screenshots and native window evidence were reviewed.
Native close coverage uses owned-window WM_CLOSE plus the frontend handshake and
final toolbar close; it is not a physical titlebar-button click or a real manga
read. No source requests or real private-data operations occurred.

The separate Dev executable is `MangaMonitor-Dev-20260927-2493fb5`, SHA-256
`e6d2d7c54022d6d95a48a34b761f7ba2a1161401fe7adead1ebff459091b6442`.
Artifact digest, ZIP CRC, x64 PE and embedded head were checked; both existing
Dev links were backed up and updated without restarting the user's application.
Private evidence and acceptance instructions are in
`Documents/Codex/MangaMonitor-review-fixes-20260927`. The user must fully exit all
main/reader windows and reopen Dev. The user subsequently accepted the corrections
and authorized release-candidate preparation. No installer,
merge or formal release was performed. Final evidence notes are retained locally
for the next necessary push, avoiding a documentation-only duplicate CI run.

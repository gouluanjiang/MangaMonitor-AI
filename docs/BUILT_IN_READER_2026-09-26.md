# Built-in reader: accepted scope and implementation

Acceptance update 2026-09-27: the user accepted steps 1–5. Step 6 (real failed-page/retry behavior) remains untested because no failure was encountered. The original reader stays available while the separately approved [independent reader-window addition](READER_WINDOWS_2026-09-27.md) is implemented. Earlier delivery statements below describe the state at that delivery.

The user completed the reader Q&A and authorized development on 2026-09-26. Local ZIP and JM/Pica online reading belong to the same batch. The recent-feed continuation/position correction was accepted before this batch. Reader engineering verification and user acceptance are separate milestones; the validation receipt below must be completed before claiming delivery.

## Reading contract

- A cover opens a choice between direct reading and work details. Existing title/detail navigation remains available. Local library and website views share the same reader; a verified source-ID registration prefers the corresponding actual local file. No title matching or new cross-site association is introduced.
- Two modes only: vertical scrolling and single-page. Every opening starts in vertical, fit-width mode. Single-page initially fits the window. Mode and zoom do not persist.
- Ctrl + mouse wheel changes zoom; normal wheel scrolls. Left-button dragging pans an enlarged page and must not accidentally turn a page. Single-page navigation is left/right half clicks and keyboard left/right arrows, without extra previous/next page buttons.
- Remember chapter, page and relative position across application restarts. Return restores the underlying page and browsing position. Merely reading cannot create a download task, owned receipt or library item.
- Start windowed. F11 and a visible toolbar button toggle fullscreen. Bottom-hover reveals the toolbar; leaving hides it. Focused controls remain accessible.
- The toolbar has chapter selection and a draggable current-chapter page slider with xx/xx text. There is no numeric page input. Chapter end stops and exposes an explicit next-chapter action.
- Online reading has a download-this-book action routed through the existing preparation and confirmation dialogs. It does not bypass confirmation or change the download executor's authority.
- Page bytes exist only in bounded runtime memory. Local ZIPs are read by entry, not extracted in full. Only small progress metadata persists. Failed pages can be retried or skipped.
- Long chapters retain logical page positions while using a bounded physical scroll segment, avoiding browser coordinate limits. Repositioning a segment preserves the current page, page-relative offset and active drag reference. Extremely narrow/tall images are proportionally constrained to a 250,000 CSS-pixel page height; original bytes remain unchanged.

## Reuse decisions

The existing `zip`/`image` crates, reviewed ZIP preflight and file-identity checks, source metadata adapters, HTTP transport, JM image transform and actual-format Pica handling are reused. Online reading is a separate read-only consumer of those components, not a simulated completed download. Existing download guards, staging, JPEG conversion, destination promotion and ownership contracts remain in their original flow.

Komga's small eager-load neighborhood predicate is adapted at pinned commit `65981e600edb24944ffaae4818ff2716a5fa08dd`; the full MIT notice is retained in `apps/local-workbench/src/reader/THIRD_PARTY_NOTICES.md`. Its reader provides useful behavior, but the Vue/server state cannot be transplanted into this React/Tauri application as a standalone component. Our layout and progress integration remain small reader-specific modules.

The reviewed JM downloader pin is `f0cdd724af6892002f2fb7be883b88832cebe7e9`; Pica downloader pin is `77c8b62ede42b3afc074506d092313816af8092d`. Their existing MIT attribution remains. The reader reuses the project's already adapted transport/image pipeline rather than duplicating that code.

Yomikiru's Electron/filesystem state and full extraction approach conflict with this batch's no-extraction contract. Suwayomi's reader depends on its MUI/state/GraphQL server architecture. Both inform interaction and bounded loading decisions, but bringing their complete subsystems would add more adapters and redundant state than this application needs. GPL reader implementations and proprietary reader code are not copied. The existing source and library contracts are the stronger fit for file identity, account scope and download state; the mature projects inform page interaction and prefetch policy.

## Module boundaries

- `src/reader`: UI, geometry/progress helpers, bounded current/neighbor page cache and native contract validation.
- `src/reader-access`: cover menu and a host that leaves underlying pages mounted; `App` provides the existing download preparation callback.
- Native `reader`: session registry, explicit local-first resolution, IPC, cancellation, fullscreen restoration and position saves.
- `workbench-library::reader`: checked ZIP entry order and original page reads.
- `workbench-storage::reader`: separate `reader-progress.json`; existing inventory and history schemas are untouched.
- `workbench-accounts` and `cloud-monitor::online_reader`: account-bound online sessions. JM metadata is fetched by chapter; Pica image-address pagination is lazy. Image processing and requests have bounded concurrency.

## Verification and boundaries

The download thaw review was reapplied to the shared transport/transform extraction and the reader's download button. The existing download constructors retain their prior limits, guards and redirect validation; the reader uses distinct bounded constructors and never calls staging, finalize, promote, delete or inventory writers. `beginDownload` now awaits preparation so its failure can be presented inside the reader; it still creates only a plan and the existing confirmation remains authoritative. Account-generation and reader-session guards apply before/after each read. The existing JM/Pica pinned protocol and download acceptance are reused; new synthetic media-reader tests cover the added consumer. This is an implementation review, not permission for a real manga download during development.

Formal logic, browser, Rust, native IPC, Clippy and desktop builds run in existing CI only. Added synthetic coverage must include page ordering/chapters, file replacement and unsupported archives, save/reopen position, out-of-order/closed reader responses, cache bounds, zoom/pan versus clicks, keyboard/slider navigation, chapter boundary, fullscreen restoration, cover/detail entry and existing download confirmation behavior. Existing interaction tests are adapted to the intentional cover menu without removing their earlier assertions.

Before delivery, inspect the running synthetic reader screenshots at desktop and narrow widths. No real library or manga samples belong in CI. No new full-author scan, automatic downloading, reader disk-image cache, installer, merge, production enablement or formal release is part of this batch. Existing Dev builds and rollback artifacts remain.

Status: engineering verification, visual inspection and Dev delivery complete; real user acceptance remains pending.

Current roadmap: reader implementation/acceptance → overall UI and interaction refinement → formal release preparation. A6 complete-author acceptance remains deferred by the user.

## Final engineering and delivery evidence

Implementation, independent review, CI, running synthetic visual review and verified Dev delivery are complete. User acceptance of local and real online reading remains pending. See [the accepted reader contract](BUILT_IN_READER_2026-09-26.md). Covers offer reading or details; local ZIP and JM/Pica online sessions share vertical/single-page modes, Ctrl-wheel zoom, panning, half-page/keyboard navigation, chapter selection, a slider, fullscreen and saved position. Returning preserves browsing state. Verified source IDs prefer the actual local file; no new identity inference is introduced. Page images remain in bounded runtime memory, with no full extraction or persistent page-image cache. Online download uses existing preparation and confirmation.

Final head `ea855c10a6218c09cfd5040c0dd5305462bc8067`, test merge `73f1e0586952d112b883b93f29f62e70f9c5896e`. [Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36256236384) passed formatting, type-check/build, 219 logic and 195 Chromium tests. [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36256236396) and [Windows CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36256236437) passed, including 43 native IPC tests, Clippy, the EXE and isolated Windows WebView startup/restart. All formal checks/builds ran only in CI. Earlier runs exposed long-chapter browser coordinate limits and fractional page-boundary behavior, an immediate fullscreen-exit race, native visibility lint errors, and asynchronous test setup/cleanup timing. Bounded scroll segments, a one-CSS-pixel boundary tolerance and synchronous fullscreen references fixed the product issues. Tests now await exact native wheel delivery and cleanup tokens without relaxing behavior assertions. The reported passing results apply to this final revision.

Three synthetic running reader screenshots were inspected: vertical, single-page and narrow toolbar. These establish synthetic interaction/layout evidence, not a real JM/Pica reading acceptance. No user account query, author scan, manga download, app start/stop or library/inventory/history edit ran during development. The small MIT Komga adaptation and existing source/ZIP/image reuse are documented with notices.

Dev delivery: `Documents/Codex/MangaMonitor-Dev-20260926-ea855c1/mangamonitor-workbench-preview.exe`, SHA-256 `81db795b1bdc81951d5f58d4383e204d4a440c10369243412c94489e65d91bb7`. Artifact digest, ZIP CRC, x64 PE, embedded revision and both existing Dev shortcuts verified. Prior executables/shortcuts remain; the running app was not closed. Exit and reopen Dev to accept the reader. Private report and receipts: `Documents/Codex/MangaMonitor-reader-development-20260926`. Dev remains 0.3.4, PR #19 draft/unmerged and production disabled; no installer installation or formal release. Evidence-only notes remain local until the next necessary push.

Current roadmap: reader user acceptance → overall UI/interaction refinement → formal release preparation. A6 all-author acceptance stays deferred. The preceding recent-feed position correction is user-accepted.

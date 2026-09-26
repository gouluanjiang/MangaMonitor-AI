# Built-in reader: accepted scope and implementation

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

Status: implementation and review in progress; CI, artifact inspection and user acceptance have not yet completed.

Current roadmap: reader implementation/acceptance → overall UI and interaction refinement → formal release preparation. A6 complete-author acceptance remains deferred by the user.

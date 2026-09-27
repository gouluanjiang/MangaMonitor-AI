# Independent reader windows

The user approved this addition on 2026-09-27 after accepting steps 1–5 of the in-app reader. Its real failed-page/retry acceptance remains untested because no failure was encountered. The existing in-app reader stays available.

## Accepted behavior

- Cover actions are 漫画详细, 程序内阅读 and 手机小框阅读. Different books can remain open in separate native windows while the main application remains usable. Reopening the same exact book focuses its existing reader window; titles are not used to guess identity.
- Initial windows use the requested portrait display proportion, approximately 1206:2622, at a usable desktop size. Frames fit within the monitor work area and windows are staggered. Users can resize them. The last normal size is remembered and clamped to the next monitor; fullscreen/maximized sizes and always-on-top are not saved as defaults.
- Always-on-top starts off in each window and can be toggled independently. Vertical mode fits image width; single-page mode fits the whole page. Deliberate Ctrl-wheel zoom retains pan/reset behavior. Existing navigation, chapters, progress slider, fullscreen and saved reading position remain.
- Closing the main window while readers exist hides the main window and leaves readers working. A reader can restore the main window. Closing the last reader while the main window is logically closed exits the application after saving progress. Minimizing the main window does not hide reader windows.
- Local ZIP and online readers use the existing approved reader paths. Window position, chapter, mode and zoom operate independently. Online session changes invalidate the affected readers; local reading remains independent.
- A reader's download action restores the main window and enters existing preparation/confirmation. It does not start downloading directly or overwrite an existing confirmation.

## Implementation boundaries

The existing reader components, ZIP/image pipeline, source adapters and progress model are reused. Native reader state is scoped by the calling window and its bound request; the child capability only permits reading and its own window controls. Main-window account, library and download mutations are not exposed to children.

Native close requests allow the frontend to flush pending reading position before destroying a child. Destruction also cancels that child's remaining work. Native image concurrency and progress writes are coordinated across windows. Image data remains in bounded runtime caches; inactive windows reduce speculative prefetch. A separate small preference document stores only normal window size.

The download thaw gate was reviewed. This change relays a validated source reference into the existing main-window prepare/confirm flow; executor protocols, upstream pins, staging, file promotion and inventory authority are unchanged. No real-source request or download is needed for engineering validation.

## Verification and delivery

Final head `2323f1132316fea9506bf95de979c017cc55dc1c`, test merge `3a07995387fb12d35cdae6be1e1e01a01fcc255b`. [Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36297439744) passed formatting, type-check/build, 224 logic and 199 Chromium tests. [Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36297439769) passed both jobs. [Windows CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36297439749) passed offline suites, 49 native tests, Clippy, EXE build and actual isolated WebView startup/restart plus multiple-reader lifecycle checks. The native smoke verified separate OS windows, independent topmost state, exact-book reuse, main navigation, main WM_CLOSE hiding while children remain alive, child restoration of main, child WM_CLOSE isolation and final toolbar-close process exit. Synthetic non-existent library references yield expected handled read errors; this verifies windows without reading user media or querying websites. WM_CLOSE exercises the system close-request path; it is not a physical mouse-click claim.

Five final-revision synthetic reader screenshots were verified and reviewed. All formal tests/builds ran only in CI. User acceptance of these independent windows remains pending. Earlier in-app reader steps 1–5 are user-accepted; real failed-page/retry behavior was not encountered and remains untested.

Dev delivery: `Documents/Codex/MangaMonitor-Dev-20260927-2323f11/mangamonitor-workbench-preview.exe`, SHA-256 `4b4abccb390903423c953f5b36ed2d50f58cc870771ac30209c8a953fdd8b883`. Artifact digest, ZIP CRC, x64 PE, embedded head and both existing Dev shortcuts verified. The previous EXE and shortcut backups remain. The running user app was not started or stopped; exit and reopen Dev for acceptance. Private report and receipts: `Documents/Codex/MangaMonitor-reader-windows-20260927`. Dev remains 0.3.4, PR #19 draft/unmerged and production disabled. No installer or formal release was created. Final evidence-only notes remain local until the next necessary push.

Validation corrected a missing storage Clone bound, a root Suspense startup retry blocked by frozen browser clocks, overly exact floating-point progress assertions and a resize-handler Clippy warning. Actual Windows smoke then exposed cross-window close events: the default Any event listener received directed events for other windows. The final revision uses the official current-window listener; the synthetic bridge now models the same target filtering. Chapter/page identity, original reload conditions and the actual native requirement that B survive A's WM_CLOSE remain intact.

User acceptance update, 2026-09-27: the independent reader-window batch passed. Its engineering evidence above is unchanged; the earlier real failed-page/retry case remains untested. The user's next requested investigation concerns JM sign-in restoration and is tracked separately.

Roadmap: JM session-restoration investigation → overall UI/interaction refinement → formal release preparation. A6 all-author acceptance remains deferred.

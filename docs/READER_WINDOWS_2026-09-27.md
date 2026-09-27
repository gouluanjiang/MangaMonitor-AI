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

Implementation and independent static review are complete; CI and delivery remain pending. Formal checks/builds run only in CI. Browser cases cover independent windows, controls, pending close/save, main restoration and download confirmation. Windows CI additionally exercises actual native windows using synthetic, non-existent library references and expected handled read errors. It sends native WM_CLOSE only to verified HWNDs belonging to its own isolated app process, checking the frontend save/close handshake and other-window survival. This establishes window lifecycle behavior, not real manga reading acceptance or a physical mouse click on X. User acceptance remains pending.

Static review corrected close/context events to target one window explicitly and added a download-handoff flight guard plus a final existing-confirmation check after asynchronous queue inspection. The browser and native regressions retain the resulting isolation assertions.

CI iteration note: the first run found a missing `Clone` document bound and a root Suspense retry held behind paused browser clocks; the corrected startup awaits only the relevant window entry before mounting. Existing frozen-clock tests remain unchanged. Page-position assertions keep exact chapter/page identity and tolerate only 1e-10 normalized offset rounding. At `5a7639f`, all 223 logic and 199 browser tests and baseline CI passed. Windows native tests passed; a `collapsible_match` lint in the resize handler requires a final correction before EXE/smoke validation. These intermediate results are not final delivery evidence.

Dev stays 0.3.4, PR #19 stays draft/unmerged and production stays disabled. No installer or formal release is part of this batch. The user's running app and manga files are not modified during development.

Roadmap: independent reader-window acceptance → overall UI/interaction refinement → formal release preparation. A6 all-author acceptance remains deferred.

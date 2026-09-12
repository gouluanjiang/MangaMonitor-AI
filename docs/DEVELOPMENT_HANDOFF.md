# Active development handoff

Updated 2026-09-12. This document separates the current continuation from historical milestone reports.

## Current follow-up: removed download files and re-download (2026-09-12)

Before the speed retest, the user reported that externally removed works still appeared downloaded and blocked a new preparation. Read [the file-presence repair](DOWNLOAD_FILE_PRESENCE_2026-09-12.md). Preserve historical completion and separately project current local files as present, missing, incomplete or unavailable. Explicit reads/queue entry/window focus recheck recorded paths and sizes; progress polling reuses process-local results. No full library scan or image-content hashing is added to polling.

Prepare and confirm independently check actual paths. A proven missing completed work can receive a new explicit plan/task; an inaccessible root is not absence. Confirm retires only proven missing same-source private index entries through CAS so existing strict registration accepts the newly created directory. Never reuse old completion/staging proof, overwrite surviving files, alter phone marks or infer deletion from an IO error. Root identity, task/session approval and output proofs remain. All formal validation/builds stay in CI. Deliver a new Dev EXE at 0.3.4 with a local result report, no installer. Both this real deletion/re-download check and the upstream-performance timing remain user acceptance items. Next feature batch remains Pica single-work downloads.

## Previous follow-up: upstream-style JM parallel download pipeline (2026-09-12)

The desktop metadata path also inherited the monitor's random 1–3 second wait before every album/chapter request (3–9 seconds of injected waiting for a single chapter). The explicit desktop download path now selects immediate album/chapter requests through a named JM constructor; ordinary monitor/CLI clients and other API routes retain their pacing. This is a code-path delay calculation, not an end-to-end benchmark.

The user retested the JPEG/two-request revision: faster, but still substantially slower than jmcomic-downloader. They explicitly removed the low-computer-load constraint for this download optimization. Read [the current performance follow-up](JM_UPSTREAM_PERFORMANCE_2026-09-12.md): 20 independent async image lifecycles, separate CPU processing/validation/hash, exact-byte parsed snapshots for private library/download documents, and cancellation that drains actual CPU work before worker release. This follows the pinned upstream scheduling structure using the existing Tokio pool. Ordered durable files/checkpoints, old WEBP tasks, JPG/GIF output and existing-directory protection remain. No installer/version bump. Formal validation and Dev builds stay in CI without duplicate local suites; real timing still requires the user's retest. Next feature batch remains Pica single-work download.

## Previous follow-up: JM JPEG output and two requests (2026-09-12)

The user accepted all previous JM single-download functional checks, then reported a same-work/fresh-download speed gap and requested output matching their existing JPG library. Read [the JPEG/performance follow-up](JM_JPEG_PERFORMANCE_2026-09-12.md). New tasks encode directly to JPEG (GIF unchanged); old saved WEBP tasks keep their original policy. Media futures are bounded to two and processed on one worker, with ordered checkpoint/file writes and no background tasks surviving cancellation. Existing output, phone data and PC copies remain. CI and a new Dev executable are the delivery path; no installer/version bump. Do not claim instrumented timing improvement until the user retests this revision. The next feature batch remains Pica single-work downloads.

## Current batch: explicit JM single-work desktop downloads (2026-09-11)

The user requested the next development batch and supplied a private single-work JM acceptance sample. Implemented scope: prepare a native-scoped plan, explicitly confirm one work and picked PC directory, run the existing A6 source/media/staging chain, save an add-only compatible directory, verify it, and register only that work in the PC index. The native queue now uses real state, pause/continue/manual retry and read-only restart recovery. Phone presence stays independent and PC copies remain.

Read [the batch report](JM_SINGLE_DOWNLOAD_BATCH_2026-09-11.md), [the session gate review](JM_DESKTOP_DOWNLOAD_GATE_2026-09-10.md), and [the updated roadmap](ROADMAP_AFTER_JM_DOWNLOAD_2026-09-11.md). All three pinned upstreams were reopened this session. The desktop manual ledger is not a synthetic PROVEN_NEW/matcher certificate and does not mutate production inventory. Completed task-owned temporary staging may be released only after verified final output, PC registration and durable task completion; final work files are never deleted by this lifecycle.

Formal test/build execution remains CI-only. Local work is limited to edits, formatter edits and distinct read-only review. Verify the exact PR #19 commit checks and final delivery evidence; code presence and synthetic tests do not prove real-source acceptance. The CI-tested EXE is available for manual acceptance without a new installer. Keep version 0.3.4, PR #19 draft/unmerged, production disabled and updater work frozen. Next development batch is Pica single-work download, followed by batch queue/history and identity confirmation.

## Previous batch: separate PC downloads and phone presence (2026-09-10)

The user also requested a roadmap after this batch. See [the updated roadmap](ROADMAP_AFTER_DUAL_LIBRARY_2026-09-10.md): current dual-library integration, then JM and Pica real single-work downloads, batch queue/identity review, discovery/monitor synchronization, rankings, and formal V1 acceptance. Forecasts are engineering effort estimates, not calendar promises; no separate 0.3.5 installer or updater work is introduced.

The user resolved the library model: phone presence means 已入库, whether or not a PC copy also exists; a PC-only copy means 已下载. PC files remain after transfer to phone. The user manually transfers files and chose manual phone marks plus updated TXT name imports. Import replaces only the previous TXT snapshot, preserving manual marks. No phone access, file transfer, PC cleanup, media mutation or production inventory authority follows from these private records.

This batch implements native-picked read-only work-directory / ZIP / CBZ indexing, JM/Pica downloader metadata compatibility, virtualized PC and phone browsing, run-only covers, and source-scoped associations/status. Explicit-reference phone marks cannot mark a different same-title source as owned. A failed initial phone read remains unknown. Source downloads and the queue are still unconnected; no real downloads or installer are part of this batch. Formal validation is assigned exclusively to the existing GitHub UI, desktop and baseline workflows. See [the current batch report](LOCAL_WORKBENCH_DUAL_LIBRARY_2026-09-10.md); do not infer user acceptance from code or CI alone.

The user then clarified future downloaded output must match their existing lanyeeee JM/Pica downloaders: work directory, metadata, cover and numbered chapter images. This newer direction supersedes ZIP-only output below. Existing archive recognition remains useful; PDF/CBZ export is still cancelled. Four reliability behaviors still apply to completed files/directories: visible errors/manual retry, no false completion, preserve already downloaded copies without overwrite/redownload, save normal-close queue state. No in-app updater before formal V1. Read [upstream directory compatibility](UPSTREAM_DIRECTORY_COMPATIBILITY_2026-09-10.md) before the future download batch; this read-only batch does not change real-executor staging/promotion gates.

## Historical scope correction: ZIP output only (2026-09-09; superseded above)

The user explicitly removed PDF/CBZ export from the product scope: downloading a work ZIP into the local library is sufficient. Do not build export controls, conversion tasks or export acceptance. Existing ZIP/CBZ library recognition was not cancelled. This supersedes historical export requirements in design previews and documents.

The user confirmed four minimal reliability behaviors: visible request failures with manual retry; incomplete work is not marked complete; completed ZIPs survive retry without overwriting/redownloading; normal close saves queue/completion state for reopening. The user also questioned the value of elaborate network-outage recovery. Explain and scope the minimum practical behavior before expanding recovery work: request failure should remain visible, incomplete output must not look complete, and retry must retain already completed ZIPs. Advanced automatic reconnect/offline orchestration is not a newly authorized workstream. Normal close/save/reopen behavior was previously selected and has not been explicitly revoked; this discussion has not changed runtime recovery or existing integrity/authority checks. Do not add elaborate offline orchestration or long automatic reconnect as new V1 requirements; preserve the four confirmed behaviors and existing integrity/authority checks. Documentation stays local for the next necessary code push; no rebuild is needed for this scope correction.

## Current execution preference: resource-aware local/cloud work

The user requires a responsive desktop and no duplicate local/cloud execution of the same task. Assign each check or suite for the same code revision and target to one location before running, accounting for automatic CI. Use local work only with measured headroom and a clear reduction in turnaround or repeated diagnosis; if CI will run a check, do not run it locally too. Keep full suites/cross-platform/release packaging in CI and use local work for distinct diagnostics, reproductions or checks not repeated there. Follow the resource gates in `AGENTS.md`, retain required validation and download authority boundaries, and batch documentation-only pushes to avoid unchanged rebuilds. The user accepted the 0.3.4 interaction but reported Pica stopping at 640/1877 and 540/1877; this is not full-catalog acceptance. The verified repair is pending the next larger delivery. The user cancelled a separate 0.3.5 installer and version bump; keep 0.3.4 until the next planned release. Automatic CI validates the executable without NSIS packaging; packaging is an explicit workflow_dispatch input. The 0.3.3 repair remains user-accepted.

## Current repair: one Pica catalog and source compatibility, pending next delivery

The user explicitly chose JM-style ordering: one newest-first catalog, complete it before reversing locally, and reuse it for later direction changes. Ordinary entry stays viewport-driven. Explicit Pica time-direction switches or selecting oldest-first start sequential full metadata reading; title sorting, ordinary Pica refresh and scope changes clear that intent. Pause/error retains progress and intent. Errors require explicit retry and cannot be silently retried by returning from settings or changing direction.

Four bounded read-only metadata GETs confirmed HTTP/API 200 responses on the reported stopping pages: oldest-first page 33 contained two completely identical records; newest-first page 28 contained a negative integer pagesCount. The application's strict parser rejected those source anomalies. Compatibility now treats only negative integer Pica page counts as unknown, retains exact same-page Pica favorite duplicates as source entries, and shows unique works separately. Conflicting records, cross-page overlap, search duplicates and JM duplicate rules remain strict. Cache page boundaries preserve the same-page restriction after restart; legacy complete unique caches remain reusable, while legacy Pica partial caches without boundaries are rebuilt once from the verified first page.

Repair head `377d05223d9a1331c27292b1dcd7492d9992b330` (CI merge `80406d05ae04da863171543ebf3dc572f4c0d0a4`) passed all CI: 87 Node, 70 Chromium, 140 Windows account/credential/source/storage, 17 native IPC, 579 Linux workspace and 63 Windows core tests, plus formatting/type/build, Clippy, isolated credentials and actual WebView/process restart. Root reviewed the final changes and exact-merge logs. All formal checks/builds ran only in CI. NSIS build/select/upload were skipped and the desktop artifact list is empty, as requested. See [the pending-release repair report](LOCAL_WORKBENCH_PICA_CONTINUITY_FIX_2026-09-09.md). Real full-catalog/search acceptance remains pending the next packaged release. Final validation docs stay local until the next necessary code push; PR metadata records verified results. No credentials, raw source responses, private account data or real work identifiers enter the repository. Covers remain run-scoped. PR #19 stays draft and production remains disabled.

## Historical: explicit Pica full-catalog reading, Windows 0.3.4 (2026-09-09)

The user selected the existing JM-style manual direction-switch trigger after discussing background indexing. Only an explicit switch between the two Pica favorite-time directions starts sequential complete metadata reading. Ordinary entry stays viewport-driven, and Pica retains native newest/oldest ordering. This does not authorize startup background full scans, larger request concurrency, updater work or real library/download integration.

Implementation is verified at application head `0d9a2c2f14e247b92070b51ce40464c65db38536` (CI merge `09abbd657804f84846bf52b053c5a5adc2932019`). The one-shot request is bound to source/session/folder/direction and consumed by its matching reader; pause and later resumes do not create a new full-read request. In-flight old-direction results cannot replace the current reader. Title sorting, ordinary Pica refresh and source/account changes stop or clear full-read intent. Completed direction caches can be reused after a first-page check; that check does not prove every remote entry is unchanged. Search remains local title/author matching and explicitly labels complete scope once all pages are read.

Root owns UI/version/docs and exact-head CI/artifact delivery; an independent read-only agent reviews state and tests. Local work is source editing/review (including formatter write, not test/check execution) and artifact verification. Formal tests, formatting checks and all builds remain in CI only. See [the batch report](LOCAL_WORKBENCH_PICA_FULL_READ_2026-09-09.md). The user's JM500-work20-30second report is a rough observation, not an instrumented benchmark or a Pica timing promise.

All exact-head CI passed: 79 Node, 65 Chromium, 134 Windows account/credential/source/storage, 17 native IPC/startup, 573 Linux workspace and 63 Windows core tests, plus isolated credentials, Clippy, NSIS and actual WebView/process restart. Installer artifact 10110539241 was verified for digest, sole entry and version 0.3.4; it was not installed by the agent. The user accepted the 0.3.4 interaction, then reported the partial-reading defect covered by pending-release above; full real-catalog acceptance remains pending. The final report and this handoff/README are queued locally for the next necessary code push to avoid documentation-only rebuilds; PR metadata is updated separately.

The next section records the accepted 0.3.3 baseline. Application updating remains frozen until formal V1 completion and later user discussion.

## JM blank metadata compatibility, Windows 0.3.3 (2026-09-09)

The user reported two JM works whose website details/images worked while the application rejected their details and cover recovery. Two bounded unauthenticated public metadata GETs returned HTTP/API 200 and matching IDs; each tag array contained one blank string. Root retained only sanitized field-shape summaries outside the repository. This is a project parser compatibility defect. It is not evidence of unavailable source content, and neither the user nor the agent audited all favorites.

Application head `5d21c7466d51bfd0f0ade081de31d82c16b3e818` (CI merge `1d381263996cdb673323a44e5e29eb2a9404e9de`) omits blank JM author/tag array placeholders after validating raw array count and each string length/type. Valid content/order/duplicates survive; required fields, the whole-work budget, Pica behavior and cover transport remain strict. Five new synthetic tests and the extended evicted-cover-descriptor recovery test pass. Recovery grants no favorite/download authority.

Verification: 79 Node, 59 Chromium, 134 Windows account/credential/source/storage (49 source), 17 native IPC/startup, 573 Linux workspace and 63 Windows core tests pass, plus isolated Credential Manager parent/child, Clippy, NSIS and actual Windows WebView/process restart. All 13 static files match the previously accepted 0.3.2 artifact. Root verified installer artifact 10107763023/run 34360339640, sole EXE entry, version 0.3.3 and SHA-256. Formal tests/builds ran only in CI; local work was distinct live metadata diagnosis, review and artifact verification. See [the 0.3.3 report](LOCAL_WORKBENCH_JM_METADATA_FIX_2026-09-09.md).

The user explicitly accepted the 0.3.3 fix; its sample retest is complete. Preserve this and the historical 0.3.2 acceptance below without claiming exhaustive work coverage. JM sorting remains accepted, covers remain run-scoped without disk image persistence, PR #19 stays draft and production stays disabled.

The user explicitly froze startup prompts and in-app updating until after formal V1 completion, when the idea may be discussed again. Only read-only investigation occurred; no updater code, dependencies, signing keys or release feed were configured. Do not resume that work before the user revisits it. Continue the actual V1 project after closing this narrow repair acceptance.

The 0.3.3 report and acceptance updates are included with the necessary Pica collection change, avoiding a separate documentation-only CI rebuild.

The following 0.3.2 and earlier sections are historical milestones.

## Session-only covers, Windows 0.3.2 (2026-09-09)

Application head `af35f96904b0d59cba04591a303933e3a3b0b3ca` (CI merge `60d549e61ba1e935ecac3045346f2c83a329aa52`) is verified. The user's final requirement supersedes the older 0.3.1 disk-cover policy: retain successful covers only for the running application, reuse through virtual-card unmount/scroll/detail/settings visits, release on exit and reload next launch. No native cover command persists image bytes. Catalog metadata/read progress, following, preferences, booklists and remembered sessions remain persistent. Fixed registered legacy covers are retired once during DesktopStore initialization before document readers receive the shared store; cleanup failure does not block opening documents. This ordering fixes the actual WebView startup BUSY race found during validation.

Verification: 79 Node, 59 Chromium, 129 Windows account/credential/source/storage, 17 native IPC/startup, 568 Linux workspace and 63 Windows core tests pass, plus isolated Credential Manager parent/child, Clippy, NSIS and actual Windows WebView/process restart. Root reviewed the bounded changes and the CI-built synthetic interface, and verified the final installer archive, version and hashes. See [the 0.3.2 report](LOCAL_WORKBENCH_COVER_SESSION_2026-09-09.md) for exact runs/artifact/digests and limits. PR #19 stays draft.

User says JM reversal is now correct: do not reopen or change it. Pica's initial transient example recovered; remaining cover/detail failures also occur on its website according to user comparison. Do not infer work deletion or put real example IDs into fixtures. On 2026-09-09 the user explicitly accepted all 0.3.2 functionality and all three remaining live checks with no observed problems: real JM list/detail covers, Pica run-scoped cover reuse/reload behavior, and the selected website favorite write with website refresh verification. Record these as user-reported live acceptance, separate from CI; do not reopen them or infer exhaustive coverage of every source/operation/work. This repair batch has no remaining acceptance items. Real inventory/download integration is the subsequent development stage; keep production disabled and existing authority gates intact.

The 0.3.1 and earlier sections below are historical snapshots; their cover disk-cache policy and next-install instruction are superseded by this section.

## Favorites and cover repair delivered (2026-09-09)

The user has now accepted real JM/Pica login, favorites reading/refresh, existing source ordering, title/author lookup, local following/booklist restart persistence, and remembered-session restart. These are user-reported results, independent of CI. The earlier suggestion that JM's default order was wrong was explicitly withdrawn: retain its default favorite-time order and do not reopen that diagnosis.

Windows 0.3.1 is implemented and verified at application head `68885c73da797e56b16263918deb070672cc1c96` (CI merge `3f4e423fc3fb26c7526501d4d9ae888360ac599f`). Favorites load one source page near the viewport bottom, with cached progress and virtualized 5/7/9 rows. Pica uses native collection-time directions; JM retains its accepted default and prepares a full index only for explicitly requested global reversal. Cover compatibility, eviction recovery, account-scoped bounded disk caching and user-scroll/resize stability are included.

Validation passed: 70 Node and 58 Chromium tests; 557 Linux workspace and 63 Windows core tests; 118 Windows account/credential/source/storage tests; an explicit isolated Credential Manager parent/child roundtrip; 15 native IPC tests; Clippy, NSIS and actual Windows WebView/process-restart smoke. Root inspected the CI-built interface with synthetic data, including the 2000-work tail and decoded covers through narrow/wide resizing. The unsigned 0.3.1 installer artifact 10102381555 was downloaded and its ZIP, sole entry, version and SHA-256 verified. See [the repair batch report](LOCAL_WORKBENCH_FAVORITES_FIX_2026-09-09.md) for exact runs, hashes and boundaries.

Next: the user should install 0.3.1 and retest repaired real covers, continuous large favorites and Pica's oldest-first mode, then validate a personally chosen website favorite write. Real library inventory and durable download integration are later work. Keep production disabled. The following 0.3.0 sections are historical snapshots; their earlier pending-login statements are superseded by the user acceptance above.

## Third batch: account acceptance entry (2026-09-09)

The user requested the next development batch and will provide real accounts. Batch 3 implementation and automated verification are complete at `0bdfcda3896c4a829c5cff3a2aaa2c414a09a88f`, PR merge `36ba43f666f11d4b18d0c4f3338c1a84c8e48b0c`. See [the third-batch report](LOCAL_WORKBENCH_BATCH3_2026-09-09.md) for CI, package digest and live-acceptance boundaries. PR #19 remains a draft.

The Windows 0.3.0 application implements JM/Pica login and optional Credential Manager server-session persistence, bounded favorites/search/details/covers, verified individual favorite changes, account-scoped local following, and real-source booklist references. Library inventory/download queues are still simulated. Real account acceptance, actual source/CDN reachability and chosen website favorite mutations have not been performed. Enter credentials only in the local app; never commit credentials, account data or personal images. On-call request errors expose fixed codes, not raw responses.

Verification passed: 56 Node and 54 Chromium tests; 526 Linux workspace and 63 Windows core tests; 88 Windows account/credential/source/storage tests; an explicit isolated Windows Credential Manager parent/child-process roundtrip; 15 native IPC tests; Clippy; the NSIS installer and actual Windows WebView/process-restart smoke. Account implementation started at `12dbe5e`; native smoke/artifact fixes and the final source-page layout fix are included in the verified head. Root visual review used the CI-built UI with explicitly synthetic IPC, including right-aligned search, approved detail proportions, account settings, name-only author rows and 100-work selection. Real credentials and local native installation remain unverified. The report includes the verified installer under artifact 10097535446/run 34335090095. The package is downloaded locally, unsigned and not installed on the user machine.

Continue with user login via Settings → accounts, bounded read-only favorites/search/details, user-selected favorite action, optional saved-session restart, and a per-source acceptance record. Source network calls are hard-disabled in GitHub Actions; synthetic protocol/browser evidence is not live-account acceptance. Real library/inventory is the next implementation batch after this acceptance boundary. Keep production disabled and the original download/import authority gates intact.

The paragraphs below preserve earlier milestones; references to accounts being unimplemented describe those earlier batches.

## Current continuation

The new development session has taken over the existing checkout and retained the prior uncommitted implementation. See [the current acceptance record](LOCAL_WORKBENCH_ACCEPTANCE_2026-09-09.md). The mandatory external UI baseline is in the parent workspace, MANGAMONITOR_UI_BASELINE_2026-09-09.md; its specified PNGs and text corrections take priority over historical proposals and the old browser sample.

The first continuation batch restores the local-only homepage with recent/all sections, browse versus selection modes, list anchors through density/settings/detail navigation, full-scope synthetic selection, and page-scoped settings without discarding temporarily unreadable backgrounds. The second continuation batch adds a Windows Tauri development shell, native preferences/booklists persistence, a native background picker and cross-source local booklist management. See [the second-batch report](LOCAL_WORKBENCH_BATCH2_2026-09-09.md) for exact validation and unfinished scope. Real accounts and source-specific integrations remain unfinished.

Second-batch verification is complete at code commit `8183f6f` (PR merge `a1ee3c8` against main `69fcf1d`): 45 Node tests, 43 Chromium tests, 461 Linux workspace tests (including two deterministic Unix lock-release regressions), 63 Windows core tests, 23 Windows storage tests, 10 native IPC tests, Clippy, the NSIS build and actual Windows WebView/process-restart persistence all passed. Native file locks now explicitly unlock on scope exit, including error returns; cross-process BUSY and stale-revision protection remain enabled. The [report](LOCAL_WORKBENCH_BATCH2_2026-09-09.md) records the installer artifact and SHA-256. PR #19 remains a draft. User-machine installer/picker interaction, real accounts and the real execution chain remain separate acceptance work.

Current implementation task (2026-09-09): the user accepted the remaining UI after the following/settings review, with explicit corrections: author names without avatars or initial circles; remove ZIP packaging and image-processing tuning controls. The user said other parts were fine and to proceed. Frontend implementation has resumed. Do not generate or regenerate design preview images for this or future edits unless explicitly requested later.

The first implementation slice covers the common narrow navigation rail, vertical 5/7/9 cover grid, A/B custom-background preferences (initial B), name-only author rows, and page-scoped appearance/resource settings. Keep the existing single batch confirmation and simulated recovery behavior. Real accounts, source favorites, native tasks, library inventory and production remain separate unfinished integrations. See [the implementation and source-review record](LOCAL_WORKBENCH_IMPLEMENTATION_2026-09-09.md).

[Feature scope](LOCAL_WORKBENCH_FEATURE_SCOPE.md) and [UI structure](LOCAL_WORKBENCH_UI_STRUCTURE.md) retain both JM/Pica accounts and favorites, mixed local booklists, source-aware deduplication/review and discovery/rankings. The user subsequently removed optional PDF/CBZ export; the latest scope correction above supersedes that historical requirement. Backup/restore and bulk link-list import remain deferred; later content is handled as new works, without old-ZIP updates or replacement. Previous paragraphs in historical design artifacts describe earlier discussion stages and do not reinstate the superseded frontend pause.
The pre-downloader reliability batch was merged as PR #18, public main `69fcf1dd24526b5920406c9b97bb3e8b8909c6c6`. PR run `34245102484` passed 435 Linux workspace tests, Clippy and Windows safety/build checks; post-main run `34245838729` passed both jobs. The frontend discussion stop was reached before any desktop UI construction.

The user has now confirmed the first local workbench product decisions in [LOCAL_WORKBENCH_V1_DESIGN.md](LOCAL_WORKBENCH_V1_DESIGN.md): cover-library homepage, independent detail and queue pages, no V1 reader, one ZIP per work directly under the library root with internal chapter directories, automatic queue execution after confirmation, batch confirmation of download-and-import, and safe pause/exit with recovery on reopening. Normal tasks complete automatically; exceptions are handled individually.

The user then asked to begin the next step. `apps/local-workbench` now contains the first React/TypeScript browser interaction sample: a cover grid, independent detail page, batch confirmation, separate simulated queue, authors and settings. Original fictional covers and visibly labeled synthetic state require no service credentials or backend. Browser-local demo persistence is strictly separate from real state.

The `Local workbench UI` workflow validates the sample in GitHub Actions and publishes a static preview artifact. The accepted design is being implemented in bounded code changes, verified in GitHub Actions without generating new design previews. The second batch adds native preferences and booklists; real ZIP packaging, task coordination, execution recovery and cloud result publication remain work to implement and verify. This is not real-execution or production acceptance.

## Historical discussion stop and current boundary

The user requested continued project development and an explicit stop when the work reaches construction of the local downloader, so the frontend on their Windows computer can be discussed together.

The user previously authorized the first isolated sample, which was constructed. They subsequently paused further UI work to agree on features and redesign the visuals together. Existing local CLI binaries remain backend building blocks; the browser sample does not expose their authority or establish a delivered desktop application.

Production activation and real task execution retain their existing separate authority gates. This continuation does not create a genuine new-work task, approve one, issue a completeness certificate from incomplete inventory, or enable production.

## Verified starting point

- New public repository: `gouluanjiang/MangaMonitor-AI`.
- Starting main: `fdf5c4febe718d4e373101fe31a2e922737807c3`.
- A03 trusted scope certificates, inventory-satisfied download suppression, V1.7 inventory apply and V1.8 exact task completion are merged (#10, #14, #16, #17).
- Bootstrap `34220356044` is already accepted: six enabled authors, two batches, 399 physical requests; durable state commit `e12ccc66...`.
- Current durable state: six authors, 15 inventory works, 358 catalog entries and 358 identity review records; zero pending tasks and zero scope certificates. Production and materialization remain disabled.
- Starting CI `34235808662`: Linux workspace 416 tests plus Clippy passed; Windows selected safety tests/build passed. Independent Windows GNU workspace run: 415 passed, with the one-test difference explained by the Unix-only symlink test.
- Independent synthetic analysis reproduced two ambiguous AUTO_EXISTING bindings: `Maße/Masse` and `Cosmic Voyage 2 Extra/Cosmic Voyage Extra 2`. Downstream stayed review in those fixtures; the defect is identity binding.

## Completed pre-downloader implementation batch

Branch: `codex/pre-downloader-reliability`.

1. Correct title normalization/structural ambiguity and add production-analysis regressions; change the matcher version so previous automatic analysis is reconsidered.
2. Implement a read-only resume/recovery classification and explicit full recovery when current authority differs from an intact partial checkpoint. Corrupt checkpoints, invalid certificates and option mismatches remain refusal cases.
3. Serialize all production trigger variants in one non-cancelling concurrency group and cover development branches/security-relevant changes in CI.
4. Independently review the integrated result, run focused/full regressions and Clippy in GitHub Actions, then inspect Linux/Windows CI for the exact proposed commit. Local compilation was stopped after the user reported desktop slowdown. The user requested GitHub-first validation and gradual local work where necessary; follow the resource-aware 2-job/2-test-thread starting budget and conditional maximum of 4/4 in `AGENTS.md`, with only one heavy pipeline across agents.
5. Record the resulting status and stop before the local downloader frontend construction boundary.

## Acceptance criteria for this batch

- Ambiguous titles without independent mapping cannot become AUTO_EXISTING; ordinary exact titles and explicit existing/human authority remain usable.
- No live or replay caller implicitly starts fresh from an incomplete scan; only audited authority-drift recovery can force a new full scan.
- Recovery begins from current authoritative exports, keeps full mode across all batches and interrupted retries, and cannot promote partial evidence to historical full coverage.
- Pure recovery preflight performs no source requests, state writes or repair operations.
- Scheduled and manual production share one concurrency group regardless of source ref; validation/bootstrap keep their deliberate separate semantics. GitHub preserves the running job but can replace an older pending job with a newer trigger; this is serialization with coalesced pending triggers, not a durable queue of every invocation.
- No weakening of the production/materialization gates or local/cloud runtime isolation.
- Current baseline tests and added behavioral tests pass; any uncovered fault remains explicitly unresolved until fixed and retested.

## Remaining local implementation and acceptance work

- Implement the already accepted scope and visual baseline. Complete the remaining source-specific pages, account storage, real inventory adapters and task recovery; do not repeat design acceptance or reintroduce declined additions or old-ZIP updates.
- Integrate the existing local CLI stages with a bounded local controller; do not let a UI manufacture task authority.
- Implement the verified local inventory/completion publication bridge (Issue #7), including remote-base races and forward reconciliation of already-materialized content.
- Complete local import/staging interruption and lost-receipt recovery, plus full V1.7/V1.8 execution-chain tests.
- Establish complete local inventory evidence, trusted author certificates and one genuinely new approved task.
- Run one real add-only end-to-end acceptance and multiple no-change/failure/retry cycles; explicitly resolve remaining A07/A08/A14/A16 requirements.
- Present final production acceptance evidence before any production-enable decision.

## Recovery interface

`phase3b --resume-preflight` reads current public exports and a separate staged checkpoint and returns machine-readable classification. It must not create its output directory, repair inventory or contact either source. The production cycle only resumes `RESUMABLE_EXACT`, and only starts `--recover-authority-drift-full` for a validated `AUTHORITY_DRIFT_REQUIRES_FULL_RECOVERY`. Other classifications require resolving the reported state or options problem.

Full recovery starts at batch zero of the current author registry, reanalyzes retained identities, and remains full across later batches and interrupted retries. Its manifest records a new recovery generation plus compact links to prior checkpoint hashes and Git bases. Original durable checkpoints remain available in Git history. The next ordinary monthly cycle may resume incremental behavior only after this recovery finishes.

Legacy checkpoints must retain their original manifest binding. Compatibility tests include the actual committed bootstrap state shape, including its missing pre-A03 authority-hash field. Unsupported fields or corrupt bindings must still be refused.

## Historical validation method for PR #18

Earlier local focused runs and code review found additional regressions, so they were not acceptance of the final patch. After desktop resource pressure was reported, all local Rust processes were stopped. The final branch passed GitHub's Linux workspace tests, Clippy, shell orchestration checks and Windows safety/build job before merge, followed by successful post-main CI. The associated PR check suite records exact commits and results; do not substitute the earlier baseline CI.

That historical pre-downloader batch did not construct the frontend, execute real manga tasks, publish business state or activate production. Its acceptance did not close final V1 end-to-end or long-running acceptance.

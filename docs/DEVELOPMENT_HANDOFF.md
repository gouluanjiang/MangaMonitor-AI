# Active development handoff

Updated 2026-09-09. This document separates the current continuation from historical milestone reports.

## Current continuation

The new development session has taken over the existing checkout and retained the prior uncommitted implementation. See [the current acceptance record](LOCAL_WORKBENCH_ACCEPTANCE_2026-09-09.md). The mandatory external UI baseline is in the parent workspace, MANGAMONITOR_UI_BASELINE_2026-09-09.md; its specified PNGs and text corrections take priority over historical proposals and the old browser sample.

The first continuation batch restores the local-only homepage with recent/all sections, browse versus selection modes, list anchors through density/settings/detail navigation, full-scope synthetic selection, and page-scoped settings without discarding temporarily unreadable backgrounds. The second continuation batch adds a Windows Tauri development shell, native preferences/booklists persistence, a native background picker and cross-source local booklist management. See [the second-batch report](LOCAL_WORKBENCH_BATCH2_2026-09-09.md) for exact validation and unfinished scope. Real accounts and source-specific integrations remain unfinished.

Second-batch verification is complete at code commit `8183f6f` (PR merge `a1ee3c8` against main `69fcf1d`): 45 Node tests, 43 Chromium tests, 461 Linux workspace tests (including two deterministic Unix lock-release regressions), 63 Windows core tests, 23 Windows storage tests, 10 native IPC tests, Clippy, the NSIS build and actual Windows WebView/process-restart persistence all passed. Native file locks now explicitly unlock on scope exit, including error returns; cross-process BUSY and stale-revision protection remain enabled. The [report](LOCAL_WORKBENCH_BATCH2_2026-09-09.md) records the installer artifact and SHA-256. PR #19 remains a draft. User-machine installer/picker interaction, real accounts and the real execution chain remain separate acceptance work.

Current implementation task (2026-09-09): the user accepted the remaining UI after the following/settings review, with explicit corrections: author names without avatars or initial circles; remove ZIP packaging and image-processing tuning controls. The user said other parts were fine and to proceed. Frontend implementation has resumed. Do not generate or regenerate design preview images for this or future edits unless explicitly requested later.

The first implementation slice covers the common narrow navigation rail, vertical 5/7/9 cover grid, A/B custom-background preferences (initial B), name-only author rows, and page-scoped appearance/resource settings. Keep the existing single batch confirmation and simulated recovery behavior. Real accounts, source favorites, native tasks, library inventory and production remain separate unfinished integrations. See [the implementation and source-review record](LOCAL_WORKBENCH_IMPLEMENTATION_2026-09-09.md).

[Feature scope](LOCAL_WORKBENCH_FEATURE_SCOPE.md) and [UI structure](LOCAL_WORKBENCH_UI_STRUCTURE.md) retain both JM/Pica accounts and favorites, mixed local booklists, source-aware deduplication/review, discovery/rankings and optional PDF/CBZ export. Backup/restore and bulk link-list import remain deferred; later content is handled as new works, without old-ZIP updates or replacement. Previous paragraphs in historical design artifacts describe earlier discussion stages and do not reinstate the superseded frontend pause.
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

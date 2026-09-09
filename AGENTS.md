# MangaMonitor-AI development guardrails

These instructions apply to future automated or human development work in this repository.

## Current execution policy

- Cloud/GitHub Actions monitoring has priority.
- On 2026-09-08 the user explicitly thawed the **smallest V1 add-only local real-download path** after the cloud production-readiness milestone. The thaw is limited to currently approved `download` tasks for genuinely new works, real JM/Pica retrieval, and command-owned isolated staging/proof generation.
- `upgrade`, replacement, overwriting an existing destination, staging-to-library promotion, inventory mutation, task completion, physical deletion, and broader local execution remain outside this thaw unless their separate roadmap gates are completed.
- The real local executor must continue to refuse GitHub Actions runtime execution. Real media downloads are local-only and must remain bound to the current task revision and current user-approval generation.
- `production_enabled=false` must remain closed unless a later, explicit production-acceptance decision changes it.
- Do not weaken fail-closed behavior, approval-generation checks, staging isolation, inventory gates, task-revision binding, replacement gates, or physical-delete gates merely to match an upstream downloader.

## Test execution and desktop resource policy

- On 2026-09-09 the user accepted the remaining UI direction with two corrections: author following uses names without avatars/initials, and download settings do not expose ZIP packaging or image-processing controls. Proceed with frontend implementation from the accepted design. Do not generate or regenerate design preview images for this or future changes unless the user later explicitly requests them. Necessary source image restoration, archive validation and execution gates are not removed by this UI simplification.

- The user requires non-duplicating local/cloud work. Before execution, assign each check or suite for the same code revision and target to exactly one location, including automatic CI triggers. Choose local work only when measured resources permit and it reduces turnaround or repeated diagnosis. If CI already runs a check, do not also run it locally; reserve local work for distinct reproductions, diagnostics or checks that CI will not repeat. Full workspace/cross-platform validation, clean/release builds, installers and long soak runs remain in GitHub Actions. Do not run the same suite on both sides for reassurance, and do not weaken required checks to avoid duplication. Batch documentation-only updates with the next necessary push instead of needlessly triggering another unchanged build.
- Measure available RAM, CPU and free disk before local compilation. Require at least 4 GiB available RAM and no sustained high CPU load. Run only one local build/test pipeline across all agents, starting with `CARGO_BUILD_JOBS=1` and `--test-threads=1`; at least 6 GiB free RAM and an otherwise idle desktop may justify at most 2 build jobs/test threads. Use below-normal priority where supported. These settings are concurrency/priority controls, not hard CPU or memory caps.
- Below 2 GiB available RAM, keep local work to minimal file operations and existing-binary checks; defer compilation, full browser suites and additional preview processes to CI. If local heavy work is active, pause it when RAM drops below 2 GiB, total CPU stays above 80% for about 30 seconds, or the user reports lag. Do not stop unrelated user processes. Recheck resources before resuming; avoid growing large caches on a constrained system drive. Cache migration/deletion remains separate scoped work.

## Mandatory gate before any real-download work

Before implementing, enabling, modifying, or reviewing any code that can perform real JM/Pica manga download execution, media materialization to the user's library, staging-to-library promotion, replacement, or deletion:

1. Read `docs/DOWNLOAD_EXECUTOR_THAW_GATE.md` in full.
2. Read `docs/upstream-source-reference.md` in full.
3. Re-open and compare the pinned upstream source revisions recorded there:
   - `hect0x7/JMComic-Crawler-Python`
   - `lanyeeee/jmcomic-downloader`
   - `lanyeeee/picacomic-downloader`
4. Re-check the current MangaMonitor-AI safety contracts and tests before adapting upstream download behavior.
5. Treat upstream downloader completion semantics as implementation references only; they do not override MangaMonitor-AI completion, approval, staging, inventory, promotion, replacement, or deletion authority.

The 2026-09-08 V1 add-only thaw review is recorded in `docs/V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md`. Future work inside that exact already-reviewed add-only staging scope may rely on the recorded gate only while the pinned upstream revisions and the relevant safety contracts remain unchanged. Any upstream-pin change, expansion to `upgrade`/replacement, library materialization, inventory mutation, task completion, deletion, or other authority expansion requires the applicable gate to be re-run in that development session.

## Upstream reuse policy

Protocol and reliability ideas may be adopted when compatible with the repository's pinned/fail-closed model. Preserve license notices for substantial copied upstream code. Dynamic trust expansion, hidden retries that evade request accounting, permissive pagination, automatic overwrite/replacement, or automatic unavailable/deletion inference are not acceptable substitutes for the project's stricter evidence model.

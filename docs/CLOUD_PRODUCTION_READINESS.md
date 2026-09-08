# Cloud production readiness

Status: **READY FOR V1 CONTINUATION — PRODUCTION STILL DISABLED**

This document records the cloud-monitoring readiness milestone after the bounded three-author dual-source runner was integrated into `main`.

Readiness here means that the cloud/GitHub Actions half of MangaMonitor-AI has enough evidence for V1 continuation. It does **not** authorize production monitoring, library mutation, replacement, or deletion.

## Accepted cloud baseline

- Main integration commit: `2c8223d1cad85b7ab034034fb5ef730b032549d5`
- Integration PR: `#34` — bounded three-author concurrency in the formal Phase3B runner
- Hardened live validation run: `34154996421`
- Sanitized evidence artifact: `phase3b-three-author-dual-source-34154996421-1`
- `production_enabled=false` remains unchanged

## Production runner shape

The formal Phase3B live-source runner now supports the conservative V1 target:

```text
at most 3 active authors concurrently
        ↓
for each author: JM || Pica
        ↓
pagination/detail remain sequential inside each source chain
        ↓
shared physical-request budget
        ↓
buffered source observations
        ↓
deterministic author → JM → Pica durable application
        ↓
existing single-writer Phase3B state pipeline
```

Operational constraints remain intentional:

- `--author-concurrency` is bounded to `1..=3`.
- Concurrent author execution is allowed only for live Phase3B runs.
- Serial execution remains available as fallback, replay oracle, and regression baseline.
- Pica authentication is prepared sequentially rather than burst concurrently.
- Durable catalog/review/pending/checkpoint mutation is single-writer and ordered.
- Physical source attempts remain accounted through `RequestTrace` and the shared request budget.
- Source/auth/network failures fail closed and never certify unavailability or removal.

## Validation evidence

The hardened live workflow completed successfully for three confirmed JM/Pica authors: `2-G`, `3104`, and `haruhisky`.

The same validation job proved all of the following:

- serial three-author Phase3B baseline completed successfully;
- formal `--author-concurrency 3` live runner completed successfully;
- recorded concurrent observations replayed successfully through the deterministic Phase3B writer;
- concurrent business state and search evidence matched the accepted serial baseline;
- deliberate Pica source failure remained fail-closed and the same output resumed cleanly after credentials were restored;
- shared request-budget exhaustion stayed within the physical request cap, remained incomplete, and created no false unavailable/removal inference;
- two additional consecutive formal live runs completed successfully;
- all three concurrent live rounds remained equivalent and clean.

Sanitized stability evidence:

| Measure | Result |
| --- | --- |
| Authors | `2-G`, `3104`, `haruhisky` |
| Author concurrency | `3` |
| Serial wall time | `381323 ms` |
| Concurrent wall times | `156670 ms`, `173008 ms`, `171460 ms` |
| Physical requests per concurrent round | `187`, `187`, `187` |
| Clean live rounds | `3` |
| Failure/resume verified | `true` |
| Budget exhaustion verified | `true` |

The speed improvement is useful but is not the acceptance criterion. Equivalence, bounded requests, fail-closed behavior, resumability, and repeatability are the reasons this milestone is accepted.

## Request-budget boundary semantics

Concurrent execution does not promise a particular network completion order at the exact instant a deliberately small shared request budget is exhausted. The production safety contract at that boundary is instead:

1. the physical request limit is never crossed;
2. the affected scan remains incomplete rather than becoming falsely `COMPLETE`;
3. source failure/budget checkpoint state cannot imply unavailability or deletion;
4. the incomplete run remains resumable;
5. completed normal-budget runs remain equivalent to the serial business-state baseline.

This is an intentional fail-closed checkpoint contract. Exact byte-identical partial progress at an arbitrary concurrent budget cut-off is not required for V1 correctness; final completed/resumed state remains subject to the normal equivalence and safety gates.

## CI and isolation requirements

The accepted cloud baseline must continue to preserve these guards:

- workspace tests and Clippy pass;
- assistant-facing state readers remain offline/read-only where specified;
- assistant publication remains state-only;
- cloud workflows do not invoke the local manga executor, importer, inventory rescan, candidate builder, or inventory-apply authorization path;
- Windows local-executor safety regressions continue to pass;
- the production gate remains closed while `monitor-config.json` has `production_enabled=false`.

## Readiness decision

With PR `#34` integrated and the hardened live validation passing, roadmap steps 1–4 are complete:

1. bounded three-author concurrency is integrated into the real Phase3B runner/scheduler;
2. concurrent failure isolation, resume, and shared-budget exhaustion regressions pass;
3. repeated live-source soak on the formal runner passes with serial/replay equivalence;
4. the cloud implementation is ready for the next V1 stage while production remains disabled.

## Local V1 transition — add-only staging thaw authorized

On 2026-09-08 the user explicitly authorized the repository's smallest V1 add-only real-download thaw. The mandatory thaw review was therefore executed in the same development session and recorded in:

- `docs/V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md`
- `docs/V1_ADD_ONLY_STAGING_ACCEPTANCE.md`

All three recorded upstream pins were reopened and their default branch heads still matched the pinned revisions. Existing A6.10–A6.15 source/media/staging code was retained rather than replaced with broader upstream filesystem semantics.

The thawed live authority is deliberately narrow:

```text
current user-approved new-work `download`
        ↓
real JM/Pica source + media reads on the local machine
        ↓
fresh commands/<command_id> isolated staging
        ↓
manifest + filesystem proof + verified execution receipt
```

The runnable queue and immediate live source-preflight barrier now both exclude `upgrade` and tasks already bound to old local items. The local executor continues to refuse GitHub Actions runtime execution.

Still not authorized:

- replacement or overwrite;
- staging-to-library materialization/promotion;
- inventory mutation;
- task completion;
- physical deletion;
- production enablement.

## Next gate — one real local staging acceptance

The repository-side thaw is complete once its tests/CI are accepted and merged. The next gate is not another code-design decision: it is one guarded real staging-only execution on the user's local machine using a **current, genuinely approved new-work task**.

Use `scripts/Run-V1AddOnlyStagingAcceptance.ps1`; do not bypass it by constructing a command manually. The wrapper verifies current queue membership, unchanged monitor state, fresh command-owned staging, complete receipt/proof, and zero downstream authority.

The repository's currently committed `monitor-state/pending.json` has no pending tasks at this transition, so the real staging acceptance must wait for an actual new-work task to be generated and approved rather than using a fabricated task to claim acceptance.

Until that real local acceptance succeeds, the correct state is:

```text
cloud production-ready
V1 add-only local source/media/staging path = thawed
real staging acceptance = pending a genuine approved task
inventory/library/task completion authority = closed
production_enabled = false
```

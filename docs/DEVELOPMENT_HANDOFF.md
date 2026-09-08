# Active development handoff

Updated 2026-09-08. This document separates the current continuation from historical milestone reports.

## User-requested stop boundary

The user requested continued project development and an explicit stop when the work reaches construction of the local downloader, so the frontend on their Windows computer can be discussed together.

Continue independently on cloud scanning, deterministic matching, state correctness, backend regression tests and GitHub PR/CI repair. **Do not scaffold, choose a frontend framework for, package, install, or start constructing the local downloader UI before that discussion.** Existing local CLI binaries are backend building blocks, not a delivered desktop application. Preparing a factual interface inventory is allowed; it is not a frontend implementation decision.

Production activation and real task execution retain their existing separate authority gates. This continuation does not create a genuine new-work task, approve one, issue a completeness certificate from incomplete inventory, or enable production.

## Verified starting point

- New public repository: `gouluanjiang/MangaMonitor-AI`.
- Starting main: `fdf5c4febe718d4e373101fe31a2e922737807c3`.
- A03 trusted scope certificates, inventory-satisfied download suppression, V1.7 inventory apply and V1.8 exact task completion are merged (#10, #14, #16, #17).
- Bootstrap `34220356044` is already accepted: six enabled authors, two batches, 399 physical requests; durable state commit `e12ccc66...`.
- Current durable state: six authors, 15 inventory works, 358 catalog entries and 358 identity review records; zero pending tasks and zero scope certificates. Production and materialization remain disabled.
- Starting CI `34235808662`: Linux workspace 416 tests plus Clippy passed; Windows selected safety tests/build passed. Independent Windows GNU workspace run: 415 passed, with the one-test difference explained by the Unix-only symlink test.
- Independent synthetic analysis reproduced two ambiguous AUTO_EXISTING bindings: `Maße/Masse` and `Cosmic Voyage 2 Extra/Cosmic Voyage Extra 2`. Downstream stayed review in those fixtures; the defect is identity binding.

## Current implementation batch

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

## Remaining work after frontend discussion

- Agree on local UI, task/approval screens, state refresh, staging/library paths, credential storage, progress/errors and receipt/recovery presentation.
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

## Validation record and merge requirements

Earlier local focused runs and code review found additional regressions, so they are not acceptance of the final patch. After desktop resource pressure was reported, all local Rust processes were stopped. The final branch must pass GitHub's Linux workspace tests, Clippy, shell orchestration checks and Windows safety/build job before merge. The associated PR check suite records exact commits and results; do not substitute the earlier baseline CI.

No frontend construction, real manga execution, business-state publication or production activation is part of this batch. Passing this batch does not close final V1 end-to-end or long-running acceptance.

# MangaMonitor-AI

MangaMonitor-AI is a safety-first manga monitoring and automation system for JM and Pica. The project is no longer targeting an indefinitely expanding collection of isolated features: the immediate release goal is a narrow V1 that can run the complete real-world pipeline safely and repeatedly, from cloud discovery to verified local inventory completion.

This repository has moved far beyond the original Phase 1A smoke test. The README is the primary project handoff and roadmap for new development sessions. Always combine it with the current repository state, `AGENTS.md`, current tests, and the safety documents referenced below; current code and GitHub state win if any historical note becomes stale.

## V1 product target

V1 is considered useful only when this complete chain can run stably for long periods without routine human intervention:

```text
long-running unattended system
        ↓
cloud detects a new manga / update
        ↓
identity and state are determined correctly
        ↓
a unique, correct pending task is created
        ↓
local executor performs the real download
        ↓
downloaded output is verified
        ↓
content is added safely to the real manga library
        ↓
inventory is updated and re-verified
        ↓
the current task revision is marked completed
        ↓
later scans do not recreate or redownload the same work
```

V1 intentionally optimizes for a small, safe, complete vertical slice rather than feature breadth. The governing principle is:

> Prefer leaving work pending for review or retry over performing an automatic action that cannot be proven safe.

### V1 scope: add-only first

The first production release should support only the safest add-only cases:

- clearly identified new works;
- clearly safe additive content that does not require overwriting an existing library target;
- guarded task creation and approval;
- real download into isolated command staging;
- filesystem/coverage/source/task-revision verification;
- add-only materialization into the real library;
- atomic/retry-safe inventory update;
- inventory re-read plus filesystem verification;
- task completion only after all required evidence is current and valid.

The following are explicitly **not required for V1** and should not distract from closing the vertical pipeline:

- automatic replacement of an existing library work;
- overwriting existing files;
- automatic deletion or cleanup of library works;
- interpreting `UNKNOWN` as permission to remove anything;
- complex replacement/version preference policies;
- 5/10/20-author concurrency experiments;
- advanced UI, notification, or library-maintenance features.

Replacement, deletion, larger concurrency, and advanced maintenance can be added after V1 is operating successfully in real use.

## Current status

| Area | Current state |
| --- | --- |
| JM/Pica metadata scan | Implemented and live-source validated |
| Full / incremental / monthly scan modes | Implemented |
| Persistent catalog, review, decisions, pending tasks | Implemented |
| Assistant read-only views / review exports | Implemented |
| Discovery → review → human decision → pending E2E | Implemented as an offline regression |
| Resume/checkpoint and replay | Implemented; fail-closed |
| Physical request accounting | Explicit `RequestTrace` accounting with per-batch budget |
| Single-author JM+Pica concurrent acquisition | Implemented and retained as validation/fallback evidence |
| Three-author bounded concurrent acquisition | **Integrated into the formal Phase3B runner/scheduler; three clean live soak rounds plus failure/resume and budget-exhaustion regressions passed** |
| Cloud production readiness | **Reached; production activation remains deliberately disabled** |
| Local staging/execution proof chain | **Repository thaw gate accepted for approved new-work `download` tasks; real local staging acceptance still requires one genuine current approved task** |
| Inventory mutation | V1.6 is authorization-only; it does not write `inventory_index.json`; real materialization/apply remains outside the accepted thaw |
| Replacement / physical deletion | Not authorized and not required for V1 |
| Production monitor | **Disabled**: `production_enabled=false` |

The cloud-monitoring half has reached the V1 production-readiness milestone, and the smallest V1 add-only source/media/staging path was explicitly thawed on 2026-09-08 after the mandatory upstream and safety review. The thaw is intentionally narrow: it permits real JM/Pica retrieval only for a current user-approved genuinely new `download` task and only into fresh command-owned staging. Library materialization, inventory mutation, task completion, replacement, deletion, and production enablement remain separate closed authorities.

The current committed `monitor-state/pending.json` contains no tasks. Therefore the next real acceptance step cannot be fabricated: the project is waiting for a genuine new-work task to be produced and approved before running the guarded local staging acceptance. See `docs/V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md` and `docs/V1_ADD_ONLY_STAGING_ACCEPTANCE.md` for the accepted boundary and procedure. Cloud readiness evidence remains in `docs/CLOUD_PRODUCTION_READINESS.md`.

## V1 execution order

Development should proceed in this order. The cloud milestones and repository-side add-only thaw review are complete; the first real staging acceptance remains evidence-gated:

1. ✅ **Integrate bounded three-author concurrency into the real Phase3B runner/scheduler.**
2. ✅ Add concurrent failure-isolation, resume, and shared-budget exhaustion regressions.
3. ✅ Run repeated live-source soak on the real runner and confirm deterministic equivalence and fail-closed behavior.
4. ✅ Reach cloud production-readiness while keeping `production_enabled=false`.
5. ✅ After an **explicit user thaw decision**, execute the repository's mandatory real-download thaw procedure.
6. ✅ Re-review the pinned JM/Pica upstream implementations and the current local executor before changing real-download behavior.
7. ✅ Restore/accept only the real-download capabilities required by the V1 add-only source→media→isolated-staging pipeline.
8. 🟨 Complete `task → command → isolated staging → manifest/proof/receipt` with one guarded **real local** download; repository implementation is ready, but acceptance is waiting for a genuine current approved new-work task.
9. ⬜ After that real staging acceptance, implement a narrow add-only library materialization/inventory apply executor.
10. ⬜ Re-read inventory and filesystem state and allow task completion only after the current evidence is verified.
11. ⬜ Run real end-to-end crash/retry/idempotence soak.
12. ⬜ Perform a final V1 production acceptance review.
13. ⬜ Change `production_enabled` only after an explicit user decision to enable production.

Do **not** automatically move from three-author concurrency to 5/10/20-author tests merely because three-author operation is fast. Three active authors are the conservative V1 target. Throughput is secondary to correctness and recoverability.

## V1 acceptance criteria

Passing unit tests alone is not V1 completion. The release must prove the real vertical pipeline:

```text
scheduled cloud scan
        ↓
real new work is discovered
        ↓
exactly one appropriate pending task is created
        ↓
a valid approved task is consumed locally
        ↓
real JM/Pica content is downloaded into isolated staging
        ↓
download proof / expected coverage verification passes
        ↓
add-only library materialization succeeds
        ↓
inventory write succeeds atomically
        ↓
inventory is re-read and reconciled with the filesystem
        ↓
the current task revision becomes completed
        ↓
a later scan creates no duplicate task
        ↓
a later local cycle performs no duplicate download or library add
```

The final soak must exercise at least these classes of behavior without weakening any safety gate:

- normal new-work discovery and completion;
- repeated scans of the same work;
- repeated execution attempts for the same task;
- source/network failures;
- authentication failures;
- partial source failures;
- download interruption;
- partial staging output;
- process crash/restart;
- request-budget exhaustion;
- inventory write failure;
- crash after inventory success but before task completion;
- task revision changing during execution;
- approval generation changing during execution;
- resume and retry;
- idempotent recovery.

Success means the system does not duplicate downloads, duplicate library entries, incorrectly complete tasks, overwrite an existing target, infer destructive action from uncertainty, or leave inventory permanently inconsistent with the filesystem.

## Intended monitoring flow

```text
author registry
    ↓
JM + Pica metadata acquisition
    ↓
search pagination + selective detail fetch
    ↓
persistent source catalog
    ↓
deterministic matcher
    ├─ known / unchanged
    ├─ review required ──→ human decision
    └─ proven new / actionable candidate
                         ↓
                    pending task
                         ↓
              guarded local pipeline
```

Creating a task is not completion. A successful staging execution is not inventory mutation. Inventory verification is not task completion. Promotion, replacement, deletion, and production enablement remain separate authorities.

## Scan model

### Full scan

A full scan walks the available search pagination for the selected authors and sources. A first full bootstrap is intentionally the most request-heavy case because previously unseen source IDs normally require detail retrieval before they can be cataloged and analyzed.

### Incremental / monthly scan

`monthly` maps to the incremental strategy. The scanner starts from the newest search pages and can stop after the configured historical-ID streak is reached. The default historical threshold is currently `5`.

The scanner distinguishes two different ideas:

- **coverage complete**: the source pagination was actually exhausted for the selected scope;
- **strategy complete**: the requested scan strategy finished successfully, including a valid incremental early stop.

An incremental early stop may therefore advance the cycle without pretending that all historical pages were re-enumerated. This distinction is required to keep removal/unavailability logic fail-closed.

### Detail refresh

New or changed search records may require a detail call. Existing pending/serial catalog entries can also require direct detail rechecks. Work skipped because a request budget is exhausted must remain pending for resume rather than being silently treated as completed.

## Request and source policy

The project deliberately favors stability and auditability over maximum throughput.

- Every physical JM/Pica request currently includes an intentional random 1–3 second delay.
- The 1–3 second pacing is a conservative policy, **not** a measured or proven optimal source rate limit.
- Physical attempts are recorded in `RequestTrace` and count against the request budget.
- The default Phase3B budget is `400` physical requests per batch.
- JM uses a fixed, pinned domain set and explicit failover only for approved transient/domain failures.
- Dynamic remote domain discovery is not trusted.
- HTTP redirects are not silently followed.
- Retries/failovers must not be hidden from request accounting.
- Pica incomplete pagination and other source/protocol ambiguity fail closed.
- Authentication/network/HTTP errors never prove that a manga is unavailable.

Current scaling configuration lives in `monitor-config.json`:

```json
{
  "production_enabled": false,
  "default_mode": "monthly",
  "historical_id_threshold": 5,
  "soft_batch_size": 5,
  "max_requests_per_batch": 400,
  "live_canary_size": 6
}
```

`soft_batch_size` is a scheduling/batching value, **not** an author-concurrency value. The production Phase3B cycle must not be interpreted as "five authors all fire requests simultaneously" merely because the soft batch size is five.

## Concurrency status and production target

Concurrency is deployed conservatively in the formal Phase3B runner and preserves the deterministic state writer.

The retained single-author acquisition shape is:

```text
one author
  ├─ JM source chain
  └─ Pica source chain
        ↓
 deterministic observation tape
        ↓
 existing Phase3B state writer
```

The accepted V1 three-author runner shape is:

```text
at most three active authors
        ↓
for each author: JM || Pica
        ↓
pagination/detail remain sequential inside each source chain
        ↓
shared physical-request budget
        ↓
observation collection
        ↓
deterministic author → JM → Pica ordering
        ↓
existing Phase3B state writer
```

Pica authentication must not be turned into an uncontrolled login burst. Concurrent acquisition must not directly mutate durable `catalog`, `events`, `review`, `pending`, checkpoint, or inventory state. Durable state application remains deterministic and ordered.

Production-adoption validation now proves:

- one author's source failure does not corrupt or discard successful observations from other authors;
- the failed source remains resumable and does not imply unavailability/removal;
- concurrent resume does not duplicate events/tasks or lose pending direct checks;
- shared request-budget exhaustion never crosses the physical-request limit and never creates false `COMPLETE` boundaries;
- replay/business-state results remain equivalent to the accepted serial baseline;
- three repeated live-source formal-runner rounds remained stable and equivalent.

The accepted evidence is recorded in `docs/CLOUD_PRODUCTION_READINESS.md`. Speed alone remains insufficient reason to increase concurrency beyond the V1 target.

## Repository layout

```text
crates/
  state-model/     shared identities, records, coverage, request traces
  rules-core/      deterministic title/version/coverage rules
  jm-adapter/      pinned JM metadata/source protocol implementation
  pica-adapter/    pinned Pica authentication and metadata/source protocol
  cloud-monitor/   Phase3B runner, matcher, persistence, assistant views,
                   guarded local safety stages, validation binaries

monitor-state/     current durable monitor state tracked by this repository
fixtures/          regression and matcher/source fixtures
scripts/           Phase3B cycle/state publication and local validation helpers
.github/workflows/ CI, Phase3B validation, live canaries, concurrency probes
third-party/       retained upstream license/reference material
docs/              current safety/upstream/thaw/readiness documentation
```

Historical `ASSISTANT_*` result documents remain as an audit trail. They are not a substitute for the current code, current tests, this V1 roadmap, or the guardrails in `AGENTS.md`.

## Clean checkout: offline verification

Rust is pinned by `rust-toolchain.toml` to the same toolchain used by CI.

From a clean checkout:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Inspect the current state without network access or mutation:

```bash
cargo run --locked -p cloud-monitor --bin assistant-view -- \
  --state monitor-state --view scan-summary

cargo run --locked -p cloud-monitor --bin assistant-view -- \
  --state monitor-state --view review-summary

cargo run --locked -p cloud-monitor --bin assistant-view -- \
  --state monitor-state --view review-batch --limit 10

cargo run --locked -p cloud-monitor --bin assistant-view -- \
  --state monitor-state --view pending

cargo run --locked -p cloud-monitor --bin assistant-view -- \
  --state monitor-state --view collection
```

`assistant-view` is a read-only/offline state reader and is checked as such in baseline CI.

### Plan a human review decision without mutating durable state

Take a `review_id` from `review-batch`, then write a decision plan into a new output directory:

```bash
cargo run --locked -p cloud-monitor --bin assistant-decision-edit -- \
  --state monitor-state \
  --output reports/demo-decision \
  --review-id '<review-id>' \
  --decision ignore
```

Supported decisions are `same`, `not-same`, and `ignore`; `same` may require `--work-id`. This command writes a proposed `decisions.json`, an audit record, and a reanalysis preview to the new output directory. It does not modify `monitor-state`.

## GitHub Actions

Important workflows include:

- `ai-ci.yml` — locked workspace tests, Clippy, read-only assistant checks, cloud/local isolation, scaling guards, and the closed production gate on Linux plus local safety regressions on Windows.
- `phase3b.yml` — Phase3B validation and the production-cycle entrypoint. Its formal live runner is wired for bounded three-author concurrency; scheduled/explicit production execution remains blocked while `production_enabled=false`.
- `phase3b-live-canary.yml` — registry-driven JM+Pica live-source canary using repository secrets for Pica authentication.
- `phase3b-single-author-dual-source.yml` — retained live validation of one-author JM/Pica concurrent acquisition and deterministic replay.
- `phase3b-three-author-dual-source.yml` — regression/equivalence/soak evidence for the adopted three-author formal runner, including failure/resume and shared-budget checks.
- `assistant-state-publish.yml` — assistant-facing state publication path; it must remain state-only and must not invoke live source scanning or the local executor.

Credentials belong in GitHub Actions Secrets or the process environment. Never commit, print, artifact, or pass Pica credentials as ordinary CLI arguments.

## Safety invariants

The following are intentional project rules, not temporary implementation limitations:

1. `UNKNOWN` does not automatically remove an existing work.
2. Pica incomplete pagination fails closed.
3. An existing work is not upgraded merely because a file/source appears larger.
4. Unreliable size data is not treated as authoritative comparison evidence.
5. Source/network/auth failures do not certify unavailability.
6. Task creation does not mean task completion.
7. Completion remains bound to the current task revision, source/site ID, target/local identity, and coverage/proof evidence.
8. Command staging is isolated under `commands/<command_id>`.
9. Staging does not overwrite an existing destination.
10. Partial staging output is not automatically deleted.
11. Staging success does not imply inventory mutation, task completion, promotion, replacement, or deletion.
12. Production remains disabled until a separate final acceptance decision explicitly enables it.

Cloud workflows must not invoke the Windows/local manga executor, importer, inventory rescan, candidate builder, or inventory-apply gate.

## Local executor and inventory policy

The repository contains a substantial guarded local safety chain through A6.15, and the 2026-09-08 thaw review accepted the smallest V1 add-only real-download scope. The currently thawed authority is only:

```text
current approved genuinely-new `download` task
        ↓
real JM/Pica source + media reads on the local machine
        ↓
fresh commands/<command_id> isolated staging
        ↓
manifest / filesystem proof / verified execution receipt
```

The real local executor remains forbidden in GitHub Actions. `upgrade`, library materialization/promotion, inventory mutation, task completion, replacement, overwrite, physical deletion, and production enablement are **not** implied by the thaw and remain closed until their separate roadmap gates are satisfied.

Before modifying the accepted add-only source/media/staging scope, follow `AGENTS.md` and the recorded thaw contract. Any upstream-pin change or authority expansion to library materialization, inventory mutation, task completion, replacement, or deletion requires the applicable gate to be re-run in that development session.

### Inventory apply target for V1

V1.6 is an inventory-apply **authorization gate only**. It does not write `inventory_index.json`.

Per the accepted thaw contract, the project must first pass one guarded real local staging-only acceptance using a genuine current approved new-work task. Only after that evidence exists should development cross into real add-only library materialization/inventory mutation.

The next V1-local transition should remain deliberately narrow and add-only. A real inventory-apply executor must, at minimum:

- reject an existing destination rather than overwrite it;
- use crash-safe/atomic filesystem and inventory operations where applicable;
- be retry-safe and idempotent for the same current task revision;
- write inventory only after required staging/proof checks pass;
- re-read inventory and verify the real filesystem afterward;
- leave the task incomplete on any inventory/materialization verification failure;
- recover safely when the process stops between materialization, inventory update, and task completion.

Replacement and physical deletion are not part of this V1 transition.

### Task completion target for V1

A task may become `completed` only when all required current evidence remains valid, including:

```text
current task revision
+
current approval generation
+
source/site identity
+
staging/download proof
+
expected file/chapter coverage
+
add-only real-library materialization
+
inventory write
+
inventory re-read and filesystem verification
```

If any step fails or becomes stale, the task remains incomplete and recoverable. Retry must not duplicate library content.

## Key documentation

- `README.md` — V1 product target, current roadmap, production acceptance definition, and new-session handoff.
- `AGENTS.md` — repository-wide development guardrails and the currently thawed authority boundary.
- `docs/CLOUD_PRODUCTION_READINESS.md` — accepted cloud runner/soak/failure/resume/budget readiness evidence.
- `docs/V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md` — accepted repository-side thaw review and exact add-only staging authority.
- `docs/V1_ADD_ONLY_STAGING_ACCEPTANCE.md` — guarded procedure for the first real local staging-only acceptance.
- `docs/upstream-source-reference.md` — pinned upstream implementation/reference notes.
- `docs/DOWNLOAD_EXECUTOR_THAW_GATE.md` — mandatory gate procedure for real-download work and authority expansion.
- `ASSISTANT_A6_15_RESULTS.md` — documented verified staging execution receipt bridge.
- `ASSISTANT_V1_6_RESULTS.md` — documented inventory-apply authorization boundary.
- `monitor-config.json` — current production gate and Phase3B scaling defaults.

Older phase/result documents are retained for traceability, but when they conflict with current code, `AGENTS.md`, current tests, current configuration, or a later explicitly accepted V1 contract, the current sources are authoritative.

## New development-session handoff

A new development conversation should not require a large copied prompt. Start by reading this README and then obtain the **current real GitHub state** before doing work.

At the start of a new session:

1. Read `README.md` and `AGENTS.md`.
2. Fetch the current `main` head, open PRs/branches relevant to the current work, and recent GitHub Actions results.
3. If real-download/local execution work is about to begin, read the current thaw/upstream documents required by `AGENTS.md` in that same development session. Do not assume the accepted scope covers a new authority expansion.
4. Continue from the most advanced real repository state; do not restart an already completed phase merely because an older result document contains stale wording.
5. Keep `production_enabled=false` until final V1 acceptance and explicit user approval.
6. Preserve all safety invariants and favor stability over throughput.

For autonomous project execution, the working rule is:

```text
read current state
        ↓
identify the next unblocked engineering action
        ↓
execute it
        ↓
test / inspect the result
        ↓
fix or continue
```

Do **not** stop merely because analysis or planning has finished. "The next step is ..." is not a completion condition.

Stop and return to the user only when one of these is true:

- the requested/current phase is genuinely complete;
- a product, safety, or production decision requires the user;
- only an external wait such as GitHub Actions remains **and** there is no independent unblocked work left;
- an external dependency makes further safe progress impossible.

If GitHub Actions are running while independent work remains, continue that work. If CI is the only remaining activity, do not waste time polling continuously; report that only CI remains and wait for the user to notify when it finishes.

## Production boundary

`production_enabled=false` is deliberate and remains the hard gate during V1 development.

Production enablement comes only after the complete V1 add-only flow has passed final acceptance, including live multi-author retrieval, pagination/isolation/deduplication, matcher/review behavior, exact state transitions, fail-closed source handling, real download, staging proof, add-only library materialization, inventory verification, task completion, crash/retry recovery, and repeated end-to-end idempotence.

The final switch to production is a separate user decision; successful development tests do not implicitly authorize it.
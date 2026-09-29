# Assistant A6 — local executor integration

## A6.1 — executor handoff contract

Status: **accepted** on `main`. A6.1 is not the full Windows downloader integration.

## Goal

A6.1 creates a narrow deterministic boundary between an exact A5 user-approved pending task and a future local Windows executor.

The boundary is intentionally non-destructive:

```text
pending task
  + exact current A5 user approval
  -> executor command
      -> DOWNLOAD_TO_STAGING_ONLY
          -> local receipt
              -> deterministic receipt validation
                  -> ready for inventory verification at most
```

A6.1 does not download manga, mutate inventory, complete tasks, promote files into the library, replace old versions, or delete anything.

## Command authorization

An executor command is emitted only when the existing A5 task-gate view reports `download_authorized=true` for the current task. That requires:

- current task status is `pending`;
- action is `download` or `upgrade`;
- an exact current gate binding exists for `task_id + task_revision + target_hash`;
- that exact gate has `user_approved=true`.

Assistant recommendation alone never creates an executor command.

## Executor command

Every command is bound to:

- deterministic `command_id`;
- `task_id`;
- `work_id`;
- `task_revision`;
- `target_hash`;
- parsed source (`jm` or `pica`);
- source work ID;
- deterministic action and target.

The v1 command intent is fixed to:

`DOWNLOAD_TO_STAGING_ONLY`

This is deliberate. A future local executor must download into isolated staging first. Promotion/replacement/deletion are separate later gates.

Unknown source namespaces fail closed.

## Queue behavior

`assistant-executor-queue` reads current monitor state plus the optional A5 assistant gate ledger and emits only currently authorized commands.

Properties:

- deterministic ordering by task ID;
- duplicate task IDs fail closed;
- bounded pagination, maximum 200 commands per view;
- absent implicit gate ledger means zero approvals, not permissive behavior;
- no network access;
- no state mutation;
- no physical deletion authority.

## Receipt contract

A future local executor returns an `ExecutorReceipt` bound to the exact command generation:

- `command_id`;
- `task_id`;
- `work_id`;
- `task_revision`;
- `target_hash`;
- outcome `SUCCEEDED`, `FAILED`, or `CANCELLED`.

A `SUCCEEDED` receipt must additionally carry completion evidence:

- downloader explicitly reported full completion;
- non-empty artifact manifest hash;
- positive file count.

This is only the first completion barrier. It is intentionally stronger than trusting a legacy "download command returned" signal, but it still does not establish inventory ownership or replacement safety.

## Receipt validation

`assistant-executor-receipt-check` validates a receipt against the **current** pending task and current A5 approval generation.

A successful receipt reaches at most:

`ready_for_inventory_verification=true`

It explicitly does **not** set or imply:

- task completion authorization;
- replacement authorization;
- physical deletion authorization.

If the task revision/target changed, the receipt is stale and cannot advance. If current user approval is no longer valid, the receipt cannot advance.

## Why A6.1 stops before a real downloader

The imported upstream downloader behavior is not yet a trusted full-completion barrier. Earlier source review established that legacy GUI download commands can return after spawning work rather than after every requested chapter/page is fully complete. Pica also requires fail-closed treatment of incomplete chapter pagination.

Therefore A6.1 freezes the safe command/receipt boundary first. A later A6 subphase must adapt the actual JM/Pica download cores so that `SUCCEEDED` is impossible until all requested target content has completed and an artifact manifest can be produced.

## A6.1 files

A6.1 adds:

- `crates/cloud-monitor/src/executor_handoff.rs`;
- `crates/cloud-monitor/src/bin/assistant-executor-queue.rs`;
- `crates/cloud-monitor/src/bin/assistant-executor-receipt-check.rs`;
- executor handoff/CLI regression tests.

It also allows the AI CI workflow to run on `assistant-*` branches so implementation branches can be validated before merging.

## A6.1 safety boundary

A6.1 does not:

- call JM or Pica;
- call OpenAI from Rust;
- perform a real download;
- write to the local manga archive;
- mutate any of the eight monitor state files;
- mark a pending task completed;
- mutate inventory;
- perform replacement;
- perform deletion;
- enable production;
- write the original No-AI repository.

## A6.1 acceptance criteria

A6.1 is accepted when Linux CI proves:

- no approval -> no command;
- recommendation without approval -> no command through the reused A5 authorization rule;
- exact current user approval -> one bound staging-only command;
- stale gate revision/target cannot authorize a command;
- unsupported source namespaces fail closed;
- queue is bounded and deterministic;
- successful receipt requires explicit full-completion evidence;
- forged command binding fails closed;
- stale task revision/target cannot advance a receipt;
- failed receipts cannot advance;
- even a current successful receipt reaches only inventory-verification readiness;
- executor queue and receipt CLIs operate offline and preserve monitor state bytes;
- all prior workspace tests pass;
- Clippy passes with warnings denied;
- the existing assistant offline/read-only guard passes;
- `production_enabled=false` remains closed.

---

## A6.2 — local executor planning skeleton

Status: **accepted** on `main` at merge commit `4078a1c42c191baba6d53bf5254a7c507e5eca11`; main CI run `34041913181` passed.

A6.2 introduces the local side of the A6.1 boundary without enabling a real downloader. The goal is to make malformed, stale, forged, unsafe, or unsupported executor commands fail before any source-specific implementation can run.

The current flow is:

```text
ExecutorCommand
    -> revalidate schema / command binding / target hash / source binding
        -> deterministic LocalExecutionPlan
            -> isolated commands/<command_id> staging namespace
                -> execution_supported=false
```

### A6.2 invariants

The local planner independently checks:

- A6.1 executor schema version;
- exact `DOWNLOAD_TO_STAGING_ONLY` intent;
- action is only `download` or `upgrade`;
- non-empty task/work/revision/hash binding;
- deterministic `command_id` recomputation;
- target object hash equals `target_hash`;
- source is exactly `jm` or `pica`;
- source work ID is non-empty;
- `target.source_key` matches `source:source_work_id` exactly.

A valid plan carries the exact command/task/work/revision/hash binding into the local layer and derives only a command-owned staging subdirectory.

### Deliberately disabled capabilities

A6.2 currently emits:

- `execution_supported=false`;
- `promotion_authorized=false`;
- `replacement_authorized=false`;
- `physical_delete_authorized=false`.

This is a hard boundary, not a TODO default. Real execution remains blocked until a trusted source-specific completion bridge exists.

### A6.2 CLI

`assistant-local-executor-plan --command <executor-command.json>` parses one serialized A6.1 command and outputs the validated staging plan. The CLI is offline and does not read or mutate the monitor state directory.

A6.2 does **not** call JM/Pica, write manga files, produce a successful downloader receipt, promote staging, complete pending tasks, replace old versions, or delete anything.

---

## A6.3 — staging artifact/completion protocol

Status: **implementation candidate on `assistant-a6-staging-manifest`**.

A6.3 defines the source-independent evidence that a future JM/Pica bridge must produce before it can claim full download completion.

The protocol binds a `StagingManifest` to the exact A6.2 local plan:

- `command_id`;
- `task_id` / `work_id`;
- `task_revision`;
- `target_hash`;
- backend and source work ID;
- exact command-owned staging subdirectory.

A manifest cannot validate unless the source-specific bridge explicitly reports all of the following:

- source enumeration is complete;
- all scheduled download tasks have actually joined/completed;
- downloader reports full completion;
- expected content-unit count is positive;
- completed content units equal expected units;
- failed content units are zero;
- at least one staged artifact exists.

This deliberately addresses the two known downloader risks: incomplete source enumeration/pagination and legacy commands returning after work is spawned rather than joined.

### Artifact manifest safety

Each artifact carries a staging-relative path, non-zero byte size, and lower-case SHA-256. Paths are portable `/`-separated relative paths and reject:

- absolute paths;
- `.` / `..` segments;
- backslashes;
- control characters;
- Windows-invalid filename characters;
- trailing dots/spaces;
- duplicate or case-insensitive colliding paths.

Artifacts must be strictly sorted by relative path so the manifest has one canonical serialization and deterministic SHA-256 manifest hash.

A validated manifest yields the exact A6.1 `CompletionEvidence` shape (`downloader_reported_full_completion=true`, manifest hash, file count), but it still grants **zero** inventory, task-completion, promotion, replacement, or deletion authority.

### A6.3 CLI

`assistant-staging-manifest-check --plan <local-plan.json> --manifest <staging-manifest.json>` validates the protocol offline. It does not read the monitor state directory or touch staged files; actual file hashing belongs to the later local executor bridge.

### A6.3 acceptance criteria

Before merge, Linux CI must prove:

- exact plan/manifest binding;
- deterministic manifest hash and completion evidence;
- incomplete pagination/enumeration fails closed;
- spawned-but-unjoined downloads fail closed;
- downloader not reporting full completion fails closed;
- expected/completed/failed content-unit mismatch fails closed;
- unsafe/traversal artifact paths fail closed;
- Windows case-insensitive path collisions fail closed;
- non-canonical artifact order fails closed;
- zero-size or malformed SHA-256 metadata fails closed;
- CLI is offline/read-only;
- all existing regression tests and Clippy pass;
- `production_enabled=false` remains closed.

A6.3 still performs no real source request, no manga download, no file hashing, no state mutation, no promotion/replacement, and no deletion.

---

Full A6 remains incomplete until a Windows-capable local executor and trusted JM/Pica completion bridge are implemented and validated. After A6.3, the next safe subphase is a filesystem-only staging verifier that hashes real staged files and proves they exactly match the validated manifest, still without promotion or deletion.

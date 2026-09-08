# Assistant A5 — recommendation and explicit download approval queue

## Goal

A5 separates an assistant recommendation from explicit user authorization for an already-existing deterministic pending task.

The existing `pending.json` task remains the backend's only source of download/upgrade intent. A5 does not duplicate task targets, alter task lifecycle state, or start a download.

## Why the semantic gate is separate from `pending.json`

Recommendation and user approval are assistant/user judgments, while `pending.json` is deterministic backend state. Keeping the semantic gate in a small assistant-owned ledger makes the boundary explicit:

- source scans can continue with no AI;
- ordinary monitor persistence cannot accidentally turn a recommendation into authorization;
- the accepted production `Task` schema and task lifecycle are not widened merely to store optional AI metadata;
- an absent assistant ledger safely means that no task is approved;
- a future local executor must combine current pending facts with the exact approval binding before it can execute anything.

This is **not a parallel task database**. The ledger stores no source metadata, title, version, action, local path, or target body. It stores only semantic gate records referencing an existing task by immutable binding data.

## Existing pending task remains authoritative

A pending task already carries:

- `task_id`;
- `work_id`;
- `task_revision`;
- deterministic target;
- action (`download` / `upgrade`);
- lifecycle status;
- old local item IDs where applicable.

A5 derives `target_hash = hash(task.target)` and never copies the target into assistant state.

## Assistant gate ledger

The optional durable assistant ledger is `assistant-task-gates.json`.

Its records are bound to:

- `task_id`;
- `task_revision`;
- `target_hash`.

Each exact binding can independently record:

- `assistant_recommended: true|false`;
- `user_approved: true|false`.

Historical bindings may remain in the ledger. They are harmless because authorization is calculated only against the **current** pending task revision and target hash.

Recommendation does not imply approval. Approval does not require a recommendation; the user may explicitly approve a deterministic task directly.

## Revision binding

A gate record is current only when all three values match the current pending task:

- task ID;
- task revision;
- deterministic target hash.

Therefore a better candidate that increments `task_revision` can never inherit an older recommendation or approval. No production-task mutation is required to invalidate it.

## Authorization rule

A future local executor may treat a task as user-authorized only when all are true:

1. current task lifecycle `status == "pending"`;
2. current task action is `download` or `upgrade`;
3. an assistant gate record exists for the exact current `task_id + task_revision + target_hash`;
4. that record has `user_approved == true`.

`assistant_recommended` is never an authorization condition.

A5 exposes this computed authorization state but performs no executor action.

## Staged operations

`assistant-task-gate-edit` supports exactly:

- `recommend` — set recommendation for the exact current binding;
- `clear-recommendation` — clear recommendation for the exact current binding;
- `approve` — set explicit user approval for the exact current binding;
- `revoke` — clear user approval for the exact current binding.

Every request must provide:

- `task_id`;
- expected `task_revision`;
- expected `target_hash`.

This protects against acting on a stale assistant view after the backend revised a task.

## Eligibility

Gate edits operate only on one known current task ID.

`recommend` and `approve` require the task to be currently `pending` and its action to be `download` or `upgrade`.

Clearing/revoking is allowed for the exact current binding even when the corresponding flag is already false; repeated clears are deterministic no-ops.

Unknown task IDs, stale revisions, stale target hashes, unsupported task actions, malformed gate ledgers, duplicate exact gate bindings, and non-pending positive gate operations fail closed.

## Staging output

The CLI reads a complete monitor state directory plus an optional current assistant gate ledger and writes only into a new output directory:

- `assistant-task-gates.json` — complete proposed semantic gate ledger;
- `task-gate-change.json` — audit of the requested operation and before/after flags;
- `executor-preview.json` — deterministic current authorization view for the affected task.

If no current assistant gate ledger exists, the input is treated as an empty version-1 ledger. The monitor state directory is never modified. Existing output directories are refused.

A5 does not publish the staged ledger back to the repository; commit-aware publication remains a separate write boundary.

## Assistant-facing task view

A5 provides a bounded deterministic task-gate view that combines current `pending.json` facts with the semantic ledger and exposes:

- task ID and work ID;
- task revision and action/status;
- deterministic current target hash;
- exact current recommendation flag;
- exact current approval flag;
- `download_authorized`;
- count of stale historical gate bindings for the same task ID.

This preserves the A1 rule that ChatGPT reads bounded structured evidence rather than unrestricted backend state.

## Safety boundary

A5 does not:

- call JM/Pica;
- call OpenAI from Rust;
- create or alter source observations;
- decide identity;
- create new work IDs;
- mutate `pending.json`;
- download anything;
- mark a task complete;
- mutate inventory;
- replace or delete local files;
- enable production;
- write the original No-AI repository.

## Acceptance criteria

A5 is accepted when tests prove:

- recommendation and approval are independently stored;
- recommendation alone never yields `download_authorized=true`;
- explicit approval of the exact current pending binding yields authorization;
- stale revision or target hash fails closed;
- an older approval cannot authorize a revised target;
- positive gate operations fail for inactive/ignored/done/superseded tasks;
- unknown task IDs and unsupported actions fail closed;
- repeated recommend/approve/clear/revoke operations are idempotent;
- malformed or duplicate gate records fail closed;
- unrelated historical gate bindings are preserved;
- bounded task-gate view exposes target hash and computed authorization correctly;
- existing monitor persistence and old pending tasks are untouched and remain fully compatible;
- input monitor state bytes remain unchanged by the staging CLI;
- an existing gate-ledger input remains unchanged;
- existing output directories are refused;
- execution succeeds with invalid loopback network proxies;
- all prior tests and Clippy continue to pass;
- `production_enabled=false` remains unchanged.

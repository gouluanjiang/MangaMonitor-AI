# Assistant A6.8 — current-generation source preflight authorization

Status: implementation/validation on `assistant-a6-preflight-authorization`.

Baseline: `main@c4615b08d7de3a344e270ec4cb7f68d54d6cc51f` (accepted A6.7).

## Goal

A6.8 defines the immediate authorization barrier that must be re-run against the current monitor state and current user-approval ledger immediately before a future live JM/Pica metadata preflight.

It closes the gap between:

- A6.6 serialized disabled source request;
- A6.7 pinned read-only source enumeration primitives;
- a future live network preflight runner.

A saved A6.6 request or saved authorization result is never sufficient by itself to access a source.

## Current-generation checks

`source_preflight_authorization::authorize` accepts the current in-memory:

- monitor `State`;
- assistant `GateLedger`;
- exact A6.1 `ExecutorCommand`;
- exact A6.2 `LocalExecutionPlan`;
- exact A6.6 `SourceBridgeRequest`.

Before source metadata access may proceed it requires:

1. the gate ledger is structurally valid;
2. the A6.2 plan regenerates exactly from the supplied A6.1 command;
3. the A6.6 request validates against that exact plan;
4. exactly one current pending task matches the command task ID;
5. current work ID, revision, target hash, and action still equal the command generation;
6. the current exact gate record still yields `download_authorized=true` and `user_approved=true`;
7. the request command/task/work/revision/target/source binding still equals the command.

Any revision/target/approval change therefore invalidates old serialized A6 artifacts before a live metadata read. A task that has disappeared from pending or is no longer pending also cannot replay old A6 artifacts.

## Non-reusable result

The successful authorization result is deliberately diagnostic rather than a transferable capability:

- `source_metadata_read_authorized=true` only records the result of the immediate check;
- `reusable_permit=false` always;
- the Rust type is `Serialize`-only and cannot be deserialized back into a typed authorization object;
- a future network runner must call `authorize` in-process against the state and ledger it is about to use;
- the output records a deterministic current-state/task-generation binding hash and gate-ledger hash for audit comparison.

## Authority boundary

A6.8 never authorizes:

- image-byte download;
- staging write;
- inventory mutation;
- task completion;
- promotion;
- replacement;
- physical deletion.

A6.8 itself performs no source network call and no state/filesystem mutation.

## CLI

`assistant-source-preflight-authorize` is an offline/read-only diagnostic CLI. For the security-sensitive state read it requires the atomically committed authoritative `checkpoint.json`; it does not reconstruct state from the eight human-facing export files and does not fall back if the checkpoint is missing. It then loads the current gate ledger, reads the exact command/plan/request files, calls the same authorization function, and prints the diagnostic result.

The CLI does not perform JM/Pica access. Tests preserve the authoritative checkpoint, monitor export files, gate file, and command/plan/request files byte-for-byte; prove that revoking the gate causes the same command/plan/request files to fail on the next invocation; and prove that removing `checkpoint.json` fails closed even when all export files remain present.

## Acceptance criteria

A6.8 is accepted only when:

- exact current state/task generation is revalidated;
- exact current user approval is revalidated;
- authoritative atomic checkpoint state is required by the CLI;
- stale revision and changed target fail closed;
- removed/non-pending task fails closed;
- revoked/stale approval fails closed;
- forged plan/request fail closed;
- ambiguous duplicate task IDs fail closed;
- the returned result is explicitly non-reusable and serialize-only;
- no source network request is made by the A6.8 module/CLI;
- no input or monitor-state mutation occurs;
- all mutation/download capability outputs remain false;
- workspace tests and Clippy pass;
- assistant offline/read-only guard passes;
- `production_enabled=false` remains closed.

## Next gate

Only after A6.8 is accepted may A6.9 combine this in-process current-generation check with the already-pinned A6.7 JM/Pica metadata enumeration primitives. A6.9 may read source metadata only. Image-byte download remains a later explicit gate.

# Assistant A6.9 — live metadata-only source preflight results

Status: **PASS candidate** on `assistant-a6-live-source-preflight`.

Baseline: `main@55aad178f1395af4c96071fe59562d9c34479375` (accepted A6.8).

## Goal

A6.9 is the first accepted implementation layer capable of reaching the pinned JM/Pica source clients for live source reads. Its authority is restricted to metadata enumeration required to produce the accepted A6.7 exact expected-source-scope proof.

It downloads no image media bytes and writes no staging content.

## Double current-generation authorization

`live_source_preflight::run_live` wraps the source metadata enumeration between two A6.8 authorization checks:

1. authorize the exact current checkpoint/task/gate generation;
2. enumerate pinned source metadata;
3. reload current state/gate inputs through the caller;
4. authorize the same command/plan/request generation again;
5. require the pre/post state-task binding hash and gate-ledger hash to be unchanged;
6. only then normalize the enumeration into the accepted A6.7 evidence/proof.

If approval is revoked, the task generation changes, the task disappears/finishes, or the gate ledger changes while enumeration is in progress, the run fails closed and does not return an accepted proof.

## JM live path

The JM path uses only the accepted A6.7 read primitives and pinned upstream behavior:

- `jm_adapter::DEFAULT_DOMAIN`, already constrained by the pinned baseline domain list;
- complete chapter enumeration through the pinned `/album` shape;
- exact per-chapter image-entry enumeration/count through the pinned `/chapter` shape;
- exact canonical chapter IDs/orders and per-chapter expected image counts.

No JM image media URL is requested by A6.9.

## Pica live path

The Pica path uses only the accepted A6.7 metadata endpoints:

- complete `comics/<id>/eps?page=N` chapter pagination;
- complete `comics/<id>/order/<order>/pages?page=N` image-metadata pagination for every chapter;
- exact A6.7 pagination and per-chapter expected-image accounting.

A hard fail-closed page budget of `1000` applies to both whole-work and per-chapter enumeration.

The Pica token is accepted by the official CLI only through `MANGAMONITOR_PICA_TOKEN`. It is never a CLI argument and is absent from command/plan/request/evidence/proof/result structures and repository fixtures.

## Live CLI

`assistant-live-source-preflight` loads:

- authoritative atomic `checkpoint.json`;
- current gate ledger;
- exact A6.1 executor command;
- exact A6.2 local plan;
- exact A6.6 source request.

It passes those into the live runner. The post-enumeration reload again uses authoritative checkpoint state and the current gate ledger.

The CLI has no staging/output-directory argument and performs no monitor-state save. Successful metadata evidence/proof is printed to stdout.

## Pre-network authorization proof

The CLI regression deliberately supplies unusable proxy settings together with a revoked gate. The invocation must fail with `SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED` and must not surface a transport/timeout error. This proves that an unauthorized command is rejected before the live enumerator can reach a source transport attempt.

The same regression verifies checkpoint/gate/command/plan/request bytes remain unchanged.

## A6.7 proof binding

After the post-enumeration authorization check passes, A6.9 calls the accepted A6.7 validator. Therefore an accepted live result contains:

- exact command/task/work/revision/target/source/upstream binding;
- exact canonical chapters and orders;
- exact per-chapter image counts;
- exact Pica pagination evidence where required;
- exact total expected content units;
- deterministic A6.7 `preflight_hash`.

The result cannot be used as completion evidence. A later downloader must still download the exact scope, join all work, hash all staged artifacts, and satisfy A6.5/A6.3/A6.4 independently.

## Authority boundary

Every accepted A6.9 result keeps all later authority false:

- `image_download_authorized=false`;
- `staging_write_authorized=false`;
- `inventory_mutation_authorized=false`;
- `task_completion_authorized=false`;
- `promotion_authorized=false`;
- `replacement_authorized=false`;
- `physical_delete_authorized=false`.

## Validation

Final code CI: `34086036529` — **SUCCESS** on code commit `64070d189105ac3bc5a571c680bfc501bdd9ac0e`.

Passed:

- complete workspace regression suite;
- stable pre/post authorization positive case with metadata-only evidence;
- approval-change-during-enumeration fail-closed case;
- task-generation-change-during-enumeration fail-closed case;
- gate-ledger-change-during-enumeration fail-closed case;
- A6.7 expected-scope validation after the second authorization;
- all downstream authority outputs remain false;
- live CLI revoked-approval-before-network regression;
- live CLI checkpoint/gate/command/plan/request byte preservation;
- Clippy with warnings denied;
- existing assistant offline/read-only regression;
- `production_enabled=false` guard.

The deterministic CI does not depend on live external JM/Pica availability and does not require private Pica credentials.

## Safety boundary and next phase

A6.9 enables only live source **metadata** enumeration. It does not download image bytes, create staging directories, write artifacts, mutate inventory/task state, promote/replace content, or delete anything.

The next safe phase must introduce a separate image-download authorization bound to the exact A6.7/A6.9 `preflight_hash`, restrict writes to `commands/<command_id>`, and generate exact A6.5 completion artifacts/hashes for A6.4 filesystem verification. No inventory or promotion transition may be coupled to the download operation itself.

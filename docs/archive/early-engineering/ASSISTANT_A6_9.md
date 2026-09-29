# Assistant A6.9 — live metadata-only source preflight

Status: implementation/validation on `assistant-a6-live-source-preflight`.

Baseline: `main@55aad178f1395af4c96071fe59562d9c34479375` (accepted A6.8).

## Goal

A6.9 is the first A6 phase permitted to perform live source network reads. Its authority is intentionally limited to the pinned JM/Pica metadata needed to build the already-accepted A6.7 expected-source-scope proof.

The live flow is:

```text
atomic current checkpoint + current gate ledger
    -> A6.8 immediate authorization
        -> pinned JM/Pica metadata enumeration only
            -> reload atomic checkpoint + gate ledger
                -> A6.8 immediate authorization again
                    -> require unchanged authorization generation
                        -> A6.7 expected-scope validation/proof
```

No image media bytes are downloaded and no staging path is written.

## Before/after authorization barrier

`live_source_preflight::run_live` calls the accepted A6.8 authorization function immediately before source enumeration and again after enumeration completes.

The result is rejected if the post-enumeration check no longer authorizes the command or if any of these diagnostic generation bindings changed during the enumeration window:

- command/task/work/revision/target/source/source-work binding;
- current state/task-generation binding hash;
- gate-ledger hash.

This means revoking approval, changing the task generation, removing/finishing the task, or changing the gate ledger while source metadata is being read invalidates the whole run. The caller receives no accepted A6.7 proof from that run.

A later image-download phase must still reauthorize independently; an A6.9 proof is expected-scope evidence, not a reusable execution permit.

## JM live metadata path

JM remains pinned to the accepted upstream and pinned baseline domains. A6.9 uses the A6.7 adapter primitives only:

1. enumerate complete chapter scope from the pinned `/album` shape;
2. for each exact chapter, enumerate/count the pinned `/chapter` image entries;
3. build the A6.7 canonical chapter IDs/orders and per-chapter expected image counts.

A6.9 uses `jm_adapter::DEFAULT_DOMAIN`, which is already restricted to the pinned baseline set. It does not request an image media URL.

## Pica live metadata path

Pica remains pinned to the accepted upstream. A6.9 uses only:

1. complete `comics/<id>/eps?page=N` chapter pagination;
2. complete `comics/<id>/order/<order>/pages?page=N` image-metadata pagination for every chapter;
3. the A6.7 exact pagination/count proof shape.

Both whole-work and per-chapter pagination have a hard fail-closed page budget of `1000`; reaching that budget before the source-reported final page is an error.

The Pica token is supplied only through the `MANGAMONITOR_PICA_TOKEN` environment variable by the CLI. It is never accepted as a CLI argument and never enters command/plan/request/evidence/proof/result JSON or test fixtures.

## CLI

`assistant-live-source-preflight` requires:

- authoritative atomic `checkpoint.json` via `persistence::load_checkpoint`;
- current assistant gate ledger;
- exact A6.1 command;
- exact A6.2 local plan;
- exact A6.6 source request.

It performs no monitor-state save and has no staging/output-path argument. Accepted results are printed to stdout as metadata-only evidence/proof.

The initial authorization occurs inside `run_live` before the enumerator is invoked. Offline regression deliberately configures unusable network proxies and a revoked gate and requires the approval error to occur before any transport error, proving that an unauthorized command cannot reach source networking.

## Result authority

Every successful A6.9 result keeps these false:

- image download authorization;
- staging write authorization;
- inventory mutation authorization;
- task completion authorization;
- promotion authorization;
- replacement authorization;
- physical deletion authorization.

The result carries both pre/post state and gate hashes plus `authorization_stable=true` only after the double-check succeeds.

## CI and live-source validation boundary

Workspace CI validates all contracts, pre/post authorization behavior, CLI preauthorization ordering, and all prior regression/safety gates without depending on external JM/Pica availability or private Pica credentials.

A later live-source smoke record may exercise the real metadata path where an appropriate current approved task and required source credential are available. External source availability is not allowed to make the deterministic repository CI flaky.

## Acceptance criteria

A6.9 is accepted only when:

- source networking is unreachable before a valid current A6.8 authorization;
- live enumeration uses only accepted A6.7 JM/Pica metadata primitives;
- Pica credential material is environment-only and absent from outputs;
- complete source scope becomes an A6.7-valid evidence/proof pair;
- post-enumeration state and gate authorization are reloaded/revalidated;
- any generation/approval change during enumeration fails closed;
- no image media request is introduced by the A6.9 runner;
- no staging or monitor-state mutation occurs;
- every downstream mutation/download authority remains false;
- workspace tests and Clippy pass;
- existing assistant read-only/offline regression passes;
- `production_enabled=false` remains closed.

## Next gate

After A6.9, real image-byte download may only be introduced behind a new explicit gate bound to the exact A6.9/A6.7 `preflight_hash`. That later phase must download into command-owned staging only and must produce exact A6.5 completion evidence before any inventory or promotion transition can advance.

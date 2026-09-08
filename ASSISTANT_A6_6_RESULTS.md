# Assistant A6.6 — disabled source-bridge request/spec results

Status: **PASS candidate** on `assistant-a6-source-bridge-request`.

Baseline: `main@754551518d331288ed46439d874cb8c5cb2ffe1e` (accepted A6.5).

## Goal

A6.6 defines the deterministic, execution-disabled request/specification that translates an accepted A6.2 local execution plan into the exact pinned JM/Pica source-bridge requirements a future worker must satisfy. It does not execute either source bridge.

## Request binding

A request is accepted only when the local plan remains bound to the expected A6 command generation and staging namespace:

- local executor schema and `DOWNLOAD_TO_STAGING_ONLY` intent are exact;
- deterministic command ID matches `task_id + task_revision + target_hash`;
- task/work/revision/hash bindings are non-empty and canonical;
- staging remains exactly `commands/<command_id>`;
- backend is exactly JM or Pica;
- JM source work IDs are decimal digits;
- Pica source work IDs are 24 hex characters;
- the request carries the exact plan command/task/work/revision/target/source/staging binding.

## Pinned source requirements

The request reuses the accepted A6.5 pinned upstream contracts:

- JM: `f0cdd724af6892002f2fb7be883b88832cebe7e9`;
- Pica: `77c8b62ede42b3afc074506d092313816af8092d`.

The request is additionally bound directly to `SOURCE_COMPLETION_SCHEMA_VERSION`, so A6.6 cannot silently drift from the A6.5 completion transcript/proof contract.

The scope is fixed to `FULL_SOURCE_WORK`.

JM requires no credential in the request (`auth_mode=NONE`). Pica declares only `auth_mode=PICA_TOKEN_REQUIRED`; no credential or token value is carried by A6.6.

## Completion requirements

Every emitted request requires:

- complete chapter enumeration;
- complete image enumeration/accounting;
- all scheduled downloads joined;
- terminal completed state;
- exact staged artifact hashes;
- task-create/spawn return is explicitly forbidden as a completion signal.

These requirements preserve the A6.5 barriers against the known JM/Pica spawn-before-complete behavior and Pica partial chapter aggregation failure mode.

## Disabled capabilities

Every accepted A6.6 request has all execution/mutation authority closed:

- `network_execution_enabled=false`;
- `staging_write_enabled=false`;
- `inventory_mutation_authorized=false`;
- `task_completion_authorized=false`;
- `promotion_authorized=false`;
- `replacement_authorized=false`;
- `physical_delete_authorized=false`.

A local plan that already claims execution/promotion/replacement/delete capability is rejected before a request is built.

## CLI

`assistant-source-bridge-request --plan <local-plan.json>` reads one serialized A6.2 plan, validates it, and prints the disabled source-bridge request. The CLI does not mutate its input or monitor state and contains no source execution path.

## Validation

Final code CI: `34083483720` — **SUCCESS** on commit `5f1dd714bd9bd8155679977dfa89d5e47730d99a`.

Passed:

- complete workspace regression suite;
- JM pinned request positive case;
- Pica pinned request positive case;
- Pica auth requirement without credential material;
- completion-contract version binding to A6.5;
- tampered upstream/contract binding fail-closed cases;
- tampered completion requirements fail-closed cases;
- attempted network/staging/mutation authority fail-closed cases;
- invalid source work ID and forged unsafe-plan negatives;
- source-bridge request CLI regression and input-byte preservation;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

## Safety boundary

A6.6 performs no JM/Pica request, reads no Pica credential, downloads no image, writes no staging manga file, mutates no monitor state, completes no pending task, promotes/replaces nothing, and deletes nothing.

Real source execution remains a later explicit gate. The next subphase must not enable a downloader merely because an A6.6 request exists; it must preserve the pinned request binding and produce observed A6.5 completion evidence from actual joined source execution before any later inventory or promotion gate can advance.

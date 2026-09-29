# Assistant A6.12 — isolated command-staging execution results

Status: **PASS candidate** on `assistant-a6-isolated-staging-execution`.

Baseline: `main@6a213242c44bd777c9d52eac7e038fa846fa6924` (accepted A6.11).

## Goal

A6.12 introduces the first A6 layer allowed to write media bytes, restricted to a fresh exact `commands/<command_id>` staging tree. It still owns no live JM/Pica network client and grants no downstream archive or task authority.

## Accepted execution chain

A6.12 requires one immutable `IsolatedStagingExecutionContext` containing the exact:

- A6.2 local plan;
- non-transferable A6.10 image-download authorization;
- original A6.7 source-preflight evidence;
- exact A6.7 proof;
- A6.11 media descriptor set;
- staging root.

The plan/source chain and A6.11 descriptor contract are revalidated before any staging mutation.

## Current-generation barrier

A caller-provided reauthorization callback must reproduce the exact original A6.10 authorization generation:

- before creating the command tree;
- before each media fetch/process operation;
- after each fetch/process operation and before its file write;
- after all writes and before completion normalization;
- after filesystem verification immediately before success.

Any task/state/gate generation drift fails closed with no successful A6.12 result. Already-written partial staging bytes may remain, but A6.12 deliberately performs no cleanup or deletion.

## Isolated filesystem writes

The staging root and existing `commands` root must be real directories, not links/reparse points.

The exact command directory is created with create-new semantics. Replaying the same command generation fails rather than reusing or overwriting it. Artifact files are also create-new only.

Every output path therefore remains below the A6.11 deterministic command-owned staging namespace. A6.12 never writes into the local manga archive.

## Processed-media binding

The fetch/process boundary must return final bytes bound to the exact A6.11 descriptor:

- source media ID;
- request URL;
- source format;
- applied transform;
- applied transform parameter.

The final bytes must be non-empty and match the expected image-format signature. Descriptor-binding mismatch or invalid byte format fails before the artifact write.

A6.12 itself does not implement real source networking; the future pinned source fetch/process implementation must plug into this boundary.

## Completion proof

Only after every expected item has been fetched/processed and written does A6.12 construct the A6.5 completion transcript. It then requires:

- exact complete chapter/image scope;
- all scheduled work joined;
- all chapters terminal `COMPLETED`;
- completed image counts exactly equal expected counts;
- zero failed images;
- exact artifact paths;
- non-zero byte sizes and SHA-256 hashes.

The transcript is normalized through A6.5/A6.3 and the actual command tree is independently verified through A6.4. Success therefore requires both the source-completion contract and the real filesystem tree to agree.

## Failure/replay behavior

Regression coverage proves:

- a stale authorization fails before command-directory creation;
- processed-media binding mismatch fails before an artifact write;
- a later fetch failure leaves only partial staging and produces no false completion;
- authorization drift after all expected writes still blocks success;
- replay of the same command generation never overwrites existing staged bytes;
- exact JM/Pica fixture bytes reach only verified command staging;
- downstream mutation/task/promotion/replacement/delete authority remains false.

## Validation

Final code CI: `34089325923` — **SUCCESS** on code commit `7b0e84a5923265fa924fcac6be1a42d59fdf40fd`.

Passed:

- complete workspace regression suite;
- A6.12 isolated staging success/failure/replay tests;
- assistant-view build;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

The earlier Clippy-only `too_many_arguments` finding was resolved by grouping the immutable trust inputs into `IsolatedStagingExecutionContext`; no safety check was removed.

## Authority boundary and next phase

A6.12 may write only final media bytes under the exact fresh command staging tree. It does not own a source network client, mutate inventory, complete tasks, promote/replace archive content, delete partial/old content, or enable production.

The next safe phase is A6.13: implement pinned JM/Pica source-specific fetch/process functions that consume only exact A6.11 descriptors and return `ProcessedMedia` into A6.12. Those functions must not gain direct filesystem or downstream mutation authority.

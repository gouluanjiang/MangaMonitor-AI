# Assistant A6.8 — current-generation source preflight authorization results

Status: **PASS candidate** on `assistant-a6-preflight-authorization`.

Baseline: `main@c4615b08d7de3a344e270ec4cb7f68d54d6cc51f` (accepted A6.7).

## Goal

A6.8 establishes the immediate, current-generation authorization barrier that a future live JM/Pica metadata preflight must pass immediately before source access. Saved A6 command/plan/request artifacts and saved authorization JSON are not transferable source-access capabilities.

## Current-generation authorization

`source_preflight_authorization::authorize` revalidates the current in-memory state and gate ledger against the exact A6.1/A6.2/A6.6 generation:

- the gate ledger must be structurally valid;
- the supplied A6.2 local plan must regenerate exactly from the supplied A6.1 command;
- the A6.6 source request must validate against that exact plan;
- exactly one current pending task must match the task ID;
- current work ID, task revision, target hash, and action must still match the command;
- the exact current gate generation must still have `user_approved=true` and `download_authorized=true`;
- request command/task/work/revision/target/source/source-work binding must still match the command.

A revision change, target change, approval revocation, stale gate record, task removal, non-pending task state, ambiguous duplicate task ID, forged plan, or forged request fails closed.

## Non-reusable diagnostic result

A successful authorization result grants only the immediate metadata-read boundary:

- `current_generation_verified=true`;
- `current_user_approval_verified=true`;
- `source_metadata_read_authorized=true`;
- `reusable_permit=false`.

The Rust result type is intentionally `Serialize`-only and cannot be deserialized back into a typed authorization object. A future network runner must call `authorize` in-process against the state and gate ledger it is about to use rather than consume a saved authorization result.

The diagnostic output records:

- exact command/task/work/revision/target/source binding;
- a deterministic current state/task-generation binding hash;
- a gate-ledger hash;
- the A6.7 source-preflight schema version.

## Atomic state source

`assistant-source-preflight-authorize` uses only the atomically committed authoritative `checkpoint.json` through `persistence::load_checkpoint` for its security-sensitive state read.

It does not reconstruct authorization state from the eight human-facing export files. If `checkpoint.json` is missing, authorization fails closed even when all export files remain present and readable. This avoids a mixed-generation read while state exports are being refreshed after the authoritative checkpoint commit.

## Read-only CLI

`assistant-source-preflight-authorize` is an offline/read-only diagnostic CLI. It loads:

- authoritative `checkpoint.json`;
- current gate ledger;
- exact executor command;
- exact local plan;
- exact source bridge request.

It then calls the same in-process authorization function and prints the non-reusable diagnostic result. The CLI performs no JM/Pica request.

Tests prove that a successful invocation preserves the checkpoint, all eight monitor export files, gate ledger, command, plan, and request byte-for-byte. Revoking the gate causes the same saved command/plan/request to fail on the next invocation.

## Authority boundary

A6.8 always keeps all later capabilities closed:

- `image_download_authorized=false`;
- `staging_write_authorized=false`;
- `inventory_mutation_authorized=false`;
- `task_completion_authorized=false`;
- `promotion_authorized=false`;
- `replacement_authorized=false`;
- `physical_delete_authorized=false`.

A6.8 itself performs no source network request and no state/filesystem mutation.

## Validation

Final code CI: `34085547467` — **SUCCESS** on code commit `8957ff18ade325294f010961164334370f690ee9`.

Passed:

- complete workspace regression suite;
- current exact approval positive case;
- revoked approval fail-closed case;
- stale gate-generation fail-closed case;
- changed task revision fail-closed case;
- changed target fail-closed case;
- removed task fail-closed case;
- non-pending task replay fail-closed case;
- ambiguous duplicate task ID fail-closed case;
- forged A6.2 plan fail-closed case;
- forged A6.6 request fail-closed case;
- serialize-only/non-reusable authorization contract;
- authoritative checkpoint requirement and no export fallback;
- CLI input/checkpoint/export byte-preservation regression;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

## Safety boundary and next phase

A6.8 does not perform live source access. Its only positive authority is an immediate in-process permission to begin pinned **metadata-only** source preflight after all current-generation checks succeed.

The next safe subphase is A6.9: combine this current-generation authorization function with the accepted A6.7 JM/Pica read-only enumeration primitives in one live runner. A6.9 may enumerate source metadata only. It must recheck authorization around the live enumeration window, must emit an A6.7 expected-scope proof, and must still download no image bytes or write staging content.

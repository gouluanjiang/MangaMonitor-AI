# Assistant A6.4 — filesystem staging verifier results

Status: **PASS candidate** on `assistant-a6-filesystem-verifier`.

Baseline: `main@641f6a6e185e854ac7d20a28a53f8e3909961e21` (accepted A6.3).

## Scope

A6.4 adds a read-only local filesystem verifier between the accepted A6.3 metadata manifest and any future inventory/promotion step.

It verifies the exact command-owned staging tree at `commands/<command_id>` and grants no mutation authority.

## Verified invariants

- Reuses A6.3 validation first, so forged/stale plan or manifest bindings fail before filesystem trust.
- Staging root, `commands`, and command directory must be real directories.
- Symlinks are rejected; Windows reparse points are also rejected by the Windows-specific metadata guard.
- Only directories required as ancestors of manifest artifacts are allowed.
- Missing files, extra files, unexpected directories, special files, and non-UTF-8/invalid filesystem paths fail closed.
- Every artifact must be a regular file with the exact manifest byte length and SHA-256.
- Canonicalized command/files must remain under the selected staging root/command root.
- The observed file set must exactly equal the A6.3 manifest file set.
- Verification is deterministic and read-only.
- CLI input plan, manifest, and staged artifact bytes remain unchanged in regression coverage.

## Safety outputs

A successful filesystem verification still emits:

- `inventory_mutation_authorized=false`
- `task_completion_authorized=false`
- `promotion_authorized=false`
- `replacement_authorized=false`
- `physical_delete_authorized=false`

A6.4 does not call JM/Pica, perform a manga download, mutate monitor state, write to the manga archive, promote staging content, replace an old version, or delete anything.

## Validation

Latest branch CI: `34081730244` — **SUCCESS**.

Passed:

- complete workspace regression suite;
- A6.4 exact-tree, missing/extra, unexpected-directory, size/hash drift, missing-root, link/symlink, forged binding tests;
- A6.4 CLI read-only regression;
- build of assistant CLI surface;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

Earlier branch CI `34081556508` also passed before the final Windows-reparse hardening; the final acceptance evidence is `34081730244`.

## Remaining boundary

A6.4 proves that a staged filesystem tree exactly matches the accepted completion manifest. It does **not** prove that a real JM/Pica downloader can yet produce such a manifest correctly.

The next safe subphase is A6.5: define and implement source-specific completion bridge contracts/adapters for JM and Pica while keeping real network/download execution disabled until those contracts are independently tested.

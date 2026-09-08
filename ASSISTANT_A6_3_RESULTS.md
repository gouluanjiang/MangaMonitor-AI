# Assistant A6.3 — Linux validation results

Status: **PASS candidate**

Branch: `assistant-a6-staging-manifest`

Validated head: `76287bc466007356e5f446f592b2bb22d4161090`

GitHub Actions run: `34042603732`

Result: **success**

## Validated behavior

A6.3 defines a source-independent staging artifact/completion protocol. A manifest is bound to the exact A6.2 plan and cannot validate unless the bridge explicitly proves complete source enumeration, all scheduled downloads joined, full downloader completion, complete content-unit accounting, and a non-empty canonical artifact list.

The validator independently rechecks the supplied local plan so a forged plan cannot become a trust bypass. It requires the staging-only intent, deterministic command generation, a valid target hash, JM/PICA backend, exact `commands/<command_id>` namespace, and all destructive/executable capabilities still disabled.

Artifact metadata is canonical and Windows-safe: paths are relative `/` paths, traversal and invalid characters are rejected, reserved Windows device names are rejected, case-insensitive collisions are rejected, ordering is strict, sizes are non-zero, and SHA-256 values are lower-case 64-hex strings.

A valid manifest produces deterministic A6.1 `CompletionEvidence` only. Inventory mutation, task completion, promotion, replacement, and physical deletion remain explicitly unauthorized.

## CI evidence

Run `34042603732` passed:

- `cargo +1.98.1 test --workspace --locked`;
- A6.3 protocol tests;
- forged-plan tests;
- Windows reserved-name/path tests;
- offline/read-only staging-manifest CLI test;
- assistant view build;
- all-target Clippy with warnings denied;
- existing assistant offline/read-only guard;
- production gate assertion.

An earlier run failed only because a test helper named `manifest()` was shadowed by a local `manifest` binding. The helper was renamed to `fixture()`; no production rule was weakened.

## Safety state

A6.3 performs no real JM/Pica request, no manga download, no staged-file hashing, no state mutation, no promotion/replacement, and no deletion.

`production_enabled=false` remains closed.

The original `gouluanjiang/MangaMonitor` No-AI repository was not modified.

## Next safe step

After merge and main CI replay, A6.4 is a filesystem-only staging verifier: hash actual staged files and prove exact equality with the validated manifest while still granting no promotion, replacement, task-completion, or delete authority.

# Local Materialization Runtime Policy

## Purpose

V1 has separate authorities for cloud monitoring, real source/media staging, library materialization, inventory mutation, and task completion. `production_enabled` controls the cloud production monitor and is not a local-library authorization switch.

The repository therefore carries a separate runtime policy file at:

`local-materialization-policy.json`

Its initial and current safe state is:

```json
{
  "schema_version": 1,
  "materialization_enabled": false,
  "command_id": null,
  "task_id": null,
  "task_revision": null,
  "target_hash": null,
  "manifest_hash": null
}
```

## Runtime contract

- Read-only local import planning remains available while materialization is disabled.
- Any mutating `mangamonitor-local-import --confirm-import ...` operation loads the fixed policy file adjacent to the supplied monitor-state directory.
- The policy parser rejects unknown fields and unsupported schema versions.
- A missing/unreadable policy fails closed.
- A disabled policy must not carry a stale binding.
- An enabled policy must bind one exact `command_id`, `task_id`, `task_revision`, `target_hash`, and accepted staging `manifest_hash`; missing or blank binding values fail closed.
- `materialization_enabled=false` fails with `LOCAL_MATERIALIZATION_DISABLED` before any library mutation.
- A command, task generation, target, or staging-manifest mismatch fails with `LOCAL_MATERIALIZATION_BINDING_MISMATCH` before any library mutation.
- The public `local_library_import_gate::execute_add_only` API also requires the opaque authority returned by the fixed-path policy loader, so the CLI is not the only enforcement layer.
- Existing Windows-only, add-only, current-task, approval, staging-proof, no-overwrite, and revalidation checks remain in force after this policy gate.
- This policy does not authorize inventory mutation, task completion, replacement, overwrite, deletion, or cloud production.

## Enablement gate

Do not set `materialization_enabled=true` merely because the implementation exists or CI is green.

For V1, enabling this policy requires all of the following:

1. a real approved new-work task;
2. successful real JM/Pica download into isolated staging;
3. successful staging acceptance and proof verification;
4. explicit user authorization to proceed from that accepted staging result to add-only library materialization;
5. a dedicated reviewed repository change that sets `materialization_enabled=true` and copies the exact accepted `command_id`, `task_id`, `task_revision`, `target_hash`, and verified staging `manifest_hash` into the policy while preserving `production_enabled=false` unless separately approved.

This is intentionally a one-staging-result authority. A later task revision, different target, or different verified manifest cannot reuse it; a new materialization decision requires a new reviewed policy binding.

After execution/acceptance, the policy should return to the disabled/null-binding state before another materialization is considered.

Inventory update and task completion remain later, independent gates even after local materialization is enabled.

## Relation to the 2026-09-08 technical audit

The audit correctly noted that the local import implementation already exposed `--confirm-import` / `execute_add_only`, while the project documentation still described materialization as a later gate. This policy makes that later gate enforceable at runtime instead of relying only on process convention.

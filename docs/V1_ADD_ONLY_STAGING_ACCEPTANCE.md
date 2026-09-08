# V1 add-only real staging acceptance

This is the required first real local acceptance after `docs/V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md` is merged.

It is intentionally **staging-only**. Passing this procedure does not materialize anything into the manga library, mutate inventory, mark a task complete, authorize replacement/deletion, or enable cloud production.

## Preconditions

Use a local Windows checkout at the accepted V1 add-only thaw commit (or later `main` containing that merge).

Required:

- Rust toolchain from `rust-toolchain.toml` is available;
- the current monitor-state directory is present locally;
- the current assistant task gate ledger is present locally when approvals are stored outside the state directory;
- the chosen task is currently user-approved and appears in `assistant-executor-queue`;
- the task action is `download` and represents a genuinely new work with no old local item binding;
- a fresh staging root exists and contains an empty/managed `commands/` directory;
- the staging root is separate from monitor state and the real manga library;
- `GITHUB_ACTIONS` is not `true`;
- for Pica only, `PICA_TOKEN` exists in the local process environment. Never put the token in a command JSON, script argument, report, or repository file.

## 1. Inspect the current executable queue

From the repository root:

```powershell
cargo run --locked -p cloud-monitor --bin assistant-executor-queue -- `
  --state .\monitor-state `
  --offset 0 `
  --limit 200
```

If the gate ledger is stored elsewhere, add:

```powershell
--gates C:\path\to\assistant-task-gates.json
```

The thawed queue deliberately excludes `upgrade` tasks and tasks already bound to local items. Do not construct an executor command by hand to bypass an empty queue.

## 2. Save exactly one current queue command

Inspect the queue and choose the exact currently approved new-work `download` command intended for the acceptance run.

Example using a queue captured as JSON:

```powershell
$queue = Get-Content .\reports\executor-queue.json -Raw | ConvertFrom-Json
$command = $queue.commands | Where-Object { $_.command_id -eq 'EXEC_...' }
if (@($command).Count -ne 1) { throw 'COMMAND_NOT_UNIQUE' }
$command | ConvertTo-Json -Depth 100 | Set-Content -Encoding utf8NoBOM .\reports\acceptance-command.json
```

Do not edit the resulting command JSON. The executor re-binds it to the current task revision, target hash, source/site ID, and current approval generation before any source access.

## 3. Prepare an isolated staging root

Use a path that is not the manga library and not inside monitor state:

```powershell
New-Item -ItemType Directory -Force C:\MangaMonitor-Staging\commands | Out-Null
```

The selected `commands/<command_id>` directory must not already exist. A prior partial attempt is preserved for audit and must not be silently overwritten. A retry requires a current newly authorized command generation or an explicitly reviewed recovery procedure; do not delete partial staging merely to force a retry.

## 4. Provide Pica credential only when required

For a Pica command, set the token only in the local process environment:

```powershell
$env:PICA_TOKEN = '<local token>'
```

JM commands do not require a Pica token.

## 5. Run the guarded acceptance wrapper

```powershell
pwsh -NoProfile -File .\scripts\Run-V1AddOnlyStagingAcceptance.ps1 `
  -StateDir .\monitor-state `
  -CommandFile .\reports\acceptance-command.json `
  -StagingRoot C:\MangaMonitor-Staging `
  -ReportPath C:\MangaMonitor-Acceptance\v1-add-only-staging.json
```

Add `-Gates C:\path\to\assistant-task-gates.json` when needed.

The wrapper refuses to proceed unless the command is still present exactly once in the current thawed executor queue. It then runs the real local executor and independently checks:

- monitor-state file hashes are identical before and after execution;
- the command JSON did not change during execution;
- the command staging directory was fresh;
- every staging filesystem change is confined to `commands/<command_id>/`;
- the local execution report is bound to the selected task/work/command;
- staging completed and the verified receipt outcome is `SUCCEEDED`;
- the receipt is still current and ready only for later inventory verification;
- inventory mutation, task completion, promotion, replacement, physical deletion, and production enablement all remain unauthorized;
- at least one real staged media file exists.

A successful wrapper result has:

```json
{
  "status": "V1_ADD_ONLY_STAGING_ACCEPTED",
  "monitor_state_unchanged": true,
  "staging_changes_confined_to_command_dir": true,
  "ready_for_inventory_verification": true,
  "inventory_mutation_authorized": false,
  "task_completion_authorized": false,
  "promotion_authorized": false,
  "replacement_authorized": false,
  "physical_delete_authorized": false,
  "production_enablement_authorized": false
}
```

## Failure handling

Any failure is a failed acceptance, not permission to weaken a gate.

- Keep partial `commands/<command_id>` output for diagnosis.
- Do not copy partial output into the real library.
- Do not alter inventory or task status.
- Do not delete/replace existing library content.
- Record the exact error and inspect source/preflight/staging evidence before deciding whether a new current command generation is safe.
- Authentication/network/source ambiguity is never an unavailable/deletion certificate.

## Acceptance boundary

After one real run returns `V1_ADD_ONLY_STAGING_ACCEPTED` and the staged files are manually confirmed to exist only under the command-owned staging directory, the real-download thaw gate is accepted for the **V1 add-only staging half**.

The next roadmap gate is separate: design and prove add-only staging-to-library materialization plus atomic/retry-safe inventory update and inventory/filesystem re-verification. That later gate must still refuse existing destinations and must not mark the task complete until all current evidence has been revalidated.

`production_enabled=false` remains unchanged throughout this acceptance.

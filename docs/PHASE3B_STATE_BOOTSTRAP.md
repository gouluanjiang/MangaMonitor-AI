# Phase3B one-time durable state bootstrap

Status: **IMPLEMENTED AS A MANUAL PRE-PRODUCTION GATE; NOT PRODUCTION ENABLEMENT**

This gate exists to break a V1 bootstrap dependency without weakening `production_enabled=false`.

The V1 local staging acceptance requires a genuine current pending task. The durable repository state initially has no completed scan, no catalog records, and no pending tasks. At the same time, scheduled/production Phase3B state mutation is intentionally blocked until final V1 production acceptance. A separate one-time bootstrap is therefore required to initialize real monitor state from the current author registry before production is enabled.

## Authority boundary

The bootstrap may:

- run the formal Phase3B live-source runner against the current `monitor-state/authors.json` registry;
- perform a full JM/Pica scan with the accepted three-author concurrency cap and normal per-batch request budget;
- accumulate all soft batches only under `reports/phase3b-bootstrap/`;
- after every batch is complete and the final state passes invariants, commit the resulting monitor state once using the existing race-protected `commit-monitor-state.sh` path.

It does **not**:

- set or require `production_enabled=true`;
- run on the schedule trigger;
- run from a non-`main` ref;
- invoke the local manga executor or download media files;
- modify the imported local inventory;
- change the author registry or assistant decisions;
- approve tasks;
- materialize staging into the manga library;
- complete tasks;
- authorize replacement or deletion.

## Manual trigger

Use `.github/workflows/phase3b.yml` with:

```text
scope = bootstrap
```

The workflow preflight requires all of the following before any live scan:

- event is a manual `workflow_dispatch` path selecting `bootstrap`;
- selected ref is `main`;
- `monitor-config.json` still has `production_enabled=false`;
- the durable state is pristine for first bootstrap.

The bootstrap script independently rechecks the durable state before any live request. It rejects an existing checkpoint/state manifest, a started/completed scan, non-empty source catalog, pending tasks, active match-review entries, stale task-gate records, a non-`monitor-state` target, or a report path inside the durable state directory.

Enabled author names must be non-empty and unique. Any enabled registry row whose ID starts with `AUTHOR_TEST_` is also rejected before source access. Test-only author rows must not silently become part of the first durable live scan.

## Atomic accumulation model

The production cycle normally commits each soft batch so later scheduled runs can resume. Bootstrap intentionally uses a different durability boundary:

```text
pristine monitor-state
        ↓
batch 0 live scan → reports/.../batch-0
        ↓
batch 1 reads batch-0 → reports/.../batch-1
        ↓
... all batches remain report-only ...
        ↓
final full-scan checks
        ↓
authors unchanged
inventory unchanged
decisions unchanged
JM + Pica full coverage for every enabled author
        ↓
exactly one race-protected monitor-state commit
```

If any source/auth/network/request-budget/batch check fails, the script exits before calling the durable committer. The repository therefore remains in the original pristine state and the failed report tree can be inspected without inventing a partial bootstrap checkpoint.

## Required final invariants

Before the one durable commit, `scripts/run-phase3b-bootstrap.sh` requires:

- every batch report is `complete=true` and strategy-complete;
- no source-error boundary exists in any accepted batch;
- every state manifest is bound to the same current main commit, full mode, exact batch index, and exact batch count;
- the final state contains the complete required Phase3B state file set;
- `authors.json` is canonically unchanged from the durable input;
- `inventory_index.json` is canonically unchanged from the durable input;
- `decisions.json` is canonically unchanged from the durable input;
- persisted `scan_state.json` is schema 3, its `phase3a_scan` is full-mode and complete, and it has no direct failures;
- `phase3a_scan.last_full` contains exactly the JM and Pica key for every enabled author in the registry, with no missing or extra full-coverage key;
- persisted `latest.json` is schema 3 and records a non-empty completed scan ID.

The final commit still goes through `scripts/commit-monitor-state.sh`, which rechecks the manifest base commit, local head, remote `main`, and the remote head again immediately before push.

## Current registry blocker

At the time this gate was implemented, the committed registry still contained an enabled `AUTHOR_TEST_0001` row for `santa`, added through direct `authors.json` edits on 2026-09-07. The normal assistant author-management code generates new user IDs as `AUTHOR_USER_<hash>`, so the test-prefixed row is deliberately treated as unsafe for durable bootstrap until it is explicitly disabled or otherwise resolved.

This is a registry decision, not a reason to weaken the bootstrap gate.

## After bootstrap

A successful bootstrap does not automatically authorize a local download. Inspect the new durable state first:

1. review `review.json`, `pending.json`, and the assistant views;
2. resolve any identity review that genuinely requires a human decision through the existing state-only publication path;
3. for a genuine new-work pending task, create the exact assistant recommendation/approval bound to that task revision and target hash;
4. only then use `docs/V1_ADD_ONLY_STAGING_ACCEPTANCE.md` for the first real local staging-only acceptance.

If bootstrap produces no genuine new-work pending task, do not fabricate one. A later controlled validation/production decision must supply real discovery evidence.

`production_enabled=false` remains unchanged throughout this gate.

# CI Action runtime maintenance (2026-10-07)

## Scope and review base

This maintenance is isolated on `codex/ci-node24-20261007`, based on PR #30's
unchanged head `32ddf512834a7fb1d36e25a95329d6c411371a47`. Its review base is
`maintenance/nsis-verification-20261005`, so the maintenance diff contains only
workflow configuration, CI contracts and this audit record. PR #30 contains the
new Windows installation contract; keeping that exact code in the base lets all
four affected CI suites validate one candidate without changing PR #30 itself.

Keep the maintenance PR separate. After PR #30 is independently resolved and its
code is on `main`, retarget this maintenance PR to `main` and inspect the resulting
diff before merging. Do not merge the maintenance into PR #30's source branch.
This task does not authorize either merge or a public release.

Application code, application tests, dependencies, lockfiles, packaging settings
and production gates are unchanged relative to that base. The existing Node
`24`, pnpm `11.19.0`, Rust `1.98.1`, Windows runner and all test assertions remain.
The original five workflow contracts are retained, with additional contracts for
the audited Action references and the new CI routing.

## Starting evidence and complete inventory

The successful 2026-10-05 runs still reported Actions targeting Node 20 and being
forced onto Node 24:

- [Desktop 37283555016](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283555016),
  Windows job `111677260118`.
- [Baseline 37283555011](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283555011),
  Linux job `111677002728`.

Both logs record Actions Runner `2.337.0`; baseline used `ubuntu-24.04` behind
`ubuntu-latest`. The desktop log resolves pnpm's floating v4 to
`b906affcce14559ad1aafd4ab0e942779e9f58b1` (v4.3.0).

Current main `f7e87fd603e4500ce21e3aa5493f083d019af1dd` has 13 workflows and
50 external Action references. Including PR #30's Windows contract gives 14
workflows and 51 references. No repository-local Action manifests, composite
Actions or reusable-workflow calls were found. All seven external Action paths
below use JavaScript manifests, and every occurrence is upgraded and pinned.

| Action path | References | Release | Audited commit |
| --- | ---: | --- | --- |
| `actions/checkout` | 18 | v5.1.0 | `fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` |
| `actions/setup-node` | 2 | v6.5.0 | `249970729cb0ef3589644e2896645e5dc5ba9c38` |
| `actions/cache` | 2 | v5.1.0 | `caa296126883cff596d87d8935842f9db880ef25` |
| `actions/cache/restore` | 3 | v5.1.0 | `caa296126883cff596d87d8935842f9db880ef25` |
| `actions/cache/save` | 1 | v5.1.0 | `caa296126883cff596d87d8935842f9db880ef25` |
| `actions/upload-artifact` | 23 | v6.0.0 | `b7c566a772e6b6bfb58ed0dc250532a479d7789f` |
| `pnpm/action-setup` | 2 | v5.0.0 | `fc06bc1257f339d1d5d8b3a19a8cae5388b55320` |

The machine-readable inventory is [action-pins.json](../.github/action-pins.json).
Each workflow retains its readable release comment beside the immutable SHA.
The CI contract checks every external `uses` reference against that audited
inventory. It is an offline regression guard; upstream runtime identity was
verified by reading the actual manifests at the pinned commits.

## Official release and compatibility verification

For every selected release, inspect the official release, resolve the tag through
GitHub's Git refs API, and read `action.yml` at the final commit. Every selected
manifest declares `runs.using: node24`, including cache's separate restore/save
manifests. Their Node 24 runner requirement is at least `2.327.1`, below the
`2.337.0` observed in the starting runs.

- [checkout v5.1.0](https://github.com/actions/checkout/releases/tag/v5.1.0)
  and its [manifest](https://github.com/actions/checkout/blob/fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09/action.yml):
  includes the July 2026 safety backport while preserving the existing credential
  layout. Existing `persist-credentials`, `fetch-depth` and checkout refs remain.
  No workflow enables `allow-unsafe-pr-checkout`.
- [setup-node v6.5.0](https://github.com/actions/setup-node/releases/tag/v6.5.0)
  and its [manifest](https://github.com/actions/setup-node/blob/249970729cb0ef3589644e2896645e5dc5ba9c38/action.yml):
  includes the July dependency fixes. The repository does not use the removed
  `always-auth` input. Explicit `package-manager-cache: false` preserves the
  previous absence of automatic npm caching; existing package installation
  commands stay unchanged. The pinned [implementation](https://github.com/actions/setup-node/blob/249970729cb0ef3589644e2896645e5dc5ba9c38/src/main.ts)
  distinguishes explicit cache configuration from automatic detection.
- [cache v5.1.0](https://github.com/actions/cache/releases/tag/v5.1.0):
  [main](https://github.com/actions/cache/blob/caa296126883cff596d87d8935842f9db880ef25/action.yml),
  [restore](https://github.com/actions/cache/blob/caa296126883cff596d87d8935842f9db880ef25/restore/action.yml),
  and [save](https://github.com/actions/cache/blob/caa296126883cff596d87d8935842f9db880ef25/save/action.yml)
  manifests were checked separately. Restore/save order, paths, cache-hit checks
  and `cache-primary-key` use remain. Only the Linux baseline namespace changes
  to separate Ubuntu 24 and 26 target artifacts; Windows caches remain compatible.
- [upload-artifact v6.0.0](https://github.com/actions/upload-artifact/releases/tag/v6.0.0)
  and its [manifest](https://github.com/actions/upload-artifact/blob/b7c566a772e6b6bfb58ed0dc250532a479d7789f/action.yml):
  this is the first major whose default manifest is actually Node 24; the release
  notes explain why v5's preliminary support is insufficient. Existing artifact
  names, paths, retention, hidden-file settings, conditions and missing-file
  failure policies are preserved.
- [pnpm/action-setup v5.0.0](https://github.com/pnpm/action-setup/releases/tag/v5.0.0)
  and its [manifest](https://github.com/pnpm/action-setup/blob/fc06bc1257f339d1d5d8b3a19a8cae5388b55320/action.yml):
  the annotated tag object `b307475762933b98ed359c036b0e51f26b63b74b` resolves to
  the commit in the table. [The comparison with v4.3.0](https://github.com/pnpm/action-setup/compare/v4.3.0...v5.0.0)
  changes only the runtime declaration and development type dependencies in
  three files. `src/` and `dist/` are identical, preserving installation, PATH,
  registry, inputs, outputs and post behavior. Explicit `version: '11.19.0'` and
  `run_install: false` remain. [pnpm's compatibility table](https://pnpm.io/installation)
  supports Node 24 with pnpm 11; actual Linux UI and Windows desktop runs are the
  final compatibility checks.

The selected releases minimize unrelated implementation changes while moving
all Action manifests onto Node 24. Newer majors exist, including ESM and other
behavior changes. No claim is made about a guaranteed support lifetime for an
older major. No runtime-force or insecure-Node opt-out environment variable is
used to suppress the warning.

## Ubuntu 26 remains a separate validation

[GitHub's official migration notice](https://github.blog/changelog/2026-09-17-ubuntu-26-generally-available-and-latest-migration/)
announces the `ubuntu-latest` transition between **2026-10-19 and 2026-11-19**.
All ordinary Linux jobs are explicitly pinned to `ubuntu-24.04` for this Action
maintenance. The baseline and UI workflows also expose a `linux_runner` choice
for manual validation, defaulting to 24.04 and allowing 26.04. A bounded
`runs-on` expression can select only those two hosted labels.

Run the two existing workflows at the same maintenance commit with
`linux_runner=ubuntu-26.04`. The original Linux baseline and full UI steps,
timeouts and assertions execute unchanged. Their concurrency groups include the
selected image, and the Linux Cargo primary key and every restore prefix include
`CI_LINUX_RUNNER`, preventing cross-release reuse of compiled targets.

The baseline Windows job runs in ordinary push/PR/default-dispatch validation.
It is skipped only for an explicit Ubuntu 26 dispatch, avoiding a duplicate test
of the same Windows candidate. Report that run as **Ubuntu 26 Linux baseline**,
not as another complete Windows/Linux baseline run. No `continue-on-error`,
weakened assertion or reduced UI suite is introduced.

The Windows installation contract's push filter now includes `assistant-*` and
`codex/**`, matching the established development branches in baseline/UI/desktop.
Its original paths, main PR filter and PowerShell commands are retained, so the
separate maintenance branch actually runs the contract.

## Validation and evidence boundaries

Formal execution belongs in GitHub Actions under the repository's AGENTS rules.
Do not duplicate these suites in the editing container. The maintenance PR body
is the run-specific evidence ledger: record exact candidate SHA, run IDs,
conclusions, runner image/version, contract/test totals and remaining failures
there after reviewing the logs. This audit document alone is not a passing run.

Required checks for the maintenance candidate:

| Check | Target and evidence |
| --- | --- |
| Baseline | Ubuntu 24 Linux regression plus Windows local-executor contracts |
| UI | Ubuntu 24 formatting, logic, build, Chromium and both existing diagnostics |
| Desktop | Windows native tests, Clippy, release NSIS build and isolated lifecycle |
| Windows installation contract | Existing PowerShell parse and 32 synthetic assertions |
| Ubuntu 26 Linux baseline | Separate dispatch of the same baseline steps and assertions |
| Ubuntu 26 UI | Separate dispatch of the same full UI workflow |

Check the completed logs for the exact pinned downloads, actual Node/pnpm
versions, successful cache restore/save and artifact uploads, and absence of the
Node 20 forced-runtime warning. Preserve any unrelated warning or failing test
as evidence rather than concealing it with retries or weaker assertions.

Only baseline and UI are being qualified on Ubuntu 26 here. Manual live-source,
production, state-publication and local acceptance workflows receive static
Action/Ubuntu pins without being executed. Their migration is not accepted by
these two Linux runs. Existing LOCAL-03, installer root-cause and native user
acceptance boundaries retain their prior status.
